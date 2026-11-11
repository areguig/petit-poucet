use std::fs;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::change;
use crate::config::Config;
use crate::note;
use crate::vault::{PREFERENCES, PROJECTS, TOPICS, Vault, write_atomic};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveRequest {
    pub path: String,
    /// `Preferences/<name>`, `Projects/<project>/<name>` or `Topics/<topic>/<name>`, without `.md`; `<name>` in lowercase letters, digits and hyphens.
    pub new_path: String,
    /// Required to move a `feedback` note to another scope: set only after the user confirmed.
    #[serde(default)]
    pub user_confirmed: bool,
}

// Returns the reply and the other notes whose links were rewritten.
pub fn move_note(
    config: &Config,
    req: MoveRequest,
    agent: &str,
) -> Result<(String, Vec<String>), String> {
    let vault = Vault::load(&config.vault)?;
    let note = change::find_note(&vault, &req.path)?;
    let (old, new) = (note.path.as_str(), req.new_path.trim_end_matches(".md"));
    check_destination(&vault, new)?;

    let scope = note::Place::of(new).scope();
    let old_file = vault.root.join(format!("{old}.md"));
    let text = match &note.frontmatter {
        Ok(fm) if fm.scope != scope => {
            change::require_confirmation(note, req.user_confirmed)?;
            let mut fm = fm.clone();
            fm.scope = scope.to_string();
            fm.updated = Some(jiff::Zoned::now().date());
            note::render(&fm, &note.body)?
        }
        _ => fs::read_to_string(&old_file).map_err(|e| e.to_string())?,
    };
    let relink =
        |text: &str| note::map_links(text, |target| (target == old).then(|| new.to_string()));
    let new_file = vault.root.join(format!("{new}.md"));
    fs::create_dir_all(new_file.parent().unwrap()).map_err(|e| e.to_string())?;
    write_atomic(&new_file, &relink(&text))?;
    fs::remove_file(&old_file).map_err(|e| e.to_string())?;
    let relinked = change::rewrite_links(&vault, old, relink)?;

    let mut changed = vec![format!("{old}.md"), format!("{new}.md")];
    changed.extend(relinked.iter().cloned());
    let (_, warning) = change::finish(config, changed, &format!("move: {old} -> {new} ({agent})"))?;
    let mut reply = vec![format!("moved {old} to {new}")];
    if !relinked.is_empty() {
        reply.push(format!("links updated in: {}", change::list(&relinked)));
    }
    reply.extend(warning);
    Ok((reply.join("\n"), relinked))
}

