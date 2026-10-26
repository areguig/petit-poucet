use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::note::Note;
use crate::project::{IDENTITY_FILE, Identity, Project};
use crate::sync_conflict::{self, Conflict};

pub const INDEX_FILE: &str = "Index.md";
pub const PREFERENCES: &str = "Preferences";
pub const PROJECTS: &str = "Projects";
pub const TOPICS: &str = "Topics";

pub struct Vault {
    pub root: PathBuf,
    pub notes: Vec<Note>,
    pub projects: Vec<Project>,
    pub stray: Vec<String>,
    pub conflicts: Vec<Conflict>,
}

impl Vault {
    pub fn load(root: &Path) -> Result<Vault, String> {
        if !root.is_dir() {
            return Err(format!("vault not found: {}", root.display()));
        }
        let mut vault = Vault {
            root: root.to_path_buf(),
            notes: Vec::new(),
            projects: Vec::new(),
            stray: Vec::new(),
            conflicts: Vec::new(),
        };
        let entries = WalkDir::new(root)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                // Hidden, except a conflict copy of a synced root file such as `.last-cleanup`.
                !name.starts_with('.')
                    || e.depth() == 1
                        && e.file_type().is_file()
                        && sync_conflict::original_of(&name).is_some()
            });
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let rel = entry.path().strip_prefix(root).unwrap();
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            // Note paths use '/' on every OS: they are keys in links, the index and MCP calls.
            let rel = parts.join("/");
            let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
            if entry.file_type().is_dir() {
                if let [PROJECTS, key] = parts[..] {
                    let identity = read(&entry.path().join(IDENTITY_FILE))
                        .map_err(|_| "missing".to_string())
                        .and_then(|text| Identity::parse(&text));
                    vault.projects.push(Project {
                        key: key.to_string(),
                        identity,
                    });
                }
                continue;
            }
            if let Some(original) = sync_conflict::original_of(&rel) {
                vault.conflicts.push(Conflict {
                    copy: rel,
                    original,
                });
                continue;
            }
            let Some(stem) = rel.strip_suffix(".md").map(str::to_string) else {
                continue;
            };
            match parts[..] {
                [INDEX_FILE] | [PROJECTS, _, IDENTITY_FILE] => {}
                [PREFERENCES, _] | [PROJECTS, _, _] | [TOPICS, _, _] => {
                    let note = match read(entry.path()) {
                        Ok(text) => Note::parse(stem, &text),
                        Err(e) => Note {
                            path: stem,
                            frontmatter: Err(e),
                            body: String::new(),
                        },
                    };
                    vault.notes.push(note);
                }
                _ => vault.stray.push(rel),
            }
        }
        Ok(vault)
    }

    pub fn has_note(&self, path: &str) -> bool {
        self.notes.iter().any(|n| n.path == path)
    }
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read: {e}"))
}

// Temp file + rename, so Obsidian or a reader never sees a half-written file.
// A note path's folder: `Preferences`, `Projects/<key>` or `Topics/<topic>`.
pub fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

pub fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("no parent folder")?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;
    // Temp files are created 0600; notes should stay readable like any other file.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o644))
            .map_err(|e| e.to_string())?;
    }
    // Not `persist`: on Windows it can't replace a file another process is reading, and `fs::rename` can.
    let tmp = tmp.into_temp_path();
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_conflict_copies_are_listed_never_loaded() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mark = ".sync-conflict-20261026-101112-ABCDEF7";
        let copies = [
            format!("Preferences/a{mark}.md"),
            format!("Projects/x/a{mark}.md"),
            format!("Projects/x/_project{mark}.md"),
            format!("Topics/t/a{mark}.md"),
            format!("{mark}.last-cleanup"),
        ];
        for file in copies.iter().map(String::as_str).chain([
            "Preferences/a.md",
            "Projects/x/_project.md",
            ".git/HEAD.sync-conflict-20261026-101112-ABCDEF7",
            ".obsidian/workspace.sync-conflict-20261026-101112-ABCDEF7.json",
        ]) {
            fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
            fs::write(root.join(file), "x").unwrap();
        }

        let vault = Vault::load(root).unwrap();
        let notes: Vec<&str> = vault.notes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(notes, ["Preferences/a"]);
        assert!(vault.stray.is_empty(), "{:?}", vault.stray);
        let mut found: Vec<(&str, &str)> = vault
            .conflicts
            .iter()
            .map(|c| (c.copy.as_str(), c.original.as_str()))
            .collect();
        found.sort();
        let mut expected: Vec<(&str, String)> = copies
            .iter()
            .map(|c| (c.as_str(), c.replace(mark, "")))
            .collect();
        expected.sort();
        let expected: Vec<(&str, &str)> = expected.iter().map(|(c, o)| (*c, o.as_str())).collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn write_atomic_replaces_content_and_keeps_the_file_readable() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(&path, "old").unwrap();
        write_atomic(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o644
            );
        }
        assert_eq!(
            fs::read_dir(tmp.path()).unwrap().count(),
            1,
            "no temp file left"
        );
    }

    // Windows refused to replace a file another process was reading: a session start reading the latest release lost the check's write.
    #[test]
    fn write_atomic_replaces_a_file_another_reader_holds_open() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("latest-release");
        fs::write(&path, "0.3.0").unwrap();
        let reader = fs::File::open(&path).unwrap();
        write_atomic(&path, "0.4.0").unwrap();
        drop(reader);
        assert_eq!(fs::read_to_string(&path).unwrap(), "0.4.0");
    }
}
