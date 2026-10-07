# snn-editor — Plan

A desktop video editor with a simplified Premiere/Resolve-style GUI where **every editing action is also an MCP tool**, so connected agents can do anything a user can.

Status: milestone 1 (skeleton) is done, plus early parts of 2 and 4: media import with an interim player, and the MCP server. Next: FFmpeg sidecar, proxies and the WebCodecs compositor. The approved UX mockup is `docs/mockup.html`.

## Running it

```
npm install
npm run tauri dev     # the app, with hot reload
npm run test:core     # unit tests for the Rust command layer
```

Needs Node, Rust (rustup) and the Microsoft C++ Build Tools.

## Where things are

| Path | What |
|---|---|
| `src-tauri/crates/snn-core` | Project model, command layer, undo history, activity log. No Tauri dependency. |
| `src-tauri/src/lib.rs` | Tauri shell: holds the one `Editor`, forwards commands, pushes state to the webview. |
| `src/` | React interface. `store.ts` talks to the core, `commands.ts` holds shortcuts and palette entries. |

Choices made in milestone 1 that differ from or add to the plan below:

- The MCP server is hand-written on `tiny_http` (`src-tauri/src/mcp.rs`) instead of `rmcp`: JSON responses to POSTs on `http://127.0.0.1:7420/mcp`, no server stream. Tools and schemas live in `snn-core/src/tools.rs`; a test runs every tool. MCP tool names use underscores (`timeline_split_clip`) because clients reject dots.
- Connect Claude Code with `claude mcp add --transport http snn-editor http://127.0.0.1:7420/mcp`.
- Media is measured by the webview on import (or by a small MP4 header reader for agents) and played with one media element per clip. Cuts can hitch; video clips carry their own sound. Both go away with the compositor.
- Not exposed to agents yet: frames and contact sheets, transcripts, pace and silence analysis, export, permission modes.

- The timeline is drawn with DOM elements for now. Move it to canvas when clip counts make it slow.
- A clip may not overlap another on the same track; the core rejects the command. Overwrite and insert edits come in milestone 3.
- Undo stores a full project snapshot per step (capped at 200). Fine at current sizes.
- Clip properties, effects and transitions are stored and editable, but nothing renders them until the compositor exists.

## 1. Goals

- Edit mostly MP4 footage on a multi-track timeline: cut, trim, ripple, move, transitions, effects, text, audio.
- Familiar keyboard shortcuts (Premiere and Resolve presets, rebindable).
- An MCP server that exposes the whole editor to external agents.
- Context awareness for agents: transcript with word timestamps, pacing, silence, frames on demand.
- Subtitle generation through OpenRouter, one click after import.
- First-class support for the YouTube gameplay format in the research notes: word-by-word captions, title cards, subscribe cards, counters, greyscale picture-in-picture, zoom punches, sound effects on cuts.

## 2. Decisions

| Area | Choice | Notes |
|---|---|---|
| Shell | Tauri 2 | Windows first (WebView2). macOS and Linux later. |
| Core | Rust | Owns the project model, command layer, undo history, MCP server, job queue. |
| UI | React + TypeScript | Custom canvas-drawn timeline. |
| Preview and export engine | WebCodecs + WebGPU in the webview (route A) | Mediabunny for MP4 read/write. Kept behind a `Compositor` interface so a native Rust + wgpu engine (route B) can replace it. |
| FFmpeg | Sidecar binary, LGPL build | Proxies, audio extraction, frame grabs, scene and silence detection, unusual codecs. Hardware H.264 encode, no x264. |
| MCP | `rmcp` embedded in the Tauri process | Streamable HTTP on localhost, plus a stdio bridge binary. |
| Transcription | OpenRouter `/api/v1/audio/transcriptions` | `verbose_json` with word timestamps. Model chosen after a Turkish test. |
| Project file | Versioned JSON | OpenTimelineIO export later. |

Assumptions made because they were not answered yet (change freely):

- Agents are external (Claude Code and similar) and connect over MCP. The app shows an Agent panel with connection status, permissions and an activity log, but has no built-in chat in v1.
- Transcription is OpenRouter only in v1. A local whisper.cpp fallback is a later option.

## 3. Architecture

```
┌───────────────────────── Tauri app ─────────────────────────┐
│  Webview (React)                 Rust core                  │
│  ├ Timeline UI                   ├ Project model (source of │
│  ├ Panels (media, inspector,     │   truth) + undo history  │
│  │   transcript, agent)          ├ Command layer  ◄─────────┼── MCP server (rmcp)
│  └ Compositor (WebCodecs +       ├ Analysis cache           │     ▲
│      WebGPU): preview + export   ├ Job queue                │     │ HTTP / stdio
│            ▲                     └ FFmpeg sidecar, OpenRouter│   Agents
│            └── state events ◄────────────┘                  │
└─────────────────────────────────────────────────────────────┘
```

Rules:

1. **One command layer.** GUI actions and MCP tools call the same Rust commands. Nothing mutates the project any other way.
2. **Every command is undoable** and recorded with its actor (user or agent name). Agent edits can be undone individually or as a group.
3. **The webview is a view.** It renders state events from the core and sends commands back.
4. **One renderer.** Export uses the same compositor as preview, so what you see is what you get.

## 4. Project model (sketch)

