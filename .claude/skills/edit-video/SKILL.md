---
name: edit-video
description: Edit a video in snn-editor through its MCP tools. Use when the user asks to edit, cut, tighten or pace a recording, build an intro or cold open, remove dead air, or check an edit against the YouTube retention research.
---

# Editing a video in snn-editor

The method lives in the editor, not here, so every agent gets the same one. This skill only says how to reach it and what is specific to working from Claude Code.

1. The editor must be running (`npm run tauri dev`) and the `snn-editor` MCP server connected. If its tools are missing, tell the user; do not edit project files by hand.
2. Call `guide_list`, then `guide_get` for `retention-edit`. Follow it, and read the guides it names (`dead-air`, `cold-open`, `pacing-check`) when you reach each pass. Read `channel-atarikafa` for this channel's videos.
3. The user works in the same open project. Before the first edit, say what you are about to do and ask them to save. Edit only what they asked for.
4. You cannot see or hear the footage. Work from the user's list of moments or their markers, as the guide says, and ask when there is none.

The guides are `guides/*.md` in this repository. When the user corrects how you edit, change the guide so the correction lasts, and tell them the app has to restart to serve it.

The research behind the numbers is in `SKILLS/` (first 90 seconds, first 5 minutes, and the ZindanCraft shooting plan). Read it when a guide does not cover a case.

To check the MCP server itself, run `node dev/mcp-smoke.mjs`. It only runs on an empty project and undoes everything it does.
