//! Editing methods an agent can follow: what to cut, how to build an opening, how to check pace.
//! The editor's tools say what is possible; a guide says what makes a good edit for one purpose.
//!
//! The text lives in `guides/*.md` at the repository root and is compiled in. Agents reach it
//! through the `guide_list` and `guide_get` tools, and MCP clients that show prompts list each
//! guide as one.
//!
//! A guide can also be a Markdown file in a guide folder, which the app names with
//! [`set_guide_folders`]. A file called `dead-air.md` there replaces the built-in guide of that name,
//! and any other name adds a guide. The folders are read on every call, so a correction to a guide
//! reaches agents at once, without building the app again.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Clone, Serialize)]
pub struct Guide {
    pub name: String,
    pub title: String,
    /// When to use it. Shown in listings, so an agent can pick without reading every guide.
    pub description: String,
    /// `built-in`, or the file the text was read from.
    pub source: String,
    #[serde(skip)]
    pub text: String,
}

/// A guide file larger than this is not a guide.
const MAX_GUIDE_BYTES: u64 = 200 * 1024;

static FOLDERS: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// Where guide files are looked for, first folder first. A folder that does not exist is skipped.
pub fn set_guide_folders(folders: Vec<PathBuf>) {
    *FOLDERS.write().unwrap() = folders;
}

fn title_of(name: &str, text: &str) -> String {
    text.lines().next().unwrap_or(name).trim_start_matches('#').trim().to_string()
}

fn built_in(name: &str, description: &str, text: &str) -> Guide {
    Guide { name: name.into(), title: title_of(name, text), description: description.into(), source: "built-in".into(), text: text.into() }
}

fn built_ins() -> Vec<Guide> {
    vec![
        built_in(
            "retention-edit",
            "Start here to turn a raw recording into a finished YouTube video. The whole method, the order of passes, and the rules for using the tools safely.",
            include_str!("../../../../guides/retention-edit.md"),
        ),
        built_in(
            "dead-air",
            "Removing silence, walking, waiting and repeated attempts, and what must never be cut.",
            include_str!("../../../../guides/dead-air.md"),
        ),
        built_in(
            "cold-open",
            "Building the first 90 seconds from the best moments of the recording: promise, teaser, subscribe card, first loop.",
            include_str!("../../../../guides/cold-open.md"),
        ),
        built_in(
            "pacing-check",
            "Measuring a finished edit against target pace with timeline_get_pace, and what to fix.",
            include_str!("../../../../guides/pacing-check.md"),
        ),
        built_in(
            "channel-atarikafa",
            "Specifics of the atarikafa channel and its ZindanCraft series: no face camera, Turkish captions, the mod's ready-made moments.",
            include_str!("../../../../guides/channel-atarikafa.md"),
        ),
    ]
}

/// The first paragraph under the title, which is what a guide says about itself.
fn first_paragraph(text: &str) -> String {
    let body = text.lines().skip(1).skip_while(|l| l.trim().is_empty());
    let paragraph: Vec<&str> = body.take_while(|l| !l.trim().is_empty() && !l.starts_with('#')).map(str::trim).collect();
    paragraph.join(" ").chars().take(300).collect()
}

/// The guide files in the folders: name (the file name without `.md`), path and text. An earlier folder wins.
fn files(folders: &[PathBuf]) -> Vec<(String, PathBuf, String)> {
    let mut found: Vec<(String, PathBuf, String)> = vec![];
    for folder in folders {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(folder).into_iter().flatten().flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let Some(name) = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()) else { continue };
            let markdown = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
            // Names are tool arguments, so they stay plain; a README beside the guides is not one of them.
            let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_GUIDE_BYTES);
            if !markdown || !plain || name == "readme" || !small || found.iter().any(|(n, _, _)| *n == name) {
                continue;
            }
            if let Some(text) = std::fs::read_to_string(&path).ok().filter(|t| !t.trim().is_empty()) {
                found.push((name, path, text.trim_start_matches('\u{feff}').replace("\r\n", "\n")));
            }
        }
    }
    found
}

/// Every guide: the built-in ones, each replaced by a file of the same name if there is one, then the files that
/// add a guide of their own.
pub fn guides() -> Vec<Guide> {
    guides_in(&FOLDERS.read().unwrap())
}

fn guides_in(folders: &[PathBuf]) -> Vec<Guide> {
    let mut all = built_ins();
    for (name, path, text) in files(folders) {
        let source = path.display().to_string();
        match all.iter_mut().find(|g| g.name == name) {
            // The description says when to use the guide, which a rewrite of its text does not change.
            Some(g) => (g.title, g.source, g.text) = (title_of(&name, &text), source, text),
            None => {
                let description = Some(first_paragraph(&text)).filter(|d| !d.is_empty()).unwrap_or_else(|| "A guide added on this computer.".into());
                all.push(Guide { title: title_of(&name, &text), name, description, source, text });
            }
        }
    }
    all
}

pub fn find_guide(name: &str) -> Result<Guide, String> {
    let all = guides();
    let names = all.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", ");
    all.into_iter().find(|g| g.name == name).ok_or_else(|| format!("no guide {name}; available: {names}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_replaces_a_built_in_guide_or_adds_one_and_is_read_fresh_each_time() {
        let dir = std::env::temp_dir().join(format!("snn-guides-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The folders are passed in here: the ones the app sets are shared by every test in the process.
        let folders = [dir.join("missing"), dir.clone()];
        let find = |name: &str| guides_in(&folders).into_iter().find(|g| g.name == name);
        let shipped = find("dead-air").unwrap();
        assert_eq!(shipped.source, "built-in");
        assert_eq!(guides_in(&folders).len(), built_ins().len(), "an empty folder changes nothing");

        std::fs::write(dir.join("dead-air.md"), "\u{feff}# Dead air, my way\r\n\r\nKeep every pause.\r\n").unwrap();
        std::fs::write(dir.join("shorts.md"), "# Shorts\n\nCutting a vertical short from a long video.\nUnder a minute.\n\n## Steps\n\n1. Pick one moment.\n").unwrap();
        std::fs::write(dir.join("README.md"), "# Notes\n\nnot a guide\n").unwrap();
        std::fs::write(dir.join("Bad Name.md"), "# x\n\ny\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "# x\n").unwrap();

        let mine = find("dead-air").unwrap();
        assert_eq!((mine.title.as_str(), mine.text.as_str()), ("Dead air, my way", "# Dead air, my way\n\nKeep every pause.\n"));
        assert_eq!(mine.description, shipped.description);
        assert!(mine.source.ends_with("dead-air.md"));
        let added = find("shorts").unwrap();
        assert_eq!((added.title.as_str(), added.description.as_str()), ("Shorts", "Cutting a vertical short from a long video. Under a minute."));
        assert_eq!(guides_in(&folders).len(), built_ins().len() + 1, "the README, the badly named file and the text file are not guides");

        // An edit is served on the next call, and removing the file brings the built-in guide back.
        std::fs::write(dir.join("dead-air.md"), "# Dead air, again\n\nCut them all.\n").unwrap();
        assert_eq!(find("dead-air").unwrap().title, "Dead air, again");
        std::fs::remove_file(dir.join("dead-air.md")).unwrap();
        assert_eq!(find("dead-air").unwrap().text, shipped.text);
        std::fs::remove_dir_all(&dir).ok();
    }
}
