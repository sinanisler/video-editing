//! The tool surface agents see over MCP: a catalog with JSON schemas, and one entry point that
//! runs a tool by name. Edit tools are the [`Command`] variants; read tools are answered here.
//!
//! MCP clients restrict tool names to letters, digits, `_` and `-`, so the catalog uses
//! `timeline_split_clip` where the command layer and the activity log say `timeline.split_clip`.

use crate::analysis::is_analysis;
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
(see project_get_info for fps): frame = round(seconds x fps), and one frame is the finest step there is. Source frames \
of a media file are counted at that same rate. Tracks are listed top to bottom; a video track draws over the video \
tracks below it, and you can add tracks. Clips on one track cannot overlap: a command that would overlap fails and \
changes nothing. Start with project_get_info and timeline_get_state, and use batch_apply to make several edits as one \
undo step. The tools say what is possible, not what makes a good edit: before editing for a purpose, call guide_list and \
read the guide that fits with guide_get. You can look at the footage: frames_contact_sheet and frames_get return pictures \
of the footage, frames_render returns the finished picture with effects, cards and captions, audio_get_activity finds \
silences and the loudest moments, and speech_transcribe returns what is said. Look before you cut. Do not convert \
between source and timeline frames by hand: timeline_locate does it, and timeline_ripple_delete_ranges takes source \
frames when you give it media. Every result ends with a context block: where the user's playhead is, what they have \
selected, and what anyone else changed since your last call. When it lists changes, read the timeline again before you \
edit. project_save saves the project. export_start renders the timeline to a video file.";

/// The export tools, which the app answers because an export needs the window. See the MCP server.
pub fn is_export(name: &str) -> bool {
    name.starts_with("export_")
}

/// Tools the app itself answers, because they need the window, the network or the user's key. See the MCP server.
pub fn is_app_tool(name: &str) -> bool {
    is_export(name) || matches!(name, "speech_transcribe" | "frames_render")
}

