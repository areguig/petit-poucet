use std::fs;
use std::path::Path;
use std::time::SystemTime;

use jiff::Timestamp;

use crate::config::Config;
use crate::vault::write_atomic;
use crate::{git, lock};

// In the vault (a dot-file the loader and Obsidian skip), synced and committed: a cleanup counts on every machine.
pub const FILE: &str = ".last-cleanup";

// When the last memory cleanup review was read to its end; none before the first.
pub fn last(root: &Path) -> Option<SystemTime> {
    let text = fs::read_to_string(root.join(FILE)).ok()?;
    Some(text.trim().parse::<Timestamp>().ok()?.into())
}

// To the nanosecond, like file times: a note saved in the same second as a cleanup is still before or after it.
pub fn record(config: &Config, at: SystemTime, agent: &str) -> Result<(), String> {
    let root = &config.vault;
    let at = Timestamp::try_from(at).map_err(|e| e.to_string())?;
    let _lock = lock::vault(root)?;
    write_atomic(&root.join(FILE), &format!("{at:.9}\n"))?;
    if config.git_autocommit {
        git::commit(
            root,
            &[FILE],
            &format!("cleanup: memory reviewed ({agent})"),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    #[test]
    fn none_until_recorded_then_the_recorded_time_to_the_nanosecond_in_a_readable_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config = Config {
            vault: tmp.path().to_path_buf(),
            git_autocommit: false,
        };
        assert_eq!(last(tmp.path()), None);
        // Windows counts system and file times in steps of 100 ns.
        let at = UNIX_EPOCH + Duration::new(1_790_000_000, 123_456_700);
        record(&config, at, "test").unwrap();
        assert_eq!(last(tmp.path()), Some(at));
        assert_eq!(
            fs::read_to_string(tmp.path().join(FILE)).unwrap(),
            "2026-09-21T14:13:20.123456700Z\n"
        );
    }
}
