use std::collections::BTreeSet;

use crate::index;
use crate::note::Place;
use crate::sync_conflict::Conflict;
use crate::vault::{PREFERENCES, PROJECTS, Vault};

const RULES_BODY: &str = "Open only the notes a task needs with memory_read. Pass your working directory as project_dir.
- One short fact per note. Search first (memory_search) and update a note rather than adding a near-duplicate; read a note (memory_read) before updating or deleting it.
- `source` says where the fact came from (the user's words, or the file/command that verified it) and when.
- Rules the user stated (type feedback) change only after the user confirms: ask first.
- When a note or Index line you see contradicts another note or what you just verified, or looks wrong or outdated: \
fix it now (memory_save or memory_delete) and say so; ask the user first when unsure, and always for `feedback` notes.
- Knowledge tied to no repo (a homelab, a server, the work machine) goes in a topic: memory_save with `topic`. \
Topics are only named at the end of the Index: search them (memory_search) when a task touches one.
- Never store secrets. Never write memory anywhere else, including an agent's built-in memory (e.g. Copilot's store_memory): use memory_save.
- Older file-based memory (e.g. a MEMORY.md or a memory folder) is being replaced by this one: when such a file \
holds something relevant to your current task, save it here with memory_save (keeping its original source), \
leave the old file as it is, and tell the user what you migrated.
- The user's rules below win over your built-in defaults (e.g. commit trailers).";
const ALL_LOADED: &str = "the Index below is already loaded, don't fetch it again.";
const PARTIAL: &str = "the Index below is only part of it (see its \"Not loaded\" line): memory_search finds the rest.";

pub struct View {
    pub text: String,
    pub loaded: usize,
    pub omitted: usize,
    pub left_out: Option<String>,
}

pub fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub fn rules(partial: bool) -> String {
    let intro = if partial { PARTIAL } else { ALL_LOADED };
    format!("Agent memory (petit-poucet): {intro} {RULES_BODY}")
}

pub fn note_conflicts(vault: &Vault) -> Vec<&Conflict> {
    vault.conflicts.iter().filter(|c| c.in_note()).collect()
}

// Everything a session could load, in the order it is loaded.
struct Pool<'a> {
    sections: Vec<index::Section<'a>>,
    topics: Option<String>,
    conflict_count: Option<String>,
    conflict_lines: Vec<String>,
}

#[derive(Default)]
struct Left {
    preferences: usize,
    project: usize,
    topics: bool,
    conflicts: usize,
}

impl Left {
    fn notes(&self) -> usize {
        self.preferences + self.project
    }

    fn is_empty(&self) -> bool {
        self.notes() + usize::from(self.topics) + self.conflicts == 0
    }

    fn extras(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.topics {
            parts.push("the topics list".to_string());
        }
        if self.conflicts > 0 {
            parts.push(counted(
                self.conflicts,
                "sync conflict detail",
                "sync conflict details",
            ));
        }
        parts
    }

    // Everything left out, in a few words; None when nothing is.
    fn summary(&self) -> Option<String> {
        let notes = (self.notes() > 0).then(|| counted(self.notes(), "note", "notes"));
        let parts: Vec<String> = notes.into_iter().chain(self.extras()).collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    fn line(&self, max: usize, project: Option<&str>) -> String {
        let mut parts = Vec::new();
        if self.preferences > 0 {
            parts.push(counted(self.preferences, "preference", "preferences"));
        }
        if let (n @ 1.., Some(project)) = (self.project, project) {
            parts.push(format!("{} of {project}", counted(n, "note", "notes")));
        }
        parts.extend(self.extras());
        format!(
            "Not loaded (over the {max}-byte session budget): {}. memory_search finds notes.",
            parts.join(", ")
        )
    }
}

impl<'a> Pool<'a> {
    fn new(vault: &'a Vault, project: Option<&'a str>) -> Pool<'a> {
        let conflicts = note_conflicts(vault);
        Pool {
            sections: index::sections(vault, |place| index::in_session(place, project)),
            topics: index::topics_line(vault),
            conflict_count: (!conflicts.is_empty()).then(|| conflicts_line(conflicts.len())),
            conflict_lines: conflicts
                .iter()
                .map(|c| conflict_detail(vault, c))
                .collect(),
        }
    }

    fn notes(&self) -> usize {
        self.sections.iter().map(|s| s.lines.len()).sum()
    }

