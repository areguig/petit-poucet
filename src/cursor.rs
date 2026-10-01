use std::path::Path;

use serde_json::{Value, json};

use crate::edit;

const SERVER: &str = "petit-poucet";
// Cursor event names, with the `petit-poucet hook` event each one runs.
const EVENTS: [(&str, &str); 2] = [("sessionStart", "session-start"), ("stop", "stop")];

fn hook_command(exe: &Path, event: &str) -> String {
    format!("\"{}\" hook {event} --agent cursor", exe.display())
}

// Ours whatever the binary is called or where it lives: matched by the arguments `hook_command` writes.
fn is_ours(entry: &Value) -> bool {
    entry["command"].as_str().is_some_and(|c| {
        EVENTS
            .iter()
            .any(|(_, ours)| c.ends_with(&format!(" hook {ours} --agent cursor")))
    })
}

fn without_ours(hooks: &mut Value) {
    if let Some(events) = hooks["hooks"].as_object_mut() {
        for entries in events.values_mut().filter_map(Value::as_array_mut) {
            entries.retain(|e| !is_ours(e));
        }
        events.retain(|_, entries| entries.as_array().is_none_or(|e| !e.is_empty()));
    }
}

pub fn setup(dir: &Path, exe: &Path) -> Result<String, String> {
    let mcp_file = dir.join("mcp.json");
    let mut mcp = edit::read_json(&mcp_file)?;
    if !mcp["mcpServers"].is_object() {
        mcp["mcpServers"] = json!({});
    }
    mcp["mcpServers"][SERVER] = json!({"command": exe.display().to_string(), "args": ["serve"]});

    let hooks_file = dir.join("hooks.json");
    let mut hooks = edit::read_json(&hooks_file)?;
    without_ours(&mut hooks);
    if hooks["version"].is_null() {
        hooks["version"] = json!(1);
    }
    if !hooks["hooks"].is_object() {
        hooks["hooks"] = json!({});
    }
    for (event, ours) in EVENTS {
        let entry = json!({"command": hook_command(exe, ours)});
        match hooks["hooks"][event].as_array_mut() {
            Some(entries) => entries.push(entry),
            None => hooks["hooks"][event] = json!([entry]),
        }
    }

    let changed = edit::write_json(&mcp_file, &mcp)? | edit::write_json(&hooks_file, &hooks)?;
    Ok(match changed {
        true => "set up (MCP server in mcp.json, hooks in hooks.json): restart Cursor".to_string(),
        false => "already set up".to_string(),
    })
}

