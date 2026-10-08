mod export;
mod mcp;

use snn_core::{Command, Editor, EditorState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// The single open project, shared by the window's commands below and the MCP server.
pub(crate) type SharedEditor = Arc<Mutex<Editor>>;
struct Core(SharedEditor);

const STATE_EVENT: &str = "editor:state";

/// Pushes the current state to every window. Called after anything that can change it.
pub(crate) fn publish(app: &AppHandle, editor: &Editor) {
    let _ = app.emit(STATE_EVENT, editor.state());
}

/// Lets the webview read the project's media files, and nothing else on disk.
pub(crate) fn allow_media(app: &AppHandle, editor: &Editor) {
    for item in &editor.project().media {
        let _ = app.asset_protocol_scope().allow_file(&item.path);
    }
}

#[tauri::command]
fn get_state(core: State<Core>) -> EditorState {
    core.0.lock().unwrap().state()
}

/// Property, effect and transition definitions the inspector is drawn from.
#[tauri::command]
fn library() -> snn_core::Library {
    snn_core::library()
}

#[tauri::command]
fn mcp_status(status: State<mcp::McpStatus>) -> mcp::McpStatus {
    status.inner().clone()
}

/// Called before the webview probes a file the user picked or dropped.
#[tauri::command]
fn allow_asset(app: AppHandle, path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).is_file() {
        return Err(format!("{path} is not a file"));
    }
    app.asset_protocol_scope().allow_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn execute(app: AppHandle, core: State<Core>, command: Command) -> Result<serde_json::Value, String> {
    let mut editor = core.0.lock().unwrap();
    let result = editor.execute("user", command);
    allow_media(&app, &editor);
    publish(&app, &editor);
    result
}

/// Several commands as one undo step; none is applied if one fails.
#[tauri::command]
fn execute_batch(app: AppHandle, core: State<Core>, commands: Vec<Command>) -> Result<serde_json::Value, String> {
    let mut editor = core.0.lock().unwrap();
    let result = editor.execute_batch("user", commands);
    publish(&app, &editor);
    result
}

#[tauri::command]
fn undo(app: AppHandle, core: State<Core>) -> Result<serde_json::Value, String> {
    let mut editor = core.0.lock().unwrap();
    let result = editor.undo("user");
    publish(&app, &editor);
    result
}

#[tauri::command]
fn redo(app: AppHandle, core: State<Core>) -> Result<serde_json::Value, String> {
    let mut editor = core.0.lock().unwrap();
    let result = editor.redo("user");
    publish(&app, &editor);
    result
}

#[tauri::command]
fn save_project(app: AppHandle, core: State<Core>, path: Option<String>) -> Result<String, String> {
    let mut editor = core.0.lock().unwrap();
    let result = editor.save(path.map(PathBuf::from).as_deref());
    publish(&app, &editor);
    result
}

#[tauri::command]
fn open_project(app: AppHandle, core: State<Core>, path: String) -> Result<String, String> {
    let mut editor = core.0.lock().unwrap();
    let result = editor.open(&PathBuf::from(path));
    allow_media(&app, &editor);
    publish(&app, &editor);
    result
}

#[tauri::command(async)]
fn export_options(app: AppHandle, core: State<Core>) -> serde_json::Value {
    export::options(&app, &core.0)
}

#[tauri::command(async)]
fn export_begin(app: AppHandle, core: State<Core>, exports: State<export::SharedExports>, settings: snn_core::ExportSettings, job: Option<u64>) -> Result<serde_json::Value, String> {
    export::begin(&app, &exports, &core.0, settings, job)
}

#[tauri::command(async)]
fn export_image(exports: State<export::SharedExports>, job: u64, name: String, data: String) -> Result<(), String> {
    export::image(&exports, job, &name, &data)
}

#[tauri::command]
fn export_run(app: AppHandle, core: State<Core>, exports: State<export::SharedExports>, job: u64, text: Vec<snn_core::TextTrack>) -> Result<(), String> {
    export::run(app, exports.inner().clone(), &core.0, job, text)
}

#[tauri::command]
fn export_fail(app: AppHandle, exports: State<export::SharedExports>, job: u64, message: String) {
    export::fail(&app, &exports, job, message)
}

#[tauri::command]
fn export_cancel(app: AppHandle, exports: State<export::SharedExports>) -> export::ExportStatus {
    export::cancel(&app, &exports)
}

#[tauri::command]
fn export_status(exports: State<export::SharedExports>) -> export::ExportStatus {
    exports.status()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let editor: SharedEditor = Arc::new(Mutex::new(Editor::default()));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Core(editor.clone()))
        .manage(export::SharedExports::default())
        .setup(move |app| {
            app.manage(mcp::start(app.handle().clone(), editor.clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            mcp_status,
            library,
            allow_asset,
            execute,
            execute_batch,
            undo,
            redo,
            save_project,
            open_project,
            export_options,
            export_begin,
            export_image,
            export_run,
            export_fail,
            export_cancel,
            export_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running snn-editor");
}
