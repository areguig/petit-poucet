use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::vault::{INDEX_FILE, PREFERENCES, PROJECTS, Vault, write_atomic};
use crate::{git, index};

pub fn init(path: &Path) -> Result<String, String> {
    for dir in [PREFERENCES, PROJECTS] {
        fs::create_dir_all(path.join(dir)).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let root = path.canonicalize().map_err(|e| e.to_string())?;

    let config_path = Config::path()?;
    let config = match fs::read_to_string(&config_path) {
        Ok(_) => {
            let existing = Config::load()?;
            if existing.vault != root {
                return Err(format!(
                    "{} already points to {}; edit it to switch vaults",
                    config_path.display(),
                    existing.vault.display()
                ));
            }
            existing
        }
        Err(_) => {
            let config = Config::new(root.clone());
            config.save()?;
            config
        }
    };

    if !git::is_repo(&root) {
        git::init(&root)?;
    }
    let vault = Vault::load(&root)?;
    write_atomic(&root.join(INDEX_FILE), &index::generate(&vault))?;
    if config.git_autocommit {
        git::commit(&root, &[INDEX_FILE], "init: vault")?;
    }
    Ok(format!(
        "vault ready at {} ({} notes); config: {}",
        root.display(),
        vault.notes.len(),
        config_path.display()
    ))
}
