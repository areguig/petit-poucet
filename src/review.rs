use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::time::SystemTime;

use jiff::ToSpan;
use jiff::civil::Date;

use crate::config::{Config, Review};
use crate::index::NO_SUMMARY;
use crate::note::Note;
use crate::usage::{self, Activity, Usage};
use crate::vault::{PREFERENCES, PROJECTS, Vault};
use crate::{check, cleanup};

// Antigravity CLI saves a tool result over about 4 KB to a file: the smallest limit of the supported agents.
pub const PAGE_BYTES: usize = 3 * 1024;

const LEGEND: &str = "slug | type | created[/updated] | reads and last read | summary";

type Item = (String, String);

// Every note in a small vault or on demand, else folders by priority; then the whole vault's problems, never cut.
pub fn review(config: &Config, page: usize, full: bool, agent: &str) -> Result<String, String> {
    let started = SystemTime::now();
    let vault = Vault::load(&config.vault)?;
    let since = cleanup::last(&vault.root);
    let changed = changed_since(&vault, since);
    let activity = usage::load(&vault.root);
    let findings: Vec<Item> = check::check(&vault)
        .into_iter()
        .map(|issue| ("check".to_string(), format!("- {issue}")))
        .collect();
    let limits = &config.review;
    let mut items = match full || vault.notes.len() <= limits.full_review_max_notes {
        true => vault.notes.iter().map(|n| item(n, &activity)).collect(),
        false => by_priority(
            &vault,
            &changed,
            &activity,
            limits,
            jiff::Zoned::now().date(),
            &findings,
        ),
    };
    let shown = items.len();
    items.extend(findings);
    let pages = paginate(&items);
    let Some(body) = pages.get(page.wrapping_sub(1)) else {
        return Err(format!(
            "no page {page}: the review has {} pages",
            pages.len()
        ));
    };
    let scope = scope(since, changed.len(), shown, vault.notes.len(), limits);
    let mut out = format!(
        "vault: {}\n{scope}, then the problems found in the whole vault\npage {page} of {} ({LEGEND})\n{body}",
        vault.root.display(),
        pages.len()
    );
    if page < pages.len() {
        let full = if full { " and full=true" } else { "" };
        writeln!(out, "more: call memory_review with page={}{full}", page + 1).unwrap();
    } else {
        cleanup::record(config, started, agent)?;
    }
    Ok(out)
}

fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

// Whole folders: preferences and changed folders always, then folders by most recent use, the rest while pages last.
fn by_priority(
    vault: &Vault,
    changed: &[&Note],
    activity: &Activity,
    limits: &Review,
    today: Date,
    findings: &[Item],
) -> Vec<Item> {
    let mut folders: BTreeMap<&str, Vec<&Note>> = BTreeMap::new();
    for note in &vault.notes {
        folders.entry(folder(&note.path)).or_default().push(note);
    }
    // Sorted, so Preferences comes first.
    let always: BTreeSet<&str> = changed
        .iter()
        .map(|n| folder(&n.path))
        .chain([PREFERENCES])
        .collect();
    let used = last_used(activity);
    let since = today.saturating_sub(limits.active_days.days());
    let mut active: Vec<&str> = folders
        .keys()
        .copied()
        .filter(|f| !always.contains(f) && used.get(*f).is_some_and(|day| *day >= since))
        .collect();
    active.sort_by_key(|f| (Reverse(used[*f]), *f));
    let rest = folders
        .keys()
        .copied()
        .filter(|f| !always.contains(f) && !active.contains(f));
    let lines = |f: &str| -> Vec<Item> {
        folders
            .get(f)
            .into_iter()
            .flatten()
            .map(|n| item(n, activity))
            .collect()
    };

    let mut items: Vec<Item> = always.iter().flat_map(|f| lines(f)).collect();
    for f in active.iter().copied().chain(rest) {
        let before = items.len();
        items.extend(lines(f));
        let with_findings: Vec<Item> = items.iter().chain(findings).cloned().collect();
        if paginate(&with_findings).len() > limits.review_max_pages {
            items.truncate(before);
            break;
        }
    }
    items
}

