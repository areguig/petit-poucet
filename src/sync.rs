use std::fs;

use crate::config::Config;
use crate::vault::{INDEX_FILE, Vault, write_atomic};
use crate::{git, index, lock, project, usage};

const MESSAGE: &str = "sync: notes changed outside petit-poucet";

// Notes arrive through Syncthing or Obsidian without petit-poucet: bring the Index and the history up to date.
pub fn catch_up(config: &Config) -> Result<Vault, String> {
    let vault = Vault::load(&config.vault)?;
    if !behind(config, &vault) {
        return Ok(vault);
    }
    match lock::vault(&config.vault).and_then(|_lock| update(config)) {
        Ok(updated) => Ok(updated),
        Err(e) => {
            eprintln!("petit-poucet: memory not caught up: {e}");
            Ok(vault)
        }
    }
}

fn behind(config: &Config, vault: &Vault) -> bool {
    vault.conflicts.iter().any(|c| c.in_identity())
        || stale_index(vault).is_some()
        || config.git_autocommit && changed_outside(config)
}

// Reloads first: another agent may have saved since the caller loaded the vault.
fn update(config: &Config) -> Result<Vault, String> {
    merge_identity_conflicts(&Vault::load(&config.vault)?)?;
    let vault = Vault::load(&config.vault)?;
    if let Some(index) = stale_index(&vault) {
        write_atomic(&vault.root.join(INDEX_FILE), &index)?;
    }
    if config.git_autocommit && changed_outside(config) {
        git::commit(&vault.root, &["."], MESSAGE)?;
    }
    Ok(vault)
}

// A copy that doesn't parse, or whose project is gone, stays for `check` to report.
fn merge_identity_conflicts(vault: &Vault) -> Result<(), String> {
    for conflict in vault.conflicts.iter().filter(|c| c.in_identity()) {
        let (kept, copy) = (
            vault.root.join(&conflict.original),
            vault.root.join(&conflict.copy),
        );
        let (Ok(kept_text), Ok(copy_text)) = (fs::read_to_string(&kept), fs::read_to_string(&copy))
        else {
            continue;
        };
        if let Ok(merged) = project::merge_identities(&kept_text, &copy_text) {
            write_atomic(&kept, &merged)?;
            fs::remove_file(&copy).map_err(|e| format!("{}: {e}", conflict.copy))?;
        }
    }
    Ok(())
}

fn stale_index(vault: &Vault) -> Option<String> {
    let index = index::generate(vault);
    let current = fs::read_to_string(vault.root.join(INDEX_FILE)).ok();
    (current.as_ref() != Some(&index)).then_some(index)
}

// Usage changes alone wait for a commit that happens anyway, or every session start would make one.
fn changed_outside(config: &Config) -> bool {
    let usage = format!("{}/", usage::DIR);
    git::changed(&config.vault).is_ok_and(|paths| paths.iter().any(|p| !p.starts_with(&usage)))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use super::*;
    use crate::init::ensure_repo;

    fn run_git(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    fn note(root: &Path, path: &str, summary: &str) {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, format!("---\ntype: user\nscope: all repos\nsummary: {summary}\ncreated: 2026-10-26\ntags: [agent-memory]\n---\n**Why:** x\n")).unwrap();
    }

    // A committed vault whose Index matches its notes, as petit-poucet leaves it.
    fn vault(autocommit: bool) -> (tempfile::TempDir, Config) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        ensure_repo(root).unwrap();
        run_git(root, &["config", "user.name", "test"]);
        run_git(root, &["config", "user.email", "test@example.com"]);
        note(root, "Preferences/kept.md", "kept");
        note(root, "Preferences/edited.md", "before");
        note(root, "Preferences/deleted.md", "deleted");
        let index = index::generate(&Vault::load(root).unwrap());
        fs::write(root.join(INDEX_FILE), index).unwrap();
        run_git(root, &["add", "--all"]);
        run_git(root, &["commit", "--quiet", "-m", "start"]);
        let config = Config {
            git_autocommit: autocommit,
            ..Config::new(root.to_path_buf())
        };
        (tmp, config)
    }

    fn log(root: &Path) -> Vec<String> {
        run_git(root, &["log", "--format=%s"])
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn notes_changed_outside_are_indexed_and_committed_once() {
        let (tmp, config) = vault(true);
        let root = tmp.path();
        note(root, "Preferences/added.md", "added");
        note(root, "Preferences/edited.md", "after");
        fs::remove_file(root.join("Preferences/deleted.md")).unwrap();
        let copy = "Preferences/kept.sync-conflict-20261026-101112-ABCDEF7.md";
        fs::write(root.join(copy), "x").unwrap();

        let vault = catch_up(&config).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(INDEX_FILE)).unwrap(),
            index::generate(&vault)
        );
        assert_eq!(log(root), [MESSAGE, "start"]);
        assert_eq!(git::changed(root).unwrap(), Vec::<String>::new());
        assert!(!run_git(root, &["ls-files"]).contains(copy));
        // No Index regeneration is pending: the conflict copy's line belongs to `check`.
        catch_up(&config).unwrap();
        assert_eq!(log(root).len(), 2);
    }

    #[test]
    fn usage_alone_waits_for_another_commit() {
        let (tmp, config) = vault(true);
        let root = tmp.path();
        fs::create_dir(root.join(usage::DIR)).unwrap();
        fs::write(root.join(usage::DIR).join("machine.json"), "{}").unwrap();
        catch_up(&config).unwrap();
        assert_eq!(log(root), ["start"]);

        note(root, "Preferences/added.md", "added");
        catch_up(&config).unwrap();
        assert_eq!(log(root), [MESSAGE, "start"]);
        assert_eq!(git::changed(root).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn without_autocommit_only_the_index_is_rewritten() {
        let (tmp, config) = vault(false);
        let root = tmp.path();
        note(root, "Preferences/added.md", "added");
        let vault = catch_up(&config).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(INDEX_FILE)).unwrap(),
            index::generate(&vault)
        );
        assert_eq!(log(root), ["start"]);
    }
}
