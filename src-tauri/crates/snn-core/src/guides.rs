//! Editing methods an agent can follow: what to cut, how to build an opening, how to check pace.
//! The editor's tools say what is possible; a guide says what makes a good edit for one purpose.
//!
//! The text lives in `guides/*.md` at the repository root and is compiled in. Agents reach it
//! through the `guide_list` and `guide_get` tools, and MCP clients that show prompts list each
//! guide as one.

use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct Guide {
    pub name: &'static str,
    pub title: &'static str,
    /// When to use it. Shown in listings, so an agent can pick without reading every guide.
    pub description: &'static str,
    #[serde(skip)]
    pub text: &'static str,
}

fn guide(name: &'static str, description: &'static str, text: &'static str) -> Guide {
    let title = text.lines().next().unwrap_or(name).trim_start_matches('#').trim();
    Guide { name, title, description, text }
}

pub fn guides() -> Vec<Guide> {
    vec![
        guide(
            "retention-edit",
            "Start here to turn a raw recording into a finished YouTube video. The whole method, the order of passes, and the rules for using the tools safely.",
            include_str!("../../../../guides/retention-edit.md"),
        ),
        guide(
            "dead-air",
            "Removing silence, walking, waiting and repeated attempts, and what must never be cut.",
            include_str!("../../../../guides/dead-air.md"),
        ),
        guide(
            "cold-open",
            "Building the first 90 seconds from the best moments of the recording: promise, teaser, subscribe card, first loop.",
            include_str!("../../../../guides/cold-open.md"),
        ),
        guide(
            "pacing-check",
            "Measuring a finished edit against target pace with timeline_get_pace, and what to fix.",
            include_str!("../../../../guides/pacing-check.md"),
        ),
        guide(
            "channel-atarikafa",
            "Specifics of the atarikafa channel and its ZindanCraft series: no face camera, Turkish captions, the mod's ready-made moments.",
            include_str!("../../../../guides/channel-atarikafa.md"),
        ),
    ]
}

pub fn find_guide(name: &str) -> Result<Guide, String> {
    let all = guides();
    let names = all.iter().map(|g| g.name).collect::<Vec<_>>().join(", ");
    all.into_iter().find(|g| g.name == name).ok_or_else(|| format!("no guide {name}; available: {names}"))
}
