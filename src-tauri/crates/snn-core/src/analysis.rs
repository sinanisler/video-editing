//! Tools that let an agent see and hear the footage: single frames, contact sheets, and where the
//! sound goes quiet or loud. They read media files with FFmpeg and never change the project.
//!
//! A call is split in two so the slow part runs without holding the editor: [`Editor::analysis_job`]
//! turns the arguments into a self-contained [`Job`], and [`Job::run`] does the work.

use crate::editor::Editor;
use crate::export::{MediaInfo, ENCODERS};
use crate::model::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

pub const ANALYSIS_TOOLS: [&str; 3] = ["frames_get", "frames_contact_sheet", "audio_get_activity"];

pub fn is_analysis(name: &str) -> bool {
    ANALYSIS_TOOLS.contains(&name)
}

const MAX_TILES: i64 = 48;
/// Each clip on the track is one FFmpeg run, so a span with more clips than this is refused.
const MAX_AUDIO_PARTS: usize = 240;

/// One picture to pull out of a file.
#[derive(Clone, Debug)]
pub struct Shot {
    path: String,
    seconds: f64,
    /// Burned into the corner of a contact-sheet tile, so the reader knows where each picture is from.
    label: String,
}

#[derive(Clone, Debug)]
pub enum Job {
    Frame { shot: Shot, width: i64, info: Value },
    Sheet { shots: Vec<Shot>, columns: i64, tile: (i64, i64), info: Value },
    Audio { path: String, from: Frame, to: Frame, fps: f64, stream: i64, threshold_db: f64, min_seconds: f64 },
    /// The sound of one audio track of the timeline, clip by clip, reported in timeline frames.
    TimelineAudio { track: String, parts: Vec<AudioPart>, from: Frame, to: Frame, fps: f64, stream: i64, threshold_db: f64, min_seconds: f64 },
}

/// The stretch of a file that one clip on the timeline plays.
#[derive(Clone, Debug)]
pub struct AudioPart {
    path: String,
    /// Source frames of the file to listen to.
    source: (Frame, Frame),
    /// Timeline frame where `source.0` is heard, and how fast the clip plays.
    at: Frame,
    speed: f64,
    /// Timeline frames the part covers.
    span: (Frame, Frame),
}

/// What FFmpeg heard in a stretch of a file: silences and the loudest moment of each second, in seconds from where
/// reading started.
struct Heard {
    silences: Vec<(f64, f64)>,
    loud: Vec<(f64, f64)>,
    /// How many audio streams the file has.
    streams: usize,
}

/// What an analysis tool returns: facts as JSON and, for the frame tools, a JPEG.
pub struct Analysis {
    pub info: Value,
    pub jpeg: Option<Vec<u8>>,
}

/// Minutes and seconds to the hundredth, as in `4m07.50s`: two frames half a second apart must not read the same.
pub fn timecode(seconds: f64) -> String {
    let hundredths = (seconds.max(0.0) * 100.0).round() as i64;
    format!("{}m{:02}.{:02}s", hundredths / 6000, hundredths / 100 % 60, hundredths % 100)
}

/// An even number of pixels, as most encoders and scalers want.
fn even(n: f64) -> i64 {
    ((n / 2.0).round() as i64 * 2).max(2)
}

impl Editor {
    /// The visible video clip an agent means by a timeline frame: the topmost one that plays a file.
    fn clip_on_timeline(&self, at: Frame) -> Option<(&Clip, &MediaItem)> {
        let p = self.project();
        let tracks = p.sequence.tracks.iter().filter(|t| t.kind == TrackKind::Video && !t.hidden);
        for t in tracks {
            let clip = p.sequence.clips.iter().find(|c| c.track == t.id && c.kind == ClipKind::Video && c.start <= at && at < c.end());
            let Some((c, m)) = clip.and_then(|c| Some((c, p.media(c.media.as_ref()?)?))) else { continue };
            if m.kind == MediaKind::Video {
                return Some((c, m));
            }
        }
        None
    }