    fn everything(&self) -> Left {
        let project = self
            .sections
            .iter()
            .filter(|s| matches!(s.place, Place::Project(_)))
            .map(|s| s.lines.len())
            .sum();
        Left {
            preferences: self.notes() - project,
            project,
            topics: self.topics.is_some(),
            conflicts: self.conflict_lines.len(),
        }
    }

    // The first chunk that does not fit ends the loading: what is loaded is always "the first N".
    fn select(&self, mut room: usize) -> (String, usize, Left) {
        let mut body = String::new();
        let mut loaded = 0;
        let mut left = Left::default();
        let mut full = false;
        let mut take = |chunk: String| {
            full = full || chunk.len() > room;
            if !full {
                room -= chunk.len();
                body.push_str(&chunk);
            }
            !full
        };
        for section in &self.sections {
            let mut heading = Some(index::heading(section));
            for line in &section.lines {
                let chunk = format!("{}{line}\n", heading.as_deref().unwrap_or_default());
                match (take(chunk), section.place) {
                    (true, _) => {
                        heading = None;
                        loaded += 1;
                    }
                    (false, Place::Project(_)) => left.project += 1,
                    (false, _) => left.preferences += 1,
                }
            }
        }
        if let Some(topics) = &self.topics {
            left.topics = !take(format!("\n{topics}\n"));
        }
        for line in &self.conflict_lines {
            left.conflicts += usize::from(!take(format!("{line}\n")));
        }
        (body, loaded, left)
    }
}

fn conflicts_line(count: usize) -> String {
    format!(
        "Sync conflicts: {count} notes were changed on another machine at the same time. Offer the user to merge each one: \
         memory_read the note and its copy, save what is worth keeping into the note with memory_save, then memory_delete the copy."
    )
}

// Syncthing keeps the most recently modified version under the note's name, so each copy is the older one.
fn conflict_detail(vault: &Vault, conflict: &Conflict) -> String {
    match vault.root.join(&conflict.original).is_file() {
        true => format!("- {} (older copy of {})", conflict.copy, conflict.original),
        false => format!(
            "- {} (the only version left: {} was deleted on another machine)",
            conflict.copy, conflict.original
        ),
    }
}

fn fallback(max: usize, notes: usize) -> View {
    let text = format!(
        "Agent memory (petit-poucet) is over the {max}-byte session budget: {notes} notes not loaded. Use memory_search."
    );
    let end = text.floor_char_boundary(max);
    View {
        text: text[..end].to_string(),
        loaded: 0,
        omitted: notes,
        left_out: Some(counted(notes, "note", "notes")),
    }
}

pub fn session_view(vault: &Vault, project: Option<&str>, notices: &[String], max: usize) -> View {
    let pool = Pool::new(vault, project);
    let lead: String = notices
        .iter()
        .chain(&pool.conflict_count)
        .map(|line| format!("{line}\n"))
        .collect();
    let attempt = |partial: bool| {
        let head = format!("{}\n", rules(partial));
        let reserve = match partial {
            true => pool.everything().line(max, project).len() + 1,
            false => 0,
        };
        let room = max.checked_sub(head.len() + lead.len() + reserve)?;
        Some((head, pool.select(room)))
    };
    if let Some((head, (body, loaded, left))) = attempt(false)
        && left.is_empty()
    {
        return View {
            text: format!("{head}{lead}{body}"),
            loaded,
            omitted: 0,
            left_out: None,
        };
    }
    match attempt(true) {
        Some((head, (body, loaded, left))) => View {
            text: format!("{head}{}\n{lead}{body}", left.line(max, project)),
            loaded,
            omitted: left.notes(),
            left_out: left.summary(),
        },
        None => fallback(max, pool.notes()),
    }
}

