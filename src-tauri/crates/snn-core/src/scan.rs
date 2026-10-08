use std::path::{Path, PathBuf};

/// The file types the importer takes. Keep in step with EXTENSIONS in src/media.ts.
const SUPPORTED: &[&str] = &[
    "mp4", "mov", "m4v", "mkv", "webm", "mp3", "wav", "m4a", "aac", "ogg", "flac", "png", "jpg", "jpeg", "webp", "gif",
];

/// How deep folders are searched. Guards against runaway trees.
const MAX_DEPTH: usize = 16;

fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SUPPORTED.contains(&e.to_ascii_lowercase().as_str()))
}

/// The files to import for `paths`. A folder contributes its supported files at any depth, in name order.
/// A path that is not a folder is returned as it was given, so the importer can say what it cannot read.
pub fn media_under(paths: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for path in paths {
        // symlink_metadata does not follow links, so a folder link cannot send the search round in a loop.
        let meta = std::fs::symlink_metadata(path).map_err(|e| format!("could not read {path}: {e}"))?;
        if meta.is_dir() {
            walk(Path::new(path), 0, &mut out)?;
        } else {
            out.push(path.clone());
        }
    }
    Ok(out)
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<String>) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let read = std::fs::read_dir(dir).map_err(|e| format!("could not read {}: {e}", dir.display()))?;
    let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for entry in entries {
        let Ok(meta) = std::fs::symlink_metadata(&entry) else { continue };
        if meta.is_dir() {
            // A subfolder we cannot read is skipped; the rest of the import still goes ahead.
            let _ = walk(&entry, depth + 1, out);
        } else if meta.is_file() && supported(&entry) {
            out.push(entry.display().to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder tree under the system temp dir, removed when dropped.
    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str) -> Tree {
            let root = std::env::temp_dir().join(format!("snn-scan-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("sub").join("deep")).unwrap();
            for file in ["a.mp4", "notes.txt", "sub/b.WAV", "sub/deep/c.png", "sub/deep/readme.md"] {
                std::fs::write(root.join(file), b"x").unwrap();
            }
            Tree(root)
        }

        fn path(&self) -> String {
            self.0.display().to_string()
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn names(found: &[String], root: &Path) -> Vec<PathBuf> {
        found.iter().map(|f| Path::new(f).strip_prefix(root).unwrap().to_path_buf()).collect()
    }

    #[test]
    fn a_folder_gives_its_supported_files_at_any_depth_in_name_order() {
        let tree = Tree::new("depth");
        let found = media_under(&[tree.path()]).unwrap();
        // notes.txt and readme.md are not media; WAV counts whatever its case.
        let expected: Vec<PathBuf> = ["a.mp4", "sub/b.WAV", "sub/deep/c.png"].iter().map(|p| p.split('/').collect()).collect();
        assert_eq!(names(&found, &tree.0), expected);
    }

    #[test]
    fn a_file_is_returned_as_given_even_when_it_is_not_media() {
        let tree = Tree::new("file");
        let note = tree.0.join("notes.txt").display().to_string();
        assert_eq!(media_under(std::slice::from_ref(&note)).unwrap(), [note]);
    }

    #[test]
    fn several_folders_and_files_come_back_in_the_order_given() {
        let tree = Tree::new("several");
        let a = tree.0.join("a.mp4").display().to_string();
        let found = media_under(&[tree.0.join("sub").display().to_string(), a.clone()]).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(found.last(), Some(&a));
    }

    #[test]
    fn a_missing_path_is_reported() {
        let missing = std::env::temp_dir().join("snn-scan-does-not-exist").display().to_string();
        assert!(media_under(&[missing]).unwrap_err().contains("could not read"));
    }
}
