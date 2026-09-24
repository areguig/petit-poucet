use std::fs;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::change;
use crate::config::Config;
use crate::note;
use crate::vault::Vault;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteRequest {
    pub path: String,
    /// Why the note is wrong or obsolete; recorded in the commit message.
    pub reason: String,
    /// Required to delete a `feedback` note: set only after the user confirmed.
    #[serde(default)]
    pub user_confirmed: bool,
}

pub fn delete(config: &Config, req: DeleteRequest, agent: &str) -> Result<String, String> {
    if req.reason.trim().is_empty() {
        return Err("reason is required".to_string());
    }
    let vault = Vault::load(&config.vault)?;
    let note = change::find_note(&vault, &req.path)?;
    change::require_confirmation(note, req.user_confirmed)?;
    let (path, title) = (note.path.as_str(), note.title());

    let file = format!("{path}.md");
    fs::remove_file(vault.root.join(&file)).map_err(|e| e.to_string())?;
    let unlinked = change::rewrite_links(&vault, path, |text| note::unlink(text, path, title))?;

    let mut changed = vec![file];
    changed.extend(unlinked.iter().cloned());
    let message = format!("delete: {path} ({agent})\n\n{}", req.reason.trim());
    let (_, warning) = change::finish(config, changed, &message)?;
    let mut reply = vec![format!("deleted {path}")];
    if !unlinked.is_empty() {
        reply.push(format!(
            "links to it removed in: {}",
            change::list(&unlinked)
        ));
    }
    reply.extend(warning);
    Ok(reply.join("\n"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::vault::{INDEX_FILE, PREFERENCES};

    fn vault_with(notes: &[(&str, &str, &str)]) -> (tempfile::TempDir, Config) {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(PREFERENCES)).unwrap();
        for (name, note_type, body) in notes {
            let text = format!(
                "---\ntype: {note_type}\nscope: all repos\nsummary: {name}\ncreated: 2026-09-24\ntags: [agent-memory]\n---\n\n{body}\n"
            );
            fs::write(tmp.path().join(format!("{PREFERENCES}/{name}.md")), text).unwrap();
        }
        let config = Config {
            vault: tmp.path().to_path_buf(),
            git_autocommit: false,
        };
        (tmp, config)
    }

    fn request(path: &str, user_confirmed: bool) -> DeleteRequest {
        DeleteRequest {
            path: path.into(),
            reason: "obsolete".into(),
            user_confirmed,
        }
    }

    #[test]
    fn deletes_and_unlinks_everywhere() {
        let (tmp, config) = vault_with(&[
            ("old", "reference", "# Old rule\n\n**Why:** x"),
            (
                "user",
                "reference",
                "See [[Preferences/old]] and [[Preferences/old|that note]].",
            ),
        ]);
        let reply = delete(&config, request("Preferences/old", false), "test").unwrap();
        assert_eq!(
            reply,
            "deleted Preferences/old\nlinks to it removed in: [[Preferences/user]]"
        );
        assert!(!tmp.path().join("Preferences/old.md").exists());
        let user = fs::read_to_string(tmp.path().join("Preferences/user.md")).unwrap();
        assert!(user.contains("See Old rule and that note."), "{user}");
        let index = fs::read_to_string(tmp.path().join(INDEX_FILE)).unwrap();
        assert!(!index.contains("Preferences/old"));
    }

    #[test]
    fn user_rules_need_confirmation_and_a_reason() {
        let (tmp, config) = vault_with(&[("rule", "feedback", "# Rule")]);
        let err = delete(&config, request("Preferences/rule", false), "test").unwrap_err();
        assert!(err.contains("user_confirmed"), "{err}");
        let mut no_reason = request("Preferences/rule", true);
        no_reason.reason = " ".into();
        assert_eq!(
            delete(&config, no_reason, "test").unwrap_err(),
            "reason is required"
        );
        assert!(tmp.path().join("Preferences/rule.md").exists());

        delete(&config, request("Preferences/rule.md", true), "test").unwrap();
        assert!(!tmp.path().join("Preferences/rule.md").exists());
    }
}
