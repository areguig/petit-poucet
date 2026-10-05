use std::collections::BTreeMap;
use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::state;
use crate::vault::write_atomic;

// In the vault, synced and committed with the notes; one file per machine, so two machines never write the same one.
pub const DIR: &str = ".usage";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub reads: u32,
    pub last_read: Date,
}

// What one machine saw, or the sum over every machine.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    #[serde(default)]
    pub notes: BTreeMap<String, Usage>,
    // Last day a session loaded each project.
    #[serde(default)]
    pub projects: BTreeMap<String, Date>,
    // The first day counted: a note can only be called unused once its whole period was counted.
    #[serde(default)]
    pub since: Option<Date>,
}

impl Activity {
    fn add(&mut self, other: Activity) {
        for (path, usage) in other.notes {
            self.notes
                .entry(path)
                .and_modify(|u| {
                    u.reads += usage.reads;
                    u.last_read = u.last_read.max(usage.last_read);
                })
                .or_insert(usage);
        }
        for (project, day) in other.projects {
            let last = self.projects.entry(project).or_insert(day);
            *last = (*last).max(day);
        }
        self.since = match (self.since, other.since) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
}

fn read(file: &Path) -> Activity {
    fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write(file: &Path, activity: &Activity) -> Result<(), String> {
    fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(activity).map_err(|e| e.to_string())?;
    write_atomic(file, &text)
}

// Created once per machine and kept out of the vault, so each machine has its own usage file.
fn machine_id(root: &Path) -> Result<String, String> {
    let file = state::dir(root)?.join("machine-id");
    if let Ok(id) = fs::read_to_string(&file) {
        return Ok(id.trim().to_string());
    }
    let id = format!("{:016x}", RandomState::new().hash_one(std::process::id()));
    write_atomic(&file, &id)?;
    Ok(id)
}

// This machine's file; the usage kept in `.petit-poucet/` before 0.4 becomes it the first time.
fn own_file(root: &Path) -> Result<PathBuf, String> {
    let file = root.join(DIR).join(format!("{}.json", machine_id(root)?));
    let old = root.join(state::DIR).join("usage.json");
    if !file.exists() && old.exists() {
        let notes: BTreeMap<String, Usage> = fs::read_to_string(&old)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        write(
            &file,
            &Activity {
                notes,
                ..Activity::default()
            },
        )?;
        fs::remove_file(&old).map_err(|e| e.to_string())?;
    }
    Ok(file)
}

fn machine_files(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root.join(DIR)) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect()
}

// The sum over every machine: reads added up, the latest days kept.
pub fn load(root: &Path) -> Activity {
    // Migrates first, so usage kept before 0.4 counts from the start.
    let _ = own_file(root);
    let mut total = Activity::default();
    for file in machine_files(root) {
        total.add(read(&file));
    }
    total
}

// What a commit that happens anyway stages for usage; a read never makes a commit of its own.
pub fn to_commit(root: &Path) -> Option<&'static str> {
    root.join(DIR).exists().then_some(DIR)
}

fn update(root: &Path, today: Date, change: impl FnOnce(&mut Activity)) -> Result<(), String> {
    let file = own_file(root)?;
    let mut activity = read(&file);
    activity.since.get_or_insert(today);
    change(&mut activity);
    write(&file, &activity)
}

pub fn record_read(root: &Path, path: &str, today: Date) -> Result<(), String> {
    update(root, today, |activity| {
        let entry = activity.notes.entry(path.to_string()).or_insert(Usage {
            reads: 0,
            last_read: today,
        });
        entry.reads += 1;
        entry.last_read = today;
    })
}

pub fn record_session(root: &Path, project: &str, today: Date) -> Result<(), String> {
    update(root, today, |activity| {
        activity.projects.insert(project.to_string(), today);
    })
}

