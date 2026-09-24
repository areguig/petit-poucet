use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const VAULT_ENV: &str = "PETIT_POUCET_VAULT";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub vault: PathBuf,
    #[serde(default = "yes")]
    pub git_autocommit: bool,
}

fn yes() -> bool {
    true
}

impl Config {
    pub fn new(vault: PathBuf) -> Config {
        Config {
            vault,
            git_autocommit: true,
        }
    }

    pub fn path() -> Result<PathBuf, String> {
        let home = std::env::home_dir().ok_or("cannot find the home folder")?;
        Ok(home.join(".config/petit-poucet/config.toml"))
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
