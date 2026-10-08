//! Runs exports for the window and for agents, one at a time.
//!
//! An export has two parts. The window first draws the text clips as images, because they are
//! styled with its CSS; then FFmpeg renders the timeline on a thread of its own. So every export,
//! including one an agent asks for, goes through the window: `begin`, `image` for each picture,
//! then `run`. Progress goes out as `export:status` events and is there to be polled.

use crate::SharedEditor;
use serde::Serialize;
use serde_json::{json, Value};
use snn_core::{
    available_encoders, check_export, codec_name, codec_of, export_presets, ffmpeg_version, plan_export, probe_streams, render, supports_export,
    unavailable_encoders, unbase64, Codec, ExportSettings, MediaInfo, TextTrack, CANCELLED, INSTALL_HINT,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const STATUS_EVENT: &str = "export:status";
/// Asks the window to start an export an agent requested.
const REQUEST_EVENT: &str = "export:request";

#[derive(Clone, Default, Serialize)]
pub struct ExportStatus {
    /// `idle`, `preparing` (the window is drawing text), `rendering`, `done`, `failed` or `cancelled`.
    pub state: &'static str,
    pub job: u64,
    pub path: String,
    pub encoder: String,
    pub frames_done: i64,
    pub frames_total: i64,
    pub seconds: f64,
    /// What went wrong, or what is happening.
    pub message: String,
    /// Things the file shows differently from the preview.
    pub warnings: Vec<String>,
}

struct Pending {
    job: u64,
    settings: ExportSettings,
    encoder: String,
    dir: PathBuf,
    /// FFmpeg has been started for it. A job renders once, however often it is asked to.
    rendering: bool,
}

pub struct Exports {
    status: Mutex<ExportStatus>,
    pending: Mutex<Option<Pending>>,
    cancel: Arc<AtomicBool>,
    next: AtomicU64,
}

impl Default for Exports {
    fn default() -> Self {
        let status = ExportStatus { state: "idle", ..Default::default() };
        Exports { status: Mutex::new(status), pending: Mutex::new(None), cancel: Default::default(), next: AtomicU64::new(1) }
    }
}

pub type SharedExports = Arc<Exports>;

impl Exports {
    pub fn status(&self) -> ExportStatus {
        self.status.lock().unwrap().clone()
    }

    fn busy(&self) -> bool {
        matches!(self.status().state, "preparing" | "rendering")
    }

    fn set(&self, app: &AppHandle, change: impl FnOnce(&mut ExportStatus)) {
        let mut status = self.status.lock().unwrap();
        change(&mut status);
        let _ = app.emit(STATUS_EVENT, status.clone());
    }

    fn finish(&self, app: &AppHandle, job: u64, state: &'static str, message: String) {
        if let Some(p) = self.pending.lock().unwrap().take_if(|p| p.job == job) {
            let _ = std::fs::remove_dir_all(p.dir);
        }
        self.set(app, |s| {
            if s.job == job {
                s.state = state;
                s.message = message;
            }
        });
    }
}

/// Presets, the encoders this machine has, and a place to save to.
pub fn options(app: &AppHandle, editor: &SharedEditor) -> Value {
    let editor = editor.lock().unwrap();
    let p = editor.project();
    let state = editor.state();
    let beside = state.path.as_deref().and_then(|path| PathBuf::from(path).parent().map(PathBuf::from));
    let folder = beside.or_else(|| app.path().video_dir().ok()).unwrap_or_default();
    let name: String = p.name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '-' } else { c }).collect();
    json!({
        "presets": export_presets(p),
        "encoders": available_encoders(),
        // Which FFmpeg this is, and whether it can export at all. An old build can list encoders and still fail every export.
        "ffmpeg": { "version": ffmpeg_version(), "supports_export": supports_export(), "install": INSTALL_HINT },
        // Encoders that did not start here, with FFmpeg's reason, so the window can say what to fix.
        "unavailable": unavailable_encoders().iter().map(|(encoder, reason)| json!({ "encoder": encoder, "reason": reason })).collect::<Vec<_>>(),
        "path": folder.join(format!("{name}.mp4")).display().to_string(),
        "has_range": p.sequence.in_point.is_some() || p.sequence.out_point.is_some(),
    })
}

