use std::collections::BTreeSet;

use crate::index;
use crate::note::Place;
use crate::vault::{PREFERENCES, PROJECTS, Vault};

// Every session loads the preferences and its project's notes: past `max`, that Index costs tokens in every session.
pub fn over(vault: &Vault, max: usize) -> Vec<(String, String)> {
    let loaded = |project: Option<&str>| {
        vault
            .notes
            .iter()
            .filter(|n| index::in_session(n.place(), project))
            .count()
    };
    let preferences = loaded(None);
    if preferences > max {
        let message =
            format!("every session loads its {preferences} notes, over {max}: merge or move some");
        return vec![(PREFERENCES.to_string(), message)];
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
        .map(|key| (key, loaded(Some(key))))
        .filter(|(_, loaded)| *loaded > max)
        .map(|(key, loaded)| {
            let message = format!("a session in it loads {loaded} notes with the preferences, over {max}: merge or move some");
            (format!("{PROJECTS}/{key}"), message)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn notes(root: &std::path::Path, folder: &str, count: usize) {
        fs::create_dir_all(root.join(folder)).unwrap();
        for i in 0..count {
            fs::write(root.join(format!("{folder}/n-{i}.md")), "x").unwrap();
        }
    }

    #[test]
    fn projects_whose_sessions_load_more_than_the_budget_or_the_preferences_alone() {
        let tmp = tempfile::tempdir().unwrap();
        notes(tmp.path(), "Preferences", 3);
        notes(tmp.path(), "Projects/big", 3);
        notes(tmp.path(), "Projects/small", 2);
        notes(tmp.path(), "Topics/huge", 50);
        let vault = Vault::load(tmp.path()).unwrap();

        assert_eq!(
            over(&vault, 5),
            [(
                "Projects/big".to_string(),
                "a session in it loads 6 notes with the preferences, over 5: merge or move some"
                    .to_string()
            )],
            "topics are never loaded at session start"
        );
        assert!(over(&vault, 6).is_empty());
        assert_eq!(
            over(&vault, 2),
            [(
                "Preferences".to_string(),
                "every session loads its 3 notes, over 2: merge or move some".to_string()
            )],
            "once, not once per project"
        );
    }
}
