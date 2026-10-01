use std::path::{Path, PathBuf};

use serde_json::json;

use crate::{edit, hooks_json};

const SERVER: &str = "petit-poucet";
const AGENT: &str = "claude";
// Claude Code event names, with the `petit-poucet hook` event each one runs.
const EVENTS: [(&str, &str); 2] = [("SessionStart", "session-start"), ("Stop", "stop")];

// User-level MCP servers live in ~/.claude.json, hooks in ~/.claude/settings.json.
fn files(home: &Path) -> (PathBuf, PathBuf) {
    (
        home.join(".claude.json"),
        home.join(".claude/settings.json"),
    )
}

pub fn setup(home: &Path, exe: &Path) -> Result<String, String> {
    let (state_file, settings_file) = files(home);
    let mut state = edit::read_json(&state_file)?;
    if !state["mcpServers"].is_object() {
        state["mcpServers"] = json!({});
    }
    // What `claude mcp add --scope user` writes.
    state["mcpServers"][SERVER] = json!({"type": "stdio", "command": exe.display().to_string(), "args": ["serve"], "env": {}});

    let mut settings = edit::read_json(&settings_file)?;
    hooks_json::remove(&mut settings, AGENT);
    for (event, ours) in EVENTS {
        hooks_json::add(&mut settings, event, hooks_json::exec(exe, ours, AGENT));
    }

    let changed =
        edit::write_json(&state_file, &state)? | edit::write_json(&settings_file, &settings)?;
    Ok(match changed {
        true => {
            "set up (MCP server in ~/.claude.json, hooks in settings.json): restart Claude Code"
                .to_string()
        }
        false => "already set up".to_string(),
    })
}

pub fn check(home: &Path) -> Result<(String, bool), String> {
    let (state_file, settings_file) = files(home);
    let state = edit::read_json(&state_file)?;
    let settings = edit::read_json(&settings_file)?;
    let mut missing = Vec::new();
    match state["mcpServers"][SERVER]["command"].as_str() {
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
        true => ("set up".to_string(), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent claude`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(home: &Path) -> Result<String, String> {
    let (state_file, settings_file) = files(home);
    let mut state = edit::read_json(&state_file)?;
    let had_server = state["mcpServers"]
        .as_object_mut()
        .is_some_and(|servers| servers.remove(SERVER).is_some());
    let mut settings = edit::read_json(&settings_file)?;
    hooks_json::remove(&mut settings, AGENT);
    let mut changed = edit::write_json_if_present(&settings_file, &settings)?;
    if had_server {
        changed |= edit::write_json(&state_file, &state)?;
    }
    Ok(match changed {
        true => "removed (MCP server and hooks)".to_string(),
        false => "nothing to remove".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;

    use super::*;

    fn read(home: &Path, file: &str) -> Value {
        serde_json::from_str(&fs::read_to_string(home.join(file)).unwrap()).unwrap()
    }

    #[test]
    fn setup_adds_its_entries_and_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        fs::create_dir(home.join(".claude")).unwrap();
        fs::write(
            home.join(".claude.json"),
            r#"{"numStartups": 3, "mcpServers": {"other": {"type": "http", "url": "http://localhost:1"}}}"#,
        )
        .unwrap();
        fs::write(
            home.join(".claude/settings.json"),
            r#"{"model": "opus", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify"}]}]}}"#,
        )
        .unwrap();
        let exe = Path::new("/opt/pp/petit-poucet");

        assert!(setup(home, exe).unwrap().starts_with("set up"));
        let state = read(home, ".claude.json");
        assert_eq!(state["numStartups"], 3);
        assert_eq!(state["mcpServers"]["other"]["url"], "http://localhost:1");
        assert_eq!(
            state["mcpServers"]["petit-poucet"],
            json!({"type": "stdio", "command": "/opt/pp/petit-poucet", "args": ["serve"], "env": {}})
        );
        let settings = read(home, ".claude/settings.json");
        assert_eq!(settings["model"], "opus");
        assert_eq!(
            settings["hooks"]["Stop"][0]["hooks"][0]["command"],
            "notify"
        );
        assert_eq!(
            settings["hooks"]["SessionStart"][0]["hooks"][0],
            json!({"type": "command", "command": "/opt/pp/petit-poucet", "args": ["hook", "session-start", "--agent", "claude"]})
        );
        assert!(edit::backup_path(&home.join(".claude.json")).is_file());

        assert_eq!(setup(home, exe).unwrap(), "already set up");
        setup(home, Path::new("/usr/local/bin/pp")).unwrap();
        let moved = read(home, ".claude/settings.json");
        assert_eq!(
            moved["hooks"]["Stop"].as_array().unwrap().len(),
            2,
            "replaced, not added"
        );
    }

    #[test]
    fn a_file_it_cannot_read_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".claude.json"), "{ not json").unwrap();
        assert!(setup(tmp.path(), Path::new("/bin/pp")).is_err());
        assert_eq!(
            fs::read_to_string(tmp.path().join(".claude.json")).unwrap(),
            "{ not json"
        );
        assert!(!tmp.path().join(".claude/settings.json").exists());
    }

    #[test]
    fn check_names_what_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        assert_eq!(
            check(home).unwrap(),
            (
                "missing the MCP server, the SessionStart hook, the Stop hook: run `petit-poucet setup --agent claude`".to_string(),
                false
            )
        );
        let exe = home.join("petit-poucet");
        fs::write(&exe, "").unwrap();
        setup(home, &exe).unwrap();
        assert_eq!(check(home).unwrap(), ("set up".to_string(), true));
        fs::remove_file(&exe).unwrap();
        let (line, ok) = check(home).unwrap();
        assert!(!ok && line.contains("(the MCP server's binary)"), "{line}");
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        fs::create_dir(home.join(".claude")).unwrap();
        let state = r#"{"numStartups": 3, "mcpServers": {"other": {"type": "http", "url": "http://localhost:1"}}}"#;
        let settings = r#"{"model": "opus"}"#;
        fs::write(home.join(".claude.json"), state).unwrap();
        fs::write(home.join(".claude/settings.json"), settings).unwrap();
        setup(home, Path::new("/bin/pp")).unwrap();

        assert_eq!(uninstall(home).unwrap(), "removed (MCP server and hooks)");
        assert_eq!(
            read(home, ".claude.json"),
            serde_json::from_str::<Value>(state).unwrap()
        );
        assert_eq!(
            read(home, ".claude/settings.json"),
            serde_json::from_str::<Value>(settings).unwrap()
        );
        assert_eq!(uninstall(home).unwrap(), "nothing to remove");
    }
}
