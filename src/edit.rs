use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

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

// A missing file is empty; one it can't parse is an error, so the user's content is never overwritten.
pub fn read_json(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(_) => Ok(json!({})),
    }
}

pub fn write_json(path: &Path, value: &Value) -> Result<bool, String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())? + "\n";
    replace(path, &text)
}

// For uninstall: never creates a file that wasn't there.
pub fn write_json_if_present(path: &Path, value: &Value) -> Result<bool, String> {
    match path.exists() {
        true => write_json(path, value),
        false => Ok(false),
    }
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

    #[test]
    fn json_files_missing_are_empty_and_unreadable_ones_are_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("agent.json");
        assert_eq!(read_json(&file).unwrap(), json!({}));
        assert!(!write_json_if_present(&file, &json!({"a": 1})).unwrap());
        assert!(!file.exists(), "uninstall never creates a file");

        assert!(write_json(&file, &json!({"b": 1, "a": 2})).unwrap());
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "{\n  \"b\": 1,\n  \"a\": 2\n}\n"
        );
        assert!(write_json_if_present(&file, &json!({"b": 2})).unwrap());

        fs::write(&file, "{ broken").unwrap();
        assert!(read_json(&file).is_err());
    }
}
