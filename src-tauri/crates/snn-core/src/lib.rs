//! Core of snn-editor: the project model, the command layer and the undo history.
//!
//! Everything that changes a project goes through [`Editor::execute`]. The GUI and the MCP
//! server are both thin callers of that one function, so an agent can do exactly what a user can.

mod analysis;
mod command;
#[cfg(test)]
mod demo;
mod editor;
mod guides;
mod library;
mod model;
mod probe;
mod tools;

pub use analysis::{base64, is_analysis, Analysis, Job};
pub use command::{apply, Command};
pub use editor::{Editor, EditorState, LogEntry};
pub use guides::{find_guide, guides, Guide};
pub use library::{library, Library};
pub use model::*;
pub use probe::{probe_mp4, Probe};
pub use tools::{catalog, ToolDef, INSTRUCTIONS};

#[cfg(test)]
mod tests;
