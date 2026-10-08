//! Core of snn-editor: the project model, the command layer and the undo history.
//!
//! Everything that changes a project goes through [`Editor::execute`]. The GUI and the MCP
//! server are both thin callers of that one function, so an agent can do exactly what a user can.

mod analysis;
mod command;
#[cfg(test)]
mod demo;
mod editor;
mod export;
mod guides;
mod library;
mod model;
mod probe;
mod render;
mod scan;
mod speech;
mod tools;

pub use analysis::{available_encoders, base64, ffmpeg_version, supports_export, unavailable_encoders, unbase64, ffmpeg_path, is_analysis, probe_streams, Analysis, Job, INSTALL_HINT};
pub use command::{apply, Command};
pub use editor::{Editor, EditorState, LogEntry};
pub use export::{codec_name, codec_of, check as check_export, plan as plan_export, presets as export_presets, AudioStreams, Chunk, Codec, ExportRange, ExportSettings, MediaInfo, Plan, Preset, Quality, RateMode, TextSpan, TextTrack, ENCODERS, SAMPLE_RATE};
pub use guides::{find_guide, guides, Guide};
pub use library::{library, Library};
pub use model::*;
pub use probe::{probe_mp4, Probe};
pub use render::{render, CANCELLED};
pub use scan::media_under;
pub use speech::{audio_streams, chunk_spans, parse_speech, speech_audio, AudioStream, SpeechCue, CHUNK_SECONDS};
pub use tools::{catalog, is_export, ToolDef, INSTRUCTIONS};

#[cfg(test)]
mod tests;