// Keeps the counts with the note when it moves, drops them when it is deleted, in every machine's file.
pub fn relocate(root: &Path, old: &str, new: Option<&str>) -> Result<(), String> {
    for file in machine_files(root) {
        let mut activity = read(&file);
        let Some(entry) = activity.notes.remove(old) else {
            continue;
        };
        if let Some(new) = new {
            activity.notes.insert(new.to_string(), entry);
        }
        write(&file, &activity)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: i8) -> Date {
        jiff::civil::date(2026, 9, d)
    }

    #[test]
    fn counts_reads_and_sessions_and_follows_moves_and_deletes() {
        let tmp = tempfile::tempdir().unwrap();
        record_read(tmp.path(), "Preferences/a", day(1)).unwrap();
        record_read(tmp.path(), "Preferences/a", day(24)).unwrap();
        record_read(tmp.path(), "Preferences/b", day(1)).unwrap();
        record_session(tmp.path(), "app", day(2)).unwrap();
        let activity = load(tmp.path());
        assert_eq!(
            activity.notes["Preferences/a"],
            Usage {
                reads: 2,
                last_read: day(24)
            }
        );
        assert_eq!(activity.projects["app"], day(2));
        assert_eq!(
            activity.since,
            Some(day(1)),
            "set by the first count, kept after"
        );

        relocate(tmp.path(), "Preferences/a", Some("Projects/p/a")).unwrap();
        relocate(tmp.path(), "Preferences/b", None).unwrap();
        relocate(tmp.path(), "Preferences/never-read", None).unwrap();
        let notes = load(tmp.path()).notes;
        assert_eq!(notes.keys().collect::<Vec<_>>(), ["Projects/p/a"]);
        assert_eq!(notes["Projects/p/a"].reads, 2);
    }

    #[test]
    fn every_machine_counts_and_moves_follow_in_every_file() {
        let tmp = tempfile::tempdir().unwrap();
        record_read(tmp.path(), "Preferences/a", day(3)).unwrap();
        record_session(tmp.path(), "app", day(3)).unwrap();
        let other = Activity {
            notes: BTreeMap::from([(
                "Preferences/a".to_string(),
                Usage {
                    reads: 4,
                    last_read: day(9),
                },
            )]),
            projects: BTreeMap::from([("app".to_string(), day(1))]),
            since: Some(day(1)),
        };
        write(&tmp.path().join(DIR).join("other.json"), &other).unwrap();

        let total = load(tmp.path());
        assert_eq!(
            total.notes["Preferences/a"],
            Usage {
                reads: 5,
                last_read: day(9)
            }
        );
        assert_eq!(total.projects["app"], day(3), "the latest day wins");
        assert_eq!(total.since, Some(day(1)), "the first day counted anywhere");

        relocate(tmp.path(), "Preferences/a", Some("Topics/t/a")).unwrap();
        let moved = read(&tmp.path().join(DIR).join("other.json"));
        assert_eq!(
            moved.notes["Topics/t/a"].reads, 4,
            "the other machine's file follows too"
        );
    }

    #[test]
    fn the_machine_id_is_made_once() {
        let tmp = tempfile::tempdir().unwrap();
        let id = machine_id(tmp.path()).unwrap();
        assert_eq!(id.len(), 16);
        assert_eq!(machine_id(tmp.path()).unwrap(), id);
    }

    #[test]
    fn usage_kept_before_0_4_moves_into_the_vault_once() {
        let tmp = tempfile::tempdir().unwrap();
        let old = state::dir(tmp.path()).unwrap().join("usage.json");
        fs::write(
            &old,
            r#"{"Preferences/a": {"reads": 3, "last_read": "2026-09-20"}}"#,
        )
        .unwrap();

        assert_eq!(load(tmp.path()).notes["Preferences/a"].reads, 3);
        assert!(!old.exists());
        record_read(tmp.path(), "Preferences/a", day(21)).unwrap();
        assert_eq!(load(tmp.path()).notes["Preferences/a"].reads, 4);
    }

    #[test]
    fn missing_or_broken_files_mean_no_usage_yet() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load(tmp.path()), Activity::default());
        fs::create_dir_all(tmp.path().join(DIR)).unwrap();
        fs::write(tmp.path().join(DIR).join("broken.json"), "not json").unwrap();
        assert_eq!(load(tmp.path()), Activity::default());
    }
}
