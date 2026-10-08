//! The tool surface agents see over MCP: a catalog with JSON schemas, and one entry point that
//! runs a tool by name. Edit tools are the [`Command`] variants; read tools are answered here.
//!
//! MCP clients restrict tool names to letters, digits, `_` and `-`, so the catalog uses
//! `timeline_split_clip` where the command layer and the activity log say `timeline.split_clip`.

use crate::command::Command;
use crate::editor::Editor;
use crate::guides::{find_guide, guides};
use crate::library::library;
use crate::model::*;
use crate::probe::probe_mp4;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
    pub read_only: bool,
    /// Valid arguments, used by the tests to prove every tool is wired up.
    pub example: Value,
}

/// Shown to the agent when it connects.
pub const INSTRUCTIONS: &str = "snn-editor is a video editor. These tools read and edit the project that is open in the \
app; the user sees every change live and can undo it. All times are integer frame numbers in the sequence frame rate \
(see project_get_info for fps). Tracks are listed top to bottom; a video track draws over the video tracks below it, and you can add tracks. Clips on \
one track cannot overlap: a command that would overlap fails and changes nothing. Start with project_get_info and \
timeline_get_state, and use batch_apply to make several edits as one undo step. The tools say what is possible, not \
what makes a good edit: before editing for a purpose, call guide_list and read the guide that fits with guide_get.";

