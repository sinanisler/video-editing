//! The AI chat. An OpenRouter model works the open project through the same tools the MCP server offers, so what it does
//! shows up in the activity log and can be undone like any other edit.
//!
//! The key is kept in the operating system's credential store, not in a project or a settings file. The requests are made
//! here rather than in the window, so the key never reaches the page.

use crate::{publish, Core, SharedEditor};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const SERVICE: &str = "snn-editor";
const KEY_ACCOUNT: &str = "openrouter-api-key";
const API: &str = "https://openrouter.ai/api/v1";
const TITLE: &str = "snn-editor";
/// The activity log names the chat's edits by this, so they show apart from the user's.
const ACTOR: &str = "ai";
/// How many rounds of tool calls one message may take before the chat stops and says so.
const MAX_ROUNDS: usize = 16;
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
are frames at the project's frame rate; project_get_info gives the fps when you talk about seconds. The user can undo \
anything you do. Keep replies short and say what you changed.";

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
/// this turn added (the model's and the tool results), and the tool calls with whether each one worked.
#[tauri::command]
pub async fn ai_chat(app: AppHandle, core: State<'_, Core>, messages: Vec<Value>) -> Result<Value, String> {
    let model = load_settings(&app).agent_model.ok_or("Choose an agent model in the AI tab first.")?;
    let key = saved_key()?.ok_or("Add your OpenRouter key in the AI tab first.")?;
    let editor = core.0.clone();
    tauri::async_runtime::spawn_blocking(move || run_chat(&app, &editor, &key, &model, messages))
        .await
        .map_err(|e| e.to_string())?
}

/// The catalog as OpenAI-style function tools. Exports are left out: they start from the Export window.
fn tool_specs() -> Vec<Value> {
    snn_core::catalog()
        .into_iter()
        .filter(|tool| !snn_core::is_export(tool.name))
        .map(|tool| json!({ "type": "function", "function": { "name": tool.name, "description": tool.description, "parameters": tool.schema } }))
        .collect()
}

/// Keeps a tool result short enough to send back to the model.
fn clip_result(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= MAX_RESULT {
        return text;
    }
    let head: String = text.chars().take(MAX_RESULT).collect();
    format!("{head} … (cut short; ask for a smaller span)")
}

/// Runs one tool the way the MCP server does, so the activity log and the window see the same thing.
fn run_tool(app: &AppHandle, editor: &SharedEditor, name: &str, args: Value) -> Result<Value, String> {
    if snn_core::is_export(name) {
        return Err("exports are started from the Export window, not from the chat".into());
    }
    if snn_core::is_analysis(name) {
        // FFmpeg runs without the editor locked, as the MCP server does.
        let job = editor.lock().unwrap().analysis_job(name, &args);
        let result = job.and_then(|job| job.run());
        let mut editor = editor.lock().unwrap();
        editor.log_analysis(ACTOR, name, args, &result);
        publish(app, &editor);
        return result.map(|analysis| analysis.info);
    }
    let mut editor = editor.lock().unwrap();
    let result = editor.call_tool(ACTOR, name, args);
    publish(app, &editor);
    result
}

fn run_chat(app: &AppHandle, editor: &SharedEditor, key: &str, model: &str, mut history: Vec<Value>) -> Result<Value, String> {
    if !history.iter().any(|m| m["role"] == "system") {
        history.insert(0, json!({ "role": "system", "content": SYSTEM }));
    }
    let tools = tool_specs();
    let mut added = Vec::new();
    let mut calls = Vec::new();
    for _ in 0..MAX_ROUNDS {
        let answer: Value = ureq::post(&format!("{API}/chat/completions"))
            .set("Authorization", &format!("Bearer {key}"))
            .set("X-Title", TITLE)
            .send_json(json!({ "model": model, "messages": history, "tools": tools, "tool_choice": "auto" }))
            .map_err(describe)?
            .into_json()
            .map_err(|e| format!("OpenRouter's answer could not be read: {e}"))?;
        let message = answer["choices"][0]["message"].clone();
        if !message.is_object() {
            return Err(format!("the model sent no message: {}", answer));
        }
        history.push(message.clone());
        added.push(message.clone());
        let requested = message["tool_calls"].as_array().cloned().unwrap_or_default();
        if requested.is_empty() {
            return Ok(json!({ "messages": added, "calls": calls }));
        }
        for call in requested {
            let name = call["function"]["name"].as_str().unwrap_or_default().to_string();
            let args: Value = call["function"]["arguments"].as_str().and_then(|text| serde_json::from_str(text).ok()).unwrap_or_else(|| json!({}));
            let result = run_tool(app, editor, &name, args);
            let content = match &result {
                Ok(value) => clip_result(value),
                Err(e) => format!("error: {e}"),
            };
            calls.push(json!({ "name": name, "ok": result.is_ok() }));
            let reply = json!({ "role": "tool", "tool_call_id": call["id"], "content": content });
            history.push(reply.clone());
            added.push(reply);
        }
    }
    Err("The model kept asking for tools past the limit. Ask for a smaller change.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chat_gets_every_tool_except_exports() {
        let specs = tool_specs();
        let names: Vec<&str> = specs.iter().filter_map(|s| s["function"]["name"].as_str()).collect();
        assert!(names.contains(&"timeline_get_state") && names.contains(&"captions_read"));
        assert!(!names.iter().any(|n| n.starts_with("export_")));
        assert!(specs.iter().all(|s| s["function"]["parameters"]["type"] == "object"));
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
