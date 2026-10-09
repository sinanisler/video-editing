//! The AI chat. An OpenRouter model works the open project through the same tools the MCP server offers, so what it does
//! shows up in the activity log and can be undone like any other edit.
//!
//! The key is kept in the operating system's credential store, not in a project or a settings file. The requests are made
//! here rather than in the window, so the key never reaches the page.

use crate::{publish, Core, SharedEditor};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const SERVICE: &str = "snn-editor";
const KEY_ACCOUNT: &str = "openrouter-api-key";
const API: &str = "https://openrouter.ai/api/v1";
const TITLE: &str = "snn-editor";
/// The activity log names the chat's edits by this, so they show apart from the user's.
const ACTOR: &str = "ai";
/// How many rounds of tool calls one message may take before the chat stops and says so. A dead-air pass on a long
/// recording looks, listens and cuts many times over; the user can stop it sooner.
const MAX_ROUNDS: usize = 60;
/// Longest tool result sent back to the model, in characters.
const MAX_RESULT: usize = 12_000;
/// The chat model until the user picks another: small, fast and able to use tools.
const DEFAULT_CHAT_MODEL: &str = "~anthropic/claude-haiku-latest";
/// The model that writes captions from a clip's sound until the user picks another.
const DEFAULT_SUBTITLE_MODEL: &str = "openai/whisper-large-v3";
/// How long one transcription request may take on this side. Upstream providers stop at 60 seconds.
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(120);

const SYSTEM: &str = "You are the editing assistant inside snn-editor, working on the project the user has open. Read before \
you change: use timeline_get_state, captions_read and the frame and audio tools, and check the result of every edit. Times \
are frames at the project's frame rate; project_get_info gives the fps when you talk about seconds. When the user says \
'this clip' or 'here', view_get tells you what they have selected and where their playhead is. Use timeline_locate to \
convert between timeline and source frames, never your own arithmetic. speech_transcribe costs the user money, so ask \
for the stretch you need. The tools say what is possible, not what makes a good edit: before an editing job (cutting a \
recording down, removing dead air, building an opening, checking pace), call guide_list, read the guide that fits with \
guide_get, and follow it. A small, exact request, such as moving or trimming one clip, needs no guide. Every tool result \
ends with a context line: the user's playhead, their selection, and what they changed since your last call; when it \
lists changes, read the timeline again before you edit. export_start returns at once: tell the user the export has \
started and that the Export window shows its progress, and do not keep asking for its status. The user can undo anything \
you do. Keep replies short and say what you changed.";

/// The models the user picked. A setting that is missing from the file takes its default.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    agent_model: Option<String>,
    whisper_model: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { agent_model: Some(DEFAULT_CHAT_MODEL.into()), whisper_model: Some(DEFAULT_SUBTITLE_MODEL.into()) }
    }
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("ai-settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_file(app).ok().and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_file(app)?, text).map_err(|e| e.to_string())
}

fn key_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, KEY_ACCOUNT).map_err(|e| format!("the credential store is not available: {e}"))
}

/// The saved OpenRouter key, if there is one.
fn saved_key() -> Result<Option<String>, String> {
    match key_entry()?.get_password() {
        Ok(key) if !key.is_empty() => Ok(Some(key)),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("could not read the saved key: {e}")),
    }
}

/// What OpenRouter said when a request failed, in words the user can act on.
fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            format!("OpenRouter answered {code}: {}", body.chars().take(400).collect::<String>())
        }
        other => format!("could not reach OpenRouter: {other}"),
    }
}

/// Whether a key is saved, and which models were chosen.
#[tauri::command]
pub fn ai_state(app: AppHandle) -> Result<Value, String> {
    let settings = load_settings(&app);
    Ok(json!({
        "has_key": saved_key()?.is_some(),
        "agent_model": settings.agent_model,
        "whisper_model": settings.whisper_model,
        // The frame, sound and export tools all run FFmpeg, so the chat says when it is missing or too old.
        "ffmpeg": { "version": snn_core::ffmpeg_version(), "supports_export": snn_core::supports_export(), "install": snn_core::INSTALL_HINT },
    }))
}

/// Saves the OpenRouter key in the credential store.
#[tauri::command]
pub fn ai_set_key(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("the key is empty".into());
    }
    key_entry()?.set_password(key).map_err(|e| format!("could not save the key: {e}"))
}

/// Removes the saved key.
#[tauri::command]
pub fn ai_clear_key() -> Result<(), String> {
    match key_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not remove the key: {e}")),
    }
}

/// Chooses the model for a role: `agent` for the chat, `whisper` for subtitles.
#[tauri::command]
pub fn ai_set_model(app: AppHandle, role: String, model: Option<String>) -> Result<(), String> {
    let mut settings = load_settings(&app);
    match role.as_str() {
        "agent" => settings.agent_model = model,
        "whisper" => settings.whisper_model = model,
        other => return Err(format!("unknown model role {other}")),
    }
    save_settings(&app, &settings)
}