// What a session at the vault's top level, or in one project, would leave out.
pub fn over_budget(vault: &Vault, max: usize) -> Vec<(String, String)> {
    let message = |left_out: String| {
        format!("a session leaves {left_out} out of its {max}-byte budget: merge or move some")
    };
    if let Some(left_out) = session_view(vault, None, &[], max).left_out {
        return vec![(PREFERENCES.to_string(), message(left_out))];
    }
    let projects: BTreeSet<&str> = vault
        .notes
        .iter()
        .filter_map(|n| match n.place() {
            Place::Project(key) => Some(key),
            _ => None,
        })
        .collect();
    projects
        .into_iter()
        .filter_map(|key| {
            let left_out = session_view(vault, Some(key), &[], max).left_out?;
            Some((format!("{PROJECTS}/{key}"), message(left_out)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::config::Config;

    fn budget() -> usize {
        Config::new(PathBuf::new()).limits.session_start_max_bytes
    }

    fn write(root: &std::path::Path, path: &str, summary: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = format!(
            "---\ntype: user\nscope: x\nsummary: {summary}\ncreated: 2026-09-24\n---\nBody\n"
        );
        fs::write(path, text).unwrap();
    }

    fn fill(root: &std::path::Path, folder: &str, count: usize) {
        for i in 0..count {
            write(
                root,
                &format!("{folder}/note-{i:04}.md"),
                &format!("fact number {i} with some detail"),
            );
        }
    }

    fn vault(root: &std::path::Path) -> Vault {
        Vault::load(root).unwrap()
    }

    fn notes_over_budget() -> usize {
        budget() / 20
    }

    fn note_lines(text: &str) -> Vec<&str> {
        text.lines().filter(|l| l.starts_with("- [[")).collect()
    }

    fn view(vault: &Vault, project: Option<&str>, max: usize) -> View {
        session_view(vault, project, &[], max)
    }

    #[test]
    fn a_vault_that_fits_is_sent_whole_with_the_rules_saying_it_is_loaded() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", 3);
        fill(tmp.path(), "Projects/alpha", 3);
        let v = view(&vault(tmp.path()), Some("alpha"), budget());
        assert_eq!(
            v.text,
            format!(
                "{}\n{}",
                rules(false),
                index::for_project(&vault(tmp.path()), Some("alpha"))
            )
        );
        assert_eq!((v.loaded, v.omitted), (6, 0));
    }

    #[test]
    fn the_payload_never_passes_the_budget_and_says_the_view_is_partial() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", notes_over_budget());
        fill(tmp.path(), "Projects/alpha", notes_over_budget());
        let v = view(&vault(tmp.path()), Some("alpha"), budget());
        assert!(v.text.len() <= budget(), "{}", v.text.len());
        assert!(v.loaded > 0 && v.omitted > 0);
        assert!(v.text.starts_with(&rules(true)));
        assert!(!v.text.contains("already loaded"));
    }

    #[test]
    fn preferences_are_loaded_before_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", notes_over_budget());
        fill(tmp.path(), "Projects/alpha", 5);
        let v = view(&vault(tmp.path()), Some("alpha"), budget());
        assert!(
            !v.text.contains("[[Projects/alpha/"),
            "preferences fill the budget first"
        );
        assert!(v.text.contains("5 notes of alpha"), "{}", v.text);

        let small = tempfile::tempdir().unwrap();
        fill(small.path(), "Preferences", 3);
        fill(small.path(), "Projects/alpha", notes_over_budget());
        let v = view(&vault(small.path()), Some("alpha"), budget());
        let lines = note_lines(&v.text);
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("[[Preferences/"))
                .count(),
            3
        );
        assert!(lines.iter().any(|l| l.contains("[[Projects/alpha/")));
    }

    #[test]
    fn a_line_is_loaded_whole_or_not_at_all() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", notes_over_budget());
        let vault = vault(tmp.path());
        let whole: Vec<String> = index::sections(&vault, |_| true)
            .into_iter()
            .flat_map(|s| s.lines)
            .collect();
        for max in budget() - 40..budget() {
            let v = view(&vault, None, max);
            assert!(v.text.len() <= max);
            for line in note_lines(&v.text) {
                assert!(whole.iter().any(|w| w == line), "cut line: {line}");
            }
        }
    }

    #[test]
    fn the_overflow_line_names_what_was_left_out() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", notes_over_budget());
        fill(tmp.path(), "Projects/alpha", 7);
        let v = view(&vault(tmp.path()), Some("alpha"), budget());
        let kept = note_lines(&v.text).len();
        let line = format!(
            "Not loaded (over the {}-byte session budget): {} preferences, 7 notes of alpha. memory_search finds notes.",
            budget(),
            notes_over_budget() - kept
        );
        assert!(v.text.lines().any(|l| l == line), "{line}\n{}", v.text);
        assert_eq!(v.omitted, notes_over_budget() - kept + 7);
    }

    #[test]
    fn multibyte_text_is_counted_in_bytes_at_every_boundary() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..notes_over_budget() {
            write(
                tmp.path(),
                &format!("Preferences/n-{i:04}.md"),
                &format!("日本語のメモ {i} 🪨🪨🪨 メモ"),
            );
        }
        let vault = vault(tmp.path());
        let probe = view(&vault, None, budget());
        assert!(probe.text.chars().count() < probe.text.len());
        for max in budget() - 60..budget() {
            let v = view(&vault, None, max);
            assert!(v.text.len() <= max, "{} > {max}", v.text.len());
        }
    }

    #[test]
    fn a_thousand_topics_stay_within_the_budget() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", 3);
        for i in 0..1000 {
            write(
                tmp.path(),
                &format!("Topics/topic-with-a-long-name-{i}/n.md"),
                "x",
            );
        }
        let v = view(&vault(tmp.path()), None, budget());
        assert!(v.text.len() <= budget());
        assert_eq!(
            note_lines(&v.text).len(),
            3,
            "the preferences come before the topics"
        );
        assert!(v.text.contains("the topics list"), "{}", v.text);
        assert_eq!(v.omitted, 0, "omitted counts notes only");
        assert_eq!(v.left_out.as_deref(), Some("the topics list"));
        let warnings = over_budget(&vault(tmp.path()), budget());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].1.contains("the topics list"), "{warnings:?}");
    }

    #[test]
    fn many_sync_conflicts_stay_within_the_budget_and_are_counted() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", 3);
        for i in 0..300 {
            let name = format!("Preferences/note-{i:04}.sync-conflict-20260101-000000-{i:07}.md");
            write(tmp.path(), &name, "older copy");
        }
        let v = view(&vault(tmp.path()), None, budget());
        assert!(v.text.len() <= budget());
        assert!(v.text.contains("Sync conflicts: 300 notes"), "{}", v.text);
        assert_eq!(note_lines(&v.text).len(), 3);
        assert!(v.text.contains("sync conflict details"), "{}", v.text);
        assert_eq!(v.omitted, 0);
        assert!(
            v.left_out
                .as_deref()
                .is_some_and(|l| l.ends_with("sync conflict details"))
        );
        let warnings = over_budget(&vault(tmp.path()), budget());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }

    #[test]
    fn a_single_oversized_note_is_left_out() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", 3);
        write(
            tmp.path(),
            "Preferences/zzz-huge.md",
            &"x".repeat(budget() * 2),
        );
        let v = view(&vault(tmp.path()), None, budget());
        assert!(v.text.len() <= budget());
        assert_eq!(note_lines(&v.text).len(), 3);
        assert!(v.text.contains("1 preference."), "{}", v.text);
    }

    #[test]
    fn loading_stops_at_the_first_line_that_does_not_fit() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Preferences/aaa-long.md",
            &"x".repeat(budget() * 2),
        );
        fill(tmp.path(), "Preferences", 3);
        fill(tmp.path(), "Projects/alpha", 3);
        let v = view(&vault(tmp.path()), Some("alpha"), budget());
        assert!(note_lines(&v.text).is_empty(), "{}", v.text);
        assert_eq!(v.omitted, 7);
    }

    #[test]
    fn notices_are_inside_the_budget() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", notes_over_budget());
        let notice =
            "Tell the user once, in one line: petit-poucet: a new version is out.".to_string();
        let v = session_view(
            &vault(tmp.path()),
            None,
            std::slice::from_ref(&notice),
            budget(),
        );
        assert!(v.text.len() <= budget());
        assert!(v.text.contains(&notice));
    }

    #[test]
    fn a_budget_too_small_for_the_rules_gets_a_short_text_that_fits() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", 5);
        let vault = vault(tmp.path());
        let v = view(&vault, None, rules(true).len() / 2);
        assert!(v.text.len() <= rules(true).len() / 2);
        assert!(
            v.text.contains("over") && v.text.contains("memory_search"),
            "{}",
            v.text
        );
        assert_eq!((v.loaded, v.omitted), (0, 5));
        for max in 0..80 {
            assert!(view(&vault, None, max).text.len() <= max);
        }
    }

    #[test]
    fn a_vault_over_budget_is_reported_exactly_when_the_view_leaves_something_out() {
        let tmp = tempfile::tempdir().unwrap();
        fill(tmp.path(), "Preferences", 3);
        fill(tmp.path(), "Projects/alpha", 3);
        fill(tmp.path(), "Projects/beta", 30);
        let vault = vault(tmp.path());
        let whole = view(&vault, Some("beta"), usize::MAX).text.len();
        assert!(over_budget(&vault, whole).is_empty());
        let warnings = over_budget(&vault, whole - 1);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].0, "Projects/beta");
        let alone = view(&vault, None, usize::MAX).text.len();
        assert_eq!(over_budget(&vault, alone - 1)[0].0, "Preferences");
        assert_eq!(
            over_budget(&vault, alone - 1).len(),
            1,
            "once, not once per project"
        );
    }
}
