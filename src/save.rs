use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::check::MAX_BODY_CHARS;
use crate::config::Config;
use crate::note::{self, ALL_REPOS, Frontmatter, NoteType, REQUIRED_TAG};
use crate::project::{self, IDENTITY_FILE};
use crate::vault::{PREFERENCES, PROJECTS, TOPICS, Vault, write_atomic};
use crate::{change, context, duplicates, git, secrets};

const PREFERENCE_SCOPE_REMINDER: &str = "Preferences load in every session of every repo: make sure this is cross-project and says when it applies, and confirm with the user when unsure; otherwise move it to the project or a topic (memory_move).";

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = require_fields_in_schema)]
pub struct SaveRequest {
    /// Existing note to update, by its Index name (`commit-rules`) or full path; omit to create a note.
    pub path: Option<String>,
    // The fields below default to empty so that `validate` can report every missing one at once.
    #[serde(rename = "type", default)]
    pub note_type: Option<NoteType>,
    /// A subject tied to no repo, e.g. `homelab` or `work-mac`: the note goes to Topics/<topic>. Overrides scope and project_dir.
    pub topic: Option<String>,
    /// `all repos`, or a project key; overrides project_dir.
    pub scope: Option<String>,
    /// The agent's working directory, used to find the project.
    pub project_dir: Option<PathBuf>,
    #[serde(default)]
    pub title: String,
    /// One line; becomes the note's line in the Index.
    #[serde(default)]
    pub summary: String,
    /// The fact itself, in a few lines.
    #[serde(default)]
    pub fact: String,
    /// Where the fact came from (the user's words, or the file/command that verified it) and when.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub how_to_apply: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Preferences only: when it applies (one line), if only in some situations; it is then not loaded at session start. Omit for one that always applies; on update, omit to keep and an empty string clears it.
    pub applies_when: Option<String>,
    /// Required to update a `feedback` note: set only after the user confirmed the change.
    #[serde(default)]
    pub user_confirmed: bool,
    // Fields agents guess (`content`, `body`), named in the error instead of silently dropped.
    #[serde(flatten)]
    #[schemars(skip)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

// serde's defaults would make the schema optional: agents must still see these fields as required.
fn require_fields_in_schema(schema: &mut schemars::Schema) {
    schema.insert(
        "required".to_string(),
        serde_json::json!(["type", "title", "summary", "fact", "source", "how_to_apply"]),
    );
}

// Returns the saved note's path and the reply for the agent.
pub fn save(config: &Config, req: SaveRequest, agent: &str) -> Result<(String, String), String> {
    let note_type = validate(&req)?;
    let root = &config.vault;
    let vault = Vault::load(root)?;
    let today = jiff::Zoned::now().date();

    let (path, previous, identity) = match &req.path {
        Some(path) => {
            let existing = change::find_note(&vault, path)?;
            change::require_confirmation(existing, req.user_confirmed)?;
            (
                existing.path.clone(),
                existing.frontmatter.as_ref().ok(),
                None,
            )
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

    let updating = req.path.is_some();
    let applies_when = match req.applies_when.as_deref().map(str::trim) {
        Some(when) if !when.is_empty() && note::Place::of(&path) != note::Place::Preferences => {
            return Err("applies_when is only for preferences (scope `all repos`)".to_string());
        }
        Some(when) => (!when.is_empty()).then(|| when.to_string()),
        None => previous.and_then(|fm| fm.applies_when.clone()),
    };
    // An update keeps the note's tags (some may come from the user, in Obsidian) and adds the requested ones.
    let mut tags = vec![REQUIRED_TAG.to_string()];
    for tag in previous.iter().flat_map(|fm| &fm.tags).chain(&req.tags) {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    let frontmatter = Frontmatter {
        note_type,
        scope: note::Place::of(&path).scope().to_string(),
        summary: Some(req.summary.trim().to_string()),
        created: previous.map_or(today, |fm| fm.created),
        updated: updating.then_some(today),
        tags,
        applies_when,
    };
    let body = format!(
        "# {}\n\n{}\n\n**Why:** {}\n**How to apply:** {}\n",
        req.title.trim(),
        req.fact.trim(),
        req.source.trim(),
        req.how_to_apply.trim()
    );

    let file = format!("{path}.md");
    let mut changed = vec![file.clone()];
    if let Some((identity_file, text)) = identity {
        fs::create_dir_all(root.join(&identity_file).parent().unwrap())
            .map_err(|e| e.to_string())?;
        write_atomic(&root.join(&identity_file), &text)?;
        changed.push(identity_file);
    }
    fs::create_dir_all(root.join(&file).parent().unwrap()).map_err(|e| e.to_string())?;
    write_atomic(&root.join(&file), &note::render(&frontmatter, &body)?)?;

    let max = config.limits.session_start_max_bytes;
    let omitted_before = context::omissions(&vault, note::Place::of(&path), max);
    let action = if updating { "update" } else { "create" };
    let (vault, commit_warning) =
        change::finish(config, changed, &format!("{action}: {path} ({agent})"))?;
    let mut reply = vec![format!("saved {path}")];
    let saved = vault
        .notes
        .iter()
        .find(|n| n.path == path)
        .ok_or("the saved note is missing")?;
    let others = vault.notes.iter().filter(|n| n.path != path);
    let similar = duplicates::similar(others, saved);
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
    let worsened = context::omissions(&vault, note::Place::of(&path), max)
        .into_iter()
        .filter(|(session, (items, _))| *items > omitted_before.get(session).map_or(0, |(n, _)| *n))
        .max_by_key(|(_, (items, _))| *items);
    if let Some((session, (_, left_out))) = worsened {
        let whose = session.map_or(String::new(), |p| format!(" in {p}"));
        reply.push(format!(
            "warning: a session{whose} now leaves {left_out} out of its {max}-byte budget: tidy memory with the user (merge or move notes)"
        ));
    }
    if !updating && path.starts_with(&format!("{PREFERENCES}/")) {
        reply.push(PREFERENCE_SCOPE_REMINDER.to_string());
    }
    reply.extend(commit_warning);
    Ok((path, reply.join("\n")))
}

// Returns the note type once every required field is there.
fn validate(req: &SaveRequest) -> Result<NoteType, String> {
    let note_type = require_fields(req)?;
    let tags = req.tags.iter().map(|tag| ("tags", tag));
    let applies_when = req.applies_when.iter().map(|when| ("applies_when", when));
    for (name, value) in text_fields(req).into_iter().chain(tags).chain(applies_when) {
        if let Some(kind) = secrets::find(value) {
            return Err(format!(
                "{name} looks like a secret ({kind}): never store secrets"
            ));
        }
    }
    for (name, value) in [("title", &req.title), ("summary", &req.summary)]
        .into_iter()
        .chain(req.applies_when.iter().map(|when| ("applies_when", when)))
    {
        if value.trim().contains('\n') {
            return Err(format!("{name} must be one line"));
        }
    }
    // The title becomes the file name.
    if slug::slugify(&req.title).is_empty() {
        return Err("title needs at least one letter or digit".to_string());
    }
    Ok(note_type)
}

fn text_fields(req: &SaveRequest) -> [(&'static str, &String); 5] {
    [
        ("title", &req.title),
        ("summary", &req.summary),
        ("fact", &req.fact),
        ("source", &req.source),
        ("how_to_apply", &req.how_to_apply),
    ]
}

// Agents guess field names; the unknown ones are listed with the field they probably meant.
fn require_fields(req: &SaveRequest) -> Result<NoteType, String> {
    let mut missing: Vec<&str> = req
        .note_type
        .is_none()
        .then_some("type")
        .into_iter()
        .collect();
    missing.extend(
        text_fields(req)
            .into_iter()
            .filter(|(_, value)| value.trim().is_empty())
            .map(|(name, _)| name),
    );
    let unknown: Vec<String> = req
        .unknown
        .keys()
        .map(|name| match name.as_str() {
            "content" | "body" => format!("`{name}` (use `fact` for the text)"),
            "description" => format!("`{name}` (use `summary` for the one-line description)"),
            _ => format!("`{name}`"),
        })
        .collect();
    match req.note_type {
        Some(note_type) if missing.is_empty() => Ok(note_type),
        _ => {
            let mut problems = vec![format!("missing: {}", missing.join(", "))];
            if !unknown.is_empty() {
                problems.push(format!("unknown: {}", unknown.join(", ")));
            }
            Err(problems.join("; "))
        }
    }
}

// Returns the note folder, plus the `_project.md` to create on the first save in an unknown repo.
fn target_folder(
    vault: &Vault,
    req: &SaveRequest,
) -> Result<(String, Option<(String, String)>), String> {
    if let Some(topic) = &req.topic {
        let key = slug::slugify(topic);
        if key.is_empty() {
            return Err("topic needs a name".to_string());
        }
        return Ok((format!("{TOPICS}/{key}"), None));
    }
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
    let folder = top
        .file_name()
        .ok_or(format!("no folder name in {}", dir.display()))?
        .to_string_lossy()
        .into_owned();
    // The vault scan skips dot folders, so such a project would be invisible.
    if folder.starts_with('.') {
        return Err(format!(
            "`{folder}` cannot name a project: pass scope to save in an existing project"
        ));
    }
    let remotes = git::remote_urls(dir);
    let key = match vault.projects.iter().find(|p| p.key == folder) {
        // Another repo already owns this folder name (resolve didn't match it): name the project after the remote.
        Some(taken) if taken.identity.is_ok() => remotes
            .first()
            .and_then(|r| project::key_from_remote(r))
            .ok_or(format!(
                "project `{folder}` belongs to another repo: pass scope to save there"
            ))?,
        _ => folder.clone(),
    };
    let identity_file = format!("{PROJECTS}/{key}/{IDENTITY_FILE}");
    if vault.root.join(&identity_file).exists() {
        return Err(format!("{identity_file} is invalid: fix it first"));
    }
    let text = project::render_identity(&remotes, std::slice::from_ref(&folder))?;
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
            git_autocommit: false,
            ..Config::new(root)
        };
        (tmp, config)
    }

    fn request(title: &str) -> SaveRequest {
        SaveRequest {
            path: None,
            note_type: Some(NoteType::Feedback),
            scope: Some(ALL_REPOS.into()),
            project_dir: None,
            topic: None,
            title: title.into(),
            summary: format!("{title} summary"),
            fact: "The fact.".into(),
            source: "the user said so on 2026-09-24".into(),
            how_to_apply: "Always.".into(),
            tags: vec!["git".into(), REQUIRED_TAG.into()],
            applies_when: None,
            user_confirmed: false,
            unknown: Default::default(),
        }
    }

    fn read(config: &Config, path: &str) -> Note {
        let text = fs::read_to_string(config.vault.join(format!("{path}.md"))).unwrap();
        Note::parse(path.into(), &text)
    }

    fn warning_of(reply: &str) -> Option<&str> {
        reply.lines().find(|l| l.contains(" now leaves "))
    }

    // A budget that holds exactly two notes, so a third tips the view over it.
    fn tight_vault() -> (tempfile::TempDir, Config) {
        let (tmp, mut config) = vault();
        save(&config, request("Rule aa"), "test").unwrap();
        save(&config, request("Rule bb"), "test").unwrap();
        let vault = Vault::load(&config.vault).unwrap();
        config.limits.session_start_max_bytes =
            crate::context::session_view(&vault, None, &[], usize::MAX)
                .text
                .len();
        (tmp, config)
    }

    #[test]
    fn a_new_preference_asks_to_confirm_its_scope_but_updates_and_other_notes_do_not() {
        let (tmp, config) = vault();
        let reply = save(&config, request("Rule aa"), "test").unwrap().1;
        assert!(reply.lines().any(|l| l == PREFERENCE_SCOPE_REMINDER));
        assert!(read(&config, "Preferences/rule-aa").frontmatter.is_ok());

        let mut update = request("Rule aa");
        update.path = Some("rule-aa".into());
        update.user_confirmed = true;
        assert!(
            !save(&config, update, "test")
                .unwrap()
                .1
                .contains("Preferences load")
        );

        let repo = tmp.path().join("my-repo");
        fs::create_dir(&repo).unwrap();
        let mut project = request("Deploy steps");
        project.note_type = Some(NoteType::Project);
        project.scope = None;
        project.project_dir = Some(repo);
        assert!(
            !save(&config, project, "test")
                .unwrap()
                .1
                .contains("Preferences load")
        );

        let mut topic = request("NAS backups");
        topic.topic = Some("homelab".into());
        assert!(
            !save(&config, topic, "test")
                .unwrap()
                .1
                .contains("Preferences load")
        );
    }

    fn update_of(path: &str, title: &str) -> SaveRequest {
        let mut update = request(title);
        update.path = Some(path.into());
        update.user_confirmed = true;
        update
    }

    fn when_of(config: &Config, path: &str) -> Option<String> {
        read(config, path).frontmatter.unwrap().applies_when
    }

    #[test]
    fn applies_when_is_set_kept_and_cleared_on_a_preference() {
        let (_tmp, config) = vault();
        let mut create = request("Rule aa");
        create.applies_when = Some("  editing Rust  ".into());
        save(&config, create, "test").unwrap();
        let path = "Preferences/rule-aa";
        assert_eq!(when_of(&config, path).as_deref(), Some("editing Rust"));

        save(&config, update_of(path, "Rule aa"), "test").unwrap();
        assert_eq!(when_of(&config, path).as_deref(), Some("editing Rust"));

        let mut change = update_of(path, "Rule aa");
        change.applies_when = Some("writing Java".into());
        save(&config, change, "test").unwrap();
        assert_eq!(when_of(&config, path).as_deref(), Some("writing Java"));

        let mut clear = update_of(path, "Rule aa");
        clear.applies_when = Some(String::new());
        save(&config, clear, "test").unwrap();
        assert_eq!(when_of(&config, path), None);
        let text = fs::read_to_string(config.vault.join(format!("{path}.md"))).unwrap();
        assert!(!text.contains("applies_when"), "{text}");
    }

    #[test]
    fn applies_when_is_refused_on_project_and_topic_notes_and_across_lines() {
        let (tmp, config) = vault();
        let repo = tmp.path().join("my-repo");
        fs::create_dir(&repo).unwrap();
        let mut project = request("Deploy steps");
        project.note_type = Some(NoteType::Project);
        project.scope = None;
        project.project_dir = Some(repo);
        project.applies_when = Some("deploying".into());
        let err = save(&config, project, "test").unwrap_err();
        assert!(err.contains("only for preferences"), "{err}");

        let mut topic = request("NAS backups");
        topic.topic = Some("homelab".into());
        topic.applies_when = Some("backing up".into());
        let err = save(&config, topic, "test").unwrap_err();
        assert!(err.contains("only for preferences"), "{err}");

        let mut lines = request("Rule aa");
        lines.applies_when = Some("a\nb".into());
        let err = save(&config, lines, "test").unwrap_err();
        assert!(err.contains("applies_when must be one line"), "{err}");
    }

    #[test]
    fn making_a_stated_rule_situational_needs_the_users_confirmation() {
        let (_tmp, config) = vault();
        save(&config, request("Rule aa"), "test").unwrap();
        let mut update = update_of("rule-aa", "Rule aa");
        update.applies_when = Some("editing Rust".into());
        update.user_confirmed = false;
        let err = save(&config, update, "test").unwrap_err();
        assert!(err.contains("user_confirmed"), "{err}");
        assert_eq!(when_of(&config, "Preferences/rule-aa"), None);
    }

    #[test]
    fn a_save_that_fits_the_session_budget_adds_no_warning() {
        let (_tmp, mut config) = tight_vault();
        config.limits.session_start_max_bytes *= 2;
        let reply = save(&config, request("Rule cc"), "test").unwrap().1;
        assert_eq!(warning_of(&reply), None, "{reply}");
    }

    #[test]
    fn a_save_that_tips_the_session_over_its_budget_says_so_and_still_saves() {
        let (_tmp, config) = tight_vault();
        let (path, reply) = save(&config, request("Rule cc"), "test").unwrap();
        let vault = Vault::load(&config.vault).unwrap();
        let left =
            crate::context::session_view(&vault, None, &[], config.limits.session_start_max_bytes);
        let left_out = left.left_out.expect("the view is over its budget");
        let max = config.limits.session_start_max_bytes;
        assert_eq!(
            warning_of(&reply),
            Some(
                format!("warning: a session now leaves {left_out} out of its {max}-byte budget: tidy memory with the user (merge or move notes)").as_str()
            ),
            "{reply}"
        );
        assert!(vault.has_note(&path));
    }

    #[test]
    fn a_preference_that_crowds_out_a_tight_project_s_notes_names_that_project() {
        let (_tmp, mut config) = vault();
        save(&config, request("Rule aa"), "test").unwrap();
        for i in 0..3 {
            let path = config.vault.join(format!("{PROJECTS}/alpha/fact-{i}.md"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("---\ntype: project\nscope: alpha\nsummary: fact {i}\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n# Fact\n\n**Why:** x\n")).unwrap();
        }
        let tight = Vault::load(&config.vault).unwrap();
        let max = crate::context::session_view(&tight, Some("alpha"), &[], usize::MAX)
            .text
            .len();
        config.limits.session_start_max_bytes = max;
        let reply = save(&config, request("Rule bb"), "test").unwrap().1;
        let warning = warning_of(&reply).unwrap_or_else(|| panic!("{reply}"));
        assert!(
            warning.starts_with("warning: a session in alpha now leaves "),
            "{warning}"
        );
    }

    #[test]
    fn a_preference_that_pushes_only_the_topics_list_out_of_the_session_warns() {
        let (_tmp, mut config) = vault();
        save(&config, request("Rule aa"), "test").unwrap();
        save(&config, request("Rule bb"), "test").unwrap();
        for i in 0..8 {
            let topic = config
                .vault
                .join(format!("Topics/homelab-number-{i}/nas.md"));
            fs::create_dir_all(topic.parent().unwrap()).unwrap();
            fs::write(&topic, "x").unwrap();
        }
        let before = Vault::load(&config.vault).unwrap();
        let (path, _) = save(&config, request("Rule cc"), "test").unwrap();
        let with_cc = Vault::load(&config.vault).unwrap();
        fs::remove_file(config.vault.join(format!("{path}.md"))).unwrap();
        let view = |vault: &Vault, max| crate::context::session_view(vault, None, &[], max);
        let max = (0..20_000)
            .find(|&max| {
                view(&before, max).left_out.is_none()
                    && view(&with_cc, max).left_out.as_deref() == Some("the topics list")
            })
            .expect(
                "a budget that fits two preferences and the topics, then loses only the topics",
            );
        config.limits.session_start_max_bytes = max;
        let reply = save(&config, request("Rule cc"), "test").unwrap().1;
        let warning = warning_of(&reply).unwrap_or_else(|| panic!("{reply}"));
        assert!(warning.contains("the topics list"), "{warning}");
    }

    #[test]
    fn an_update_that_leaves_the_overflow_as_it_was_adds_no_warning() {
        let (_tmp, config) = tight_vault();
        save(&config, request("Rule cc"), "test").unwrap();
        let mut update = request("Rule cc");
        update.path = Some("Preferences/rule-cc".into());
        update.user_confirmed = true;
        let reply = save(&config, update, "test").unwrap().1;
        assert_eq!(warning_of(&reply), None, "{reply}");
    }

    #[test]
    fn creates_a_valid_note_and_updates_the_index() {
        let (_tmp, config) = vault();
        let reply = save(&config, request("Commit rules"), "test").unwrap().1;
        assert_eq!(
            reply,
            format!("saved Preferences/commit-rules\n{PREFERENCE_SCOPE_REMINDER}")
        );

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
        assert!(
            crate::check::check(
                &vault,
                &crate::config::Config::new(Default::default()).limits
            )
            .is_empty()
        );
    }

    #[test]
    fn rejects_incomplete_or_unsafe_notes() {
        let (_tmp, config) = vault();
        type Change = fn(&mut SaveRequest);
        let cases: [(Change, &str); 6] = [
            (|r| r.source = " ".into(), "missing: source"),
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
            (
                |r| r.title = "???".into(),
                "title needs at least one letter or digit",
            ),
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
    fn an_update_keeps_the_notes_tags_and_adds_the_requested_ones() {
        let (_tmp, config) = vault();
        fs::write(
            config.vault.join("Preferences/commit-rules.md"),
            "---\ntype: feedback\nscope: all repos\nsummary: s\ncreated: 2026-09-14\ntags: [agent-memory, from-obsidian, git]\n---\n\n# Commit rules\n",
        )
        .unwrap();

        let mut update = request("Commit rules");
        update.path = Some("Preferences/commit-rules".into());
        update.user_confirmed = true;
        update.tags = vec!["new".into(), "git".into()];
        save(&config, update, "test").unwrap();
        let fm = read(&config, "Preferences/commit-rules")
            .frontmatter
            .unwrap();
        assert_eq!(fm.tags, [REQUIRED_TAG, "from-obsidian", "git", "new"]);
    }

    #[test]
    fn updating_a_note_with_broken_frontmatter_repairs_it_as_an_update() {
        let (_tmp, config) = vault();
        fs::write(
            config.vault.join("Preferences/broken.md"),
            "no frontmatter\n",
        )
        .unwrap();
        let mut update = request("Broken");
        update.note_type = Some(NoteType::User);
        update.path = Some("Preferences/broken".into());
        save(&config, update, "test").unwrap();
        let fm = read(&config, "Preferences/broken").frontmatter.unwrap();
        let today = jiff::Zoned::now().date();
        assert_eq!((fm.created, fm.updated), (today, Some(today)));
        assert_eq!(fm.tags, [REQUIRED_TAG, "git"]);
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

        // Unrelated titles: "First fact" and "Second fact" would be near-duplicates.
        for title in ["Deploy steps", "Database host"] {
            let mut req = request(title);
            req.note_type = Some(NoteType::Project);
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
            read(&config, "Projects/my-repo/deploy-steps")
                .frontmatter
                .unwrap()
                .scope,
            "my-repo"
        );
        assert!(
            crate::check::check(
                &vault,
                &crate::config::Config::new(Default::default()).limits
            )
            .is_empty()
        );
    }

    #[test]
    fn a_repo_whose_folder_name_is_taken_gets_a_project_named_after_its_remote() {
        let (tmp, config) = vault();
        let checkout = |parent: &str, remote: &str| {
            let repo = tmp.path().join(parent).join("api");
            fs::create_dir_all(&repo).unwrap();
            git::init(&repo).unwrap();
            std::process::Command::new("git")
                .args([
                    "-C",
                    repo.to_str().unwrap(),
                    "remote",
                    "add",
                    "origin",
                    remote,
                ])
                .status()
                .unwrap();
            repo
        };
        let save_in = |repo: std::path::PathBuf, title: &str| {
            let mut req = request(title);
            req.note_type = Some(NoteType::Project);
            req.scope = None;
            req.project_dir = Some(repo);
            save(&config, req, "test").unwrap().1
        };
        let mine = checkout("mine", "git@github.com:me/api.git");
        let theirs = checkout("theirs", "git@gitlab.com:someone-else/api.git");
        assert_eq!(save_in(mine, "Mine"), "saved Projects/api/mine");
        assert_eq!(
            save_in(theirs.clone(), "Theirs"),
            "saved Projects/someone-else-api/theirs"
        );
        assert_eq!(
            save_in(theirs, "Deploy"),
            "saved Projects/someone-else-api/deploy",
            "the new project is then found by its remote"
        );
        assert!(
            crate::check::check(
                &Vault::load(&config.vault).unwrap(),
                &crate::config::Config::new(Default::default()).limits
            )
            .is_empty()
        );
    }

    #[test]
    fn reports_similar_notes() {
        let (_tmp, config) = vault();
        let mut first = request("Commit rules");
        first.summary = "commit locally per step, never push".into();
        save(&config, first, "test").unwrap();

        let mut second = request("Commit locally");
        second.summary = "commit each step locally, no push".into();
        let reply = save(&config, second, "test").unwrap().1;
        assert!(
            reply.contains(
                "similar notes, merge if they say the same: [[Preferences/commit-rules]]\n"
            ),
            "{reply}"
        );
    }

    #[test]
    fn a_topic_note_goes_to_its_folder_with_the_topic_as_scope() {
        let (_tmp, config) = vault();
        let mut req = request("NAS backups");
        req.note_type = Some(NoteType::Reference);
        req.topic = Some("Home Lab".into());
        req.scope = Some("app".into());
        assert_eq!(
            save(&config, req, "test").unwrap().1,
            "saved Topics/home-lab/nas-backups"
        );
        let note = read(&config, "Topics/home-lab/nas-backups");
        assert_eq!(note.frontmatter.unwrap().scope, "home-lab");
        assert!(
            crate::check::check(
                &Vault::load(&config.vault).unwrap(),
                &crate::config::Config::new(Default::default()).limits
            )
            .is_empty()
        );

        let mut nameless = request("x");
        nameless.topic = Some("  ".into());
        assert_eq!(
            save(&config, nameless, "test").unwrap_err(),
            "topic needs a name"
        );
    }
}