fn obj(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn frame(description: &str) -> Value {
    json!({ "type": "integer", "minimum": 0, "description": description })
}

fn text(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn def(name: &'static str, description: &'static str, schema: Value, example: Value) -> ToolDef {
    ToolDef { name, description, schema, read_only: false, example }
}

fn read(name: &'static str, description: &'static str, schema: Value, example: Value) -> ToolDef {
    ToolDef { name, description, schema, read_only: true, example }
}

pub fn catalog() -> Vec<ToolDef> {
    let clip = || text("Clip id, for example c12");
    vec![
        read(
            "project_get_info",
            "Project name, resolution, frame rate, total length, tracks and counts. Call this first.",
            obj(json!({}), &[]),
            json!({}),
        ),
        read(
            "timeline_get_state",
            "Tracks, clips, markers and the in/out range. Pass from/to (frames) or track to limit the clips returned.",
            obj(
                json!({
                    "from": frame("Only clips that end after this frame"),
                    "to": frame("Only clips that start before this frame"),
                    "track": text("Only clips on this track id"),
                }),
                &[],
            ),
            json!({ "from": 0, "to": 900, "track": "V1" }),
        ),
        read("timeline_get_clip", "Everything stored about one clip.", obj(json!({ "clip": clip() }), &["clip"]), json!({ "clip": "c1" })),
        read(
            "library_list",
            "Everything a clip can carry: property keys with their ranges and defaults, effect names, and transition \
             names. Read this before clip_set_property, clip_add_effect or clip_set_transition.",
            obj(json!({}), &[]),
            json!({}),
        ),
        read(
            "guide_list",
            "Editing methods written for this editor: how to cut a raw recording for viewer retention, build an \
             opening, check pace. Call this before a real editing job and read the guide that fits.",
            obj(json!({}), &[]),
            json!({}),
        ),
        read(
            "guide_get",
            "The full text of one guide from guide_list, as Markdown.",
            obj(json!({ "name": text("Guide name, for example retention-edit") }), &["name"]),
            json!({ "name": "retention-edit" }),
        ),
        read(
            "timeline_get_pace",
            "How often the picture changes: the number of clip starts on visible video tracks in each minute and in \
             the first 30 seconds, and the longest stretches with no change. Captions are not counted.",
            obj(json!({}), &[]),
            json!({}),
        ),
        read("media_list", "Files in the media pool, with id, path, kind, length in frames and size.", obj(json!({}), &[]), json!({})),
        read(
            "history_list",
            "Recent commands, newest first, with who ran them and what they returned.",
            obj(json!({ "limit": { "type": "integer", "minimum": 1, "maximum": 200, "description": "Default 20" } }), &[]),
            json!({ "limit": 5 }),
        ),
        read(
            "view_set",
            "Point the user at something in the app window: move the playhead to a frame and/or select clips, which \
             opens them in the inspector. Changes nothing in the project.",
            obj(json!({ "frame": frame("Frame to move the playhead to"), "select": { "type": "array", "items": { "type": "string" }, "description": "Clip ids to select" } }), &[]),
            json!({ "frame": 120, "select": ["c1"] }),
        ),
        def("history_undo", "Undo the most recent change, whoever made it.", obj(json!({}), &[]), json!({})),
        def("history_redo", "Redo the change that was last undone.", obj(json!({}), &[]), json!({})),
        def(
            "batch_apply",
            "Run several edit tools as one undo step. If any command fails, none of them is applied.",
            obj(
                json!({ "commands": {
                    "type": "array",
                    "minItems": 1,
                    "description": "Edit tools to run in order",
                    "items": obj(json!({ "tool": text("Tool name, for example timeline_split_clip"), "args": { "type": "object" } }), &["tool", "args"]),
                } }),
                &["commands"],
            ),
            json!({ "commands": [
                { "tool": "timeline_add_marker", "args": { "at": 30, "label": "A" } },
                { "tool": "timeline_split_clip", "args": { "at": 600 } },
            ] }),
        ),
        def(
            "project_new",
            "Replace the open project with an empty one. Unsaved work in the app is lost unless the user undoes this.",
            obj(json!({ "name": text("Project name") }), &["name"]),
            json!({ "name": "Untitled" }),
        ),
        def(
            "media_import",
            "Add a file to the media pool and return its media id. MP4, MOV and M4V are measured automatically. For \
             other formats pass kind and duration yourself, or ask the user to import the file in the app.",
            obj(
                json!({
                    "path": text("Absolute path of the file"),
                    "name": text("Display name; defaults to the file name"),
                    "kind": { "type": "string", "enum": ["video", "audio", "image"] },
                    "duration": frame("Length in frames; 0 for images"),
                    "width": { "type": "integer", "minimum": 0 },
                    "height": { "type": "integer", "minimum": 0 },
                }),
                &["path"],
            ),
            json!({ "path": "C:/clips/run.mp4", "kind": "video", "duration": 300, "width": 1920, "height": 1080 }),
        ),
        def(
            "media_remove",
            "Remove a file from the media pool. Fails while clips still use it.",
            obj(json!({ "media": text("Media id, for example media3") }), &["media"]),
            json!({ "media": "media0" }),
        ),
        def(
            "timeline_add_clip",
            "Put a clip on a track. Give media (a media id) for footage, sound or an image; or kind graphic with a \
             template and text for a title; or kind caption with text. Video and graphic clips go on video tracks, \
             audio on audio tracks, captions on caption tracks.",
            obj(
                json!({
                    "track": text("Track id, for example V1"),
                    "start": frame("Timeline frame where the clip begins"),
                    "duration": { "type": "integer", "minimum": 1, "description": "Length in frames" },
                    "name": text("Label shown on the clip"),
                    "kind": { "type": "string", "enum": ["video", "graphic", "caption", "audio"] },
                    "media": text("Media id backing the clip"),
                    "source_in": frame("First frame of the media to show; default 0"),
                    "text": text("Text for graphic and caption clips"),
                    "template": { "type": "string", "enum": ["title", "subscribe", "counter", "chapter", "lower"], "description": "Graphic style" },
                }),
                &["track", "start", "duration", "name", "kind"],
            ),
            json!({ "track": "V3", "start": 6000, "duration": 90, "name": "Title", "kind": "graphic", "template": "title", "text": "HELLO" }),
        ),
        def(
            "timeline_move_clip",
            "Move a clip to a new start frame and optionally to another track of the same kind.",
            obj(json!({ "clip": clip(), "start": frame("New start frame"), "track": text("Target track id") }), &["clip", "start"]),
            json!({ "clip": "c1", "start": 9000 }),
        ),
        def(
            "timeline_move_clips",
            "Shift several clips by the same number of frames (negative moves them earlier), keeping their tracks. \
             Either all of them move or none does.",
            obj(json!({ "clips": { "type": "array", "items": { "type": "string" }, "minItems": 1 }, "delta": { "type": "integer", "description": "Frames to shift by" } }), &["clips", "delta"]),
            json!({ "clips": ["c1", "c2"], "delta": 6000 }),
        ),
        def(
            "timeline_duplicate_clips",
            "Copy clips, with their properties, effects and transitions, delta frames away on the same tracks. \
             Returns the new clip ids.",
            obj(json!({ "clips": { "type": "array", "items": { "type": "string" }, "minItems": 1 }, "delta": { "type": "integer", "description": "Frames between each original and its copy" } }), &["clips", "delta"]),
            json!({ "clips": ["c1"], "delta": 6000 }),
        ),
        def(
            "timeline_trim_clip",
            "Set a clip's start and length. Moving the start later hides the beginning of the source; a media clip \
             cannot extend past its source.",
            obj(
                json!({ "clip": clip(), "start": frame("New start frame"), "duration": { "type": "integer", "minimum": 1 } }),
                &["clip", "start", "duration"],
            ),
            json!({ "clip": "c1", "start": 10, "duration": 60 }),
        ),
        def(
            "timeline_split_clip",
            "Cut clips in two at a frame. Without clips, every clip under that frame on unlocked tracks is cut.",
            obj(json!({ "at": frame("Frame to cut at"), "clips": { "type": "array", "items": { "type": "string" }, "description": "Clip ids to cut" } }), &["at"]),
            json!({ "at": 600 }),
        ),
        def("timeline_delete_clip", "Delete a clip and leave a gap.", obj(json!({ "clip": clip() }), &["clip"]), json!({ "clip": "c1" })),
        def(
            "timeline_ripple_delete_clip",
            "Delete a clip and pull later clips on its track left to close the gap.",
            obj(json!({ "clip": clip() }), &["clip"]),
            json!({ "clip": "c1" }),
        ),
        def(
            "timeline_ripple_delete_range",
            "Remove a span of time from every unlocked track and close the gap. This is how to cut out a silence or a \
             boring section. Markers move with the content.",
            obj(json!({ "from": frame("First frame to remove"), "to": frame("First frame to keep") }), &["from", "to"]),
            json!({ "from": 930, "to": 1050 }),
        ),
        def(
            "timeline_add_marker",
            "Add a labelled marker at a frame.",
            obj(json!({ "at": frame("Frame"), "label": text("Marker text") }), &["at", "label"]),
            json!({ "at": 300, "label": "Check this" }),
        ),
        def(
            "timeline_remove_marker",
            "Remove a marker.",
            obj(json!({ "marker": text("Marker id, for example m4") }), &["marker"]),
            json!({ "marker": "m27" }),
        ),
        def(
            "timeline_set_range",
            "Set or clear the in and out points. Omitted or null points are cleared.",
            obj(json!({ "in_point": { "type": ["integer", "null"] }, "out_point": { "type": ["integer", "null"] } }), &[]),
            json!({ "in_point": 30, "out_point": 90 }),
        ),
        def(
            "timeline_add_track",
            "Add a track. There is no fixed number of tracks: add as many as the edit needs. New video tracks go on \
             top of the video stack, new audio tracks at the bottom. Returns the new track id.",
            obj(json!({ "kind": { "type": "string", "enum": ["video", "audio", "caption"] }, "name": text("Defaults to Video N or Audio N") }), &["kind"]),
            json!({ "kind": "video" }),
        ),
        def(
            "timeline_remove_track",
            "Remove a track. It must have no clips on it.",
            obj(json!({ "track": text("Track id") }), &["track"]),
            json!({ "track": "T1" }),
        ),
        def(
            "timeline_rename_track",
            "Give a track a name that describes what is on it.",
            obj(json!({ "track": text("Track id"), "name": text("New name") }), &["track", "name"]),
            json!({ "track": "V1", "name": "Gameplay" }),
        ),
        def(
            "timeline_set_track_state",
            "Lock, mute or hide a track. Locked tracks reject edits and are skipped by ripple deletes.",
            obj(
                json!({ "track": text("Track id"), "locked": { "type": "boolean" }, "muted": { "type": "boolean" }, "hidden": { "type": "boolean" } }),
                &["track"],
            ),
            json!({ "track": "A2", "muted": true }),
        ),
        def(
            "clip_set_property",
            "Set one property of a clip: name, text, or a key from library_list such as zoom_x, pos_x, rotation, \
             crop_left, opacity, blend, speed, volume or color. Positions and crops are in project pixels. An effect's \
             strength is the key fx.<effect name>, 0 to 100. Null resets a property to its default.",
            obj(json!({ "clip": clip(), "key": text("Property name"), "value": {} }), &["clip", "key", "value"]),
            json!({ "clip": "c1", "key": "zoom_x", "value": 1.2 }),
        ),
        def(
            "clip_add_effect",
            "Add an effect to a clip. Names are in library_list (Greyscale, Blur, Vignette, Zoom punch, Shake, \
             Rounded corners, Border and more). Set its strength with clip_set_property and the key fx.<name>.",
            obj(json!({ "clip": clip(), "effect": text("Effect name") }), &["clip", "effect"]),
            json!({ "clip": "c1", "effect": "Zoom punch" }),
        ),
        def(
            "clip_remove_effect",
            "Remove an effect from a clip.",
            obj(json!({ "clip": clip(), "effect": text("Effect name") }), &["clip", "effect"]),
            json!({ "clip": "c1", "effect": "Zoom punch" }),
        ),
        def(
            "clip_set_transition",
            "Set the transition at a clip's start (edge in, the default) or end (edge out). Names are in \
             library_list, for example Cross dissolve, Dip to black, Push left, Wipe right or Whoosh zoom. Omit \
             transition for a plain cut.",
            obj(
                json!({ "clip": clip(), "transition": { "type": ["string", "null"] }, "duration": { "type": "integer", "minimum": 1, "description": "Frames; default half a second" }, "edge": { "type": "string", "enum": ["in", "out"] } }),
                &["clip"],
            ),
            json!({ "clip": "c1", "transition": "Cross dissolve" }),
        ),
    ]
}

/// `timeline_split_clip` → `timeline.split_clip`. Names that already contain a dot pass through.
fn internal(name: &str) -> String {
    if name.contains('.') {
        name.to_string()
    } else {
        name.replacen('_', ".", 1)
    }
}

fn to_command(name: &str, args: Value) -> Result<Command, String> {
    serde_json::from_value(json!({ "tool": internal(name), "args": args }))
        .map_err(|e| format!("invalid arguments for {name}: {e}"))
}

impl Editor {
    /// Runs one catalog tool for `actor`. Reads are logged too, so the user can see what an agent looked at.
    pub fn call_tool(&mut self, actor: &str, name: &str, args: Value) -> Result<Value, String> {
        let args = if args.is_null() { json!({}) } else { args };
        let tool = catalog().into_iter().find(|t| t.name == name).ok_or_else(|| format!("unknown tool {name}"))?;
        if tool.read_only {
            let result = self.read_tool(name, &args);
            self.record(actor, internal(name), args, &result);
            return result;
        }
        match name {
            "history_undo" => self.undo(actor),
            "history_redo" => self.redo(actor),
            "batch_apply" => {
                let items = args["commands"].as_array().ok_or("commands must be an array")?;
                let mut commands = vec![];
                for (i, item) in items.iter().enumerate() {
                    let tool = item["tool"].as_str().ok_or_else(|| format!("command {i} has no tool name"))?;
                    commands.push(to_command(tool, item["args"].clone()).map_err(|e| format!("command {i}: {e}"))?);
                }
                self.execute_batch(actor, commands)
            }
            "media_import" => {
                let mut args = args;
                if args.get("duration").is_none() || args.get("kind").is_none() {
                    let path = args["path"].as_str().ok_or("path must be a string")?.to_string();
                    let info = probe_mp4(Path::new(&path)).map_err(|e| {
                        format!("could not measure {path} ({e}); pass kind and duration, or import it in the app")
                    })?;
                    args["kind"] = serde_json::to_value(info.kind).unwrap();
                    args["duration"] = json!(self.project().settings.frames(info.seconds));
                    args["width"] = json!(info.width);
                    args["height"] = json!(info.height);
                }
                self.execute(actor, to_command(name, args)?)
            }
            _ => self.execute(actor, to_command(name, args)?),
        }
    }

    fn read_tool(&self, name: &str, args: &Value) -> Result<Value, String> {
        let p = self.project();
        let seq = &p.sequence;
        match name {
            "project_get_info" => {
                let s = &p.settings;
                Ok(json!({
                    "name": p.name,
                    "sequence": seq.name,
                    "width": s.width,
                    "height": s.height,
                    "fps": s.fps_num as f64 / s.fps_den as f64,
                    "duration_frames": p.duration(),
                    "tracks": seq.tracks,
                    "clips": seq.clips.len(),
                    "markers": seq.markers.len(),
                    "media": p.media.len(),
                    "time_unit": "frames",
                }))
            }
            "timeline_get_state" => {
                let from = args["from"].as_i64().unwrap_or(0);
                let to = args["to"].as_i64().unwrap_or(Frame::MAX);
                let track = args["track"].as_str();
                let mut clips: Vec<&Clip> = seq
                    .clips
                    .iter()
                    .filter(|c| c.end() > from && c.start < to && track.is_none_or(|t| c.track == t))
                    .collect();
                clips.sort_by_key(|c| (c.start, c.track.clone()));
                Ok(json!({
                    "tracks": seq.tracks,
                    "clips": clips,
                    "markers": seq.markers,
                    "in_point": seq.in_point,
                    "out_point": seq.out_point,
                    "duration_frames": p.duration(),
                }))
            }
            "timeline_get_clip" => {
                let id = args["clip"].as_str().ok_or("clip must be a string")?;
                let c = p.clip(id).ok_or_else(|| format!("no clip {id}"))?;
                Ok(serde_json::to_value(c).unwrap())
            }
            // The window applies this; see the MCP server.
            "view_set" => {
                for id in args["select"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    p.clip(id).ok_or_else(|| format!("no clip {id}"))?;
                }
                Ok(json!({ "shown": args }))
            }
            "guide_list" => Ok(json!({ "guides": guides() })),
            "guide_get" => {
                let name = args["name"].as_str().ok_or("name must be a string")?;
                Ok(Value::String(find_guide(name)?.text.to_string()))
            }
            "timeline_get_pace" => {
                let fps = p.settings.fps_num as f64 / p.settings.fps_den as f64;
                let visible = |c: &&Clip| p.track(&c.track).is_some_and(|t| t.kind == TrackKind::Video && !t.hidden);
                let end = seq.clips.iter().filter(visible).map(Clip::end).max().unwrap_or(0);
                let changes: BTreeSet<Frame> = seq.clips.iter().filter(visible).map(|c| c.start).filter(|s| *s > 0).collect();
                let minute = p.settings.frames(60.0).max(1);
                let mut minutes = vec![0u32; ((end + minute - 1) / minute) as usize];
                for at in &changes {
                    minutes[(at / minute) as usize] += 1;
                }
                let points: Vec<Frame> = std::iter::once(0).chain(changes.iter().copied()).chain([end]).collect();
                let mut still: Vec<(Frame, Frame)> = points.windows(2).map(|w| (w[0], w[1])).filter(|(a, b)| b > a).collect();
                still.sort_by_key(|(a, b)| a - b);
                let seconds = |f: Frame| (f as f64 / fps * 10.0).round() / 10.0;
                Ok(json!({
                    "fps": fps,
                    "duration_frames": end,
                    "changes": changes.len(),
                    "first_30_seconds": changes.iter().filter(|at| **at < p.settings.frames(30.0)).count(),
                    "per_minute": minutes,
                    "longest_without_change": still.iter().take(5).map(|(a, b)| json!({ "from": a, "to": b, "seconds": seconds(b - a) })).collect::<Vec<_>>(),
                }))
            }
            "media_list" => Ok(json!({ "media": p.media })),
            "library_list" => Ok(serde_json::to_value(library()).unwrap()),
            "history_list" => {
                let limit = args["limit"].as_u64().unwrap_or(20) as usize;
                Ok(json!({ "entries": self.log().iter().take(limit).collect::<Vec<_>>() }))
            }
            _ => Err(format!("unknown tool {name}")),
        }
    }
}