// The last day each folder was used: a note read in it, or a session loading its project.
fn last_used(activity: &Activity) -> BTreeMap<String, Date> {
    let reads = activity
        .notes
        .iter()
        .map(|(path, u)| (folder(path).to_string(), u.last_read));
    let sessions = activity
        .projects
        .iter()
        .map(|(key, day)| (format!("{PROJECTS}/{key}"), *day));
    let mut used = BTreeMap::new();
    for (folder, day) in reads.chain(sessions) {
        let last = used.entry(folder).or_insert(day);
        *last = (*last).max(day);
    }
    used
}

fn scope(
    since: Option<SystemTime>,
    changed: usize,
    shown: usize,
    total: usize,
    limits: &Review,
) -> String {
    let date = since
        .and_then(|t| jiff::Timestamp::try_from(t).ok())
        .map(|t| t.strftime("%Y-%m-%d").to_string());
    match (shown == total, date) {
        (true, None) => format!("first cleanup: all {total} notes"),
        (true, Some(date)) => {
            format!("all {total} notes, {changed} changed since the last cleanup ({date})")
        }
        (false, date) => format!(
            "{shown} of {total} notes: preferences, folders changed since {}, then folders by recent use, up to {} pages (full=true for every note)",
            date.unwrap_or_default(),
            limits.review_max_pages
        ),
    }
}

// Every note before the first cleanup; a note whose file time can't be read counts as changed.
fn changed_since(vault: &Vault, since: Option<SystemTime>) -> Vec<&Note> {
    let modified = |note: &Note| {
        fs::metadata(vault.root.join(format!("{}.md", note.path)))
            .and_then(|m| m.modified())
            .ok()
    };
    vault
        .notes
        .iter()
        .filter(|note| since.is_none_or(|since| modified(note).is_none_or(|m| m > since)))
        .collect()
}

// Items are (section, line); a page that continues a section repeats its heading.
fn paginate(items: &[Item]) -> Vec<String> {
    let mut pages = vec![String::new()];
    let mut heading = "";
    for (section, line) in items {
        let starts = section != heading;
        let size = line.len() + 1 + if starts { section.len() + 4 } else { 0 };
        let current = pages.last().unwrap();
        if !current.is_empty() && current.len() + size > PAGE_BYTES {
            pages.push(String::new());
        }
        let page = pages.last_mut().unwrap();
        if starts || page.is_empty() {
            writeln!(page, "## {section}").unwrap();
            heading = section;
        }
        writeln!(page, "{line}").unwrap();
    }
    pages
}

// One (folder, line) per note.
fn item(note: &Note, activity: &Activity) -> Item {
    let (folder, slug) = note.path.rsplit_once('/').unwrap_or(("", &note.path));
    (
        folder.to_string(),
        line(note, slug, activity.notes.get(&note.path)),
    )
}

