//! MCP server for agents, speaking the Streamable HTTP transport on localhost.
//!
//! It answers each JSON-RPC POST with a plain JSON response and offers no server-initiated
//! stream, which the transport allows. Tools come from `snn_core::catalog` and run through the
//! same `Editor` the window uses, so the user watches agent edits happen and can undo them.

use crate::{allow_media, publish, SharedEditor};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};
use tiny_http::{Header, Method, Request, Response, Server};

pub const PORT: u16 = 7420;
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_BODY: u64 = 8 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct McpStatus {
    pub running: bool,
    pub url: String,
    pub error: Option<String>,
    pub tools: usize,
}

/// Session id → client name, taken from the client's `initialize` call and used as the actor.
type Sessions = HashMap<String, String>;

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header")
}

fn find_header(req: &Request, name: &'static str) -> Option<String> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str().to_string())
}

fn reply(req: Request, status: u16, body: String, extra: Option<Header>) {
    let mut response = Response::from_string(body).with_status_code(status).with_header(header("Content-Type", "application/json"));
    if let Some(h) = extra {
        response = response.with_header(h);
    }
    let _ = req.respond(response);
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// A web page must not be able to drive the editor, so browser origins other than this machine are refused.
fn origin_allowed(req: &Request) -> bool {
    match find_header(req, "Origin") {
        None => true,
        Some(o) => ["http://localhost", "http://127.0.0.1", "https://localhost", "tauri://"].iter().any(|ok| o.starts_with(ok)),
    }
}

struct Ctx<'a> {
    app: &'a AppHandle,
    editor: &'a SharedEditor,
    sessions: &'a mut Sessions,
    next_session: &'a AtomicU64,
}

/// Handles one JSON-RPC message. Returns the response, if the message was a request, and a new session id, if one was created.
fn handle_message(ctx: &mut Ctx, msg: &Value, session: Option<&str>) -> (Option<Value>, Option<String>) {
    let Some(id) = msg.get("id").filter(|v| !v.is_null()) else {
        return (None, None); // A notification: nothing to answer.
    };
    let ok = |result: Value| Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    match msg["method"].as_str().unwrap_or("") {
        "initialize" => {
            let wanted = params["protocolVersion"].as_str().unwrap_or("");
            let version = if PROTOCOLS.contains(&wanted) { wanted } else { PROTOCOLS[0] };
            let client = params["clientInfo"]["name"].as_str().unwrap_or("agent").to_string();
            let sid = format!("snn-{:x}-{:x}", std::process::id(), ctx.next_session.fetch_add(1, Ordering::Relaxed));
            ctx.sessions.insert(sid.clone(), client);
            let result = json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "snn-editor", "version": env!("CARGO_PKG_VERSION") },
                "instructions": snn_core::INSTRUCTIONS,
            });
            (ok(result), Some(sid))
        }
        "ping" => (ok(json!({})), None),
        "tools/list" => {
            let tools: Vec<Value> = snn_core::catalog()
                .into_iter()
                .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema, "annotations": { "readOnlyHint": t.read_only } }))
                .collect();
            (ok(json!({ "tools": tools })), None)
        }
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return (Some(rpc_error(id, -32602, "tools/call needs a tool name")), None);
            };
            let actor = session.and_then(|s| ctx.sessions.get(s)).cloned().unwrap_or_else(|| "agent".into());
            let mut editor = ctx.editor.lock().unwrap();
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            let result = editor.call_tool(&actor, name, arguments.clone());
            if name == "view_set" && result.is_ok() {
                let _ = ctx.app.emit("editor:view", arguments);
            }
            allow_media(ctx.app, &editor);
            publish(ctx.app, &editor);
            let (text, is_error) = match result {
                Ok(v) => (serde_json::to_string_pretty(&v).unwrap_or_default(), false),
                Err(e) => (e, true),
            };
            (ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })), None)
        }
        other => (Some(rpc_error(id, -32601, &format!("method {other} is not supported"))), None),
    }
}

fn handle(ctx: &mut Ctx, mut req: Request) {
    if req.url().split('?').next() != Some("/mcp") {
        return reply(req, 404, json!({ "error": "the MCP endpoint is /mcp" }).to_string(), None);
    }
    if !origin_allowed(&req) {
        return reply(req, 403, json!({ "error": "origin not allowed" }).to_string(), None);
    }
    let session = find_header(&req, "Mcp-Session-Id");
    match req.method() {
        Method::Post => {}
        Method::Delete => {
            if let Some(s) = &session {
                ctx.sessions.remove(s);
            }
            return reply(req, 200, "{}".into(), None);
        }
        // No server-initiated stream is offered.
        _ => return reply(req, 405, json!({ "error": "use POST" }).to_string(), Some(header("Allow", "POST, DELETE"))),
    }
    let mut body = String::new();
    if req.as_reader().take(MAX_BODY).read_to_string(&mut body).is_err() {
        return reply(req, 400, rpc_error(&Value::Null, -32700, "unreadable body").to_string(), None);
    }
    let Ok(parsed) = serde_json::from_str::<Value>(&body) else {
        return reply(req, 400, rpc_error(&Value::Null, -32700, "body is not JSON").to_string(), None);
    };

    let mut new_session = None;
    let mut respond_to = |ctx: &mut Ctx, msg: &Value| {
        let (response, sid) = handle_message(ctx, msg, session.as_deref());
        new_session = new_session.take().or(sid);
        response
    };
    let body = match &parsed {
        Value::Array(batch) => {
            let responses: Vec<Value> = batch.iter().filter_map(|m| respond_to(ctx, m)).collect();
            (!responses.is_empty()).then(|| Value::Array(responses))
        }
        single => respond_to(ctx, single),
    };
    match body {
        Some(v) => reply(req, 200, v.to_string(), new_session.map(|s| header("Mcp-Session-Id", &s))),
        None => reply(req, 202, String::new(), None),
    }
}

/// Starts the server on its own thread and reports whether it is listening.
pub fn start(app: AppHandle, editor: SharedEditor) -> McpStatus {
    let url = format!("http://127.0.0.1:{PORT}/mcp");
    let tools = snn_core::catalog().len();
    let server = match Server::http(("127.0.0.1", PORT)) {
        Ok(s) => s,
        Err(e) => return McpStatus { running: false, url, error: Some(format!("port {PORT} is not available: {e}")), tools },
    };
    std::thread::spawn(move || {
        let mut sessions = Sessions::new();
        let next_session = AtomicU64::new(1);
        for request in server.incoming_requests() {
            let mut ctx = Ctx { app: &app, editor: &editor, sessions: &mut sessions, next_session: &next_session };
            handle(&mut ctx, request);
        }
    });
    McpStatus { running: true, url, error: None, tools }
}
