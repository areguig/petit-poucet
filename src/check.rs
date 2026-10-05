use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;

use crate::config::Limits;
use crate::note::{self, REQUIRED_TAG};
use crate::project::IDENTITY_FILE;
use crate::vault::{INDEX_FILE, PREFERENCES, PROJECTS, TOPICS, Vault};
use crate::{budget, checkouts, duplicates, index, secrets, stale_paths};

// A note is one short fact; past this the body is probably several facts or history.
pub const MAX_BODY_CHARS: usize = 1500;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warning,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Issue {
    pub file: String,
    pub level: Level,
    pub message: String,
}

impl Issue {
    fn error(file: impl Into<String>, message: impl Into<String>) -> Issue {
        Issue {
            file: file.into(),
            level: Level::Error,
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let level = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        write!(f, "{}: {level}: {}", self.file, self.message)
    }
}

pub fn check(vault: &Vault, limits: &Limits) -> Vec<Issue> {
    let mut issues = Vec::new();

    for note in &vault.notes {
        let file = format!("{}.md", note.path);
        let mut error = |message: String| issues.push(Issue::error(&file, message));
        match &note.frontmatter {
            Err(e) => error(format!("frontmatter: {e}")),
            Ok(fm) => {
                match fm.summary.as_deref().map(str::trim) {
                    None | Some("") => error("missing summary".into()),
                    Some(s) if s.contains('\n') => error("summary must be one line".into()),
                    _ => {}
                }
                let expected_scope = note.place().scope();
                if fm.scope != expected_scope {
                    error(format!(
                        "scope is `{}`, expected `{expected_scope}`",
                        fm.scope
                    ));
                }
                if fm.updated.is_some_and(|updated| updated < fm.created) {
                    error("updated is before created".into());
                }
                if !fm.tags.iter().any(|t| t == REQUIRED_TAG) {
                    error(format!("tags must include `{REQUIRED_TAG}`"));
                }
            }
        }
        if !note.body.contains("**Why:**") {
            error("missing **Why:** line".into());
        }
        for target in note::links(&note.body) {
            if !link_exists(vault, target) {
                error(format!("broken link [[{target}]]"));
            }
        }
        if let Some(kind) = secrets::find(&note.body).or_else(|| secrets::find(note.summary()?)) {
            error(format!("looks like a secret ({kind})"));
        }
        let max = limits.summary_max_chars;
        if note.summary().is_some_and(|s| s.chars().count() > max) {
            issues.push(Issue {
                file: file.clone(),
                level: Level::Warning,
                message: format!(
                    "summary over {max} characters: one short fact per note, split or shorten it"
                ),
            });
        }
        if note.body.chars().count() > MAX_BODY_CHARS {
            issues.push(Issue {
                file,
                level: Level::Warning,
                message: format!(
                    "longer than {MAX_BODY_CHARS} characters: one short fact per note"
                ),
            });
        }
    }

    for (note, path, checkout) in stale_paths::find(&vault.notes, &checkouts::load(&vault.root)) {
        issues.push(Issue {
            file: format!("{}.md", note.path),
            level: Level::Warning,
            message: format!("names `{path}`, missing from {}", checkout.display()),
        });
    }

    for (file, message) in budget::over(vault, limits.index_max_notes) {
        issues.push(Issue {
            file,
            level: Level::Warning,
            message,
        });
    }

    // One short line per note after its group's first: a big group never makes a line too long for a review page.
    for group in duplicates::groups(&vault.notes) {
        for note in &group[1..] {
            issues.push(Issue {
                file: format!("{}.md", note.path),
                level: Level::Warning,
                message: format!(
                    "near-duplicate of [[{}]]: merge them if they say the same",
                    group[0].path
                ),
            });
        }
    }

    let mut remotes: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for project in &vault.projects {
        let file = format!("{PROJECTS}/{}/{IDENTITY_FILE}", project.key);
        match &project.identity {
            Err(e) => issues.push(Issue::error(file, e)),
            Ok(id) => {
                for remote in &id.remotes {
                    remotes
                        .entry(crate::project::normalise_remote(remote))
                        .or_default()
                        .push(&project.key);
                }
            }
        }
    }
    for (remote, keys) in remotes.into_iter().filter(|(_, k)| k.len() > 1) {
        for key in &keys {
            issues.push(Issue::error(
                format!("{PROJECTS}/{key}/{IDENTITY_FILE}"),
                format!("remote {remote} is claimed by projects {}", keys.join(", ")),
            ));
        }
    }

    for file in &vault.stray {
        issues.push(Issue::error(
            file,
            format!("not in {PREFERENCES}/, {PROJECTS}/<project>/ or {TOPICS}/<topic>/"),
        ));
    }

    check_index(vault, &mut issues);
    issues.sort();
    issues
}

fn link_exists(vault: &Vault, target: &str) -> bool {
    let target = target.strip_suffix(".md").unwrap_or(target);
    vault.has_note(target) || vault.root.join(target).is_file()
}

fn check_index(vault: &Vault, issues: &mut Vec<Issue>) {
    let Ok(current) = fs::read_to_string(vault.root.join(INDEX_FILE)) else {
        issues.push(Issue::error(INDEX_FILE, "missing"));
        return;
    };
    if current == index::generate(vault) {
        return;
    }
    let listed: BTreeSet<&str> = note::links(&current).collect();
    let mut detailed = false;
    for path in listed.iter().filter(|p| !vault.has_note(p)) {
        issues.push(Issue::error(
            INDEX_FILE,
            format!("lists missing note [[{path}]]"),
        ));
        detailed = true;
    }
    for note in vault
        .notes
        .iter()
        .filter(|n| !listed.contains(n.path.as_str()))
    {
        issues.push(Issue::error(
            INDEX_FILE,
            format!("does not list [[{}]]", note.path),
        ));
        detailed = true;
    }
    if !detailed {
        issues.push(Issue::error(INDEX_FILE, "out of date"));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn limits() -> Limits {
        crate::config::Config::new(std::path::PathBuf::new()).limits
    }

    #[test]
    fn a_session_index_over_its_budget_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(PREFERENCES)).unwrap();
        for slug in ["a", "b"] {
            fs::write(tmp.path().join(format!("{PREFERENCES}/{slug}.md")), "x").unwrap();
        }
        let limits = Limits {
            index_max_notes: 1,
            ..limits()
        };
        let issues: Vec<String> = check(&Vault::load(tmp.path()).unwrap(), &limits)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            issues.contains(&"Preferences: warning: every session loads its 2 notes, over 1: merge or move some".to_string()),
            "{issues:?}"
        );
    }

    #[test]
    fn a_summary_over_the_limit_is_one_warning() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(PREFERENCES)).unwrap();
        for (slug, length) in [("at-limit", 200), ("over", 201)] {
            fs::write(
                tmp.path().join(format!("{PREFERENCES}/{slug}.md")),
                format!(
                    "---\ntype: user\nscope: all repos\nsummary: {}\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n**Why:** x\n",
                    "é".repeat(length)
                ),
            )
            .unwrap();
        }
        let warnings: Vec<String> = check(&Vault::load(tmp.path()).unwrap(), &limits())
            .iter()
            .filter(|i| i.level == Level::Warning)
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            warnings,
            [
                "Preferences/over.md: warning: summary over 200 characters: one short fact per note, split or shorten it"
            ],
            "counted in characters, not bytes"
        );
    }

    fn index_issues(vault_root: &std::path::Path) -> Vec<String> {
        check(&Vault::load(vault_root).unwrap(), &limits())
            .iter()
            .filter(|i| i.file == INDEX_FILE)
            .map(|i| i.message.clone())
            .collect()
    }

    #[test]
    fn index_must_exist_and_match_the_notes() {
        let tmp = tempfile::tempdir().unwrap();
        let note = tmp.path().join("Preferences/a.md");
        fs::create_dir(note.parent().unwrap()).unwrap();
        fs::write(&note, "---\ntype: user\nscope: all repos\nsummary: old\ncreated: 2026-09-24\ntags: [agent-memory]\n---\n**Why:** test\n").unwrap();
        assert_eq!(index_issues(tmp.path()), ["missing"]);

        let vault = Vault::load(tmp.path()).unwrap();
        fs::write(tmp.path().join(INDEX_FILE), index::generate(&vault)).unwrap();
        assert!(check(&vault, &limits()).is_empty());

        fs::write(
            &note,
            fs::read_to_string(&note).unwrap().replace("old", "new"),
        )
        .unwrap();
        assert_eq!(index_issues(tmp.path()), ["out of date"]);
    }

    #[test]
    fn secrets_are_found_in_the_summary_too() {
        let tmp = tempfile::tempdir().unwrap();
        let note = tmp.path().join("Preferences/a.md");
        fs::create_dir(note.parent().unwrap()).unwrap();
        fs::write(&note, "---\ntype: user\nscope: all repos\nsummary: key AKIAIOSFODNN7EXAMPLE\ncreated: 2026-09-24\ntags: [agent-memory]\n---\n**Why:** test\n").unwrap();
        let issues = check(&Vault::load(tmp.path()).unwrap(), &limits());
        assert!(
            issues
                .iter()
                .any(|i| i.message == "looks like a secret (AWS access key)")
        );
    }

    #[test]
    fn each_near_duplicate_after_the_first_names_it_once() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(PREFERENCES)).unwrap();
        let note = |summary: &str| {
            format!(
                "---\ntype: user\nscope: all repos\nsummary: {summary}\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n# Commits\n\n**Why:** x\n"
            )
        };
        for (name, summary) in [
            ("a", "commit locally per step, never push"),
            ("b", "commit each step locally, no push"),
            ("c", "commit each step locally"),
            ("d", "the staging database runs on port 5433"),
        ] {
            fs::write(
                tmp.path().join(format!("{PREFERENCES}/{name}.md")),
                note(summary),
            )
            .unwrap();
        }
        let vault = Vault::load(tmp.path()).unwrap();
        let found: Vec<String> = check(&vault, &limits())
            .iter()
            .filter(|i| i.message.starts_with("near-duplicate"))
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            found,
            [
                "Preferences/b.md: warning: near-duplicate of [[Preferences/a]]: merge them if they say the same",
                "Preferences/c.md: warning: near-duplicate of [[Preferences/a]]: merge them if they say the same",
            ]
        );
    }

    #[test]
    fn summary_must_be_one_line() {
        let tmp = tempfile::tempdir().unwrap();
        let note = tmp.path().join("Preferences/a.md");
        fs::create_dir(note.parent().unwrap()).unwrap();
        fs::write(&note, "---\ntype: user\nscope: all repos\nsummary: |\n  two\n  lines\ncreated: 2026-09-24\ntags: [agent-memory]\n---\n**Why:** test\n").unwrap();
        let issues = check(&Vault::load(tmp.path()).unwrap(), &limits());
        assert!(
            issues
                .iter()
                .any(|i| i.message == "summary must be one line")
        );
    }
}