/// The models OpenRouter offers now, with what the picker needs: whether the model can use tools, what it reads and
/// writes, and its prices. Fetched when the AI tab opens, so new models appear without an app update.
/// The picker's view of OpenRouter's model list: what each model reads and writes, whether it can use tools, and its prices.
fn picker_models(body: &Value) -> Vec<Value> {
    let strings = |v: &Value| -> Vec<String> { v.as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect() };
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            json!({
                "id": m["id"],
                "name": m["name"],
                "context_length": m["context_length"],
                "tools": strings(&m["supported_parameters"]).iter().any(|p| p.as_str() == "tools"),
                "input": strings(&m["architecture"]["input_modalities"]),
                "output": strings(&m["architecture"]["output_modalities"]),
                "prompt_price": m["pricing"]["prompt"],
                "completion_price": m["pricing"]["completion"],
            })
        })
        .collect()
}

/// The models OpenRouter offers now. Fetched when the AI tab opens, so new models appear without an app update.
/// Speech-to-text models are left out unless every output kind is asked for, so the list asks for all of them.
#[tauri::command]
pub async fn ai_models() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut request = ureq::get(&format!("{API}/models?output_modalities=all")).set("X-Title", TITLE);
        if let Some(key) = saved_key()? {
            request = request.set("Authorization", &format!("Bearer {key}"));
        }
        let body: Value = request
            .call()
            .map_err(describe)?
            .into_json()
            .map_err(|e| format!("the model list could not be read: {e}"))?;
        Ok(json!({ "models": picker_models(&body) }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Makes subtitles for part of a media file. The sound of one track, or one channel of it, is encoded to small MP3 parts
/// first; then each part is sent to the subtitle model. Returns the lines with the seconds they are heard, from the start
/// of the file. Nothing is written to the project here; the window adds the subtitles as one undo step.
#[tauri::command]
pub async fn ai_transcribe(app: AppHandle, core: State<'_, Core>, media: String, track: usize, channel: Option<usize>, from: f64, to: f64) -> Result<Vec<snn_core::SpeechCue>, String> {
    // The editor is not held while the sound is cut and sent, so the timeline stays usable meanwhile.
    let path = {
        let editor = core.0.lock().unwrap();
        editor.project().media(&media).map(|m| m.path.clone()).ok_or_else(|| format!("no media {media} in the project"))?
    };
    let model = load_settings(&app).whisper_model.ok_or("Choose a subtitle model in AI Settings first.")?;
    let key = saved_key()?.ok_or("Add your OpenRouter key in AI Settings first.")?;
    tauri::async_runtime::spawn_blocking(move || transcribe(&app, &key, &model, &path, track, channel, from, to))
        .await
        .map_err(|e| e.to_string())?
}

/// The most sound one `speech_transcribe` call takes: three parts, each a request of up to a minute.
const MAX_SPEECH_SECONDS: f64 = 1800.0;

/// The `speech_transcribe` tool: what is said in a stretch of a media file, timed in source frames. It uses the
/// subtitle model and the key from AI Settings, as the Caption tab does, so an agent hears what the user would.
pub fn speech_tool(app: &AppHandle, editor: &SharedEditor, args: &Value) -> Result<Value, String> {
    let id = args["media"].as_str().ok_or("media must be a media id")?;
    let (path, fps, from, to) = {
        let editor = editor.lock().unwrap();
        let p = editor.project();
        let m = p.media(id).ok_or_else(|| format!("no media {id}"))?;
        if m.kind == snn_core::MediaKind::Image {
            return Err(format!("{} is an image and has no sound", m.name));
        }
        let from = args["from"].as_i64().unwrap_or(0).max(0);
        let to = args["to"].as_i64().unwrap_or(m.duration).min(m.duration);
        (m.path.clone(), p.settings.fps_num as f64 / p.settings.fps_den as f64, from, to)
    };
    if to <= from {
        return Err(format!("nothing to transcribe between frame {from} and frame {to}"));
    }
    let seconds = (to - from) as f64 / fps;
    if seconds > MAX_SPEECH_SECONDS {
        let most = (MAX_SPEECH_SECONDS * fps) as i64;
        return Err(format!("that is {:.0} minutes of sound; one call takes at most 30 ({most} frames). Pass from and to for the stretch you need", seconds / 60.0));
    }
    let model = load_settings(app).whisper_model.ok_or("no subtitle model is chosen; ask the user to choose one in the app's AI Settings")?;
    let key = saved_key()?.ok_or("no OpenRouter key is saved; ask the user to add one in the app's AI Settings")?;
    let stream = args["audio_stream"].as_u64().unwrap_or(0) as usize;
    let channel = args["channel"].as_u64().map(|c| c as usize);
    let cues = transcribe(app, &key, &model, &path, stream, channel, from as f64 / fps, to as f64 / fps)?;
    let frame = |seconds: f64| (seconds * fps).round() as i64;
    let lines: Vec<Value> = cues.iter().map(|c| json!({ "from": frame(c.start), "to": frame(c.end), "time": snn_core::timecode(c.start), "text": c.text })).collect();
    Ok(json!({
        "frames": "source frames of the media",
        "media": id,
        "from": from,
        "to": to,
        "audio_stream": stream,
        "model": model,
        "lines": lines,
    }))
}

/// The request for one part of sound: a mono MP3 in base64, asking for timings for each segment of speech.
fn transcription_request(model: &str, mp3: &[u8]) -> Value {
    json!({
        "model": model,
        "input_audio": { "data": snn_core::base64(mp3), "format": "mp3" },
        "response_format": "verbose_json",
        "timestamp_granularities": ["segment"],
    })
}

/// Removes a folder when it goes out of scope, so encoded audio never outlives the subtitles it was made for.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// A folder name no other run is using: the process and the time.
fn run_folder() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

/// Encodes the sound first, every part of it, and only then sends the parts one at a time. Progress is reported as
/// "encoding" (parts finished) and "sending" (the part going out, counted from 1).
fn transcribe(app: &AppHandle, key: &str, model: &str, path: &str, track: usize, channel: Option<usize>, from: f64, to: f64) -> Result<Vec<snn_core::SpeechCue>, String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("subtitles").join(run_folder());
    let _cleanup = Cleanup(dir.clone());
    let parts = snn_core::encode_speech(path, from, to, track, channel, &dir, snn_core::PART_SECONDS, |done, total| {
        app.emit("subtitles:progress", json!({ "stage": "encoding", "done": done, "total": total })).ok();
    })?;
    if parts.is_empty() {
        return Err("the clip has no sound to make subtitles from".into());
    }
    let mut lines = vec![];
    for (i, (offset, file)) in parts.iter().enumerate() {
        let mp3 = std::fs::read(file).map_err(|e| format!("could not read the encoded audio: {e}"))?;
        app.emit("subtitles:progress", json!({ "stage": "sending", "done": i + 1, "total": parts.len() })).ok();
        let answer: Value = ureq::post(&format!("{API}/audio/transcriptions"))
            .set("Authorization", &format!("Bearer {key}"))
            .set("X-Title", TITLE)
            .timeout(TRANSCRIBE_TIMEOUT)
            .send_json(transcription_request(model, &mp3))
            .map_err(describe)?
            .into_json()
            .map_err(|e| format!("OpenRouter's answer could not be read: {e}"))?;
        lines.extend(snn_core::parse_speech(&answer, *offset)?);
    }
    Ok(lines)
}

/// One message from the user: runs the chat until the model answers without asking for tools. Returns the messages
/// this turn added (the model's and the tool results), the tool calls with whether each one worked, and, when the
/// turn ended early, why.
#[tauri::command]
pub async fn ai_chat(app: AppHandle, core: State<'_, Core>, messages: Vec<Value>) -> Result<Value, String> {
    let model = load_settings(&app).agent_model.ok_or("Choose an agent model in the AI tab first.")?;
    let key = saved_key()?.ok_or("Add your OpenRouter key in the AI tab first.")?;
    let editor = core.0.clone();
    tauri::async_runtime::spawn_blocking(move || run_chat(&app, &editor, &key, &model, messages))
        .await
        .map_err(|e| e.to_string())?
}

/// Stops the chat after the step it is on. What it has done stays, and can be undone.
#[tauri::command]
pub fn ai_cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// Told to the window as the chat works: which step it is on and which tool it is running.
const PROGRESS_EVENT: &str = "ai:progress";
/// The most pictures one round of tool calls sends to the model. Each one costs about as much as a page of text.
const MAX_PICTURES: usize = 8;

static CANCEL: AtomicBool = AtomicBool::new(false);
/// The newest log entry the chat has been told about, so each result can say what the user changed since.
static SEEN: Mutex<Option<u64>> = Mutex::new(None);
/// Whether each model that has been used takes pictures, asked of OpenRouter once.
static SEES: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

/// Whether an entry of OpenRouter's model list takes pictures as input.
fn takes_pictures(models: &Value, model: &str) -> Option<bool> {
    let entry = models["data"].as_array()?.iter().find(|m| m["id"] == model)?;
    Some(entry["architecture"]["input_modalities"].as_array().is_some_and(|kinds| kinds.iter().any(|k| k == "image")))
}

/// Whether the chat model can be shown pictures. A model that is not in OpenRouter's list, or a list that cannot
/// be fetched, counts as one that cannot: sending a picture to a model without sight fails the whole request.
fn sees_pictures(key: &str, model: &str) -> bool {
    if let Some(known) = SEES.lock().unwrap().as_ref().and_then(|seen| seen.get(model).copied()) {
        return known;
    }
    let list: Option<Value> = ureq::get(&format!("{API}/models?output_modalities=all"))
        .set("Authorization", &format!("Bearer {key}"))
        .set("X-Title", TITLE)
        .timeout(Duration::from_secs(20))
        .call()
        .ok()
        .and_then(|r| r.into_json().ok());
    let Some(answer) = list.as_ref().and_then(|list| takes_pictures(list, model)) else { return false };
    SEES.lock().unwrap().get_or_insert_with(HashMap::new).insert(model.to_string(), answer);
    answer
}

/// The catalog as OpenAI-style function tools. A model that cannot be shown pictures is not offered the rendered
/// frame, whose whole answer is a picture.
fn tool_specs(pictures: bool) -> Vec<Value> {
    snn_core::catalog()
        .into_iter()
        .filter(|tool| pictures || tool.name != "frames_render")
        .map(|tool| json!({ "type": "function", "function": { "name": tool.name, "description": tool.description, "parameters": tool.schema } }))
        .collect()
}

/// Keeps a tool result short enough to send back to the model.
fn clip_result(value: &Value) -> String {
    let text = match value {
        // A guide is Markdown; as a JSON string every line break in it would be escaped.
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if text.chars().count() <= MAX_RESULT {
        return text;
    }
    let head: String = text.chars().take(MAX_RESULT).collect();
    format!("{head} … (cut short; ask for a smaller span)")
}

/// What goes in the activity log for a transcript: how much was heard, not every line of it.
fn speech_summary(result: &Result<Value, String>) -> Result<Value, String> {
    result.as_ref().map(|v| json!({ "lines": v["lines"].as_array().map_or(0, Vec::len), "model": v["model"] })).map_err(String::clone)
}

/// Runs one tool the way the MCP server does, so the activity log and the window see the same thing. Returns the
/// facts and, for the frame tools, the picture.
fn run_tool(app: &AppHandle, editor: &SharedEditor, name: &str, args: Value) -> Result<(Value, Option<Vec<u8>>), String> {
    if snn_core::is_export(name) {
        let exports = app.state::<crate::export::SharedExports>();
        let result = crate::export::tool(app, &exports, editor, name, &args);
        if name != "export_get_status" {
            let mut editor = editor.lock().unwrap();
            editor.log_tool(ACTOR, name, args, &result);
            publish(app, &editor);
        }
        return result.map(|value| (value, None));
    }
    if name == "speech_transcribe" {
        let result = speech_tool(app, editor, &args);
        let mut editor = editor.lock().unwrap();
        editor.log_tool(ACTOR, name, args, &speech_summary(&result));
        publish(app, &editor);
        return result.map(|value| (value, None));
    }
    if name == "frames_render" || snn_core::is_analysis(name) {
        // FFmpeg and the window work without the editor locked, as for the MCP server.
        let result = if name == "frames_render" {
            let exports = app.state::<crate::export::SharedExports>();
            crate::export::render_frame(app, &exports, editor, &args)
        } else {
            let job = editor.lock().unwrap().analysis_job(name, &args);
            job.and_then(|job| job.run())
        };
        let mut editor = editor.lock().unwrap();
        editor.log_analysis(ACTOR, name, args, &result);
        publish(app, &editor);
        return result.map(|analysis| (analysis.info, analysis.jpeg));
    }
    let mut editor = editor.lock().unwrap();
    let result = editor.call_tool(ACTOR, name, args.clone());
    if name == "view_set" && result.is_ok() {
        let _ = app.emit("editor:view", args);
    }
    crate::allow_media(app, &editor);
    publish(app, &editor);
    result.map(|value| (value, None))
}

/// The context block for the chat: what the user is looking at and what changed since the chat was last told.
fn context(editor: &SharedEditor) -> String {
    let editor = editor.lock().unwrap();
    let mut seen = SEEN.lock().unwrap();
    let text = editor.context(ACTOR, seen.unwrap_or_else(|| editor.seq())).to_string();
    *seen = Some(editor.seq());
    text
}

/// The message that carries the pictures of one round of tool calls. A tool result is text in this format, so the
/// pictures follow in a message of their own, each named after the call it answers.
fn picture_message(pictures: &[(String, Vec<u8>)]) -> Value {
    let mut parts = vec![json!({ "type": "text", "text": "The pictures your tool calls returned, in order. The app sends them; the user did not write this." })];
    for (label, jpeg) in pictures {
        parts.push(json!({ "type": "text", "text": label }));
        parts.push(json!({ "type": "image_url", "image_url": { "url": format!("data:image/jpeg;base64,{}", snn_core::base64(jpeg)) } }));
    }
    json!({ "role": "user", "content": parts })
}

/// The system message, which says whether this model will see the pictures the frame tools return.
fn system_message(pictures: bool) -> Value {
    let sight = if pictures {
        "You can see: the pictures the frame tools return are sent to you right after their results. Look before you cut."
    } else {
        "This model cannot be shown pictures, so the frame tools give you only their numbers. Work from sound, speech and \
         the timeline, say so when a decision needs eyes, and suggest the user choose a model with image input in AI Settings."
    };
    json!({ "role": "system", "content": format!("{SYSTEM} {sight}") })
}

/// What a chat turn needs from outside: the model, the editor's tools and the window. The app provides the real
/// ones; the tests provide a scripted model, so the turn itself can be checked without a key or a window.
trait ChatHost {
    /// Sends the conversation to the model and returns its answer.
    fn ask(&mut self, request: &Value) -> Result<Value, String>;
    fn tool(&mut self, name: &str, args: Value) -> Result<(Value, Option<Vec<u8>>), String>;
    /// The context block: what the user is looking at and what changed since it was last asked for.
    fn context(&mut self) -> String;
    fn progress(&mut self, round: usize, tool: Option<&str>);
    fn cancelled(&self) -> bool;
}

struct Live<'a> {
    app: &'a AppHandle,
    editor: &'a SharedEditor,
    key: &'a str,
}

impl ChatHost for Live<'_> {
    fn ask(&mut self, request: &Value) -> Result<Value, String> {
        ureq::post(&format!("{API}/chat/completions"))
            .set("Authorization", &format!("Bearer {}", self.key))
            .set("X-Title", TITLE)
            .send_json(request)
            .map_err(describe)?
            .into_json()
            .map_err(|e| format!("OpenRouter's answer could not be read: {e}"))
    }

    fn tool(&mut self, name: &str, args: Value) -> Result<(Value, Option<Vec<u8>>), String> {
        run_tool(self.app, self.editor, name, args)
    }

    fn context(&mut self) -> String {
        context(self.editor)
    }

    fn progress(&mut self, round: usize, tool: Option<&str>) {
        self.app.emit(PROGRESS_EVENT, json!({ "round": round, "tool": tool })).ok();
    }

    fn cancelled(&self) -> bool {
        CANCEL.load(Ordering::Relaxed)
    }
}

