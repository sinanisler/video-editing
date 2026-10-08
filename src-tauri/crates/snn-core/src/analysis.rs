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
    Frame { shot: Shot, width: i64 },
    Sheet { shots: Vec<Shot>, columns: i64, tile: (i64, i64), info: Value },
    Audio { path: String, from: Frame, to: Frame, fps: f64, stream: i64, threshold_db: f64, min_seconds: f64 },
}

/// What an analysis tool returns: facts as JSON and, for the frame tools, a JPEG.
pub struct Analysis {
    pub info: Value,
    pub jpeg: Option<Vec<u8>>,
}

fn timecode(seconds: f64) -> String {
    let s = seconds.max(0.0) as i64;
    format!("{}m{:02}s", s / 60, s % 60)
}

/// An even number of pixels, as most encoders and scalers want.
fn even(n: f64) -> i64 {
    ((n / 2.0).round() as i64 * 2).max(2)
}

impl Editor {
    /// The visible video clip an agent means by a timeline frame: the topmost one that plays a file.
    fn shot_on_timeline(&self, at: Frame) -> Option<Shot> {
        let p = self.project();
        let fps = p.settings.fps_num as f64 / p.settings.fps_den as f64;
        let tracks = p.sequence.tracks.iter().filter(|t| t.kind == TrackKind::Video && !t.hidden);
        for t in tracks {
            let clip = p.sequence.clips.iter().find(|c| c.track == t.id && c.kind == ClipKind::Video && c.start <= at && at < c.end());
            let Some((c, m)) = clip.and_then(|c| Some((c, p.media(c.media.as_ref()?)?))) else { continue };
            if m.kind != MediaKind::Video {
                continue;
            }
            let source = c.source_in as f64 + (at - c.start) as f64 * c.speed();
            return Some(Shot { path: m.path.clone(), seconds: source / fps, label: format!("T {} f{at}", timecode(at as f64 / fps)) });
        }
        None
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
                let shot = match media {
                    Some(id) => {
                        let m = self.video(id)?;
                        if at >= m.duration {
                            return Err(format!("{} is only {} frames long", m.name, m.duration));
                        }
                        Shot { path: m.path.clone(), seconds: at as f64 / fps, label: String::new() }
                    }
                    None => self.shot_on_timeline(at).ok_or_else(|| format!("no video clip is visible at timeline frame {at}"))?,
                };
                Ok(Job::Frame { shot, width: int("width", 960, 160, 1920) })
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

fn run(args: Vec<String>) -> Result<Output, String> {
    ffmpeg_command().args(args).output().map_err(|e| format!("could not run FFmpeg ({e}); install it (winget install Gyan.FFmpeg) or point SNN_FFMPEG at ffmpeg.exe"))
}

/// The last lines FFmpeg printed, which is where it says what went wrong.
fn failure(out: &Output) -> String {
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

/// The number after `key` in a line of FFmpeg's log, as in `silence_start: 12.5`.
fn after(line: &str, key: &str) -> Option<f64> {
    line.split(key).nth(1)?.split_whitespace().next()?.parse().ok()
}

impl Job {
    pub fn run(&self) -> Result<Analysis, String> {
        match self {
            Job::Frame { shot, width } => {
                let mut args = vec!["-ss".into(), format!("{:.4}", shot.seconds), "-i".into(), shot.path.clone()];
                args.extend(["-vf".into(), format!("scale={width}:-2")]);
                args.extend(TO_JPEG.map(String::from));
                picture(args, json!({ "seconds_into_file": (shot.seconds * 1000.0).round() / 1000.0, "width": width }))
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
                        (w / 18).max(12),
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
                let start = *from as f64 / fps;
                let length = (to - from) as f64 / fps;
                let filter = format!("silencedetect=noise={threshold_db}dB:d={min_seconds},ebur128");
                let out = run(vec![
                    "-ss".into(), format!("{start:.4}"), "-t".into(), format!("{length:.4}"), "-i".into(), path.clone(),
                    "-vn".into(), "-map".into(), format!("0:a:{stream}"), "-af".into(), filter, "-f".into(), "null".into(), "-".into(),
                ])?;
                let log = String::from_utf8_lossy(&out.stderr);
                // The file's own streams are listed before the output's.
                let streams = log.lines().take_while(|l| !l.starts_with("Output #")).filter(|l| l.trim_start().starts_with("Stream #0:") && l.contains("Audio:")).count();
                if !out.status.success() {
                    return Err(match streams {
                        0 => "the file has no sound".into(),
                        n if *stream >= n as i64 => format!("the file has {n} audio stream(s); audio_stream counts from 0"),
                        _ => failure(&out),
                    });
                }
                // Times in the log count from where reading started.
                let frame = |seconds: f64| from + (seconds * fps).round() as Frame;
                let mut silences = vec![];
                let mut open = None;
                // Loudest moment of each second, so a shout cannot hide inside a quiet stretch.
                let mut loud: Vec<(f64, f64)> = vec![];
                for line in log.lines() {
                    if let Some(t) = after(line, "silence_start:") {
                        open = Some(t.max(0.0));
                    } else if let Some(t) = after(line, "silence_end:") {
                        let Some(s) = open.take() else { continue };
                        silences.push(json!({ "from": frame(s), "to": frame(t.min(length)), "seconds": ((t - s) * 10.0).round() / 10.0 }));
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
                    silences.push(json!({ "from": frame(s), "to": to, "seconds": ((length - s) * 10.0).round() / 10.0 }));
                }
                let silent: f64 = silences.iter().filter_map(|s| s["seconds"].as_f64()).sum();
                // The loudest moments, at least ten seconds apart so one long shout is listed once.
                loud.sort_by(|a, b| b.1.total_cmp(&a.1));
                let typical = loud.get(loud.len() / 2).map(|(_, level)| *level);
                let mut peaks: Vec<(f64, f64)> = vec![];
                for (t, level) in loud {
                    if peaks.len() < 20 && peaks.iter().all(|(p, _)| (p - t).abs() >= 10.0) {
                        peaks.push((t, level));
                    }
                }
                peaks.sort_by(|a, b| a.0.total_cmp(&b.0));
                Ok(Analysis {
                    info: json!({
                        "frames": "source frames of the media",
                        "from": from,
                        "to": to,
                        "audio_streams_in_file": streams,
                        "audio_stream": stream,
                        "silence_threshold_db": threshold_db,
                        "silent_seconds": (silent.max(0.0) * 10.0).round() / 10.0,
                        "analysed_seconds": (length * 10.0).round() / 10.0,
                        "silences": silences,
                        "typical_lufs": typical,
                        "loudest": peaks.iter().map(|(t, level)| json!({ "at": frame(*t), "lufs": level })).collect::<Vec<_>>(),
                    }),
                    jpeg: None,
                })
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