/// Reserves the next job for these settings, after checking them against the project.
fn reserve(app: &AppHandle, exports: &Exports, editor: &SharedEditor, settings: &ExportSettings, message: &str) -> Result<u64, String> {
    check_export(editor.lock().unwrap().project(), settings)?;
    if exports.busy() {
        return Err("an export is already running; cancel it or wait for it to finish".into());
    }
    let job = exports.next.fetch_add(1, Ordering::Relaxed);
    exports.cancel.store(false, Ordering::Relaxed);
    exports.set(app, |s| *s = ExportStatus { state: "preparing", job, path: settings.path.clone(), message: message.into(), ..Default::default() });
    Ok(job)
}

/// An agent's export: checked here, then handed to the window, which runs it like its own.
pub fn request(app: &AppHandle, exports: &Exports, editor: &SharedEditor, settings: ExportSettings) -> Result<u64, String> {
    let job = reserve(app, exports, editor, &settings, "Waiting for the window to draw the text")?;
    app.emit(REQUEST_EVENT, json!({ "job": job, "settings": settings })).map_err(|e| e.to_string())?;
    Ok(job)
}

/// First step: checks the settings, picks the encoder and makes the job's working folder.
/// `job` is set when the window is answering a request, which already reserved one.
pub fn begin(app: &AppHandle, exports: &Exports, editor: &SharedEditor, settings: ExportSettings, job: Option<u64>) -> Result<Value, String> {
    // A request is answered once: a second window, or a listener registered twice, must not start a second render of the same job.
    if job.is_some_and(|j| exports.pending.lock().unwrap().as_ref().is_some_and(|p| p.job == j)) {
        return Err("that export has already begun".into());
    }
    let job = match job.filter(|j| exports.status().job == *j && exports.status().state == "preparing") {
        Some(job) => job,
        None => reserve(app, exports, editor, &settings, "Drawing text")?,
    };
    let started = (|| {
        let (from, to) = check_export(editor.lock().unwrap().project(), &settings)?;
        if ffmpeg_version().is_empty() {
            return Err(format!("FFmpeg was not found; install it with: {INSTALL_HINT}"));
        }
        if !supports_export() {
            return Err(format!("this FFmpeg ({}) is too old for exports; FFmpeg 7.0 or newer is needed. Install it with: {INSTALL_HINT}", ffmpeg_version()));
        }
        let found = available_encoders();
        if found.is_empty() {
            return Err(format!("none of FFmpeg's encoders works here; install FFmpeg with: {INSTALL_HINT}"));
        }
        let codec = settings.codec;
        let encoder = match settings.encoder.as_str() {
            // Two passes run on the processor, so an automatic choice takes the software encoder for the codec.
            "" if settings.two_pass => {
                let software = if codec == Codec::Hevc { "libx265" } else { "libx264" };
                if !found.iter().any(|e| e == software) {
                    return Err(format!("{software} does not work here, so two-pass cannot run"));
                }
                software.to_string()
            }
            "" => found
                .iter()
                .find(|e| codec_of(e) == Some(codec))
                .cloned()
                .ok_or_else(|| format!("this computer has no {} encoder; it has: {}", codec_name(codec), found.join(", ")))?,
            wanted if found.iter().any(|e| e == wanted) => wanted.to_string(),
            wanted => return Err(format!("this machine cannot use {wanted}; it has: {}", found.join(", "))),
        };
        let dir = std::env::temp_dir().join(format!("snn-export-{}-{job}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        *exports.pending.lock().unwrap() = Some(Pending { job, settings: settings.clone(), encoder: encoder.clone(), dir, rendering: false });
        exports.set(app, |s| s.encoder = encoder);
        Ok(json!({ "job": job, "from": from, "to": to, "width": settings.width, "height": settings.height }))
    })();
    if let Err(e) = &started {
        exports.finish(app, job, "failed", e.clone());
    }
    started
}

/// Saves one picture the window drew. Names are the window's own, so only plain ones are taken.
pub fn image(exports: &Exports, job: u64, name: &str, data: &str) -> Result<(), String> {
    let pending = exports.pending.lock().unwrap();
    let p = pending.as_ref().filter(|p| p.job == job).ok_or("that export is no longer running")?;
    if !name.ends_with(".png") || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        return Err(format!("{name} is not a picture name"));
    }
    std::fs::write(p.dir.join(name), unbase64(data)?).map_err(|e| e.to_string())
}

/// Last step: renders on a thread and returns at once.
pub fn run(app: AppHandle, exports: SharedExports, editor: &SharedEditor, job: u64, text: Vec<TextTrack>) -> Result<(), String> {
    let (settings, encoder, dir) = {
        let mut pending = exports.pending.lock().unwrap();
        let p = pending.as_mut().filter(|p| p.job == job).ok_or("that export is no longer running")?;
        if std::mem::replace(&mut p.rendering, true) {
            return Err("that export is already rendering".into());
        }
        (p.settings.clone(), p.encoder.clone(), p.dir.clone())
    };
    // The export works from the project as it is now; later edits do not change the file.
    let project = editor.lock().unwrap().project().clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let streams: Mutex<HashMap<String, MediaInfo>> = Default::default();
        let info = |m: &snn_core::MediaItem| *streams.lock().unwrap().entry(m.id.clone()).or_insert_with(|| probe_streams(&m.path));
        let result = plan_export(&project, &settings, &encoder, &text, &info, &dir).and_then(|plan| {
            exports.set(&app, |s| {
                s.state = "rendering";
                s.message.clear();
                s.frames_total = plan.total_frames;
                s.warnings = plan.warnings.clone();
            });
            let mut told = Instant::now();
            render(&plan, &exports.cancel, &mut |frames| {
                // Often enough for a smooth bar, not so often that the window does nothing else.
                if told.elapsed() > Duration::from_millis(200) || frames == plan.total_frames {
                    told = Instant::now();
                    exports.set(&app, |s| {
                        s.frames_done = frames;
                        s.seconds = started.elapsed().as_secs_f64();
                    });
                }
            })
        });
        let (state, message) = match result {
            Ok(()) => ("done", String::new()),
            Err(e) if e == CANCELLED => ("cancelled", String::new()),
            Err(e) => ("failed", e),
        };
        if state != "done" {
            let _ = std::fs::remove_file(&settings.path);
        }
        exports.set(&app, |s| s.seconds = started.elapsed().as_secs_f64());
        exports.finish(&app, job, state, message);
    });
    Ok(())
}

