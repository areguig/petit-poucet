use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::state;
use crate::vault::write_atomic;

// Where each project is checked out on this machine: per machine, so kept out of the vault's sync and history.
const FILE: &str = "checkouts.json";

pub fn load(root: &Path) -> BTreeMap<String, PathBuf> {
    fs::read_to_string(root.join(state::DIR).join(FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

// Writes only when the checkout moved, since every session start calls it.
pub fn record(root: &Path, project: &str, checkout: &Path) -> Result<(), String> {
    let mut checkouts = load(root);
    if checkouts
        .get(project)
        .is_some_and(|known| known == checkout)
    {
        return Ok(());
    }
    checkouts.insert(project.to_string(), checkout.to_path_buf());
    let text = serde_json::to_string_pretty(&checkouts).map_err(|e| e.to_string())?;
    write_atomic(&state::dir(root)?.join(FILE), &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_the_latest_checkout_of_each_project() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(load(tmp.path()).is_empty());
        record(tmp.path(), "app", Path::new("/src/app")).unwrap();
        record(tmp.path(), "web", Path::new("/src/web")).unwrap();
        record(tmp.path(), "app", Path::new("/work/app")).unwrap();
        assert_eq!(
            load(tmp.path()),
            BTreeMap::from([
                ("app".to_string(), PathBuf::from("/work/app")),
                ("web".to_string(), PathBuf::from("/src/web")),
            ])
        );
    }
}
