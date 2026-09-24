use std::fmt::Write;

use crate::config::Config;
use crate::index::NO_SUMMARY;
use crate::vault::Vault;
use crate::{check, usage};

// Everything a cleanup review needs in one line per note; bodies stay on disk.
pub fn review(config: &Config) -> Result<String, String> {
    let vault = Vault::load(&config.vault)?;
    let usage = usage::load(&vault.root);
    let mut out = format!(
        "vault: {}\nnotes (path | type | created | updated | reads | summary):\n",
        vault.root.display()
    );
    for note in &vault.notes {
        let fm = note.frontmatter.as_ref().ok();
        let date = |d: Option<jiff::civil::Date>| d.map_or("-".to_string(), |d| d.to_string());
        let reads = usage.get(&note.path).map_or("never read".to_string(), |u| {
            format!("{} reads, last {}", u.reads, u.last_read)
        });
        writeln!(
            out,
            "- {} | {} | {} | {} | {reads} | {}",
            note.path,
            fm.map_or("invalid", |f| f.note_type.name()),
            date(fm.map(|f| f.created)),
            date(fm.and_then(|f| f.updated)),
            note.summary().unwrap_or(NO_SUMMARY),
        )
        .unwrap();
    }
    let issues = check::check(&vault);
    if !issues.is_empty() {
        out.push_str("check:\n");
        for issue in issues {
            writeln!(out, "- {issue}").unwrap();
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn lists_every_note_with_its_dates_usage_and_the_check_findings() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Projects/app")).unwrap();
        fs::create_dir(tmp.path().join("Preferences")).unwrap();
        let note = |scope: &str, extra: &str| {
            format!(
                "---\ntype: feedback\nscope: {scope}\nsummary: s\ncreated: 2026-09-01\n{extra}tags: [agent-memory]\n---\n**Why:** x\n"
            )
        };
        fs::write(
            tmp.path().join("Preferences/rule.md"),
            note("all repos", "updated: 2026-09-20\n"),
        )
        .unwrap();
        fs::write(tmp.path().join("Projects/app/fact.md"), note("app", "")).unwrap();
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

        let text = review(&config).unwrap();
        assert!(text.contains("- Preferences/rule | feedback | 2026-09-01 | 2026-09-20 | 1 reads, last 2026-09-24 | s\n"), "{text}");
        assert!(
            text.contains("- Projects/app/fact | feedback | 2026-09-01 | - | never read | s\n"),
            "{text}"
        );
        assert!(
            text.contains("check:\n- Index.md: error: missing\n"),
            "{text}"
        );
    }
}