/// The window could not draw the text.
pub fn fail(app: &AppHandle, exports: &Exports, job: u64, message: String) {
    if exports.status().state == "preparing" {
        exports.finish(app, job, "failed", message);
    }
}

pub fn cancel(app: &AppHandle, exports: &Exports) -> ExportStatus {
    let status = exports.status();
    match status.state {
        // FFmpeg is stopped by the render loop, which then reports the end itself.
        "rendering" => exports.cancel.store(true, Ordering::Relaxed),
        "preparing" => exports.finish(app, status.job, "cancelled", String::new()),
        _ => {}
    }
    exports.status()
}

/// The export tools agents call. `export_start` returns as soon as the job is accepted.
pub fn tool(app: &AppHandle, exports: &Exports, editor: &SharedEditor, name: &str, args: &Value) -> Result<Value, String> {
    let report = |s: ExportStatus| {
        let percent = if s.frames_total > 0 { (s.frames_done as f64 / s.frames_total as f64 * 1000.0).round() / 10.0 } else { 0.0 };
        let mut v = serde_json::to_value(&s).unwrap_or_default();
        v["percent"] = json!(percent);
        v
    };
    match name {
        "export_list_presets" => Ok(options(app, editor)),
        "export_get_status" => Ok(report(exports.status())),
        "export_cancel" => Ok(report(cancel(app, exports))),
        "export_start" => {
            let all = options(app, editor);
            let wanted = args["preset"].as_str().unwrap_or("project");
            let preset = all["presets"].as_array().into_iter().flatten().find(|p| p["id"] == wanted).ok_or_else(|| format!("no preset {wanted}; see export_list_presets"))?;
            // A preset fills in what the call leaves out.
            let mut settings = json!({
                "path": all["path"], "width": preset["width"], "height": preset["height"], "fps": preset["fps"], "quality": preset["quality"],
                "codec": preset["codec"], "rate": preset["rate"], "bitrate_kbps": preset["bitrate_kbps"], "max_kbps": preset["max_kbps"],
            });
            for key in [
                "path", "width", "height", "fps", "quality", "encoder", "range", "audio", "codec", "rate", "bitrate_kbps", "max_kbps", "two_pass",
                "keyframe_frames", "b_frames", "audio_kbps",
            ] {
                if !args[key].is_null() {
                    settings[key] = args[key].clone();
                }
            }
            let settings: ExportSettings = serde_json::from_value(settings).map_err(|e| format!("invalid export settings: {e}"))?;
            let job = request(app, exports, editor, settings.clone())?;
            Ok(json!({ "job": job, "state": "preparing", "settings": settings, "next": "Call export_get_status until state is done, failed or cancelled." }))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}
