use std::path::{Path, PathBuf};

use serde_json::json;

use crate::{edit, hooks_json};

const SERVER: &str = "petit-poucet";
const AGENT: &str = "gemini";
// Gemini CLI event names, with the `petit-poucet hook` event each one runs.
const EVENTS: [(&str, &str); 2] = [("SessionStart", "session-start"), ("AfterAgent", "stop")];
// Gemini CLI runs MCP servers only in folders the user trusted.
const TRUST: &str = "Gemini CLI starts it only in folders you trust";

fn settings_file(dir: &Path) -> PathBuf {
    dir.join("settings.json")
}

pub fn setup(dir: &Path, exe: &Path) -> Result<String, String> {
    let mut settings = edit::read_json(&settings_file(dir))?;
    if !settings["mcpServers"].is_object() {
        settings["mcpServers"] = json!({});
    }
    settings["mcpServers"][SERVER] =
        json!({"command": exe.display().to_string(), "args": ["serve"]});
    hooks_json::remove(&mut settings, AGENT);
    for (event, ours) in EVENTS {
        let command = hooks_json::command(exe, ours, AGENT);
        // Gemini CLI timeouts are in milliseconds.
        let hook = json!({"name": SERVER, "type": "command", "command": command, "timeout": 30000});
        hooks_json::add(&mut settings, event, hook);
    }
    Ok(match edit::write_json(&settings_file(dir), &settings)? {
        true => format!("set up (MCP server and hooks in settings.json): {TRUST}"),
        false => "already set up".to_string(),
    })
}

pub fn check(dir: &Path) -> Result<(String, bool), String> {
    let settings = edit::read_json(&settings_file(dir))?;
    let mut missing = Vec::new();
    match settings["mcpServers"][SERVER]["command"].as_str() {
        None => missing.push("the MCP server".to_string()),
        Some(path) if !Path::new(path).is_file() => {
            missing.push(format!("{path} (the MCP server's binary)"))
        }
        Some(_) => {}
    }
    for (event, _) in EVENTS {
        if !hooks_json::has(&settings, event, AGENT) {
            missing.push(format!("the {event} hook"));
        }
    }
    Ok(match missing.is_empty() {
        true => (format!("set up ({TRUST})"), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent gemini`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(dir: &Path) -> Result<String, String> {
    let file = settings_file(dir);
    let mut settings = edit::read_json(&file)?;
    let before = settings.clone();
    if let Some(servers) = settings["mcpServers"].as_object_mut() {
        servers.remove(SERVER);
    }
    hooks_json::drop_if_empty(&mut settings, "mcpServers");
    hooks_json::remove(&mut settings, AGENT);
    Ok(
        match settings != before && edit::write_json_if_present(&file, &settings)? {
            true => "removed (MCP server and hooks)".to_string(),
            false => "nothing to remove".to_string(),
        },
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;

    use super::*;

    fn read(dir: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(settings_file(dir)).unwrap()).unwrap()
    }

    const MINE: &str = r#"{"theme": "Dracula", "security": {"auth": {"selectedType": "oauth-personal"}},
        "mcpServers": {"other": {"url": "http://localhost:1"}},
        "hooks": {"AfterAgent": [{"hooks": [{"type": "command", "command": "notify"}]}]}}"#;

    #[test]
    fn setup_adds_its_entries_and_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(settings_file(dir), MINE).unwrap();
        let exe = Path::new("/opt/pp/petit-poucet");

        assert!(setup(dir, exe).unwrap().starts_with("set up"));
        let settings = read(dir);
        assert_eq!(settings["theme"], "Dracula");
        assert_eq!(
            settings["security"]["auth"]["selectedType"],
            "oauth-personal"
        );
        assert_eq!(settings["mcpServers"]["other"]["url"], "http://localhost:1");
        assert_eq!(
            settings["mcpServers"]["petit-poucet"],
            json!({"command": "/opt/pp/petit-poucet", "args": ["serve"]})
        );
        assert_eq!(
            settings["hooks"]["AfterAgent"][0]["hooks"][0]["command"],
            "notify"
        );
        assert_eq!(
            settings["hooks"]["AfterAgent"][1]["hooks"][0],
            json!({"name": "petit-poucet", "type": "command", "command": "\"/opt/pp/petit-poucet\" hook stop --agent gemini", "timeout": 30000})
        );
        assert_eq!(
            settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "\"/opt/pp/petit-poucet\" hook session-start --agent gemini"
        );
        assert!(edit::backup_path(&settings_file(dir)).is_file());

        assert_eq!(setup(dir, exe).unwrap(), "already set up");
        setup(dir, Path::new("/usr/local/bin/pp")).unwrap();
        assert_eq!(
            read(dir)["hooks"]["AfterAgent"].as_array().unwrap().len(),
            2,
            "replaced, not added"
        );
    }

    #[test]
    fn a_settings_file_it_cannot_read_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(settings_file(tmp.path()), "{ not json").unwrap();
        assert!(setup(tmp.path(), Path::new("/bin/pp")).is_err());
        assert_eq!(
            fs::read_to_string(settings_file(tmp.path())).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn check_names_what_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert_eq!(
            check(dir).unwrap(),
            (
                "missing the MCP server, the SessionStart hook, the AfterAgent hook: run `petit-poucet setup --agent gemini`".to_string(),
                false
            )
        );
        let exe = dir.join("petit-poucet");
        fs::write(&exe, "").unwrap();
        setup(dir, &exe).unwrap();
        assert!(check(dir).unwrap().1);
        fs::remove_file(&exe).unwrap();
        let (line, ok) = check(dir).unwrap();
        assert!(!ok && line.contains("(the MCP server's binary)"), "{line}");
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(settings_file(dir), MINE).unwrap();
        setup(dir, Path::new("/bin/pp")).unwrap();

        assert_eq!(uninstall(dir).unwrap(), "removed (MCP server and hooks)");
        assert_eq!(read(dir), serde_json::from_str::<Value>(MINE).unwrap());
        assert_eq!(uninstall(dir).unwrap(), "nothing to remove");
    }
}
