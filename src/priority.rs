use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use jiff::ToSpan;
use jiff::civil::Date;

use crate::usage::Activity;
use crate::vault::{PREFERENCES, PROJECTS, folder};

// The folders a big vault's review always sends (preferences, then changed ones), and the others in the order they fill the pages left.
pub fn order<'a>(
    folders: &BTreeSet<&'a str>,
    changed: &BTreeSet<&str>,
    activity: &Activity,
    active_days: i64,
    today: Date,
) -> (Vec<&'a str>, Vec<&'a str>) {
    let always: Vec<&'a str> = folders
        .get(PREFERENCES)
        .into_iter()
        .chain(
            folders
                .iter()
                .filter(|f| **f != PREFERENCES && changed.contains(*f)),
        )
        .copied()
        .collect();
    let used = last_used(activity);
    let since = today.saturating_sub(active_days.days());
    let mut active: Vec<&str> = folders
        .iter()
        .copied()
        .filter(|f| !always.contains(f) && used.get(*f).is_some_and(|day| *day >= since))
        .collect();
    active.sort_by_key(|f| (Reverse(used[*f]), *f));
    let rest = folders
        .iter()
        .copied()
        .filter(|f| !always.contains(f) && !active.contains(f));
    let then = active.iter().copied().chain(rest).collect();
    (always, then)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Usage;

    #[test]
    fn preferences_and_changed_folders_first_then_recent_use_then_the_rest() {
        let today = jiff::civil::date(2026, 10, 4);
        let folders = BTreeSet::from([
            "Preferences",
            "Projects/a-old",
            "Projects/edited",
            "Projects/loaded",
            "Projects/read",
            "Projects/0-never",
        ]);
        let changed = BTreeSet::from(["Projects/edited"]);
        let read = |day| Usage {
            reads: 1,
            last_read: day,
        };
        let activity = Activity {
            notes: BTreeMap::from([
                ("Projects/read/x".to_string(), read(today - 2.days())),
                ("Projects/a-old/x".to_string(), read(today - 31.days())),
                // A read in a changed folder doesn't move it.
                ("Projects/edited/x".to_string(), read(today)),
            ]),
            projects: BTreeMap::from([("loaded".to_string(), today - 1.days())]),
        };

        let (always, then) = order(&folders, &changed, &activity, 30, today);
        assert_eq!(always, ["Preferences", "Projects/edited"]);
        assert_eq!(
            then,
            [
                "Projects/loaded",
                "Projects/read",
                "Projects/0-never",
                "Projects/a-old"
            ],
            "used in the last 30 days, most recent first, then the rest in order"
        );
    }

    #[test]
    fn a_vault_without_preferences_still_has_its_changed_folders_first() {
        let folders = BTreeSet::from(["Projects/a", "Projects/b"]);
        let changed = BTreeSet::from(["Projects/b"]);
        let today = jiff::civil::date(2026, 10, 4);
        let (always, then) = order(&folders, &changed, &Activity::default(), 30, today);
        assert_eq!((always, then), (vec!["Projects/b"], vec!["Projects/a"]));
    }
}
