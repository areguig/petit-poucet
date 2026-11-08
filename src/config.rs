use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use toml_edit::DocumentMut;

const VAULT_ENV: &str = "PETIT_POUCET_VAULT";
// Room for a page footer and one line of a continued page.
pub const MIN_SESSION_START_BYTES: usize = 2000;
const OBSOLETE_SETTINGS: [&str; 1] = ["index_max_notes"];

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
    pub session_start_max_bytes: usize,
    pub cleanup_reminder_notes: usize,
    pub cleanup_reminder_days: u64,
}

impl Limits {
    pub fn validate(&self) -> Result<(), String> {
        let bytes = self.session_start_max_bytes;
        match bytes < MIN_SESSION_START_BYTES {
            true => Err(format!(
                "session_start_max_bytes is {bytes}, below the minimum of {MIN_SESSION_START_BYTES} bytes"
            )),
            false => Ok(()),
        }
    }
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
                session_start_max_bytes: 8000,
                cleanup_reminder_notes: 30,
                cleanup_reminder_days: 30,
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
                let updated = with_missing(&text).map_err(error)?;
                // The file stays the one place every value is seen; a read-only one still loads.
                if updated != text
                    && let Err(e) = crate::vault::write_atomic(&path, &updated)
                {
                    eprintln!(
                        "petit-poucet: {}: settings not written in: {e}",
                        path.display()
                    );
                }
                let config = toml::from_str::<Config>(&updated)
                    .map_err(|e| error(e.message().to_string()))?;
                config.limits.validate().map_err(error)?;
                Some(config)
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

// A config written by an older version, with the settings added since appended, the dropped ones removed and its own lines kept.
fn with_missing(text: &str) -> Result<String, String> {
    let mut config: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    let defaults: DocumentMut = toml::to_string(&Config::new(PathBuf::new()))
        .map_err(|e| e.to_string())?
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    for key in OBSOLETE_SETTINGS {
        config.remove(key);
    }
    for (key, value) in defaults.iter().filter(|(key, _)| *key != "vault") {
        if !config.contains_key(key) {
            config.insert(key, value.clone());
        }
    }
    Ok(config.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_config_gets_the_missing_settings_once_and_keeps_its_own() {
        let old = "# my vault\nvault = \"/v\"\nreview_max_pages = 4\n";
        let text = with_missing(old).unwrap();
        assert!(text.starts_with(old), "{text}");
        let config: Config = toml::from_str(&text).unwrap();
        let mut expected = Config::new("/v".into());
        expected.limits.review_max_pages = 4;
        assert_eq!(config, expected);
        assert_eq!(with_missing(&text).unwrap(), text);
    }

    #[test]
    fn a_setting_that_no_longer_exists_is_dropped_with_the_rest_kept() {
        let old = "# mine\nvault = \"/v\"\nindex_max_notes = 5\nreview_max_pages = 4 # pages\n";
        let text = with_missing(old).unwrap();
        assert!(!text.contains("index_max_notes"), "{text}");
        assert!(
            text.starts_with("# mine\nvault = \"/v\"\nreview_max_pages = 4 # pages\n"),
            "{text}"
        );
        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.limits.review_max_pages, 4);
        assert_eq!(
            config.limits.session_start_max_bytes,
            Config::new("/v".into()).limits.session_start_max_bytes
        );
    }

    #[test]
    fn a_session_budget_below_the_minimum_is_refused_naming_the_setting() {
        let mut limits = Config::new("/v".into()).limits;
        assert_eq!(limits.validate(), Ok(()));
        limits.session_start_max_bytes = MIN_SESSION_START_BYTES - 1;
        let error = limits.validate().unwrap_err();
        assert_eq!(
            error,
            format!(
                "session_start_max_bytes is {}, below the minimum of {MIN_SESSION_START_BYTES} bytes",
                MIN_SESSION_START_BYTES - 1
            )
        );
        limits.session_start_max_bytes = MIN_SESSION_START_BYTES;
        assert_eq!(limits.validate(), Ok(()));
    }

    #[test]
    fn a_new_config_holds_every_setting() {
        let text = toml::to_string(&Config::new("/v".into())).unwrap();
        assert_eq!(
            text,
            "vault = \"/v\"\ngit_autocommit = true\nfull_review_max_notes = 300\nreview_max_pages = 10\nactive_days = 30\nunused_days = 90\nsummary_max_chars = 200\nsession_start_max_bytes = 8000\ncleanup_reminder_notes = 30\ncleanup_reminder_days = 30\n"
        );
    }
}
