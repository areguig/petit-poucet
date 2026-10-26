use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::vault::{INDEX_FILE, PREFERENCES, PROJECTS, Vault, write_atomic};
use crate::{git, index};

pub const GITIGNORE: &str = ".gitignore";

// Obsidian rewrites its workspace files constantly; only the notes belong in the history.
const IGNORED: [&str; 5] = [
    ".obsidian/",
    ".trash/",
    ".DS_Store",
    ".petit-poucet/",
    "*.sync-conflict-*",
];

pub fn ensure_repo(root: &Path) -> Result<(), String> {
    if !git::is_repo(root) {
        git::init(root)?;
    }
    add_missing_lines(&root.join(GITIGNORE), &IGNORED)
}

// Syncthing never syncs `.stignore`, so every machine writes its own, even before Syncthing is installed.
// Git history, the generated Index and machine state stay per machine; `.usage` and `.last-cleanup` must sync.
pub const STIGNORE: &str = ".stignore";
const NOT_SYNCED: [&str; 5] = [
    "/.git",
    "/.petit-poucet",
    "/Index.md",
    "/.obsidian/workspace*.json",
    "(?d).DS_Store",
];

pub fn ensure_vault_files(root: &Path) -> Result<(), String> {
    ensure_repo(root)?;
    add_missing_lines(&root.join(STIGNORE), &NOT_SYNCED)
}

// The user's own lines are kept; each entry is added once.
fn add_missing_lines(file: &Path, entries: &[&str]) -> Result<(), String> {
    let mut text = fs::read_to_string(file).unwrap_or_default();
    let missing: Vec<&str> = entries
        .iter()
        .copied()
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
    write_atomic(file, &text)
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
    // Not `canonicalize`: on Windows its `\\?\` paths would end up in the config.
    let root = dunce::canonicalize(&path).map_err(|e| e.to_string())?;

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

    ensure_vault_files(&root)?;
    let vault = Vault::load(&root)?;
    write_atomic(&root.join(INDEX_FILE), &index::generate(&vault))?;
    // The vault works without its first commit, so a refused one (no git identity) only warns.
    let committed = match config.git_autocommit {
        true => git::commit(&root, &[INDEX_FILE, GITIGNORE, STIGNORE], "init: vault").err(),
        false => None,
    };
    let mut reply = format!(
        "vault ready at {} ({} notes)\nto use another folder, change `vault` in {}",
        root.display(),
        vault.notes.len(),
        config_path.display()
    );
    if let Some(e) = committed {
        reply.push_str(&format!(
            "\nnot committed: {e}\nset your git identity (`git config --global user.name …`, `git config --global user.email …`) so the vault keeps its history"
        ));
    }
    Ok(reply)
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
            "custom\n.obsidian/\n.trash/\n.DS_Store\n.petit-poucet/\n*.sync-conflict-*\n"
        );
        assert!(git::is_repo(tmp.path()));
    }

    #[test]
    fn stignore_keeps_the_user_s_lines_and_lets_usage_and_cleanup_time_sync() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(STIGNORE), "/Index.md\ncustom").unwrap();
        ensure_vault_files(tmp.path()).unwrap();
        ensure_vault_files(tmp.path()).unwrap();
        let text = fs::read_to_string(tmp.path().join(STIGNORE)).unwrap();
        assert_eq!(
            text,
            "/Index.md\ncustom\n/.git\n/.petit-poucet\n/.obsidian/workspace*.json\n(?d).DS_Store\n"
        );
        assert!(!text.contains(crate::usage::DIR) && !text.contains(".last-cleanup"));
    }
}
