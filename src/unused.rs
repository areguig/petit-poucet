use jiff::ToSpan;
use jiff::civil::Date;

use crate::note::NoteType;
use crate::usage::Activity;
use crate::vault::Vault;

// Notes not read, created or updated for `days`, once usage has counted that long; `feedback` notes apply from the Index unread.
pub fn find(vault: &Vault, activity: &Activity, days: i64, today: Date) -> Vec<String> {
    let before = today.saturating_sub(days.days());
    if activity.since.is_none_or(|since| since > before) {
        return Vec::new();
    }
    vault
        .notes
        .iter()
        .filter_map(|note| {
            let fm = note.frontmatter.as_ref().ok()?;
            let read = activity.notes.get(&note.path).map(|u| u.last_read);
            let last = [Some(fm.created), fm.updated, read]
                .into_iter()
                .flatten()
                .max()?;
            (fm.note_type != NoteType::Feedback && last < before).then(|| match read {
                Some(day) => format!("{}.md: not read since {day}", note.path),
                None => format!("{}.md: never read, written {last}", note.path),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use super::*;
    use crate::usage::Usage;

    fn day(d: i8) -> Date {
        jiff::civil::date(2026, 6, d)
    }

    fn write(root: &std::path::Path, slug: &str, note_type: &str, dates: &str) {
        fs::write(
            root.join(format!("Preferences/{slug}.md")),
            format!("---\ntype: {note_type}\nscope: all repos\nsummary: s\n{dates}tags: [agent-memory]\n---\nx\n"),
        )
        .unwrap();
    }

    #[test]
    fn notes_nobody_opened_for_the_period_once_usage_counted_it_feedback_aside() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("Preferences")).unwrap();
        write(tmp.path(), "never-read", "user", "created: 2026-01-10\n");
        write(tmp.path(), "read-long-ago", "user", "created: 2026-01-10\n");
        write(tmp.path(), "read-lately", "user", "created: 2026-01-10\n");
        write(
            tmp.path(),
            "updated-lately",
            "user",
            "created: 2026-01-10\nupdated: 2026-05-20\n",
        );
        write(tmp.path(), "new", "user", "created: 2026-05-20\n");
        write(tmp.path(), "rule", "feedback", "created: 2026-01-10\n");
        let vault = Vault::load(tmp.path()).unwrap();
        let read = |last_read| Usage {
            reads: 1,
            last_read,
        };
        let mut activity = Activity {
            notes: BTreeMap::from([
                (
                    "Preferences/read-long-ago".to_string(),
                    read(jiff::civil::date(2026, 2, 1)),
                ),
                ("Preferences/read-lately".to_string(), read(day(1))),
            ]),
            since: Some(jiff::civil::date(2026, 1, 1)),
            ..Activity::default()
        };
        let today = day(30);

        assert_eq!(
            find(&vault, &activity, 90, today),
            [
                "Preferences/never-read.md: never read, written 2026-01-10",
                "Preferences/read-long-ago.md: not read since 2026-02-01",
            ]
        );
        activity.since = Some(day(1));
        assert!(
            find(&vault, &activity, 90, today).is_empty(),
            "usage counted for less than 90 days"
        );
        activity.since = None;
        assert!(
            find(&vault, &activity, 90, today).is_empty(),
            "nothing counted yet"
        );
    }
}
