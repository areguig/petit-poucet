use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::vault::write_atomic_mode;

pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".petit-poucet.bak");
    path.with_file_name(name)
}

// An agent's file can hold secrets: its rewrite and backup are never more readable than it was.
#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).map_or(0o644, |m| m.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn mode(_: &Path) -> u32 {
    0o644
}

pub fn back_up(path: &Path, old: &str) -> Result<(), String> {
    write_atomic_mode(&backup_path(path), old, mode(path))
}

// For agents' own files: writes only a real change, and keeps the version it replaces next to it.
pub fn replace(path: &Path, text: &str) -> Result<bool, String> {
    let old = fs::read_to_string(path).ok();
    if old.as_deref() == Some(text) {
        return Ok(false);
    }
    if let Some(old) = old {
        back_up(path, &old)?;
    }
    let dir = path.parent().ok_or("no parent folder")?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    write_atomic_mode(path, text, mode(path))?;
    Ok(true)
}

// A missing file is empty; one it can't parse is an error, so the user's content is never overwritten.
// Missing or blank (agents leave empty files behind): no settings yet.
pub fn read_json(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => {
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
        }
        _ => Ok(json!({})),
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

// After an uninstall, a section only petit-poucet used is left out rather than empty.
pub fn drop_if_empty(settings: &mut Value, key: &str) {
    if let Some(object) = settings.as_object_mut()
        && object
            .get(key)
            .and_then(Value::as_object)
            .is_some_and(|o| o.is_empty())
    {
        object.remove(key);
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

    // An agent's file can hold secrets (#169).
    #[cfg(unix)]
    #[test]
    fn rewrites_and_backups_keep_the_file_s_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("config.toml");
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;

        assert!(replace(&file, "v1").unwrap());
        assert_eq!(mode(&file), 0o644, "a new file is readable like a note");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        for text in ["v2", "v3"] {
            assert!(replace(&file, text).unwrap());
            assert_eq!(mode(&file), 0o600);
            assert_eq!(mode(&backup_path(&file)), 0o600);
        }
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

        fs::write(&file, " \n").unwrap();
        assert_eq!(read_json(&file).unwrap(), json!({}), "blank");
        fs::write(&file, "{ broken").unwrap();
        assert!(read_json(&file).is_err());
    }
}
