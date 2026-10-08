# Editing brief

The brief that used to be here is now served by the editor itself, so every agent that connects gets it.

- The text is in `guides/*.md`: `retention-edit` (start there), `dead-air`, `cold-open`, `pacing-check` and `channel-atarikafa`.
- Agents read it with the `guide_list` and `guide_get` tools. MCP clients that show prompts list each guide as one.
- Channel-specific material goes in a channel guide, not in the method guides.

To add a guide, write the Markdown file and add it to `src-tauri/crates/snn-core/src/guides.rs`.
