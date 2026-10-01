use std::fs;
use std::path::{Path, PathBuf};

use crate::vault::write_atomic;

pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".petit-poucet.bak");
    path.with_file_name(name)
}

// For agents' own files: writes only a real change, and keeps the version it replaces next to it.
pub fn replace(path: &Path, text: &str) -> Result<bool, String> {
    let old = fs::read_to_string(path).ok();
    if old.as_deref() == Some(text) {
        return Ok(false);
    }
    if let Some(old) = old {
        write_atomic(&backup_path(path), &old)?;
    }
    let dir = path.parent().ok_or("no parent folder")?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    write_atomic(path, text)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_only_changes_and_keeps_what_it_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("agent/config.toml");
        let backup = tmp.path().join("agent/config.toml.petit-poucet.bak");

        assert!(replace(&file, "v1").unwrap(), "created");
        assert!(!backup.exists(), "nothing to back up");
        assert!(!replace(&file, "v1").unwrap(), "unchanged");
        assert!(replace(&file, "v2").unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), "v2");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "v1");
    }
}
