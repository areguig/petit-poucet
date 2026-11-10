use super::report::{Item, Status, edit};
use crate::config::Config;

pub fn inspect() -> (Option<Config>, Vec<Item>) {
    let path = match Config::path() {
        Ok(path) => path,
        Err(_) => {
            return (
                None,
                vec![Item::new(
                    "config",
                    Status::Error,
                    "home directory unavailable",
                    Some("petit-poucet setup".into()),
                )],
            );
        }
    };
    let present = path.exists();
    match Config::read() {
        Err(_) => {
            let (message, fix) = if present {
                (
                    format!(
                        "{} at {}; edit the configuration",
                        invalid_description(&path),
                        path.display()
                    ),
                    edit(&path),
                )
            } else {
                (
                    format!("missing at {}", path.display()),
                    "petit-poucet setup".into(),
                )
            };
            (
                None,
                vec![Item::new("config", Status::Error, message, Some(fix))],
            )
        }
        Ok(read) => {
            let mut items = vec![Item::new(
                "config",
                if present { Status::Ok } else { Status::Info },
                if present {
                    path.display().to_string()
                } else {
                    "no file; using PETIT_POUCET_VAULT and defaults".into()
                },
                None,
            )];
            items.extend(settings(&read.config, &read.missing, read.unknown, &path));
            (Some(read.config), items)
        }
    }
}

fn invalid_description(path: &std::path::Path) -> String {
    let invalid = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| invalid_keys(&text));
    match invalid {
        Some(keys) if !keys.is_empty() => format!("invalid keys: {}", keys.join(", ")),
        _ => "invalid or unreadable settings".into(),
    }
}

fn invalid_keys(text: &str) -> Option<Vec<String>> {
    let supplied: toml::Table = toml::from_str(text).ok()?;
    let defaults: toml::Table =
        toml::from_str(&toml::to_string(&Config::new(std::path::PathBuf::new())).ok()?).ok()?;
    let mut invalid = Vec::new();
    for key in defaults.keys() {
        let Some(value) = supplied.get(key) else {
            if key == "vault" {
                invalid.push(key.clone());
            }
            continue;
        };
        let mut candidate = defaults.clone();
        candidate.insert(key.clone(), value.clone());
        match toml::Value::Table(candidate).try_into::<Config>() {
            Ok(config) if config.limits.validate().is_ok() => {}
            _ => invalid.push(key.clone()),
        }
    }
    Some(invalid)
}

fn settings(
    config: &Config,
    missing: &[String],
    unknown: usize,
    path: &std::path::Path,
) -> Vec<Item> {
    let mut items = Vec::new();
    let fields = serde_json::to_value(config).expect("config serializes");
    for (key, value) in fields.as_object().expect("config is an object") {
        let missing = missing.iter().any(|m| m == key);
        items.push(Item::new(
            format!("config {key}"),
            if missing { Status::Warning } else { Status::Ok },
            if missing {
                format!("missing; using default {value}")
            } else {
                value.to_string()
            },
            missing.then(|| "petit-poucet setup".into()),
        ));
    }
    if unknown > 0 {
        items.push(Item::new(
            "config keys",
            Status::Warning,
            format!(
                "{} unrecognized or obsolete keys; edit the configuration (values withheld)",
                unknown
            ),
            Some(edit(path)),
        ));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn invalid_keys_are_named_without_repeating_values() {
        let keys = invalid_keys("vault='synthetic-vault'\ngit_autocommit='synthetic-private-value'\nsession_start_max_bytes=1\n").unwrap();
        assert_eq!(keys, ["git_autocommit", "session_start_max_bytes"]);
        assert_eq!(invalid_keys("vault=["), None);
        assert_eq!(invalid_keys(""), Some(vec!["vault".into()]));
    }

    #[test]
    fn missing_keys_name_setup_and_unknown_keys_name_the_editor_without_values() {
        let config = Config::new("synthetic-vault".into());
        let items = settings(
            &config,
            &["review_max_pages".into()],
            1,
            Path::new("config.toml"),
        );
        let missing = items
            .iter()
            .find(|item| item.label == "config review_max_pages")
            .unwrap();
        assert_eq!(missing.status, Status::Warning);
        assert!(missing.message.contains("missing; using default 10"));
        assert_eq!(missing.fix.as_deref(), Some("petit-poucet setup"));
        let unknown = items.last().unwrap();
        assert_eq!(unknown.status, Status::Warning);
        assert_eq!(
            unknown.fix.as_deref(),
            Some(edit(Path::new("config.toml")).as_str())
        );
        assert!(unknown.message.contains("values withheld"));
        assert!(
            settings(&config, &[], 0, Path::new("config.toml"))
                .iter()
                .all(|item| item.status == Status::Ok)
        );
    }
}
