use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// Counters older than this belong to finished sessions.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

// In the temp folder: the stop hook runs even when the vault is missing.
fn dir() -> PathBuf {
    std::env::temp_dir().join("petit-poucet-stops")
}

// This session's stop count, this stop included; None without a usable session id.
pub fn record(session: &str) -> Option<u32> {
    record_in(&dir(), session)
}

fn record_in(dir: &Path, session: &str) -> Option<u32> {
    // The id becomes a file name: keep only characters that can't leave the folder.
    let name: String = session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    if name.is_empty() {
        return None;
    }
    fs::create_dir_all(dir).ok()?;
    let file = dir.join(name);
    let count = match fs::read_to_string(&file) {
        Ok(n) => n.trim().parse::<u32>().unwrap_or(0) + 1,
        Err(_) => {
            prune(dir);
            1
        }
    };
    fs::write(&file, count.to_string()).ok()?;
    Some(count)
}

fn prune(dir: &Path) {
    let now = SystemTime::now();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let modified = entry.metadata().and_then(|m| m.modified());
        if modified.is_ok_and(|t| now.duration_since(t).unwrap_or_default() > MAX_AGE) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_per_session() {
        let tmp = tempfile::tempdir().unwrap();
        let counts: Vec<_> = ["a", "a", "b", "a"]
            .iter()
            .map(|s| record_in(tmp.path(), s))
            .collect();
        assert_eq!(counts, [Some(1), Some(2), Some(1), Some(3)]);
    }

    #[test]
    fn a_session_id_never_leaves_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("stops");
        assert_eq!(record_in(&dir, "../../x/y"), Some(1));
        assert!(dir.join("xy").exists());
        assert_eq!(record_in(&dir, "../"), None);
        assert_eq!(record_in(&dir, ""), None);
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_new_session_drops_counters_of_old_ones() {
        let tmp = tempfile::tempdir().unwrap();
        record_in(tmp.path(), "old");
        record_in(tmp.path(), "recent");
        let two_days_ago = SystemTime::now() - 2 * MAX_AGE;
        fs::File::options()
            .write(true)
            .open(tmp.path().join("old"))
            .unwrap()
            .set_modified(two_days_ago)
            .unwrap();

        record_in(tmp.path(), "recent");
        assert!(tmp.path().join("old").exists(), "only a new session prunes");
        record_in(tmp.path(), "new");
        assert!(!tmp.path().join("old").exists());
        assert_eq!(record_in(tmp.path(), "recent"), Some(3));
    }
}