/// Whether a tool only reads. Takes the catalog name or the dotted name the activity log uses.
pub(crate) fn is_read(tool: &str) -> bool {
    static READS: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    let reads = READS.get_or_init(|| catalog().into_iter().filter(|t| t.read_only).flat_map(|t| [t.name.to_string(), internal(t.name)]).collect());
    reads.contains(tool)
}

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
            "Project name, resolution, frame rate, total length, tracks and counts, the file it is saved in and whether \
             it has unsaved changes, and the user's playhead and selection. Call this first.",
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
            "timeline_locate",
            "Convert between timeline frames and source frames, so you never do it by hand. With at (a timeline \
             frame): every clip under that frame, and for each the source frame of its media shown there. With media \
             and source (a source frame of that file): every place on the timeline where that frame is now, or an \
             empty list when it has been cut out. Clip speed is taken into account.",
            obj(
                json!({
                    "at": frame("Timeline frame to look under"),
                    "media": text("Media id, with source"),
                    "source": frame("Source frame of the media to find on the timeline"),
                }),
                &[],
            ),
            json!({ "at": 600 }),
        ),
        read(
            "view_get",
            "What the user is looking at in the app window: the playhead frame and the selected clips. When the user \
             says 'this clip' or 'here', they usually mean these.",
            obj(json!({}), &[]),
            json!({}),
        ),
        read(
            "captions_read",
            "The caption text on the caption tracks, in time order, with the frames and seconds each one shows. Use it to \
             know what is said or shown at a moment. from and to limit it to a span of frames.",
            obj(
                json!({
                    "from": frame("First frame to include; default the start of the timeline"),
                    "to": frame("Frame the span ends at; default the end of the timeline"),
                }),
                &[],
            ),
            json!({}),
        ),
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
        read(
            "frames_contact_sheet",
            "See a stretch of footage as one picture: up to 48 evenly spaced frames in a grid, each labelled with \
             its time and frame number. With media, from and to are source frames of that file; without it they are \
             timeline frames and the sheet shows the edit as it plays. Start wide (a whole recording in 48 tiles), \
             then ask again for a narrow range to find the exact frame to cut on.",
            obj(
                json!({
                    "media": text("Media id to look at. Omit to look at the timeline."),
                    "from": frame("First frame; default 0"),
                    "to": frame("End frame; default the end of the media or timeline"),
                    "count": { "type": "integer", "minimum": 1, "maximum": 48, "description": "Number of tiles; default 24" },
                    "columns": { "type": "integer", "minimum": 1, "maximum": 12, "description": "Default 6" },
                    "tile_width": { "type": "integer", "minimum": 120, "maximum": 640, "description": "Pixels; default 320. Use 480 or more to read on-screen text." },
                }),
                &[],
            ),
            json!({ "from": 0, "to": 900, "count": 6, "columns": 3 }),
        ),
        read(
            "frames_get",
            "One frame as a picture, large enough to read on-screen text, labelled with its time and frame number. \
             With media, at is a source frame of that file; without it, at is a timeline frame and you get the video \
             clip visible there (without effects, cards or captions; frames_render shows those). The answer names \
             the frame in both timeline and source numbers.",
            obj(
                json!({
                    "media": text("Media id. Omit to look at the timeline."),
                    "at": frame("Frame to show"),
                    "width": { "type": "integer", "minimum": 160, "maximum": 1920, "description": "Pixels; default 960" },
                }),
                &["at"],
            ),
            json!({ "at": 120 }),
        ),
        read(
            "frames_render",
            "One timeline frame as the finished video shows it: every layer, effect, transition, card and caption. Use \
             it to check your own work, for example that a title is readable or an inset sits where you meant. It \
             runs a one-frame export through the app window, so it takes a few seconds and cannot run while an \
             export is running. To look at footage, frames_get and frames_contact_sheet are much faster.",
            obj(
                json!({
                    "at": frame("Timeline frame to render"),
                    "width": { "type": "integer", "minimum": 160, "maximum": 1920, "description": "Pixels; default 960" },
                }),
                &["at"],
            ),
            json!({ "at": 120 }),
        ),
        read(
            "speech_transcribe",
            "What is said in a stretch of a media file: each line of speech with the source frames it is heard at. It \
             sends the sound to the speech model chosen in the app's AI settings, through the user's OpenRouter key, \
             so it costs the user money and takes about a minute for each ten minutes of sound: ask for the stretch \
             you need, at most 30 minutes a call. If the microphone is on its own audio stream, pass audio_stream. \
             Use it to cut between sentences, find a line the user quotes, and write captions.",
            obj(
                json!({
                    "media": text("Media id"),
                    "from": frame("First source frame; default 0"),
                    "to": frame("End source frame; default the end of the file"),
                    "audio_stream": { "type": "integer", "minimum": 0, "description": "Which audio stream of the file, counting from 0; default 0" },
                    "channel": { "type": "integer", "minimum": 0, "description": "One channel of that stream, counting from 0; default all of them mixed" },
                }),
                &["media"],
            ),
            json!({ "media": "media0", "from": 0, "to": 9000 }),
        ),
        read(
            "audio_get_activity",
            "Hear sound in numbers: every silence longer than min_seconds, and the 20 loudest moments (shouts, \
             laughter, explosions) at least 10 seconds apart, with the typical level to compare them against. With \
             media, a file is analysed and times are source frames of it. Without media, the edit is analysed as it \
             now plays on one audio track, and times are timeline frames: use that to check what a pass left behind. \
             Silences are candidates for cutting; loud moments are candidates for the opening. If the microphone was \
             recorded on its own audio stream, pass audio_stream to analyse the voice alone, which is far more useful \
             than voice mixed with game sound. A two-hour file takes around a minute.",
            obj(
                json!({
                    "media": text("Media id. Omit to analyse the timeline."),
                    "track": text("Audio track id, when analysing the timeline; default the first audio track that is not muted"),
                    "from": frame("First frame; default 0"),
                    "to": frame("End frame; default the end of the file or the timeline"),
                    "audio_stream": { "type": "integer", "minimum": 0, "description": "Which audio stream of the file, counting from 0; default 0" },
                    "threshold_db": { "type": "number", "minimum": -90, "maximum": 0, "description": "Quieter than this is silence; default -35" },
                    "min_seconds": { "type": "number", "minimum": 0.1, "maximum": 60, "description": "Shortest silence to report; default 0.5" },
                }),
                &[],
            ),
            json!({ "media": "media0" }),
        ),
        read(
            "export_list_presets",
            "Ready-made export settings (size, frame rate, codec, rate control and bitrates), the encoders this machine \
             has, the ones that do not work here with FFmpeg's reason, and a suggested output path.",
            obj(json!({}), &[]),
            json!({}),
        ),
        read(
            "export_get_status",
            "How the export is doing: state (idle, preparing, rendering, done, failed, cancelled), frames done and \
             total, percent, seconds so far, any error, and warnings about things the file shows differently from \
             the preview.",
            obj(json!({}), &[]),
            json!({}),
        ),
        def(
            "export_start",
            "Render the timeline to a video file, as it looks in the preview: cuts, layers, effects, transitions, \
             cards, captions and sound. Returns at once; follow it with export_get_status. One export runs at a \
             time. A preset fills in whatever you leave out; the size must have the project's shape. The project \
             itself is not changed.",
            obj(
                json!({
                    "path": text("Full path of the file to write, ending in .mp4, .mov or .mkv. Default: the suggested path from export_list_presets."),
                    "preset": text("Preset id from export_list_presets; default project"),
                    "width": { "type": "integer", "minimum": 16 },
                    "height": { "type": "integer", "minimum": 16 },
                    "fps": { "type": "number", "minimum": 1, "maximum": 240 },
                    "quality": { "type": "string", "enum": ["high", "standard", "small"], "description": "Picture quality when rate is quality" },
                    "codec": { "type": "string", "enum": ["h264", "hevc"], "description": "h264 plays everywhere; hevc makes smaller files and is not read by every player" },
                    "rate": { "type": "string", "enum": ["quality", "vbr", "cbr"], "description": "quality: one quality for the whole picture; vbr: a target that rises and falls with the picture, up to max_kbps; cbr: the same bitrate every second" },
                    "bitrate_kbps": { "type": "integer", "minimum": 100, "maximum": 200000, "description": "Target video bitrate in kilobits a second, for vbr and cbr" },
                    "max_kbps": { "type": "integer", "minimum": 100, "maximum": 200000, "description": "Highest bitrate in kilobits a second, for vbr; at least bitrate_kbps" },
                    "two_pass": { "type": "boolean", "description": "Two passes for an even bitrate across the file; vbr on a software encoder only" },
                    "keyframe_frames": { "type": "integer", "minimum": 0, "maximum": 1000, "description": "Frames between keyframes; 0 for half the frame rate, which YouTube asks for" },
                    "b_frames": { "type": "integer", "minimum": 0, "maximum": 4, "description": "Consecutive B-frames; YouTube asks for 2" },
                    "audio_kbps": { "type": "integer", "minimum": 32, "maximum": 512, "description": "Stereo AAC bitrate in kilobits a second; YouTube asks for 384" },
                    "encoder": text("One of the encoders from export_list_presets, for the chosen codec; default the best available"),
                    "range": { "type": "string", "enum": ["whole", "in_out"], "description": "in_out renders only between the in and out points" },
                    "audio": { "type": "string", "enum": ["first", "all"], "description": "all mixes every audio stream of each file, for recordings with the microphone on its own stream; the preview plays only the first" },
                }),
                &[],
            ),
            json!({ "preset": "youtube_1080p60", "path": "C:/Videos/episode-1.mp4" }),
        ),
        def("export_cancel", "Stop the running export and delete what it wrote.", obj(json!({}), &[]), json!({})),
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
        def(
            "project_save",
            "Save the project to its file. Pass path (ending in .snn) the first time, or to save a copy somewhere \
             else; without it the project is saved where it was opened from or last saved. Save before and after a \
             long pass: unsaved work is lost if the app closes.",
            obj(json!({ "path": text("Full path of the project file, ending in .snn") }), &[]),
            json!({}),
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
            "project_set_settings",
            "Change the project's picture size or frame rate. Changing the frame rate keeps every clip at the same              moment, so all frame numbers change: read the timeline again afterwards. Use 60 for gameplay recorded at              60 fps.",
            obj(
                json!({
                    "width": { "type": "integer", "minimum": 16, "description": "Pixels, even" },
                    "height": { "type": "integer", "minimum": 16, "description": "Pixels, even" },
                    "fps": { "type": "number", "minimum": 1, "maximum": 240, "description": "Frames per second, for example 30, 60 or 29.97" },
                }),
                &[],
            ),
            json!({ "fps": 60 }),
        ),
        def(
            "media_import",
            "Add a file to the media pool and return its media id. The file is measured automatically: its kind, \
             length, picture size and whether it has sound. Pass kind and duration only for a file that is not on \
             this computer yet.",
            obj(
                json!({
                    "path": text("Absolute path of the file"),
                    "name": text("Display name; defaults to the file name"),
                    "kind": { "type": "string", "enum": ["video", "audio", "image"] },
                    "duration": frame("Length in frames; 0 for images"),
                    "width": { "type": "integer", "minimum": 0 },
                    "height": { "type": "integer", "minimum": 0 },
                    "has_audio": { "type": "boolean", "description": "Whether the file has a sound track. Measured for MP4, MOV and M4V; pass it for other formats" },
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
                    "link_to": text("Clip id to link the new clip with. Linked clips select and move together in time, for example a video and the sound taken from it"),
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
            "Shift several clips by the same number of frames (negative moves them earlier). Clips keep their tracks \
             unless tracks names a new one for them; the others stay put. Either all of them move or none does. \
             Pass markers true to carry along the markers that sit over the moved clips, for example when shifting \
             the whole body to make room for an intro.",
            obj(
                json!({
                    "clips": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                    "delta": { "type": "integer", "description": "Frames to shift by" },
                    "tracks": { "type": "object", "additionalProperties": { "type": "string" }, "description": "Map from clip id to the track it moves to, for example {\"c4\": \"V2\"}" },
                    "markers": { "type": "boolean", "description": "Also shift the markers between the first moved clip's start and the last one's end; default false" },
                }),
                &["clips", "delta"],
            ),
            json!({ "clips": ["c1", "c2"], "delta": 6000 }),
        ),
        def(
            "timeline_link",
            "Link clips into one group. Linked clips select together, and move together in time when they are \
             moved along the timeline. Linking replaces any links the clips already had.",
            obj(json!({ "clips": { "type": "array", "items": { "type": "string" }, "minItems": 2 } }), &["clips"]),
            json!({ "clips": ["c1", "c2"] }),
        ),
        def(
            "timeline_unlink",
            "Break the links of the groups these clips belong to, so each clip moves on its own.",
            obj(json!({ "clips": { "type": "array", "items": { "type": "string" }, "minItems": 1 } }), &["clips"]),
            json!({ "clips": ["c1"] }),
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
            "timeline_ripple_delete_ranges",
            "Remove several spans of time in one undo step and close every gap. This is the tool for a dead-air pass. \
             Give all the spans as they are now, in any order: you do not have to send them last to first or allow \
             for earlier removals moving later ones, and overlapping spans are joined. With media, the spans are \
             source frames of that file (what audio_get_activity and speech_transcribe report) and whatever part of \
             them is on the timeline is removed, wherever earlier cuts have moved it. Cuts every unlocked track; \
             markers move with the content.",
            obj(
                json!({
                    "ranges": {
                        "type": "array",
                        "minItems": 1,
                        "items": obj(json!({ "from": frame("First frame to remove"), "to": frame("First frame to keep") }), &["from", "to"]),
                    },
                    "media": text("Media id. Give it when the ranges are source frames of that file; omit it for timeline frames."),
                }),
                &["ranges"],
            ),
            json!({ "ranges": [{ "from": 930, "to": 1050 }, { "from": 300, "to": 360 }] }),
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
        if is_app_tool(name) {
            let result = Err("this tool runs in the app window; call it over MCP".to_string());
            self.record(actor, internal(name), args, &result);
            return result;
        }
        if is_analysis(name) {
            // The MCP server runs these itself so the editor is not held while FFmpeg works.
            let result = self.analysis_job(name, &args).and_then(|job| job.run());
            self.log_analysis(actor, name, args, &result);
            return result.map(|a| a.info);
        }
        if tool.read_only {
            let result = self.read_tool(name, &args);
            // Pointing the user at something changes what they are looking at, which the next context block must say.
            if name == "view_set" && result.is_ok() {
                let view = self.view().clone();
                let selected = args["select"].as_array().map(|ids| ids.iter().filter_map(Value::as_str).map(String::from).collect());
                self.set_view(args["frame"].as_i64().unwrap_or(view.playhead), selected.unwrap_or(view.selection));
            }
            self.record(actor, internal(name), args, &result);
            return result;
        }
        match name {
            "project_save" => {
                let path = args["path"].as_str().map(Path::new);
                if path.is_some_and(|p| !p.is_absolute() || p.extension().is_none_or(|e| e != "snn")) {
                    return Err("path must be a full path ending in .snn, such as C:/Videos/episode.snn".into());
                }
                let result = self.save(path).map(|saved| json!({ "saved": saved }));
                self.record(actor, internal(name), args, &result);
                result
            }
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
                    // The MP4 reader needs no program to run; FFmpeg measures everything else.
                    let info = probe_mp4(Path::new(&path)).or_else(|mp4| {
                        crate::analysis::probe_file(&path).map_err(|e| format!("could not measure {path} ({mp4}; {e}); pass kind and duration for a file that is not here yet"))
                    })?;
                    args["kind"] = serde_json::to_value(info.kind).unwrap();
                    args["duration"] = json!(self.project().settings.frames(info.seconds));
                    args["width"] = json!(info.width);
                    args["height"] = json!(info.height);
                    args["has_audio"] = json!(info.has_audio);
                }
                self.execute(actor, to_command(name, args)?)
            }
            _ => self.execute(actor, to_command(name, args)?),
        }
    }

    /// Notes a tool the app answered itself in the activity log.
    pub fn log_tool(&mut self, actor: &str, name: &str, args: Value, result: &Result<Value, String>) {
        self.record(actor, internal(name), args, result);
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
                    "file": self.state().path,
                    "unsaved_changes": self.state().dirty,
                    "playhead": self.view().playhead,
                    "selection": self.view().selection,
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
            "captions_read" => {
                let from = args["from"].as_i64().unwrap_or(0);
                let to = args["to"].as_i64().unwrap_or(Frame::MAX);
                let fps = p.settings.fps_num as f64 / p.settings.fps_den as f64;
                let mut captions: Vec<&Clip> = seq.clips.iter().filter(|c| matches!(c.kind, ClipKind::Caption) && c.end() > from && c.start < to).collect();
                captions.sort_by_key(|c| (c.start, c.id.clone()));
                let captions: Vec<Value> = captions
                    .iter()
                    .map(|c| {
                        json!({
                            "clip": c.id,
                            "track": c.track,
                            "text": c.text.clone().unwrap_or_else(|| c.name.clone()),
                            "start_frame": c.start,
                            "end_frame": c.end(),
                            "start_seconds": c.start as f64 / fps,
                            "end_seconds": c.end() as f64 / fps,
                        })
                    })
                    .collect();
                Ok(json!({ "captions": captions, "fps": fps }))
            }
            // The window applies this; see the MCP server.
            "view_set" => {
                for id in args["select"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    p.clip(id).ok_or_else(|| format!("no clip {id}"))?;
                }
                Ok(json!({ "shown": args }))
            }
            "view_get" => {
                let fps = p.settings.fps_num as f64 / p.settings.fps_den as f64;
                let view = self.view();
                let selected: Vec<&Clip> = view.selection.iter().filter_map(|id| p.clip(id)).collect();
                Ok(json!({
                    "playhead": view.playhead,
                    "playhead_time": crate::analysis::timecode(view.playhead as f64 / fps),
                    "selection": selected.iter().map(|c| json!({ "clip": c.id, "name": c.name, "track": c.track, "start": c.start, "end": c.end() })).collect::<Vec<_>>(),
                }))
            }
            "timeline_locate" => {
                let fps = p.settings.fps_num as f64 / p.settings.fps_den as f64;
                let time = |f: Frame| crate::analysis::timecode(f as f64 / fps);
                match (args["at"].as_i64(), args["media"].as_str(), args["source"].as_i64()) {
                    (Some(at), None, None) => {
                        let mut under: Vec<&Clip> = seq.clips.iter().filter(|c| c.start <= at && at < c.end()).collect();
                        under.sort_by_key(|c| seq.tracks.iter().position(|t| t.id == c.track));
                        let clips: Vec<Value> = under
                            .iter()
                            .map(|c| {
                                let mut found = json!({ "clip": c.id, "track": c.track, "kind": c.kind, "name": c.name, "start": c.start, "end": c.end() });
                                if let Some(m) = c.media.as_ref().and_then(|id| p.media(id)).filter(|m| m.kind != MediaKind::Image) {
                                    let source = c.source_at(at).round() as Frame;
                                    found["media"] = json!(m.id);
                                    found["source_frame"] = json!(source);
                                    found["source_time"] = json!(time(source));
                                }
                                found
                            })
                            .collect();
                        Ok(json!({ "timeline_frame": at, "time": time(at), "clips": clips }))
                    }
                    (None, Some(media), Some(source)) => {
                        let m = p.media(media).ok_or_else(|| format!("no media {media}"))?;
                        if source >= m.duration && m.kind != MediaKind::Image {
                            return Err(format!("{} is only {} frames long", m.name, m.duration));
                        }
                        let places: Vec<Value> = crate::command::source_on_timeline(p, media, source, source + 1)
                            .into_iter()
                            .filter_map(|(id, span)| Some((p.clip(&id)?, span)))
                            .map(|(c, span)| json!({ "clip": c.id, "track": c.track, "kind": c.kind, "timeline_frame": span.from, "time": time(span.from) }))
                            .collect();
                        let mut found = json!({ "media": media, "source_frame": source, "source_time": time(source), "on_timeline": places });
                        if found["on_timeline"].as_array().is_some_and(Vec::is_empty) {
                            found["note"] = "that source frame is not on the timeline: it was cut out, or never added".into();
                        }
                        Ok(found)
                    }
                    _ => Err("give at (a timeline frame), or media and source (a source frame of that file)".into()),
                }
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