fn run_chat(app: &AppHandle, editor: &SharedEditor, key: &str, model: &str, history: Vec<Value>) -> Result<Value, String> {
    CANCEL.store(false, Ordering::Relaxed);
    // A new conversation is told about changes from now on, not about everything already in the log.
    if !history.iter().any(|m| m["role"] == "assistant") {
        *SEEN.lock().unwrap() = None;
    }
    chat_turn(&mut Live { app, editor, key }, model, sees_pictures(key, model), history)
}

/// One turn: the model is asked, its tool calls are run and answered, and so on until it answers in words, is
/// stopped, fails, or has taken as many steps as one message may.
fn chat_turn(host: &mut dyn ChatHost, model: &str, pictures: bool, mut history: Vec<Value>) -> Result<Value, String> {
    if !history.iter().any(|m| m["role"] == "system") {
        history.insert(0, system_message(pictures));
    }
    // The user's message goes out with what they are looking at, so "this clip" and "here" mean something. Only the
    // copy sent to the model carries it; the window keeps the message as it was typed.
    if let Some(text) = history.last().filter(|m| m["role"] == "user").and_then(|m| m["content"].as_str()).map(String::from) {
        let last = history.len() - 1;
        history[last]["content"] = json!(format!("{text}\n\n[editor context, added by the app: {}]", host.context()));
    }
    let tools = tool_specs(pictures);
    let mut added = Vec::new();
    let mut calls = Vec::new();
    // Ends the turn early with a message from the app, keeping what the model did so far.
    let stop = |mut added: Vec<Value>, calls: Vec<Value>, reason: &str, text: String| {
        added.push(json!({ "role": "assistant", "content": text }));
        Ok(json!({ "messages": added, "calls": calls, "stopped": reason }))
    };
    for round in 1..=MAX_ROUNDS {
        if host.cancelled() {
            return stop(added, calls, "cancelled", "Stopped. What I did so far is on the timeline and can be undone.".into());
        }
        host.progress(round, None);
        let answer = host.ask(&json!({ "model": model, "messages": history, "tools": tools, "tool_choice": "auto" }));
        let message = match answer.map(|a| a["choices"][0]["message"].clone()) {
            Ok(message) if message.is_object() => message,
            // Nothing has happened yet, so the message simply did not go through.
            Ok(_) if added.is_empty() => return Err("the model sent no message".into()),
            Err(e) if added.is_empty() => return Err(e),
            // Edits were made before it failed. They are kept, and so is the conversation that led to them.
            Ok(_) => return stop(added, calls, "failed", "The model stopped answering. What I did so far is on the timeline and can be undone.".into()),
            Err(e) => return stop(added, calls, "failed", format!("I had to stop: {e}. What I did so far is on the timeline and can be undone.")),
        };
        history.push(message.clone());
        added.push(message.clone());
        let requested = message["tool_calls"].as_array().cloned().unwrap_or_default();
        if requested.is_empty() {
            return Ok(json!({ "messages": added, "calls": calls }));
        }
        let mut shown: Vec<(String, Vec<u8>)> = vec![];
        for call in requested {
            let name = call["function"]["name"].as_str().unwrap_or_default().to_string();
            let raw = call["function"]["arguments"].as_str().unwrap_or("{}");
            let args: Value = serde_json::from_str(raw).unwrap_or_else(|_| json!({}));
            host.progress(round, Some(&name));
            let result = host.tool(&name, args);
            let mut content = match &result {
                Ok((value, _)) => clip_result(value),
                Err(e) => format!("error: {e}"),
            };
            if let Ok((_, Some(jpeg))) = result.as_ref() {
                if !pictures {
                    content += "\n(This model cannot be shown the picture.)";
                } else if shown.len() < MAX_PICTURES {
                    shown.push((format!("{name} {raw}"), jpeg.clone()));
                    // The picture is in the conversation for this message only; later ones carry this note in its place.
                    content += "\n(The picture follows this round of results. It is shown once: ask again if you need it later.)";
                } else {
                    content += "\n(Too many pictures in one step, so this one was not sent. Ask for it again on its own.)";
                }
            }
            content += &format!("\ncontext: {}", host.context());
            calls.push(json!({ "name": name, "ok": result.is_ok() }));
            let reply = json!({ "role": "tool", "tool_call_id": call["id"], "content": content });
            history.push(reply.clone());
            added.push(reply);
        }
        if !shown.is_empty() {
            // Sent to the model now and not kept by the window, which would otherwise send every picture again with every message.
            history.push(picture_message(&shown));
        }
    }
    stop(added, calls, "limit", format!("I stopped after {MAX_ROUNDS} steps, which is the most one message may take. What I did so far is on the timeline. Say “continue” and I will carry on."))
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A model that answers from a script, and tools that answer from a table. It keeps every request it was sent.
    #[derive(Default)]
    struct Scripted {
        answers: Vec<Result<Value, String>>,
        requests: Vec<Value>,
        steps: Vec<String>,
        /// Stop once this many requests have gone out.
        cancel_after: Option<usize>,
    }

    impl ChatHost for Scripted {
        fn ask(&mut self, request: &Value) -> Result<Value, String> {
            self.requests.push(request.clone());
            // With the script used up, the model keeps asking for the same tool.
            if self.answers.is_empty() {
                return Ok(calls(&[("timeline_get_state", "{}")]));
            }
            self.answers.remove(0)
        }

        fn tool(&mut self, name: &str, args: Value) -> Result<(Value, Option<Vec<u8>>), String> {
            match name {
                "frames_get" => Ok((json!({ "timeline_frame": args["at"] }), Some(vec![0xff, 0xd8, 0xff]))),
                "timeline_delete_clip" => Err("no clip c9".into()),
                _ => Ok((json!({ "clips": [] }), None)),
            }
        }

        fn context(&mut self) -> String {
            r#"{"playhead":90,"selection":["c3"]}"#.into()
        }

        fn progress(&mut self, round: usize, tool: Option<&str>) {
            self.steps.push(format!("{round} {}", tool.unwrap_or("-")));
        }

        fn cancelled(&self) -> bool {
            self.cancel_after.is_some_and(|n| self.requests.len() >= n)
        }
    }

    /// A model answer that asks for these tools.
    fn calls(tools: &[(&str, &str)]) -> Value {
        let asked: Vec<Value> = tools.iter().enumerate().map(|(i, (name, args))| json!({ "id": format!("call{i}"), "type": "function", "function": { "name": name, "arguments": args } })).collect();
        json!({ "choices": [{ "message": { "role": "assistant", "content": null, "tool_calls": asked } }] })
    }

    fn says(text: &str) -> Value {
        json!({ "choices": [{ "message": { "role": "assistant", "content": text } }] })
    }

    fn user(text: &str) -> Vec<Value> {
        vec![json!({ "role": "user", "content": text })]
    }

    #[test]
    fn a_turn_shows_the_model_its_pictures_and_what_the_user_is_looking_at() {
        let mut host = Scripted { answers: vec![Ok(calls(&[("frames_get", r#"{"at":60}"#), ("timeline_delete_clip", r#"{"clip":"c9"}"#)])), Ok(says("Trimmed it."))], ..Default::default() };
        let turn = chat_turn(&mut host, "a/eyes", true, user("trim this clip")).unwrap();

        // The first request carries the system message and, with the user's words, what they have selected.
        let first = host.requests[0]["messages"].as_array().unwrap();
        assert_eq!(first[0]["role"], "system");
        let asked = first[1]["content"].as_str().unwrap();
        assert!(asked.starts_with("trim this clip") && asked.contains(r#""selection":["c3"]"#), "{asked}");
        assert!(host.requests[0]["tools"].as_array().unwrap().iter().any(|t| t["function"]["name"] == "frames_render"));

        // The second request has both results, each with the context line, then the picture in a message of its own.
        let second = host.requests[1]["messages"].as_array().unwrap();
        let (seen, failed, picture) = (&second[3], &second[4], &second[5]);
        assert_eq!((seen["role"].as_str(), seen["tool_call_id"].as_str()), (Some("tool"), Some("call0")));
        let text = seen["content"].as_str().unwrap();
        assert!(text.contains(r#""timeline_frame":60"#) && text.contains("The picture follows") && text.contains(r#"context: {"playhead":90"#), "{text}");
        assert!(failed["content"].as_str().unwrap().starts_with("error: no clip c9"));
        assert_eq!(picture["role"], "user");
        let parts = picture["content"].as_array().unwrap();
        assert_eq!(parts.iter().filter(|p| p["type"] == "image_url").count(), 1);
        assert_eq!(parts[1]["text"], r#"frames_get {"at":60}"#);

        // The window gets the model's messages and the results, not the picture: it would send it again every time.
        let kept = turn["messages"].as_array().unwrap();
        assert_eq!(kept.iter().map(|m| m["role"].as_str().unwrap()).collect::<Vec<_>>(), ["assistant", "tool", "tool", "assistant"]);
        assert_eq!(kept[3]["content"], "Trimmed it.");
        assert_eq!(turn["calls"], json!([{ "name": "frames_get", "ok": true }, { "name": "timeline_delete_clip", "ok": false }]));
        assert!(turn.get("stopped").is_none());
        assert_eq!(host.steps, ["1 -", "1 frames_get", "1 timeline_delete_clip", "2 -"]);
    }

    #[test]
    fn a_model_without_sight_gets_the_numbers_and_is_told_so() {
        let mut host = Scripted { answers: vec![Ok(calls(&[("frames_get", r#"{"at":60}"#)])), Ok(says("ok"))], ..Default::default() };
        chat_turn(&mut host, "a/text", false, user("look")).unwrap();
        assert!(!host.requests[0]["tools"].as_array().unwrap().iter().any(|t| t["function"]["name"] == "frames_render"));
        let second = host.requests[1]["messages"].as_array().unwrap();
        assert_eq!(second.len(), 4, "no picture message: {second:?}");
        assert!(second[3]["content"].as_str().unwrap().contains("cannot be shown the picture"));
        assert!(!host.requests[1].to_string().contains("image_url"));
    }

    #[test]
    fn a_turn_that_runs_long_stops_at_the_limit_and_keeps_its_work() {
        let mut host = Scripted::default();
        let turn = chat_turn(&mut host, "a/eyes", true, user("cut everything")).unwrap();
        assert_eq!(host.requests.len(), MAX_ROUNDS);
        assert_eq!(turn["stopped"], "limit");
        let kept = turn["messages"].as_array().unwrap();
        assert_eq!(kept.len(), MAX_ROUNDS * 2 + 1);
        let last = kept.last().unwrap();
        assert!(last["role"] == "assistant" && last["content"].as_str().unwrap().contains("continue"));
        assert_eq!(turn["calls"].as_array().unwrap().len(), MAX_ROUNDS);
    }

    #[test]
    fn stopping_ends_the_turn_after_the_step_it_is_on() {
        let mut host = Scripted { cancel_after: Some(2), ..Default::default() };
        let turn = chat_turn(&mut host, "a/eyes", true, user("cut everything")).unwrap();
        assert_eq!((host.requests.len(), turn["stopped"].as_str()), (2, Some("cancelled")));
        // Each request was answered with its tool result before the turn ended, so the conversation can go on.
        let roles: Vec<&str> = turn["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["assistant", "tool", "assistant", "tool", "assistant"]);
    }

    #[test]
    fn a_failed_request_loses_nothing_that_was_already_done() {
        // Before anything happened, the message simply did not go through.
        let mut host = Scripted { answers: vec![Err("OpenRouter answered 402: no credit".into())], ..Default::default() };
        assert!(chat_turn(&mut host, "a/eyes", true, user("hi")).unwrap_err().contains("402"));
        // After an edit, the turn ends with what was done and the reason.
        let mut host = Scripted { answers: vec![Ok(calls(&[("timeline_get_state", "{}")])), Err("OpenRouter answered 502: bad gateway".into())], ..Default::default() };
        let turn = chat_turn(&mut host, "a/eyes", true, user("hi")).unwrap();
        assert_eq!(turn["stopped"], "failed");
        let last = turn["messages"].as_array().unwrap().last().unwrap().clone();
        assert!(last["content"].as_str().unwrap().contains("502") && turn["calls"].as_array().unwrap().len() == 1);
    }

    /// The chat is told to read a guide before an editing job, as agents that connect over MCP are.
    #[test]
    fn the_chat_is_sent_to_the_guides() {
        assert!(SYSTEM.contains("guide_list") && SYSTEM.contains("guide_get"));
        assert!(snn_core::INSTRUCTIONS.contains("guide_list"));
    }

    #[test]
    fn the_chat_gets_every_tool_and_the_rendered_frame_only_when_it_can_see() {
        let names = |specs: &[Value]| specs.iter().filter_map(|s| s["function"]["name"].as_str().map(String::from)).collect::<Vec<_>>();
        let seeing = tool_specs(true);
        assert_eq!(seeing.len(), snn_core::catalog().len());
        assert!(seeing.iter().all(|s| s["function"]["parameters"]["type"] == "object"));
        for tool in ["timeline_get_state", "captions_read", "speech_transcribe", "timeline_locate", "export_start", "frames_render"] {
            assert!(names(&seeing).iter().any(|n| n == tool), "{tool}");
        }
        let blind = names(&tool_specs(false));
        assert!(!blind.iter().any(|n| n == "frames_render") && blind.iter().any(|n| n == "frames_get"));
        assert!(system_message(true)["content"].as_str().unwrap().contains("You can see"));
        assert!(system_message(false)["content"].as_str().unwrap().contains("cannot be shown pictures"));
    }

    #[test]
    fn a_model_takes_pictures_when_its_entry_lists_image_input() {
        let list = json!({ "data": [
            { "id": "a/eyes", "architecture": { "input_modalities": ["text", "image"] } },
            { "id": "a/text", "architecture": { "input_modalities": ["text"] } },
        ] });
        assert_eq!(takes_pictures(&list, "a/eyes"), Some(true));
        assert_eq!(takes_pictures(&list, "a/text"), Some(false));
        assert_eq!(takes_pictures(&list, "a/unknown"), None);
        assert_eq!(takes_pictures(&json!({ "error": "x" }), "a/eyes"), None);
    }

    #[test]
    fn pictures_go_to_the_model_as_one_message_after_the_results() {
        let message = picture_message(&[("frames_get {\"at\":60}".into(), vec![0xff, 0xd8, 0xff]), ("frames_render {\"at\":90}".into(), vec![0xff, 0xd8])]);
        assert_eq!(message["role"], "user");
        let parts = message["content"].as_array().unwrap();
        assert_eq!(parts.len(), 5, "a note, then a label and a picture for each");
        assert_eq!(parts[1]["text"], "frames_get {\"at\":60}");
        assert_eq!(parts[2]["image_url"]["url"], format!("data:image/jpeg;base64,{}", snn_core::base64(&[0xff, 0xd8, 0xff])));
        assert_eq!(parts[4]["type"], "image_url");
    }

    #[test]
    fn a_guide_reaches_the_model_as_markdown() {
        assert_eq!(clip_result(&json!("# Title

Text")), "# Title

Text");
    }

    #[test]
    fn the_models_start_as_the_small_chat_model_and_whisper() {
        let settings = Settings::default();
        assert_eq!(settings.agent_model.as_deref(), Some("~anthropic/claude-haiku-latest"));
        assert_eq!(settings.whisper_model.as_deref(), Some("openai/whisper-large-v3"));
        // A settings file written before a setting existed still gets its default for it.
        let older: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(older.agent_model, settings.agent_model);
        assert_eq!(older.whisper_model, settings.whisper_model);
        let chosen: Settings = serde_json::from_str(r#"{ "whisper_model": "openai/whisper-1" }"#).unwrap();
        assert_eq!(chosen.agent_model, settings.agent_model);
        assert_eq!(chosen.whisper_model.as_deref(), Some("openai/whisper-1"));
    }

    #[test]
    fn speech_to_text_models_come_through_the_picker_with_their_output_kind() {
        // Shaped like OpenRouter's entry for Whisper, which the default model list leaves out.
        let body = json!({ "data": [
            {
                "id": "openai/whisper-large-v3", "name": "OpenAI: Whisper Large V3", "context_length": 0,
                "architecture": { "input_modalities": ["audio"], "output_modalities": ["transcription"] },
                "pricing": { "prompt": "0.0000075", "completion": "0" }, "supported_parameters": []
            },
            {
                "id": "~anthropic/claude-haiku-latest", "name": "Anthropic: Claude Haiku Latest", "context_length": 1000000,
                "architecture": { "input_modalities": ["text", "image"], "output_modalities": ["text"] },
                "pricing": { "prompt": "0.0000001", "completion": "0.0000005" }, "supported_parameters": ["tools", "temperature"]
            }
        ] });
        let models = picker_models(&body);
        assert_eq!(models[0]["id"], "openai/whisper-large-v3");
        assert_eq!(models[0]["output"], json!(["transcription"]));
        assert_eq!(models[0]["tools"], false);
        assert_eq!(models[1]["tools"], true);
        assert_eq!(models[1]["output"], json!(["text"]));
    }

    #[test]
    fn a_transcription_request_sends_mp3_and_asks_for_segment_timings() {
        let request = transcription_request("openai/whisper-large-v3", b"ID3");
        assert_eq!(request["model"], "openai/whisper-large-v3");
        assert_eq!(request["input_audio"]["format"], "mp3");
        assert_eq!(request["input_audio"]["data"], snn_core::base64(b"ID3").as_str());
        assert_eq!(request["response_format"], "verbose_json");
        assert_eq!(request["timestamp_granularities"], json!(["segment"]));
    }

    #[test]
    fn long_tool_results_are_cut_for_the_model() {
        let big = json!({ "text": "x".repeat(MAX_RESULT * 2) });
        let clipped = clip_result(&big);
        assert!(clipped.chars().count() <= MAX_RESULT + 60);
        assert!(clipped.ends_with("ask for a smaller span)"));
        assert_eq!(clip_result(&json!({ "a": 1 })), "{\"a\":1}");
    }
}