    fn shot_on_timeline(&self, at: Frame) -> Option<Shot> {
        let p = self.project();
        let fps = p.settings.fps_num as f64 / p.settings.fps_den as f64;
        let (c, m) = self.clip_on_timeline(at)?;
        Some(Shot { path: m.path.clone(), seconds: c.source_at(at) / fps, label: format!("T {} f{at}", timecode(at as f64 / fps)) })
    }

    fn video(&self, id: &str) -> Result<&MediaItem, String> {
        let m = self.project().media(id).ok_or_else(|| format!("no media {id}"))?;
        if m.kind != MediaKind::Video {
            return Err(format!("{} is not a video", m.name));
        }
        Ok(m)
    }

    /// Checks the arguments of an analysis tool against the project and works out what to read.
    pub fn analysis_job(&self, name: &str, args: &Value) -> Result<Job, String> {
        let p = self.project();
        let fps = p.settings.fps_num as f64 / p.settings.fps_den as f64;
        let media = args["media"].as_str();
        let int = |key: &str, default: i64, min: i64, max: i64| args[key].as_i64().unwrap_or(default).clamp(min, max);
        match name {
            "frames_get" => {
                let at = args["at"].as_i64().filter(|a| *a >= 0).ok_or("at must be a frame number")?;
                let width = int("width", 960, 160, 1920);
                let time = timecode(at as f64 / fps);
                // The answer says which frame the picture is, in both kinds of frame number when it came off the timeline.
                let (shot, info) = match media {
                    Some(id) => {
                        let m = self.video(id)?;
                        if at >= m.duration {
                            return Err(format!("{} is only {} frames long", m.name, m.duration));
                        }
                        let shot = Shot { path: m.path.clone(), seconds: at as f64 / fps, label: format!("{time} f{at}") };
                        (shot, json!({ "frames": "source frames of the media", "media": id, "source_frame": at, "time": time, "seconds": at as f64 / fps, "width": width }))
                    }
                    None => {
                        let shot = self.shot_on_timeline(at).ok_or_else(|| format!("no video clip is visible at timeline frame {at}"))?;
                        let (c, m) = self.clip_on_timeline(at).expect("the shot came from this clip");
                        let source = c.source_at(at).round() as Frame;
                        let info = json!({
                            "frames": "timeline frames",
                            "timeline_frame": at,
                            "time": time,
                            "seconds": at as f64 / fps,
                            "clip": c.id,
                            "track": c.track,
                            "media": m.id,
                            "source_frame": source,
                            "source_time": timecode(source as f64 / fps),
                            "shows": "the footage only: no effects, cards or captions. frames_render shows the finished picture.",
                            "width": width,
                        });
                        (shot, info)
                    }
                };
                Ok(Job::Frame { shot, width, info })
            }
            "frames_contact_sheet" => {
                let end = match media {
                    Some(id) => self.video(id)?.duration,
                    None => p.duration(),
                };
                let from = args["from"].as_i64().unwrap_or(0).max(0);
                let to = args["to"].as_i64().unwrap_or(end).min(end);
                if to <= from {
                    return Err(format!("nothing to show between frame {from} and frame {to}"));
                }
                let count = int("count", 24, 1, MAX_TILES).min(to - from);
                let frames: Vec<Frame> = (0..count).map(|i| from + (to - from) * i / count).collect();
                let (shots, aspect) = match media {
                    Some(id) => {
                        let m = self.video(id)?;
                        let shot = |f: &Frame| Shot { path: m.path.clone(), seconds: *f as f64 / fps, label: format!("{} f{f}", timecode(*f as f64 / fps)) };
                        let aspect = if m.width > 0 && m.height > 0 { m.width as f64 / m.height as f64 } else { 16.0 / 9.0 };
                        (frames.iter().map(shot).collect::<Vec<_>>(), aspect)
                    }
                    None => {
                        let shots: Vec<Shot> = frames.iter().filter_map(|f| self.shot_on_timeline(*f)).collect();
                        (shots, p.settings.width as f64 / p.settings.height as f64)
                    }
                };
                if shots.is_empty() {
                    return Err(format!("no video clip is visible between timeline frames {from} and {to}"));
                }
                let columns = int("columns", 6, 1, 12).min(shots.len() as i64);
                // Wider than this and clients shrink the picture, which makes the labels unreadable.
                let width = int("tile_width", 320, 120, 640).min(1920 / columns);
                let info = json!({
                    "frames": if media.is_some() { "source frames of the media" } else { "timeline frames" },
                    "from": from,
                    "to": to,
                    "tiles": shots.len(),
                    "frames_between_tiles": (to - from) / count,
                    "columns": columns,
                    "order": "left to right, then top to bottom; each tile is labelled with its time and frame",
                });
                Ok(Job::Sheet { shots, columns, tile: (even(width as f64), even(width as f64 / aspect)), info })
            }
            "audio_get_activity" if media.is_none() => {
                // No media: the sound of the edit, on one audio track, as it now plays.
                let audible = |t: &&Track| t.kind == TrackKind::Audio && p.sequence.clips.iter().any(|c| c.track == t.id && c.media.is_some());
                let track = match args["track"].as_str() {
                    Some(id) => p.track(id).filter(|t| t.kind == TrackKind::Audio).ok_or_else(|| format!("{id} is not an audio track"))?,
                    None => p
                        .sequence
                        .tracks
                        .iter()
                        .filter(audible)
                        .find(|t| !t.muted)
                        .or_else(|| p.sequence.tracks.iter().find(audible))
                        .ok_or("pass media to analyse a file; the timeline has no audio clips to analyse")?,
                };
                let end = p.duration();
                let from = args["from"].as_i64().unwrap_or(0).max(0);
                let to = args["to"].as_i64().unwrap_or(end).min(end);
                if to <= from {
                    return Err(format!("nothing to analyse between frame {from} and frame {to}"));
                }
                let mut clips: Vec<&Clip> = p.sequence.clips.iter().filter(|c| c.track == track.id && c.end() > from && c.start < to).collect();
                clips.sort_by_key(|c| c.start);
                let mut parts = vec![];
                for c in clips {
                    let Some(m) = c.media.as_ref().and_then(|id| p.media(id)) else { continue };
                    let span = (c.start.max(from), c.end().min(to));
                    let source = (c.source_at(span.0).round() as Frame, (c.source_at(span.1).round() as Frame).min(m.duration));
                    if source.1 > source.0 {
                        parts.push(AudioPart { path: m.path.clone(), source, at: span.0, speed: c.speed(), span });
                    }
                }
                if parts.is_empty() {
                    return Err(format!("track {} has no audio clips between frame {from} and frame {to}", track.id));
                }
                if parts.len() > MAX_AUDIO_PARTS {
                    return Err(format!("track {} has {} clips in that span; each is read on its own, so pass from and to for a span with at most {MAX_AUDIO_PARTS}", track.id, parts.len()));
                }
                Ok(Job::TimelineAudio {
                    track: track.id.clone(),
                    parts,
                    from,
                    to,
                    fps,
                    stream: int("audio_stream", 0, 0, 31),
                    threshold_db: args["threshold_db"].as_f64().unwrap_or(-35.0).clamp(-90.0, 0.0),
                    min_seconds: args["min_seconds"].as_f64().unwrap_or(0.5).clamp(0.1, 60.0),
                })
            }
            "audio_get_activity" => {
                let m = p.media(media.ok_or("media must be a media id")?).ok_or("no such media")?;
                if m.kind == MediaKind::Image {
                    return Err(format!("{} is an image and has no sound", m.name));
                }
                let from = args["from"].as_i64().unwrap_or(0).max(0);
                let to = args["to"].as_i64().unwrap_or(m.duration).min(m.duration);
                if to <= from {
                    return Err(format!("nothing to analyse between frame {from} and frame {to}"));
                }
                Ok(Job::Audio {
                    path: m.path.clone(),
                    from,
                    to,
                    fps,
                    stream: int("audio_stream", 0, 0, 31),
                    threshold_db: args["threshold_db"].as_f64().unwrap_or(-35.0).clamp(-90.0, 0.0),
                    min_seconds: args["min_seconds"].as_f64().unwrap_or(0.5).clamp(0.1, 60.0),
                })
            }
            _ => Err(format!("unknown tool {name}")),
        }
    }