pub fn check(dir: &Path) -> Result<(String, bool), String> {
    let mcp = edit::read_json(&dir.join("mcp.json"))?;
    let hooks = edit::read_json(&dir.join("hooks.json"))?;
    let mut missing = Vec::new();
    match mcp["mcpServers"][SERVER]["command"].as_str() {
        None => missing.push("the MCP server".to_string()),
        Some(path) if !Path::new(path).is_file() => {
            missing.push(format!("{path} (the MCP server's binary)"))
        }
        Some(_) => {}
    }
    for (event, _) in EVENTS {
        let found = hooks["hooks"][event]
            .as_array()
            .is_some_and(|entries| entries.iter().any(is_ours));
        if !found {
            missing.push(format!("the {event} hook"));
        }
    }
    Ok(match missing.is_empty() {
        true => ("set up".to_string(), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent cursor`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(dir: &Path) -> Result<String, String> {
    let mcp_file = dir.join("mcp.json");
    let mut mcp = edit::read_json(&mcp_file)?;
    let had_server = mcp["mcpServers"]
        .as_object_mut()
        .is_some_and(|servers| servers.remove(SERVER).is_some());
    let hooks_file = dir.join("hooks.json");
    let mut hooks = edit::read_json(&hooks_file)?;
    without_ours(&mut hooks);
    let mut changed = edit::write_json_if_present(&hooks_file, &hooks)?;
    if had_server {
        changed |= edit::write_json(&mcp_file, &mcp)?;
    }
    Ok(match changed {
        true => "removed (MCP server and hooks)".to_string(),
        false => "nothing to remove".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn read(dir: &Path, file: &str) -> Value {
        serde_json::from_str(&fs::read_to_string(dir.join(file)).unwrap()).unwrap()
    }

    #[test]
    fn setup_adds_its_entries_and_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(
            dir.join("mcp.json"),
            r#"{"mcpServers": {"other": {"url": "http://localhost:1"}}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("hooks.json"),
            r#"{"version": 1, "hooks": {"stop": [{"command": "notify"}]}}"#,
        )
        .unwrap();
        let exe = Path::new("/opt/pp/petit-poucet");

        assert!(setup(dir, exe).unwrap().starts_with("set up"));
        let mcp = read(dir, "mcp.json");
        assert_eq!(mcp["mcpServers"]["other"]["url"], "http://localhost:1");
        assert_eq!(
            mcp["mcpServers"]["petit-poucet"],
            json!({"command": "/opt/pp/petit-poucet", "args": ["serve"]})
        );
        let hooks = read(dir, "hooks.json");
        assert_eq!(hooks["version"], 1);
        assert_eq!(hooks["hooks"]["stop"][0]["command"], "notify");
        assert_eq!(
            hooks["hooks"]["stop"][1]["command"],
            "\"/opt/pp/petit-poucet\" hook stop --agent cursor"
        );
        assert_eq!(
            hooks["hooks"]["sessionStart"][0]["command"],
            "\"/opt/pp/petit-poucet\" hook session-start --agent cursor"
        );
        assert!(edit::backup_path(&dir.join("mcp.json")).is_file());

        assert_eq!(setup(dir, exe).unwrap(), "already set up");
        setup(dir, Path::new("/usr/local/bin/pp")).unwrap();
        let hooks = read(dir, "hooks.json");
        assert_eq!(
            hooks["hooks"]["stop"].as_array().unwrap().len(),
            2,
            "replaced, not added"
        );
    }

    #[test]
    fn setup_on_a_fresh_cursor_writes_both_files() {
        let tmp = tempfile::tempdir().unwrap();
        setup(tmp.path(), Path::new("/bin/pp")).unwrap();
        assert_eq!(
            read(tmp.path(), "hooks.json"),
            json!({"version": 1, "hooks": {
                "sessionStart": [{"command": "\"/bin/pp\" hook session-start --agent cursor"}],
                "stop": [{"command": "\"/bin/pp\" hook stop --agent cursor"}],
            }})
        );
    }

    #[test]
    fn a_file_it_cannot_read_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("mcp.json"), "{ not json").unwrap();
        assert!(setup(tmp.path(), Path::new("/bin/pp")).is_err());
        assert_eq!(
            fs::read_to_string(tmp.path().join("mcp.json")).unwrap(),
            "{ not json"
        );
        assert!(!tmp.path().join("hooks.json").exists(), "nothing written");
    }

    #[test]
    fn check_names_what_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert_eq!(
            check(dir).unwrap(),
            (
                "missing the MCP server, the sessionStart hook, the stop hook: run `petit-poucet setup --agent cursor`".to_string(),
                false
            )
        );
        let exe = dir.join("petit-poucet");
        fs::write(&exe, "").unwrap();
        setup(dir, &exe).unwrap();
        assert_eq!(check(dir).unwrap(), ("set up".to_string(), true));
        fs::remove_file(&exe).unwrap();
        let (line, ok) = check(dir).unwrap();
        assert!(!ok && line.contains("(the MCP server's binary)"), "{line}");
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mcp = r#"{"mcpServers": {"other": {"url": "http://localhost:1"}}}"#;
        let hooks = r#"{"version": 1, "hooks": {"stop": [{"command": "notify"}]}}"#;
        fs::write(dir.join("mcp.json"), mcp).unwrap();
        fs::write(dir.join("hooks.json"), hooks).unwrap();
        setup(dir, Path::new("/bin/pp")).unwrap();

        assert_eq!(uninstall(dir).unwrap(), "removed (MCP server and hooks)");
        assert_eq!(
            read(dir, "mcp.json"),
            serde_json::from_str::<Value>(mcp).unwrap()
        );
        assert_eq!(
            read(dir, "hooks.json"),
            serde_json::from_str::<Value>(hooks).unwrap()
        );
        assert_eq!(uninstall(dir).unwrap(), "nothing to remove");
    }
}
