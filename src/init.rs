use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::vault::{INDEX_FILE, PREFERENCES, PROJECTS, Vault, write_atomic};
use crate::{git, index};

pub const GITIGNORE: &str = ".gitignore";

// Obsidian rewrites its workspace files constantly; only the notes belong in the history.
const IGNORED: [&str; 4] = [".obsidian/", ".trash/", ".DS_Store", ".petit-poucet/"];

pub fn ensure_repo(root: &Path) -> Result<(), String> {
    if !git::is_repo(root) {
        git::init(root)?;
    }
    let gitignore = root.join(GITIGNORE);
    let mut text = fs::read_to_string(&gitignore).unwrap_or_default();
    let missing: Vec<&str> = IGNORED
        .into_iter()
        .filter(|entry| !text.lines().any(|line| line == *entry))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for entry in missing {
        text.push_str(entry);
        text.push('\n');
    }
    write_atomic(&gitignore, &text)
}

const DEFAULT_VAULT: &str = "agent-memory";

pub fn init(path: Option<PathBuf>) -> Result<String, String> {
    let path = match path {
        Some(path) => path,
        None => std::env::home_dir()
            .ok_or("cannot find the home folder")?
            .join(DEFAULT_VAULT),
    };
    for dir in [PREFERENCES, PROJECTS] {
        fs::create_dir_all(path.join(dir)).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let root = path.canonicalize().map_err(|e| e.to_string())?;

    let config_path = Config::path()?;
    let config = match fs::read_to_string(&config_path) {
        Ok(_) => {
            let existing = Config::load()?;
            if existing.vault != root {
                return Err(format!(
                    "{} already points to {}; edit it to switch vaults",
                    config_path.display(),
                    existing.vault.display()
                ));
            }
            existing
        }
        Err(_) => {
            let config = Config::new(root.clone());
            config.save()?;
            config
        }
    };

    ensure_repo(&root)?;
    let vault = Vault::load(&root)?;
    write_atomic(&root.join(INDEX_FILE), &index::generate(&vault))?;
    if config.git_autocommit {
        git::commit(&root, &[INDEX_FILE, GITIGNORE], "init: vault")?;
    }
    Ok(format!(
        "vault ready at {} ({} notes)\nto use another folder, change `vault` in {}",
        root.display(),
        vault.notes.len(),
        config_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_repo_adds_only_the_missing_ignore_lines() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(GITIGNORE), "custom\n.obsidian/").unwrap();
        ensure_repo(tmp.path()).unwrap();
        ensure_repo(tmp.path()).unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join(GITIGNORE)).unwrap(),
            "custom\n.obsidian/\n.trash/\n.DS_Store\n.petit-poucet/\n"
        );
        assert!(git::is_repo(tmp.path()));
    }
}