    /// Notes an analysis call in the activity log, without the picture.
    pub fn log_analysis(&mut self, actor: &str, name: &str, args: Value, result: &Result<Analysis, String>) {
        let logged = match result {
            Ok(a) => Ok(a.info.clone()),
            Err(e) => Err(e.clone()),
        };
        self.record(actor, name.replacen('_', ".", 1), args, &logged);
    }
}

/// Where FFmpeg is: `SNN_FFMPEG`, next to the app, on the PATH, or where winget puts it.
pub fn ffmpeg_path() -> PathBuf {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    if let Some(path) = std::env::var_os("SNN_FFMPEG").map(PathBuf::from).filter(|p| p.is_file()) {
        return path;
    }
    if let Some(beside) = std::env::current_exe().ok().and_then(|p| Some(p.parent()?.join(exe))).filter(|p| p.is_file()) {
        return beside;
    }
    let on_path = std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|dir| dir.join(exe)).find(|p| p.is_file()));
    if let Some(found) = on_path {
        return found;
    }
    // A terminal opened before FFmpeg was installed does not have it on its PATH yet.
    let packages = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Microsoft/WinGet/Packages"));
    let dirs = |dir: PathBuf| std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path());
    packages
        .into_iter()
        .flat_map(dirs)
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("Gyan.FFmpeg")))
        .flat_map(dirs)
        .map(|build| build.join("bin").join(exe))
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(exe))
}