fn check_destination(vault: &Vault, new: &str) -> Result<(), String> {
    // The shape memory_save gives names: anything else could write lines into the Index or a path outside the vault.
    let is_slug = |name: &str| !name.is_empty() && slug::slugify(name) == name;
    let known_project = |key: &str| {
        vault
            .projects
            .iter()
            .any(|p| p.key == key && p.identity.is_ok())
    };
    let valid = match new.split('/').collect::<Vec<_>>()[..] {
        [PREFERENCES, name] => is_slug(name),
        [PROJECTS, key, name] => is_slug(name) && known_project(key),
        [TOPICS, key, name] => is_slug(name) && is_slug(key),
        _ => false,
    };
    if !valid {
        return Err(format!(
            "new_path must be {PREFERENCES}/<name>, {PROJECTS}/<known project>/<name> or {TOPICS}/<topic>/<name>, with <name> and <topic> in lowercase letters, digits and hyphens"
        ));
    }
    if vault.root.join(format!("{new}.md")).exists() {
        return Err(format!("{new} already exists"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use crate::vault::INDEX_FILE;

    fn vault() -> (tempfile::TempDir, Config) {
        let tmp = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = tmp.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        let note = |note_type: &str, scope: &str, body: &str| {
            format!(
                "---\ntype: {note_type}\nscope: {scope}\nsummary: s\ncreated: 2026-09-24\ntags: [agent-memory]\n---\n\n{body}\n"
            )
        };
        write(
            "Projects/app/_project.md",
            "---\ntype: project-identity\nfolders: [app]\n---\n",
        );
        write(
            "Preferences/rule.md",
            &note("feedback", "all repos", "# Rule"),
        );
        write(
            "Preferences/fact.md",
            &note("reference", "all repos", "# Fact"),
        );
        write(
            "Preferences/other.md",
            &note(
                "reference",
                "all repos",
                "See [[Preferences/fact#Part|the fact]].",
            ),
        );
        write(
            "Preferences/self.md",
            &note(
                "reference",
                "all repos",
                "See [[Preferences/self#Examples|examples]] and [[Preferences/other]].",
            ),
        );
        let config = Config {
            git_autocommit: false,
            ..Config::new(tmp.path().to_path_buf())
        };
        (tmp, config)
    }

    fn request(path: &str, new_path: &str) -> MoveRequest {
        MoveRequest {
            path: path.into(),
            new_path: new_path.into(),
            user_confirmed: false,
        }
    }

    #[test]
    fn moves_into_a_project_and_rewrites_links() {
        let (tmp, config) = vault();
        let (reply, _) = move_note(
            &config,
            request("Preferences/fact", "Projects/app/fact"),
            "test",
        )
        .unwrap();
        assert_eq!(
            reply,
            "moved Preferences/fact to Projects/app/fact\nlinks updated in: [[Preferences/other]]"
        );
        assert!(!tmp.path().join("Preferences/fact.md").exists());
        let moved = fs::read_to_string(tmp.path().join("Projects/app/fact.md")).unwrap();
        let fm = Note::parse("Projects/app/fact".into(), &moved)
            .frontmatter
            .unwrap();
        assert_eq!(fm.scope, "app");
        assert!(fm.updated.is_some());
        let other = fs::read_to_string(tmp.path().join("Preferences/other.md")).unwrap();
        assert!(
            other.contains("[[Projects/app/fact#Part|the fact]]"),
            "{other}"
        );
        let index = fs::read_to_string(tmp.path().join(INDEX_FILE)).unwrap();
        assert!(index.contains("[[Projects/app/fact]]") && !index.contains("[[Preferences/fact]]"));
    }

    #[test]
    fn renaming_keeps_the_file_as_is() {
        let (tmp, config) = vault();
        let before = fs::read_to_string(tmp.path().join("Preferences/rule.md")).unwrap();
        move_note(
            &config,
            request("Preferences/rule", "Preferences/commit-rule"),
            "test",
        )
        .unwrap();
        let after = fs::read_to_string(tmp.path().join("Preferences/commit-rule.md")).unwrap();
        assert_eq!(after, before, "same scope: no confirmation, no rewrite");
    }

    #[test]
    fn a_rename_rewrites_the_note_s_own_links() {
        let (tmp, config) = vault();
        move_note(
            &config,
            request("Preferences/self", "Preferences/renamed"),
            "test",
        )
        .unwrap();
        let moved = fs::read_to_string(tmp.path().join("Preferences/renamed.md")).unwrap();
        assert!(
            moved.contains(
                "See [[Preferences/renamed#Examples|examples]] and [[Preferences/other]]."
            ),
            "{moved}"
        );
    }

    #[test]
    fn a_move_to_another_scope_rewrites_the_note_s_own_links() {
        let (tmp, config) = vault();
        move_note(
            &config,
            request("Preferences/self", "Projects/app/self"),
            "test",
        )
        .unwrap();
        let moved = fs::read_to_string(tmp.path().join("Projects/app/self.md")).unwrap();
        assert_eq!(
            Note::parse("Projects/app/self".into(), &moved)
                .frontmatter
                .unwrap()
                .scope,
            "app"
        );
        assert!(
            moved
                .contains("See [[Projects/app/self#Examples|examples]] and [[Preferences/other]]."),
            "{moved}"
        );
    }

    #[test]
    fn moving_a_user_rule_to_another_scope_needs_confirmation() {
        let (_tmp, config) = vault();
        let err = move_note(
            &config,
            request("Preferences/rule", "Projects/app/rule"),
            "test",
        )
        .unwrap_err();
        assert!(err.contains("user_confirmed"), "{err}");
        let mut confirmed = request("Preferences/rule", "Projects/app/rule");
        confirmed.user_confirmed = true;
        move_note(&config, confirmed, "test").unwrap();
    }

    #[test]
    fn refuses_bad_destinations() {
        let (_tmp, config) = vault();
        for (new_path, expected) in [
            ("Preferences/fact", "already exists"),
            ("Projects/unknown/fact", "known project"),
            ("Projects/app/_project", "known project"),
            ("Preferences/../x", "new_path must be"),
            ("Elsewhere/fact", "new_path must be"),
            ("Topics/Bad Name/fact", "new_path must be"),
            ("Topics//fact", "new_path must be"),
            ("Preferences/", "new_path must be"),
            ("Preferences/.hidden", "new_path must be"),
            ("Preferences/Fact Renamed", "new_path must be"),
            ("Preferences/x]]\n- [[y", "new_path must be"),
            (r"Preferences/a\..\escaped", "new_path must be"),
        ] {
            let err =
                move_note(&config, request("Preferences/other", new_path), "test").unwrap_err();
            assert!(err.contains(expected), "{new_path}: {err}");
        }
    }

    #[test]
    fn an_empty_topic_leaves_the_vault_untouched() {
        let (tmp, config) = vault();
        let files = ["Preferences/fact.md", "Preferences/other.md", INDEX_FILE];
        let read = || files.map(|f| fs::read(tmp.path().join(f)).ok());
        let before = read();
        let err =
            move_note(&config, request("Preferences/fact", "Topics//fact"), "test").unwrap_err();
        assert!(err.contains("new_path must be"), "{err}");
        assert_eq!(read(), before);
        assert!(!tmp.path().join("Topics").exists());
    }

    #[test]
    fn moves_into_a_new_topic() {
        let (tmp, config) = vault();
        move_note(
            &config,
            request("Preferences/fact", "Topics/homelab/fact"),
            "test",
        )
        .unwrap();
        let moved = fs::read_to_string(tmp.path().join("Topics/homelab/fact.md")).unwrap();
        assert_eq!(
            Note::parse("Topics/homelab/fact".into(), &moved)
                .frontmatter
                .unwrap()
                .scope,
            "homelab"
        );
    }
}