- `Project`: settings (resolution, fps), media, sequences.
- `MediaItem`: path, probe data, proxy path, transcript id, analysis id.
- `Sequence`: ordered tracks, markers, in/out range.
- `Track`: kind (video, audio, caption), locked, muted, hidden.
- `Clip`: media ref or generator (text, graphic), timeline start, duration, source in-point, speed, properties, keyframes, effects, transition in/out, link group.
- `Transcript`: segments → words with start/end, language, model.
- `Analysis`: scene cuts, silence ranges, loudness curve.

Time is stored as rational frame counts, never floats.

## 5. MCP tool catalog (draft)

Around 50 tools in ten groups, plus a batch tool. Names are `group.verb_noun`.

| Group | Tools |
|---|---|
| `project` | get_info, save, list_sequences, set_settings |
| `media` | import, list, get_info, remove, make_proxy |
| `timeline` (read) | get_state, get_clips_in_range, get_clip, list_markers |
| `timeline` (edit) | add_clip, move_clip, trim_clip, split_clip, delete_clip, ripple_delete_clip, ripple_delete_range, set_speed, link, unlink, add_track, set_track_state, add_marker, remove_marker, set_range |
| `clip` | set_property, set_keyframe, remove_keyframe, add_effect, remove_effect, set_transition |
| `text` | add_title, add_template (subscribe card, counter, chapter card, lower third), set_caption_style, edit_caption |
| `audio` | set_volume, set_fade, duck_under_voice, add_sfx |
| `analysis` | get_transcript, search_transcript, get_pace, get_silences, get_loudness, detect_scenes |
| `frames` | get_frame, get_contact_sheet, get_preview_frame (composited timeline frame) |
| `transcription` | generate, get_status |
| `export` | start, get_status, list_presets |
| `history` | undo, redo, list, undo_group |
| `batch` | apply (list of commands, atomic) |

Design points:

- Read tools return compact JSON with a `range` filter so agents don't pull the whole timeline each time.
- Frame tools return images at a requested size; contact sheets give many frames in one image.
- A permission mode in the app: read-only, ask before destructive edits, or auto-apply.
- MCP resources mirror the read tools (`timeline://current`, `transcript://{media}`) for clients that support them.

## 6. Agent context pipeline

Runs as background jobs when media is imported (transcription on click):

1. Probe with ffprobe.
2. Build a proxy if the source is heavy.
3. Extract mono 16 kHz audio, chunk if needed, send to OpenRouter, store word timestamps.
4. Scene detection (threshold 0.25, matching the research notes) → cuts per minute.
5. Silence detection and loudness curve.
6. Cache everything next to the project.

The timeline shows this as two lanes (pace, silence) and the Transcript panel.

## 7. GUI scope

Shown in `docs/mockup.html`:

- Media bin with tabs: Media, Text, Transitions, Effects, Audio.
- Preview monitor with transport, timecode and JKL playback.
- Inspector: transform, opacity, speed, crop, text, audio, effects, transitions, keyframe toggles.
- Transcript panel: generate, click a word to seek, silence rows with one-click cut.
- Agent panel: MCP endpoint, clients, permission mode, live activity log.
- Timeline: tools (select, razor), snapping, markers, in/out, zoom, track lock/mute/hide, move and trim by drag, pace and silence lanes.
- Command palette, shortcuts overlay, export dialog, status bar with jobs.

## 8. Milestones

| # | Milestone | Done when |
|---|---|---|
| 0 | UX mockup | Done. `docs/mockup.html` approved. |
| 1 | Skeleton | Done. Tauri 2 app, Rust project model, command layer with undo, React shell with the approved layout. |
| 2 | Media and playback | Import MP4, probe, proxy, single-clip preview through the compositor, frame-accurate seek. |
| 3 | Timeline editing | Multi-track, move, trim, split, ripple, snapping, markers, shortcuts, save/load. |
| 4 | MCP server | All commands from milestones 1–3 exposed, permission modes, activity log, tested from Claude Code. |
| 5 | Context | Transcription via OpenRouter, scene/silence analysis, frame and contact-sheet tools, transcript panel. |
| 6 | Looks | Transitions, effects, keyframes, text and caption styles, templates. |
| 7 | Audio | Volume, fades, ducking, sound effects, waveforms. |
| 8 | Export | Presets (YouTube 1080p, Shorts 9:16), burned-in captions, progress, cancel. |
| 9 | Polish | Rebindable shortcuts, autosave and recovery, installer. |

MCP lands at milestone 4 on purpose: early enough that every later feature ships with its tool.

## 9. Risks

| Risk | Mitigation |
|---|---|
| WebView2 decode or WebGPU limits on long 4K footage | Proxies by default; compositor interface allows the native engine later. |
| Preview and export drift | Single compositor for both. |
| Audio sync in a WebCodecs pipeline | Audio clock drives video; test early in milestone 2. |
| Too many MCP tools eat agent context | Grouped tools, range filters, batch tool. |
| Transcription quality in Turkish | Compare two or three OpenRouter models on real footage before fixing a default. |
| FFmpeg licensing | LGPL build, hardware encoders, sidecar process. |

## 10. Open questions

1. Built-in chat agent, or external agents only?
2. Local transcription fallback?
3. macOS and Linux timing?