/// FFmpeg, set up to run quietly from inside a window.
pub(crate) fn ffmpeg_command() -> Command {
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-nostats", "-nostdin"]).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing up over the editor
        // Labels are drawn with a font named by file, and a Windows path has a colon the filter syntax trips on.
        cmd.current_dir(std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| "C:/Windows".into()).join("Fonts"));
    }
    cmd
}

pub(crate) fn run(args: Vec<String>) -> Result<Output, String> {
    ffmpeg_command().args(args).output().map_err(|e| format!("could not run FFmpeg ({e}); install it ({INSTALL_HINT}) or point SNN_FFMPEG at ffmpeg.exe"))
}

/// What to install when FFmpeg is missing or cannot export. 7.0 is the oldest build with the `-/filter_complex` option
/// the exports use; 7.0.2 is the one the NVENC driver tests were run with.
pub const INSTALL_HINT: &str = "winget install Gyan.FFmpeg --version 7.0.2";

/// The version of the FFmpeg this app runs, such as "7.0.2-full_build-www.gyan.dev". Empty when FFmpeg does not run.
pub fn ffmpeg_version() -> String {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            run(vec!["-version".into()])
                .ok()
                .and_then(|out| String::from_utf8_lossy(&out.stdout).split_whitespace().nth(2).map(str::to_string))
                .unwrap_or_default()
        })
        .clone()
}

/// Whether this FFmpeg accepts the `-/filter_complex` option that every export uses. Older builds, such as 6.1, stop with
/// "Option not found". The check runs a one-line filter graph, because the option is not listed in the help text of every build.
pub fn supports_export() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        let script = std::env::temp_dir().join(format!("snn-filter-probe-{}.txt", std::process::id()));
        if std::fs::write(&script, "[0:v]null[v]").is_err() {
            return false;
        }
        let args: Vec<String> = ["-f", "lavfi", "-i", "color=s=16x16:d=0.1", "-/filter_complex", &script.display().to_string(), "-map", "[v]", "-f", "null", "-"]
            .map(String::from)
            .into();
        let works = run(args).is_ok_and(|out| out.status.success());
        std::fs::remove_file(&script).ok();
        works
    })
}

