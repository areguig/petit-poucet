use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::DocumentMut;

const VAULT_ENV: &str = "PETIT_POUCET_VAULT";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub vault: PathBuf,
    #[serde(default = "yes")]
    pub git_autocommit: bool,
    #[serde(flatten)]
    pub review: Review,
}

fn yes() -> bool {
    true
}

// A config written before 0.4 gets these defaults until `setup` writes them in.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Review {
    pub full_review_max_notes: usize,
    pub review_max_pages: usize,
    pub active_days: i64,
}

impl Default for Review {
    fn default() -> Review {
        Review {
            full_review_max_notes: 300,
            review_max_pages: 10,
            active_days: 30,
        }
    }
}

impl Config {
    pub fn new(vault: PathBuf) -> Config {
        Config {
            vault,
            git_autocommit: true,
            review: Review::default(),
        }
    }

    pub fn path() -> Result<PathBuf, String> {
        let home = std::env::home_dir().ok_or("cannot find the home folder")?;
        Ok(home.join(".config/petit-poucet/config.toml"))
    }

    pub fn is_set() -> bool {
        std::env::var_os(VAULT_ENV).is_some() || Config::path().is_ok_and(|p| p.exists())
    }

    pub fn load() -> Result<Config, String> {
        let path = Config::path()?;
        let file = match fs::read_to_string(&path) {
            Ok(text) => Some(
                toml::from_str::<Config>(&text)
                    .map_err(|e| format!("{}: {}", path.display(), e.message()))?,
            ),
            Err(_) => None,
        };
        match (std::env::var_os(VAULT_ENV), file) {
            (Some(vault), Some(config)) => Ok(Config {
                vault: vault.into(),
                ..config
            }),
            (Some(vault), None) => Ok(Config::new(vault.into())),
            (None, Some(config)) => Ok(config),
            (None, None) => Err(format!(
                "no vault configured: run `petit-poucet init <path>` or set {VAULT_ENV}"
            )),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Config::path()?;
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let text = toml::to_string(self).map_err(|e| e.to_string())?;
        crate::vault::write_atomic(&path, &text)
    }
}

// Writes in the settings an older config lacks, keeping the user's own lines and comments.
pub fn add_missing(path: &Path) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut config: DocumentMut = text
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let defaults: DocumentMut = toml::to_string(&Review::default())
        .map_err(|e| e.to_string())?
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    let mut added = Vec::new();
    for (key, value) in defaults.iter() {
        if !config.contains_key(key) {
            config.insert(key, value.clone());
            added.push(key.to_string());
        }
    }
    if !added.is_empty() {
        crate::vault::write_atomic(path, &config.to_string())?;
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_from_before_0_4_reads_with_the_defaults_and_gets_them_written_in_once() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let old = "# my vault\nvault = \"/v\"\ngit_autocommit = false\nreview_max_pages = 4\n";
        fs::write(&path, old).unwrap();
        let config: Config = toml::from_str(old).unwrap();
        assert_eq!(
            config.review,
            Review {
                review_max_pages: 4,
                ..Review::default()
            }
        );

        assert_eq!(
            add_missing(&path).unwrap(),
            ["full_review_max_notes", "active_days"]
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with(old), "{text}");
        assert_eq!(
            toml::from_str::<Config>(&text).unwrap().review,
            config.review
        );
        assert!(add_missing(&path).unwrap().is_empty());
    }

    #[test]
    fn a_new_config_holds_every_setting() {
        let text = toml::to_string(&Config::new("/v".into())).unwrap();
        assert_eq!(
            text,
            "vault = \"/v\"\ngit_autocommit = true\nfull_review_max_notes = 300\nreview_max_pages = 10\nactive_days = 30\n"
        );
    }
}
