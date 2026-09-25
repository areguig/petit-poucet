use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::note::{self, Note, NoteType};
use crate::project::{self, IDENTITY_FILE};
use crate::vault::{INDEX_FILE, PROJECTS, Vault, write_atomic};
use crate::{git, index};

pub fn find_note<'a>(vault: &'a Vault, path: &str) -> Result<&'a Note, String> {
    let path = path.strip_suffix(".md").unwrap_or(path);
    vault
        .notes
        .iter()
        .find(|n| n.path == path)
        .ok_or(format!("no note at {path}"))
}

pub fn require_confirmation(note: &Note, user_confirmed: bool) -> Result<(), String> {
    let is_rule = note
        .frontmatter
        .as_ref()
        .is_ok_and(|fm| fm.note_type == NoteType::Feedback);
    if is_rule && !user_confirmed {
        return Err(format!(
            "{} is a rule the user stated: ask the user, then retry with user_confirmed: true",
            note.path
        ));
    }
    Ok(())
}

// Every change ends the same way: regenerate the Index, then commit what changed.
// Returns the reloaded vault and, when the commit failed, a line for the reply.
pub fn finish(
    config: &Config,
    mut changed: Vec<String>,
    message: &str,
) -> Result<(Vault, Option<String>), String> {
    let root = &config.vault;
    let vault = Vault::load(root)?;
    write_atomic(&root.join(INDEX_FILE), &index::generate(&vault))?;
    if !config.git_autocommit {
        return Ok((vault, None));
    }
    changed.push(INDEX_FILE.to_string());
    let paths: Vec<&str> = changed.iter().map(String::as_str).collect();
    let warning = git::commit(root, &paths, message)
        .err()
        .map(|e| format!("not committed: {e}"));
    Ok((vault, warning))
}

// Applies `rewrite` to every other note that links to `target`; returns the files it changed.
pub fn rewrite_links(
    vault: &Vault,
    target: &str,
    rewrite: impl Fn(&str) -> String,
) -> Result<Vec<String>, String> {
    let mut changed = Vec::new();
    for note in &vault.notes {
        if note.path == target || !note::links(&note.body).any(|link| link == target) {
            continue;
        }
        let file = format!("{}.md", note.path);
        let text = fs::read_to_string(vault.root.join(&file)).map_err(|e| e.to_string())?;
        write_atomic(&vault.root.join(&file), &rewrite(&text))?;
        changed.push(file);
    }
    Ok(changed)
}

pub fn list(files: &[String]) -> String {
    files
        .iter()
        .map(|f| format!("[[{}]]", f.strip_suffix(".md").unwrap_or(f)))
        .collect::<Vec<_>>()
        .join(", ")
}

// The project a working directory belongs to; records its git remote when the project has none yet.
pub fn identify(config: &Config, vault: &Vault, dir: &Path, agent: &str) -> Option<String> {
    let key = project::resolve(&vault.projects, dir)?.to_string();
    let found = vault.projects.iter().find(|p| p.key == key)?;
    if let (Some(remotes), Ok(identity)) = (project::missing_remotes(found, dir), &found.identity) {
        let file = format!("{PROJECTS}/{key}/{IDENTITY_FILE}");
        let saved = project::render_identity(&remotes, &identity.folders)
            .and_then(|text| write_atomic(&vault.root.join(&file), &text))
            .and_then(|()| match config.git_autocommit {
                true => git::commit(
                    &vault.root,
                    &[&file],
                    &format!("identify: {file} ({agent})"),
                ),
                false => Ok(()),
            });
        if let Err(e) = saved {
            eprintln!("petit-poucet: remote of {key} not recorded: {e}");
        }
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(root: &Path, key: &str, remotes: &str) {
        let dir = root.join(PROJECTS).join(key);
        fs::create_dir_all(&dir).unwrap();
        let text =
            format!("---\ntype: project-identity\nremotes: {remotes}\nfolders: [{key}]\n---\n");
        fs::write(dir.join(IDENTITY_FILE), text).unwrap();
    }

    fn checkout(parent: &Path, name: &str, remote: &str) -> std::path::PathBuf {
        let dir = parent.join(name);
        fs::create_dir_all(&dir).unwrap();
        git::init(&dir).unwrap();
        std::process::Command::new("git")
            .args([
                "-C",
                dir.to_str().unwrap(),
                "remote",
                "add",
                "origin",
                remote,
            ])
            .status()
            .unwrap();
        dir
    }

    #[test]
    fn a_project_without_remotes_learns_them_from_its_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("vault");
        identity(&root, "migrated", "[]");
        identity(&root, "known", "[github.com/me/known]");
        let config = Config {
            vault: root.clone(),
            git_autocommit: false,
        };
        let vault = Vault::load(&root).unwrap();

        let migrated = checkout(tmp.path(), "migrated", "git@github.com:me/migrated.git");
        assert_eq!(
            identify(&config, &vault, &migrated, "test").as_deref(),
            Some("migrated")
        );
        let text = fs::read_to_string(root.join("Projects/migrated/_project.md")).unwrap();
        assert!(text.contains("github.com/me/migrated"), "{text}");

        // Same folder name, other repo: matched by folder, but never merged into the known remotes.
        let other = checkout(
            &tmp.path().join("elsewhere"),
            "known",
            "git@github.com:someone/else.git",
        );
        identify(&config, &vault, &other, "test");
        let text = fs::read_to_string(root.join("Projects/known/_project.md")).unwrap();
        assert!(!text.contains("someone/else"), "{text}");
    }
}