/// The last lines FFmpeg printed, which is where it says what went wrong.
pub(crate) fn failure(out: &Output) -> String {
    let text = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    format!("FFmpeg could not read the file: {}", lines[lines.len().saturating_sub(3)..].join(" | "))
}

fn picture(args: Vec<String>, info: Value) -> Result<Analysis, String> {
    let out = run(args)?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(failure(&out));
    }
    Ok(Analysis { info, jpeg: Some(out.stdout) })
}

const TO_JPEG: [&str; 9] = ["-frames:v", "1", "-q:v", "4", "-f", "image2pipe", "-c:v", "mjpeg", "-"];

/// The first frame of a video file as a JPEG `width` pixels wide.
pub fn first_frame_jpeg(path: &str, width: i64) -> Result<Vec<u8>, String> {
    let mut args = vec!["-i".into(), path.into(), "-vf".into(), format!("scale={width}:-2")];
    args.extend(TO_JPEG.map(String::from));
    picture(args, Value::Null).map(|a| a.jpeg.unwrap_or_default())
}

/// The number after `key` in a line of FFmpeg's log, as in `silence_start: 12.5`.
fn after(line: &str, key: &str) -> Option<f64> {
    line.split(key).nth(1)?.split_whitespace().next()?.parse().ok()
}

/// Listens to `length` seconds of one audio stream of a file from `start`, finding silences of at least `min_seconds`.
fn listen(path: &str, start: f64, length: f64, stream: i64, threshold_db: f64, min_seconds: f64) -> Result<Heard, String> {
    let filter = format!("silencedetect=noise={threshold_db}dB:d={min_seconds},ebur128");
    let out = run(vec![
        "-ss".into(), format!("{start:.4}"), "-t".into(), format!("{length:.4}"), "-i".into(), path.into(),
        "-vn".into(), "-map".into(), format!("0:a:{stream}"), "-af".into(), filter, "-f".into(), "null".into(), "-".into(),
    ])?;
    let log = String::from_utf8_lossy(&out.stderr);
    // The file's own streams are listed before the output's.
    let streams = log.lines().take_while(|l| !l.starts_with("Output #")).filter(|l| l.trim_start().starts_with("Stream #0:") && l.contains("Audio:")).count();
    if !out.status.success() {
        return Err(match streams {
            0 => "the file has no sound".into(),
            n if stream >= n as i64 => format!("the file has {n} audio stream(s); audio_stream counts from 0"),
            _ => failure(&out),
        });
    }
    let mut silences = vec![];
    let mut open = None;
    // Loudest moment of each second, so a shout cannot hide inside a quiet stretch.
    let mut loud: Vec<(f64, f64)> = vec![];
    for line in log.lines() {
        if let Some(t) = after(line, "silence_start:") {
            open = Some(t.max(0.0));
        } else if let Some(t) = after(line, "silence_end:") {
            let Some(s) = open.take() else { continue };
            silences.push((s, t.min(length)));
        } else if let (true, Some(t), Some(m)) = (line.contains("TARGET:"), after(line, " t:"), after(line, " M:").filter(|m| m.is_finite())) {
            match loud.last_mut() {
                Some((at, level)) if at.floor() == t.floor() => {
                    if m > *level {
                        (*at, *level) = (t, m);
                    }
                }
                _ => loud.push((t, m)),
            }
        }
    }
    if let Some(s) = open {
        silences.push((s, length));
    }
    Ok(Heard { silences, loud, streams })
}

