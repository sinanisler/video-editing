---
name: edit-video
description: Edit a video in snn-editor through its MCP tools. Use when the user asks to edit, cut, tighten or pace a recording, build an intro or cold open, remove dead air, or check an edit against the YouTube retention research.
---

# Editing a video in snn-editor

The method lives in the editor, not here, so every agent gets the same one. This skill only says how to reach it and what is specific to working from Claude Code.

1. The editor must be running (`npm run tauri dev`) and the `snn-editor` MCP server connected. If its tools are missing, tell the user; do not edit project files by hand.
2. Call `guide_list`, then `guide_get` for `retention-edit`. Follow it, and read the guides it names (`dead-air`, `cold-open`, `pacing-check`) when you reach each pass. Read `channel-atarikafa` for this channel's videos.
3. The user works in the same open project. Before the first edit, say what you are about to do and save with `project_save` (ask where, if the project has no file yet). Edit only what they asked for. Every tool result ends with a `context` block: the user's playhead and selection, and what they changed since your last call. Read it.
4. Look before you cut. `frames_contact_sheet`, `frames_get` and `audio_get_activity` show you the footage, and `frames_render` shows a finished frame with effects, cards and captions; the guide says how to build an inventory with them. `speech_transcribe` returns what is said, but it spends the user's OpenRouter credit, so transcribe the stretches you need and ask before doing a whole recording.
5. Never convert between source and timeline frames yourself. `timeline_locate` does it, and `timeline_ripple_delete_ranges` takes source frames directly.

The guides are `guides/*.md` in this repository. When the user corrects how you edit, change the guide so the correction lasts. A development build reads these files from disk on every call, so the change is served at once; an installed build needs a rebuild, or the same file placed in the user's guides folder (shown in the app under AI, Connect MCP, Guides).

The research behind the numbers is in `SKILLS/` (first 90 seconds, first 5 minutes, and the ZindanCraft shooting plan). Read it when a guide does not cover a case.

When the user wants the video file, export it with `export_start` as the guide describes; the export dialog in the app (Ctrl+E) does the same thing.

To check the MCP server itself, run `node dev/mcp-smoke.mjs`. It only runs on an empty project and undoes everything it does.