fn line(note: &Note, slug: &str, usage: Option<&Usage>) -> String {
    let fm = note.frontmatter.as_ref().ok();
    let dates = fm.map_or("-".to_string(), |f| match f.updated {
        Some(updated) => format!("{}/{updated}", f.created),
        None => f.created.to_string(),
    });
    let reads = usage.map_or("never".to_string(), |u| {
        format!("{}r {}", u.reads, u.last_read)
    });
    format!(
        "- {slug} | {} | {dates} | {reads} | {}",
        fm.map_or("invalid", |f| f.note_type.name()),
        note.summary().unwrap_or(NO_SUMMARY),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn note(scope: &str, extra: &str) -> String {
        format!(
            "---\ntype: feedback\nscope: {scope}\nsummary: s\ncreated: 2026-09-01\n{extra}tags: [agent-memory]\n---\n**Why:** x\n"
        )
    }

    #[test]
    fn a_small_vault_is_one_page_of_short_lines_under_their_folder() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Projects/app")).unwrap();
        fs::create_dir(tmp.path().join("Preferences")).unwrap();
        fs::write(
            tmp.path().join("Preferences/rule.md"),
            note("all repos", "updated: 2026-09-20\n"),
        )
        .unwrap();
        fs::write(tmp.path().join("Projects/app/fact.md"), note("app", "")).unwrap();
        fs::write(tmp.path().join("Projects/app/broken.md"), "no frontmatter").unwrap();
        usage::record_read(
            tmp.path(),
            "Preferences/rule",
            jiff::civil::date(2026, 9, 24),
        )
        .unwrap();
        let config = Config {
            git_autocommit: false,
            ..Config::new(tmp.path().to_path_buf())
        };

        let text = review(&config, 1, false, "test").unwrap();
        assert!(
            text.contains("\nfirst cleanup: all 3 notes, then the problems"),
            "{text}"
        );
        assert!(text.contains("\npage 1 of 1 ("), "{text}");
        assert!(
            text.contains(
                "## Preferences\n- rule | feedback | 2026-09-01/2026-09-20 | 1r 2026-09-24 | s\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("## Projects/app\n- broken | invalid | - | never | ")
                && text.contains("\n- fact | feedback | 2026-09-01 | never | s\n"),
            "{text}"
        );
        assert!(
            text.contains("## check\n") && text.contains("- Index.md: error: missing\n"),
            "{text}"
        );
        assert!(!text.contains("more:"), "{text}");
        assert_eq!(
            review(&config, 2, false, "test").unwrap_err(),
            "no page 2: the review has 1 pages"
        );
        assert!(review(&config, 0, false, "test").is_err());
    }

    // A valid note, so the vault's only problem is its missing Index.
    fn write(root: &std::path::Path, path: &str) {
        let file = root.join(format!("{path}.md"));
        let dir = file.parent().unwrap();
        fs::create_dir_all(dir).unwrap();
        let scope = match path.split('/').collect::<Vec<_>>()[..] {
            [_, key, _] => key,
            _ => "all repos",
        };
        if path.starts_with("Projects/") {
            fs::write(
                dir.join("_project.md"),
                format!("---\ntype: project-identity\nremotes: []\nfolders: [{scope}]\n---\n"),
            )
            .unwrap();
        }
        fs::write(file, note(scope, "")).unwrap();
    }

    fn touch(root: &std::path::Path, path: &str, at: SystemTime) {
        fs::File::options()
            .write(true)
            .open(root.join(format!("{path}.md")))
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    fn limited(
        root: &std::path::Path,
        full_review_max_notes: usize,
        review_max_pages: usize,
    ) -> Config {
        Config {
            git_autocommit: false,
            review: Review {
                full_review_max_notes,
                review_max_pages,
                ..Review::default()
            },
            ..Config::new(root.to_path_buf())
        }
    }

    fn every_page(config: &Config, full: bool) -> String {
        let mut text = String::new();
        for page in 1.. {
            let next = review(config, page, full, "test").unwrap();
            text.push_str(&next);
            if !next.contains("more:") {
                return text;
            }
        }
        unreachable!()
    }

    #[test]
    fn a_big_vault_sends_whole_folders_by_priority_until_the_pages_run_out() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mut paths = vec![
            "Preferences/rule".to_string(),
            "Projects/edited/x".to_string(),
            "Projects/edited/neighbour".to_string(),
            "Projects/read/x".to_string(),
            "Projects/loaded/x".to_string(),
            "Projects/a-stale/x".to_string(),
        ];
        paths.extend((0..200).map(|i| format!("Topics/big/n-{i}")));
        let day = std::time::Duration::from_secs(86_400);
        let cleanup = SystemTime::now() - 10 * day;
        for path in &paths {
            write(root, path);
            touch(root, path, cleanup - day);
        }
        touch(root, "Projects/edited/x", cleanup + day);
        let today = jiff::Zoned::now().date();
        usage::record_read(root, "Projects/read/x", today - 2.days()).unwrap();
        usage::record_read(root, "Projects/a-stale/x", today - 60.days()).unwrap();
        usage::record_session(root, "loaded", today - 1.days()).unwrap();
        let config = limited(root, 205, 2);
        cleanup::record(&config, cleanup, "test").unwrap();

        let text = every_page(&config, false);
        assert!(
            text.contains("\n6 of 206 notes: preferences, folders changed since 20"),
            "{text}"
        );
        assert!(
            text.contains("up to 2 pages (full=true for every note), then the problems"),
            "{text}"
        );
        let at = |heading: &str| {
            text.find(&format!("## {heading}\n"))
                .unwrap_or_else(|| panic!("{heading}: {text}"))
        };
        // Changed folders come whole; folders used lately come next, most recent first; the rest while pages last.
        assert!(text.contains("- neighbour | "), "{text}");
        assert!(at("Preferences") < at("Projects/edited"));
        assert!(at("Projects/edited") < at("Projects/loaded"));
        assert!(at("Projects/loaded") < at("Projects/read"));
        assert!(at("Projects/read") < at("Projects/a-stale"));
        assert!(!text.contains("## Topics/big"), "{text}");
        // Problems come from the whole vault, the folders left out included.
        assert!(text.contains("## check\n"), "{text}");
        assert!(text.contains("- Index.md: error: missing\n"), "{text}");
        assert!(!text.contains(cleanup::FILE), "{text}");
        assert!(
            cleanup::last(root).unwrap() > cleanup + day,
            "the review read to its end is the new cleanup"
        );
    }

    #[test]
    fn up_to_the_threshold_or_on_demand_every_note_comes_whatever_the_pages() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "Preferences/rule");
        for i in 0..200 {
            write(root, &format!("Topics/big/n-{i}"));
        }
        cleanup::record(&limited(root, 201, 1), SystemTime::now(), "test").unwrap();

        let at_threshold = every_page(&limited(root, 201, 1), false);
        assert!(
            at_threshold.contains("\nall 201 notes, 0 changed since the last cleanup ("),
            "{at_threshold}"
        );
        assert!(at_threshold.contains("- n-199 | "), "{at_threshold}");

        let over = every_page(&limited(root, 200, 1), false);
        assert!(over.contains("\n1 of 201 notes: "), "{over}");
        assert!(!over.contains("- n-0 | "), "{over}");

        let full = every_page(&limited(root, 200, 1), true);
        assert!(full.contains("\nall 201 notes, "), "{full}");
        assert!(
            full.contains("more: call memory_review with page=2 and full=true\n"),
            "{full}"
        );
        assert!(full.contains("- n-199 | "), "{full}");
    }

    #[test]
    fn a_cleanup_counts_only_once_its_last_page_is_read() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("Preferences")).unwrap();
        for i in 0..80 {
            fs::write(
                tmp.path().join(format!("Preferences/rule-{i}.md")),
                note("all repos", ""),
            )
            .unwrap();
        }
        let config = Config {
            git_autocommit: false,
            ..Config::new(tmp.path().to_path_buf())
        };

        let first = review(&config, 1, false, "test").unwrap();
        assert!(
            first.contains("more: call memory_review with page=2"),
            "{first}"
        );
        assert_eq!(cleanup::last(tmp.path()), None);
        let mut page = 2;
        while review(&config, page, false, "test")
            .unwrap()
            .contains("more:")
        {
            page += 1;
        }
        assert!(cleanup::last(tmp.path()).is_some());
    }

    #[test]
    fn pages_fit_the_budget_repeat_their_heading_and_keep_every_line_in_order() {
        let items: Vec<(String, String)> = (0..150)
            .map(|i| {
                let section = if i < 100 { "Projects/big" } else { "check" };
                (
                    section.to_string(),
                    format!("- line {i} of about the length of a real one"),
                )
            })
            .collect();

        let pages = paginate(&items);
        assert!(pages.len() > 2);
        let mut lines = Vec::new();
        for page in &pages {
            assert!(page.len() <= PAGE_BYTES, "{} bytes", page.len());
            assert!(page.starts_with("## "), "{page}");
            lines.extend(page.lines().filter(|l| !l.starts_with("## ")));
        }
        let expected: Vec<&str> = items.iter().map(|(_, line)| line.as_str()).collect();
        assert_eq!(lines, expected);
    }
}