/// The report both kinds of sound analysis give: the silences, how much of the stretch they are, and the 20 loudest
/// moments at least ten seconds apart, so one long shout is listed once. All in frames.
fn sound_report(silences: &[(Frame, Frame)], mut loud: Vec<(Frame, f64)>, fps: f64) -> Value {
    let tenth = |frames: Frame| (frames as f64 / fps * 10.0).round() / 10.0;
    let silent: Frame = silences.iter().map(|(a, b)| b - a).sum();
    loud.sort_by(|a, b| b.1.total_cmp(&a.1));
    let typical = loud.get(loud.len() / 2).map(|(_, level)| *level);
    let apart = (10.0 * fps) as Frame;
    let mut peaks: Vec<(Frame, f64)> = vec![];
    for (at, level) in loud {
        if peaks.len() < 20 && peaks.iter().all(|(p, _)| (p - at).abs() >= apart) {
            peaks.push((at, level));
        }
    }
    peaks.sort_by_key(|(at, _)| *at);
    json!({
        "silent_seconds": tenth(silent),
        "silences": silences.iter().map(|(a, b)| json!({ "from": a, "to": b, "seconds": tenth(b - a) })).collect::<Vec<_>>(),
        "typical_lufs": typical,
        "loudest": peaks.iter().map(|(at, level)| json!({ "at": at, "time": timecode(*at as f64 / fps), "lufs": level })).collect::<Vec<_>>(),
    })
}

/// Adds the fields of `more` to `info`.
fn with(mut info: Value, more: Value) -> Value {
    if let (Some(info), Value::Object(more)) = (info.as_object_mut(), more) {
        info.extend(more);
    }
    info
}

impl Job {
    pub fn run(&self) -> Result<Analysis, String> {
        match self {
            Job::Frame { shot, width, info } => {
                let font = if cfg!(windows) { "fontfile=arialbd.ttf:" } else { "" };
                // The same label a contact-sheet tile carries, so a picture passed along still says where it is from.
                let label = format!(
                    "drawtext={font}text='{}':fontsize={}:fontcolor=white:box=1:boxcolor=black@0.7:boxborderw=5:x=6:y=6",
                    shot.label,
                    (width / 40).max(14),
                );
                let mut args = vec!["-ss".into(), format!("{:.4}", shot.seconds), "-i".into(), shot.path.clone()];
                args.extend(["-vf".into(), format!("scale={width}:-2,{label}")]);
                args.extend(TO_JPEG.map(String::from));
                picture(args, with(info.clone(), json!({ "seconds_into_file": (shot.seconds * 1000.0).round() / 1000.0 })))
            }
            Job::Sheet { shots, columns, tile: (w, h), info } => {
                let mut args = vec![];
                let mut graph = String::new();
                let mut layout = vec![];
                let font = if cfg!(windows) { "fontfile=arialbd.ttf:" } else { "" };
                for (i, s) in shots.iter().enumerate() {
                    args.extend(["-ss".into(), format!("{:.4}", s.seconds), "-i".into(), s.path.clone()]);
                    graph += &format!(
                        "[{i}:v]trim=end_frame=1,scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,\
                         drawtext={font}text='{}':fontsize={}:fontcolor=white:box=1:boxcolor=black@0.7:boxborderw=4:x=4:y=4[t{i}];",
                        s.label,
                        (w / 20).max(12),
                    );
                    layout.push(format!("{}_{}", (i as i64 % columns) * w, (i as i64 / columns) * h));
                }
                let inputs: String = (0..shots.len()).map(|i| format!("[t{i}]")).collect();
                graph += &match shots.len() {
                    1 => "[t0]null[out]".to_string(),
                    n => format!("{inputs}xstack=inputs={n}:layout={}:fill=black[out]", layout.join("|")),
                };
                args.extend(["-filter_complex".into(), graph, "-map".into(), "[out]".into()]);
                args.extend(TO_JPEG.map(String::from));
                picture(args, info.clone())
            }
            Job::Audio { path, from, to, fps, stream, threshold_db, min_seconds } => {
                let length = (to - from) as f64 / fps;
                let heard = listen(path, *from as f64 / fps, length, *stream, *threshold_db, *min_seconds)?;
                // Times in the log count from where reading started.
                let frame = |seconds: f64| (from + (seconds * fps).round() as Frame).min(*to);
                let silences: Vec<(Frame, Frame)> = heard.silences.iter().map(|(a, b)| (frame(*a), frame(*b))).filter(|(a, b)| b > a).collect();
                let loud = heard.loud.iter().map(|(t, level)| (frame(*t), *level)).collect();
                let info = json!({
                    "frames": "source frames of the media",
                    "from": from,
                    "to": to,
                    "audio_streams_in_file": heard.streams,
                    "audio_stream": stream,
                    "silence_threshold_db": threshold_db,
                    "analysed_seconds": (length * 10.0).round() / 10.0,
                });
                Ok(Analysis { info: with(info, sound_report(&silences, loud, *fps)), jpeg: None })
            }
            Job::TimelineAudio { track, parts, from, to, fps, stream, threshold_db, min_seconds } => {
                let mut quiet: Vec<(Frame, Frame)> = vec![];
                let mut loud = vec![];
                // Where the track plays nothing, nothing is heard.
                let mut covered = *from;
                for part in parts {
                    if part.span.0 > covered {
                        quiet.push((covered, part.span.0));
                    }
                    covered = covered.max(part.span.1);
                    let length = (part.source.1 - part.source.0) as f64 / fps;
                    // A silence can run across a cut, so short ones are kept here and judged once they are joined.
                    let heard = listen(&part.path, part.source.0 as f64 / fps, length, *stream, *threshold_db, min_seconds.min(0.1))?;
                    let frame = |seconds: f64| (part.at + (seconds * fps / part.speed).round() as Frame).clamp(part.span.0, part.span.1);
                    quiet.extend(heard.silences.iter().map(|(a, b)| (frame(*a), frame(*b))).filter(|(a, b)| b > a));
                    loud.extend(heard.loud.iter().map(|(t, level)| (frame(*t), *level)));
                }
                if covered < *to {
                    quiet.push((covered, *to));
                }
                quiet.sort();
                let mut silences: Vec<(Frame, Frame)> = vec![];
                for (a, b) in quiet {
                    match silences.last_mut() {
                        // Silences that meet at a cut, give or take a frame of rounding, are one silence.
                        Some(last) if a <= last.1 + 1 => last.1 = last.1.max(b),
                        _ => silences.push((a, b)),
                    }
                }
                silences.retain(|(a, b)| (b - a) as f64 / fps >= *min_seconds);
                let info = json!({
                    "frames": "timeline frames",
                    "track": track,
                    "clips_read": parts.len(),
                    "from": from,
                    "to": to,
                    "audio_stream": stream,
                    "silence_threshold_db": threshold_db,
                    "analysed_seconds": ((to - from) as f64 / fps * 10.0).round() / 10.0,
                });
                Ok(Analysis { info: with(info, sound_report(&silences, loud, *fps)), jpeg: None })
            }
        }
    }
}

