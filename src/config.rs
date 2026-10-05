use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use toml_edit::DocumentMut;

const VAULT_ENV: &str = "PETIT_POUCET_VAULT";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub vault: PathBuf,
    pub git_autocommit: bool,
    #[serde(flatten)]
    pub limits: Limits,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Limits {
    pub full_review_max_notes: usize,
    pub review_max_pages: usize,
    pub active_days: i64,
    pub unused_days: i64,
    pub summary_max_chars: usize,
    pub index_max_notes: usize,
}

impl Config {
    pub fn new(vault: PathBuf) -> Config {
        Config {
            vault,
            git_autocommit: true,
            limits: Limits {
                full_review_max_notes: 300,
                review_max_pages: 10,
                active_days: 30,
                unused_days: 90,
                summary_max_chars: 200,
                index_max_notes: 100,
            },
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
            Ok(text) => {
                let error = |e: String| format!("{}: {e}", path.display());
                let (text, added) = with_missing(&text).map_err(error)?;
                // The file stays the one place every value is seen; a read-only one still loads.
                if !added.is_empty()
                    && let Err(e) = crate::vault::write_atomic(&path, &text)
                {
                    eprintln!(
                        "petit-poucet: {}: settings not written in: {e}",
                        path.display()
                    );
                }
                Some(toml::from_str::<Config>(&text).map_err(|e| error(e.message().to_string()))?)
            }
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

// A config written by an older version, with the settings added since appended and its own lines kept.
fn with_missing(text: &str) -> Result<(String, Vec<String>), String> {
    let mut config: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    let defaults: DocumentMut = toml::to_string(&Config::new(PathBuf::new()))
        .map_err(|e| e.to_string())?
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    let mut added = Vec::new();
    for (key, value) in defaults.iter().filter(|(key, _)| *key != "vault") {
        if !config.contains_key(key) {
            config.insert(key, value.clone());
            added.push(key.to_string());
        }
    }
    Ok((config.to_string(), added))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_config_gets_the_missing_settings_once_and_keeps_its_own() {
        let old = "# my vault\nvault = \"/v\"\nreview_max_pages = 4\n";
        let (text, added) = with_missing(old).unwrap();
        assert_eq!(
            added,
            [
                "git_autocommit",
                "full_review_max_notes",
                "active_days",
                "unused_days",
                "summary_max_chars",
                "index_max_notes"
            ]
        );
        assert!(text.starts_with(old), "{text}");
        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.limits.review_max_pages, 4);
        assert_eq!(config.limits.active_days, 30);
        assert!(config.git_autocommit);
        assert_eq!(with_missing(&text).unwrap(), (text.clone(), vec![]));
    }

    #[test]
    fn a_new_config_holds_every_setting() {
        let text = toml::to_string(&Config::new("/v".into())).unwrap();
        assert_eq!(
            text,
            "vault = \"/v\"\ngit_autocommit = true\nfull_review_max_notes = 300\nreview_max_pages = 10\nactive_days = 30\nunused_days = 90\nsummary_max_chars = 200\nindex_max_notes = 100\n"
        );
    }
}
