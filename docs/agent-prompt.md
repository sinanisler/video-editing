# Editing brief

The brief that used to be here is now served by the editor itself, so every agent that connects gets it.

- The text is in `guides/*.md`: `retention-edit` (start there), `dead-air`, `cold-open`, `pacing-check` and `channel-atarikafa`.
- Agents read it with the `guide_list` and `guide_get` tools. MCP clients that show prompts list each guide as one.
- Channel-specific material goes in a channel guide, not in the method guides.

To add a built-in guide, write the Markdown file and add it to `src-tauri/crates/snn-core/src/guides.rs`.

Guides are also read from disk on every call, so they can change without a rebuild:

- The user's guides folder (`guides` in the app's data folder; the app shows the path under AI, Connect MCP, Guides). A file named like a built-in guide replaces it; any other name adds a guide.
- In a development build, this repository's `guides/` folder, so an edit here is served at once.

The in-app chat is told to read a guide before an editing job, the same as agents that connect over MCP.
