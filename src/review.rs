use std::fmt::Write;
use std::fs;
use std::time::SystemTime;

use crate::config::Config;
use crate::index::NO_SUMMARY;
use crate::note::Note;
use crate::usage::{self, Usage};
use crate::vault::Vault;
use crate::{check, cleanup};

// Antigravity CLI saves a tool result over about 4 KB to a file: the smallest limit of the supported agents.
pub const PAGE_BYTES: usize = 3 * 1024;

const LEGEND: &str = "slug | type | created[/updated] | reads and last read | summary";

// Notes changed since the last cleanup (all the first time), then the whole vault's problems: sized by activity.
pub fn review(config: &Config, page: usize, agent: &str) -> Result<String, String> {
    let started = SystemTime::now();
    let vault = Vault::load(&config.vault)?;
    let since = cleanup::last(&vault.root);
    let changed = changed_since(&vault, since);
    let pages = paginate(&items(&vault, &changed));
    let Some(body) = pages.get(page.wrapping_sub(1)) else {
        return Err(format!(
            "no page {page}: the review has {} pages",
            pages.len()
        ));
    };
    let mut out = format!(
        "vault: {}\n{}, then the problems found in the whole vault\npage {page} of {} ({LEGEND})\n{body}",
        vault.root.display(),
        scope(since, changed.len(), vault.notes.len()),
        pages.len()
    );
    if page < pages.len() {
        writeln!(out, "more: call memory_review with page={}", page + 1).unwrap();
    } else {
        cleanup::record(config, started, agent)?;
    }
    Ok(out)
}

// One (folder, line) per changed note, then ("check", finding) for the whole vault.
fn items(vault: &Vault, changed: &[&Note]) -> Vec<(String, String)> {
    let usage = usage::load(&vault.root);
    let notes = changed.iter().map(|note| {
        let (folder, slug) = note.path.rsplit_once('/').unwrap_or(("", &note.path));
        (folder.to_string(), line(note, slug, usage.get(&note.path)))
    });
    let findings = check::check(vault)
        .into_iter()
        .map(|issue| ("check".to_string(), format!("- {issue}")));
    notes.chain(findings).collect()
}

fn scope(since: Option<SystemTime>, changed: usize, total: usize) -> String {
    match since.and_then(|t| jiff::Timestamp::try_from(t).ok()) {
        None => format!("first cleanup: all {total} notes"),
        Some(t) => format!(
            "{changed} of {total} notes changed since the last cleanup ({})",
            t.strftime("%Y-%m-%d")
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
fn paginate(items: &[(String, String)]) -> Vec<String> {
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
            vault: tmp.path().to_path_buf(),
            git_autocommit: false,
        };

        let text = review(&config, 1, "test").unwrap();
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
            review(&config, 2, "test").unwrap_err(),
            "no page 2: the review has 1 pages"
        );
        assert!(review(&config, 0, "test").is_err());
    }

    #[test]
    fn after_a_cleanup_only_notes_changed_since_come_back_with_every_problem() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("Preferences")).unwrap();
        for name in ["kept", "edited", "broken"] {
            fs::write(
                tmp.path().join(format!("Preferences/{name}.md")),
                note("all repos", ""),
            )
            .unwrap();
        }
        let config = Config {
            vault: tmp.path().to_path_buf(),
            git_autocommit: false,
        };
        let day = std::time::Duration::from_secs(86_400);
        let cleanup = SystemTime::now() - 10 * day;
        cleanup::record(&config, cleanup, "test").unwrap();
        let touch = |name: &str, at: SystemTime| {
            fs::File::options()
                .write(true)
                .open(tmp.path().join(format!("Preferences/{name}.md")))
                .unwrap()
                .set_modified(at)
                .unwrap();
        };
        touch("kept", cleanup - day);
        touch("broken", cleanup - day);
        touch("edited", cleanup + day);

        let text = review(&config, 1, "test").unwrap();
        assert!(
            text.contains("\n1 of 3 notes changed since the last cleanup ("),
            "{text}"
        );
        assert!(text.contains("## Preferences\n- edited | "), "{text}");
        assert!(
            !text.contains("- kept |") && !text.contains("- broken |"),
            "{text}"
        );
        // Problems come from the whole vault, changed or not; the cleanup's own file is none of them.
        assert!(text.contains("- Index.md: error: missing\n"), "{text}");
        assert!(!text.contains(cleanup::FILE), "{text}");
        let recorded = cleanup::last(tmp.path()).unwrap();
        assert!(
            recorded > cleanup + day,
            "the review read to its end is the new cleanup"
        );
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
            vault: tmp.path().to_path_buf(),
            git_autocommit: false,
        };

        let first = review(&config, 1, "test").unwrap();
        assert!(
            first.contains("more: call memory_review with page=2"),
            "{first}"
        );
        assert_eq!(cleanup::last(tmp.path()), None);
        let mut page = 2;
        while review(&config, page, "test").unwrap().contains("more:") {
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