/// How many audio streams a file has, so an export does not ask for sound that is not there.
pub fn probe_streams(path: &str) -> MediaInfo {
    // Naming no output makes FFmpeg list the file's streams and stop.
    let log = run(vec!["-i".into(), path.into()]).map(|out| String::from_utf8_lossy(&out.stderr).into_owned()).unwrap_or_default();
    MediaInfo { audio_streams: log.lines().filter(|l| l.trim_start().starts_with("Stream #0:") && l.contains("Audio:")).count() }
}

/// What the probe found for each encoder in [`ENCODERS`]: the ones that work, and the others with the reason.
struct Probe {
    found: Vec<String>,
    missing: Vec<(String, String)>,
}

fn probe() -> &'static Probe {
    static PROBE: OnceLock<Probe> = OnceLock::new();
    PROBE.get_or_init(|| {
        let (mut found, mut missing) = (vec![], vec![]);
        // Each is tried once on a blank picture, because FFmpeg lists graphics-card encoders whether or not the card is there.
        for encoder in ENCODERS {
            let args = ["-f", "lavfi", "-i", "color=s=256x256:r=30:d=0.2", "-c:v", encoder, "-f", "null", "-"];
            match run(args.map(String::from).into()) {
                Ok(out) if out.status.success() => found.push(encoder.to_string()),
                Ok(out) => missing.push((encoder.to_string(), reason(&out.stderr))),
                Err(e) => missing.push((encoder.to_string(), e)),
            }
        }
        Probe { found, missing }
    })
}

