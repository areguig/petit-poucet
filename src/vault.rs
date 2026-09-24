use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::note::Note;
use crate::project::{IDENTITY_FILE, Identity, Project};

pub const INDEX_FILE: &str = "Index.md";
pub const PREFERENCES: &str = "Preferences";
pub const PROJECTS: &str = "Projects";
pub const TOPICS: &str = "Topics";

pub struct Vault {
    pub root: PathBuf,
    pub notes: Vec<Note>,
    pub projects: Vec<Project>,
    pub stray: Vec<String>,
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
        };
        let entries = WalkDir::new(root)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'));
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let rel = entry.path().strip_prefix(root).unwrap();
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
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
            let Some(stem) = rel
                .to_string_lossy()
                .strip_suffix(".md")
                .map(str::to_string)
            else {
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
                _ => vault.stray.push(rel.to_string_lossy().into_owned()),
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
    tmp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
