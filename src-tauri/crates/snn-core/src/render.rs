//! Runs the FFmpeg commands of an export [`Plan`], reporting progress and stopping when asked.

use crate::analysis::ffmpeg_command;
use std::path::Path;
use crate::export::Plan;
use std::io::{BufRead, BufReader, Read};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

pub const CANCELLED: &str = "the export was cancelled";

/// One FFmpeg run. `frame` is called with the number of frames written so far.
fn run(args: &[String], cancel: &AtomicBool, frame: &mut dyn FnMut(i64)) -> Result<(), String> {
    let mut child = ffmpeg_command()
        .args(["-progress", "pipe:1"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run FFmpeg ({e}); install it (winget install Gyan.FFmpeg) or point SNN_FFMPEG at ffmpeg.exe"))?;
    // The log is read on its own thread, or a full pipe would stall FFmpeg.
    let mut log = child.stderr.take().expect("piped");
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = log.read_to_string(&mut text);
        text
    });
    for line in BufReader::new(child.stdout.take().expect("piped")).lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break;
        }
        if let Some(n) = line.strip_prefix("frame=").and_then(|n| n.trim().parse().ok()) {
            frame(n);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let log = errors.join().unwrap_or_default();
    if cancel.load(Ordering::Relaxed) {
        return Err(CANCELLED.into());
    }
    if !status.success() {
        let lines: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
        // The cause is the first complaint; what follows is every other part of FFmpeg giving up.
        let cause = lines.iter().find(|l| ["Error", "Invalid", "Unable", "No such", "Unknown", "not found", "Cannot"].iter().any(|k| l.contains(k)));
        return Err(format!("FFmpeg failed: {}", cause.or(lines.last()).unwrap_or(&"no message")));
    }
    Ok(())
}

/// Renders every chunk, then joins them. `progress` gets frames finished out of `plan.total_frames`.
/// Working files are the caller's to remove.
pub fn render(plan: &Plan, cancel: &AtomicBool, progress: &mut dyn FnMut(i64)) -> Result<(), String> {
    for (path, text) in &plan.files {
        std::fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    }
    let mut done = 0;
    for chunk in &plan.chunks {
        // FFmpeg's count can dip when a filter restarts; the bar must not go backwards.
        let mut most = 0;
        run(&chunk.args, cancel, &mut |n| {
            most = most.max(n.min(chunk.frames));
            progress(done + most)
        })?;
        done += chunk.frames;
        progress(done);
    }
    run(&plan.finish, cancel, &mut |_| {})?;
    match plan.finish.last() {
        Some(file) => playable(Path::new(file)),
        None => Ok(()),
    }
}

/// Decodes the start and the end of a finished file. A file that was joined badly looks complete
/// and has the right length, so this is the only way to know it plays.
fn playable(file: &Path) -> Result<(), String> {
    let name = file.display().to_string();
    for seek in [&["-t", "2"][..], &["-sseof", "-2"][..]] {
        let out = ffmpeg_command().args(["-v", "error"]).args(seek).args(["-i", &name, "-f", "null", "-"]).output().map_err(|e| e.to_string())?;
        let log = String::from_utf8_lossy(&out.stderr);
        if !out.status.success() || !log.trim().is_empty() {
            let first = log.lines().find(|l| !l.trim().is_empty()).unwrap_or("it could not be opened");
            return Err(format!("the file was written but does not play back cleanly ({first}); please report this"));
        }
    }
    Ok(())
}
