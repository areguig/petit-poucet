use std::fs;

use crate::config::Config;
use crate::note::{self, Note, NoteType};
use crate::vault::{INDEX_FILE, Vault, write_atomic};
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
