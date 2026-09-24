use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::vault::write_atomic;

// Next to the notes but hidden: the vault loader and Obsidian skip dot-folders, git ignores it.
pub const DIR: &str = ".petit-poucet";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub reads: u32,
    pub last_read: Date,
}

fn file(root: &Path) -> PathBuf {
    root.join(DIR).join("usage.json")
}

pub fn load(root: &Path) -> BTreeMap<String, Usage> {
    fs::read_to_string(file(root))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(root: &Path, usage: &BTreeMap<String, Usage>) -> Result<(), String> {
    fs::create_dir_all(root.join(DIR)).map_err(|e| e.to_string())?;
    // Ignores itself, so vaults created before this folder existed keep a clean git status.
    let ignore = root.join(DIR).join(".gitignore");
    if !ignore.exists() {
        write_atomic(&ignore, "*\n")?;
    }
    let text = serde_json::to_string_pretty(usage).map_err(|e| e.to_string())?;
    write_atomic(&file(root), &text)
}

pub fn record_read(root: &Path, path: &str, today: Date) -> Result<(), String> {
    let mut usage = load(root);
    let entry = usage.entry(path.to_string()).or_insert(Usage {
        reads: 0,
        last_read: today,
    });
    entry.reads += 1;
    entry.last_read = today;
    save(root, &usage)
}

// Keeps the counts with the note when it moves, drops them when it is deleted.
pub fn relocate(root: &Path, old: &str, new: Option<&str>) -> Result<(), String> {
    let mut usage = load(root);
    let Some(entry) = usage.remove(old) else {
        return Ok(());
    };
    if let Some(new) = new {
        usage.insert(new.to_string(), entry);
    }
    save(root, &usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_reads_and_follows_moves_and_deletes() {
        let tmp = tempfile::tempdir().unwrap();
        let (day1, day2) = (
            jiff::civil::date(2026, 9, 1),
            jiff::civil::date(2026, 9, 24),
        );
        record_read(tmp.path(), "Preferences/a", day1).unwrap();
        record_read(tmp.path(), "Preferences/a", day2).unwrap();
        record_read(tmp.path(), "Preferences/b", day1).unwrap();
        assert_eq!(
            load(tmp.path())["Preferences/a"],
            Usage {
                reads: 2,
                last_read: day2
            }
        );

        relocate(tmp.path(), "Preferences/a", Some("Projects/p/a")).unwrap();
        relocate(tmp.path(), "Preferences/b", None).unwrap();
        relocate(tmp.path(), "Preferences/never-read", None).unwrap();
        let usage = load(tmp.path());
        assert_eq!(usage.keys().collect::<Vec<_>>(), ["Projects/p/a"]);
        assert_eq!(
            fs::read_to_string(tmp.path().join(DIR).join(".gitignore")).unwrap(),
            "*\n"
        );
        assert_eq!(usage["Projects/p/a"].reads, 2);
    }

    #[test]
    fn a_missing_or_broken_file_means_no_usage_yet() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(load(tmp.path()).is_empty());
        fs::create_dir(tmp.path().join(DIR)).unwrap();
        fs::write(file(tmp.path()), "not json").unwrap();
        assert!(load(tmp.path()).is_empty());
    }
}