/// The line FFmpeg printed that says why an encoder would not start, when there is one.
fn reason(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    // Log lines look like "[h264_nvenc @ 0000] message"; the message is what follows the last bracket.
    let lines: Vec<&str> = text.lines().map(|l| l.rsplit("] ").next().unwrap_or(l).trim()).filter(|l| !l.is_empty()).collect();
    lines
        .iter()
        .find(|l| l.contains("minimum required") || l.contains("does not support") || l.contains("Unknown encoder"))
        .map(|l| l.to_string())
        .unwrap_or_else(|| "this encoder would not start on this computer".into())
}

/// The encoders from [`ENCODERS`] that work on this machine, best first.
pub fn available_encoders() -> &'static [String] {
    &probe().found
}

/// The encoders from [`ENCODERS`] that do not work here, each with the reason it does not.
pub fn unavailable_encoders() -> &'static [(String, String)] {
    &probe().missing
}

/// Reads standard base64, as the window sends the pictures it draws.
pub fn unbase64(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut count) = (0u32, 0);
    for b in text.bytes().filter(|b| *b != b'=' && !b.is_ascii_whitespace()) {
        let value = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("the picture data is not base64".into()),
        };
        bits = bits << 6 | value as u32;
        count += 1;
        if count == 4 {
            out.extend([(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
            (bits, count) = (0, 0);
        }
    }
    match count {
        2 => out.push((bits >> 4) as u8),
        3 => out.extend([(bits >> 10) as u8, (bits >> 2) as u8]),
        _ => {}
    }
    Ok(out)
}

/// Standard base64, for sending a picture inside a JSON response.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char } else { '=' });
        }
    }
    out
}

/// Length, picture size and sound of any file FFmpeg reads. The MP4 reader is tried first because it does not
/// start a program; this covers MKV, WebM, AVI, sound files and pictures.
pub fn probe_file(path: &str) -> Result<crate::probe::Probe, String> {
    let out = run(vec!["-i".into(), path.into()])?;
    parse_probe(&String::from_utf8_lossy(&out.stderr), path).ok_or_else(|| failure(&out))
}

/// Reads FFmpeg's description of a file: its `Duration:` line and its `Stream` lines.
pub(crate) fn parse_probe(log: &str, path: &str) -> Option<crate::probe::Probe> {
    if !log.contains("Input #0") {
        return None;
    }
    let seconds = log
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("Duration:"))
        .and_then(|rest| {
            let mut parts = rest.split(',').next()?.trim().split(':').map(|n| n.parse::<f64>().ok());
            Some(parts.next()?? * 3600.0 + parts.next()?? * 60.0 + parts.next()??)
        })
        .unwrap_or(0.0);
    let streams: Vec<&str> = log.lines().filter(|l| l.trim_start().starts_with("Stream #0:")).collect();
    // Cover art in a music file is listed as a video stream.
    let video = streams.iter().find(|l| l.contains("Video:") && !l.contains("attached pic"));
    let has_audio = streams.iter().any(|l| l.contains("Audio:"));
    let size = video.and_then(|l| {
        l.split(',').filter_map(|field| field.split_whitespace().next()).find_map(|token| {
            let (w, h) = token.split_once('x')?;
            Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
        })
    });
    let extension = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let picture = matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff");
    let kind = match (video.is_some(), has_audio) {
        (true, _) if picture || seconds < 0.05 => MediaKind::Image,
        (true, _) => MediaKind::Video,
        (false, true) => MediaKind::Audio,
        (false, false) => return None,
    };
    let (width, height) = size.unwrap_or((0, 0));
    Some(crate::probe::Probe { kind, seconds: if kind == MediaKind::Image { 0.0 } else { seconds }, width, height, has_audio: has_audio && kind != MediaKind::Image })
}
