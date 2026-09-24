use std::fs;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::check::MAX_BODY_CHARS;
use crate::config::Config;
use crate::note::{self, ALL_REPOS, Frontmatter, NoteType, REQUIRED_TAG};
use crate::project::{self, IDENTITY_FILE};
use crate::vault::{INDEX_FILE, PREFERENCES, PROJECTS, Vault, write_atomic};
use crate::{git, index, search, secrets};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveRequest {
    /// Existing note to update, e.g. `Preferences/commit-rules`; omit to create a note.
    pub path: Option<String>,
    #[serde(rename = "type")]
    pub note_type: NoteType,
    /// `all repos`, or a project key; overrides project_dir.
    pub scope: Option<String>,
    /// The agent's working directory, used to find the project.
    pub project_dir: Option<PathBuf>,
    pub title: String,
    /// One line; becomes the note's line in the Index.
    pub summary: String,
    /// The fact itself, in a few lines.
    pub fact: String,
    /// Where the fact came from (the user's words, or the file/command that verified it) and when.
    pub source: String,
    pub how_to_apply: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Required to update a `feedback` note: set only after the user confirmed the change.
    #[serde(default)]
    pub user_confirmed: bool,
}

pub fn save(config: &Config, req: SaveRequest, agent: &str) -> Result<String, String> {
    validate(&req)?;
    let root = &config.vault;
    let vault = Vault::load(root)?;
    let today = jiff::Zoned::now().date();

    let (path, created, identity) = match &req.path {
        Some(path) => {
            let path = path.strip_suffix(".md").unwrap_or(path).to_string();
            let existing = vault
                .notes
                .iter()
                .find(|n| n.path == path)
                .ok_or(format!("no note at {path}"))?;
            let old = existing.frontmatter.as_ref().ok();
            if old.is_some_and(|fm| fm.note_type == NoteType::Feedback) && !req.user_confirmed {
                return Err(format!(
                    "{path} is a rule the user stated: ask the user, then retry with user_confirmed: true"
                ));
            }
            (path, old.map(|fm| fm.created), None)
        }
        None => {
            let (folder, identity) = target_folder(&vault, &req)?;
            let path = format!("{folder}/{}", slug::slugify(&req.title));
            if vault.has_note(&path) {
                return Err(format!("{path} already exists: pass its path to update it"));
            }
            (path, None, identity)
        }
    };

    let mut tags = vec![REQUIRED_TAG.to_string()];
    for tag in &req.tags {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    let frontmatter = Frontmatter {
        note_type: req.note_type,
        scope: note::project_of(&path).unwrap_or(ALL_REPOS).to_string(),
        summary: Some(req.summary.trim().to_string()),
        created: created.unwrap_or(today),
        updated: created.map(|_| today),
        tags,
    };
    let body = format!(
        "# {}\n\n{}\n\n**Why:** {}\n**How to apply:** {}\n",
        req.title.trim(),
        req.fact.trim(),
        req.source.trim(),
        req.how_to_apply.trim()
    );

    let file = format!("{path}.md");
    let mut changed = vec![file.clone(), INDEX_FILE.to_string()];
    if let Some((identity_file, text)) = identity {
        fs::create_dir_all(root.join(&identity_file).parent().unwrap())
            .map_err(|e| e.to_string())?;
        write_atomic(&root.join(&identity_file), &text)?;
        changed.push(identity_file);
    }
    write_atomic(&root.join(&file), &note::render(&frontmatter, &body)?)?;
    let vault = Vault::load(root)?;
    write_atomic(&root.join(INDEX_FILE), &index::generate(&vault))?;

    let action = if created.is_some() {
        "update"
    } else {
        "create"
    };
    let mut reply = vec![format!("saved {path}")];
    let others = vault.notes.iter().filter(|n| n.path != path);
    let similar = search::similar(others, &req.title, &req.summary);
    if !similar.is_empty() {
        let list: Vec<String> = similar.iter().map(|n| format!("[[{}]]", n.path)).collect();
        reply.push(format!(
            "similar notes, merge if they say the same: {}",
            list.join(", ")
        ));
    }
    if body.chars().count() > MAX_BODY_CHARS {
        reply.push(format!(
            "warning: longer than {MAX_BODY_CHARS} characters: one short fact per note"
        ));
    }
    if config.git_autocommit {
        let paths: Vec<&str> = changed.iter().map(String::as_str).collect();
        if let Err(e) = git::commit(root, &paths, &format!("{action}: {path} ({agent})")) {
            reply.push(format!("not committed: {e}"));
        }
    }
    Ok(reply.join("\n"))
}

fn validate(req: &SaveRequest) -> Result<(), String> {
    let fields = [
        ("title", &req.title),
        ("summary", &req.summary),
        ("fact", &req.fact),
        ("source", &req.source),
        ("how_to_apply", &req.how_to_apply),
    ];
    for (name, value) in fields {
        if value.trim().is_empty() {
            return Err(format!("{name} is required"));
        }
        if let Some(kind) = secrets::find(value) {
            return Err(format!(
                "{name} looks like a secret ({kind}): never store secrets"
            ));
        }
    }
    for (name, value) in [("title", &req.title), ("summary", &req.summary)] {
        if value.trim().contains('\n') {
            return Err(format!("{name} must be one line"));
        }
    }
    Ok(())
}

// Returns the note folder, plus the `_project.md` to create on the first save in an unknown repo.
fn target_folder(
    vault: &Vault,
    req: &SaveRequest,
) -> Result<(String, Option<(String, String)>), String> {
    match req.scope.as_deref() {
        Some(ALL_REPOS) => return Ok((PREFERENCES.to_string(), None)),
        Some(key) if vault.projects.iter().any(|p| p.key == key) => {
            return Ok((format!("{PROJECTS}/{key}"), None));
        }
        Some(key) => {
            return Err(format!(
                "unknown project `{key}`: use project_dir for a new project"
            ));
        }
        None => {}
    }
    let dir = req
        .project_dir
        .as_deref()
        .ok_or("give scope (`all repos` or a project) or project_dir")?;
    if let Some(key) = project::resolve(&vault.projects, dir) {
        return Ok((format!("{PROJECTS}/{key}"), None));
    }
    let top = git::toplevel(dir)
        .map(PathBuf::from)
        .unwrap_or(dir.to_path_buf());
    let key = top
        .file_name()
        .ok_or(format!("no folder name in {}", dir.display()))?
        .to_string_lossy()
        .into_owned();
    let identity_file = format!("{PROJECTS}/{key}/{IDENTITY_FILE}");
    if vault.root.join(&identity_file).exists() {
        return Err(format!("{identity_file} is invalid: fix it first"));
    }
    let text = project::render_identity(&git::remote_urls(dir), &key)?;
    Ok((format!("{PROJECTS}/{key}"), Some((identity_file, text))))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::note::Note;

    fn vault() -> (tempfile::TempDir, Config) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("vault");
        fs::create_dir_all(root.join(PREFERENCES)).unwrap();
        fs::create_dir_all(root.join(PROJECTS)).unwrap();
        let config = Config {
            vault: root,
            git_autocommit: false,
        };
        (tmp, config)
    }

    fn request(title: &str) -> SaveRequest {
        SaveRequest {
            path: None,
            note_type: NoteType::Feedback,
            scope: Some(ALL_REPOS.into()),
            project_dir: None,
            title: title.into(),
            summary: format!("{title} summary"),
            fact: "The fact.".into(),
            source: "the user said so on 2026-09-24".into(),
            how_to_apply: "Always.".into(),
            tags: vec!["git".into(), REQUIRED_TAG.into()],
            user_confirmed: false,
        }
    }

    fn read(config: &Config, path: &str) -> Note {
        let text = fs::read_to_string(config.vault.join(format!("{path}.md"))).unwrap();
        Note::parse(path.into(), &text)
    }

    #[test]
    fn creates_a_valid_note_and_updates_the_index() {
        let (_tmp, config) = vault();
        let reply = save(&config, request("Commit rules"), "test").unwrap();
        assert_eq!(reply, "saved Preferences/commit-rules");

        let note = read(&config, "Preferences/commit-rules");
        let fm = note.frontmatter.as_ref().unwrap();
        assert_eq!(fm.scope, ALL_REPOS);
        assert_eq!(fm.tags, [REQUIRED_TAG, "git"]);
        assert_eq!(fm.updated, None);
        assert_eq!(
            note.body,
            "# Commit rules\n\nThe fact.\n\n**Why:** the user said so on 2026-09-24\n**How to apply:** Always.\n"
        );
        let vault = Vault::load(&config.vault).unwrap();
        assert!(crate::check::check(&vault).is_empty());
    }

    #[test]
    fn rejects_incomplete_or_unsafe_notes() {
        let (_tmp, config) = vault();
        type Change = fn(&mut SaveRequest);
        let cases: [(Change, &str); 5] = [
            (|r| r.source = " ".into(), "source is required"),
            (
                |r| r.fact = "key AKIAIOSFODNN7EXAMPLE".into(),
                "fact looks like a secret",
            ),
            (
                |r| r.summary = "two\nlines".into(),
                "summary must be one line",
            ),
            (|r| r.scope = Some("nope".into()), "unknown project `nope`"),
            (|r| r.scope = None, "give scope"),
        ];
        for (change, expected) in cases {
            let mut req = request("Commit rules");
            change(&mut req);
            let err = save(&config, req, "test").unwrap_err();
            assert!(err.starts_with(expected), "{err}");
        }
        assert!(Vault::load(&config.vault).unwrap().notes.is_empty());
    }

    #[test]
    fn updating_a_user_rule_needs_confirmation_and_keeps_created() {
        let (_tmp, config) = vault();
        save(&config, request("Commit rules"), "test").unwrap();
        let path = config.vault.join("Preferences/commit-rules.md");
        let text = fs::read_to_string(&path).unwrap();
        let today = jiff::Zoned::now().date().to_string();
        fs::write(&path, text.replace(&today, "2026-01-01")).unwrap();

        let mut update = request("Commit rules");
        update.path = Some("Preferences/commit-rules".into());
        update.fact = "Changed.".into();
        let err = save(&config, update, "test").unwrap_err();
        assert!(err.contains("user_confirmed"), "{err}");

        let mut update = request("Commit rules");
        update.path = Some("Preferences/commit-rules.md".into());
        update.user_confirmed = true;
        save(&config, update, "test").unwrap();
        let fm = read(&config, "Preferences/commit-rules")
            .frontmatter
            .unwrap();
        assert_eq!(fm.created, jiff::civil::date(2026, 1, 1));
        assert_eq!(fm.updated.unwrap().to_string(), today);
    }

    #[test]
    fn refuses_to_create_over_an_existing_note_or_update_a_missing_one() {
        let (_tmp, config) = vault();
        save(&config, request("Commit rules"), "test").unwrap();
        let err = save(&config, request("Commit rules"), "test").unwrap_err();
        assert!(err.contains("already exists"), "{err}");

        let mut update = request("Other");
        update.path = Some("Preferences/other".into());
        assert_eq!(
            save(&config, update, "test").unwrap_err(),
            "no note at Preferences/other"
        );
    }

    #[test]
    fn first_save_in_an_unknown_repo_creates_the_project() {
        let (tmp, config) = vault();
        let repo = tmp.path().join("my-repo");
        fs::create_dir(&repo).unwrap();
        git::init(&repo).unwrap();
        std::process::Command::new("git")
            .args([
                "-C",
                repo.to_str().unwrap(),
                "remote",
                "add",
                "origin",
                "git@github.com:me/my-repo.git",
            ])
            .status()
            .unwrap();

        for title in ["First fact", "Second fact"] {
            let mut req = request(title);
            req.note_type = NoteType::Project;
            req.scope = None;
            req.project_dir = Some(repo.join("src"));
            fs::create_dir_all(repo.join("src")).unwrap();
            save(&config, req, "test").unwrap();
        }

        let vault = Vault::load(&config.vault).unwrap();
        let project = &vault.projects[0];
        let identity = project.identity.as_ref().unwrap();
        assert_eq!(project.key, "my-repo");
        assert_eq!(identity.remotes, ["github.com/me/my-repo"]);
        assert_eq!(identity.folders, ["my-repo"]);
        assert_eq!(vault.notes.len(), 2);
        assert_eq!(
            read(&config, "Projects/my-repo/first-fact")
                .frontmatter
                .unwrap()
                .scope,
            "my-repo"
        );
        assert!(crate::check::check(&vault).is_empty());
    }

    #[test]
    fn reports_similar_notes() {
        let (_tmp, config) = vault();
        let mut first = request("Commit rules");
        first.summary = "commit locally per step, never push".into();
        save(&config, first, "test").unwrap();

        let mut second = request("Commit locally");
        second.summary = "commit each step locally, no push".into();
        let reply = save(&config, second, "test").unwrap();
        assert!(
            reply.ends_with(
                "similar notes, merge if they say the same: [[Preferences/commit-rules]]"
            ),
            "{reply}"
        );
    }
}
