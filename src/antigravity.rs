use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::{edit, hooks_json};

// Also the name of our entry in hooks.json, which holds named hooks.
const SERVER: &str = "petit-poucet";
const AGENT: &str = "antigravity";
// Antigravity CLI event names, with the `petit-poucet hook` event each one runs.
// PreInvocation runs before every model call: its message reaches each call once and never piles up.
const EVENTS: [(&str, &str); 2] = [("PreInvocation", "session-start"), ("Stop", "stop")];

// The global customization root, shared by every Antigravity product.
pub fn dir(home: &Path) -> PathBuf {
    home.join(".gemini/config")
}

pub fn setup(dir: &Path, exe: &Path) -> Result<String, String> {
    let mcp_file = dir.join("mcp_config.json");
    let mut mcp = edit::read_json(&mcp_file)?;
    if !mcp["mcpServers"].is_object() {
        mcp["mcpServers"] = json!({});
    }
    mcp["mcpServers"][SERVER] = json!({"command": exe.display().to_string(), "args": ["serve"]});

    let hooks_file = dir.join("hooks.json");
    let mut hooks = edit::read_json(&hooks_file)?;
    let ours = EVENTS.map(|(event, ours)| {
        let command = hooks_json::command(exe, ours, AGENT);
        (
            event.to_string(),
            json!([{"type": "command", "command": command, "timeout": 30}]),
        )
    });
    hooks[SERVER] = Value::Object(ours.into_iter().collect());

    let changed = edit::write_json(&mcp_file, &mcp)? | edit::write_json(&hooks_file, &hooks)?;
    Ok(match changed {
        true => "set up (MCP server in mcp_config.json, hooks in hooks.json)".to_string(),
        false => "already set up".to_string(),
    })
}

pub fn check(dir: &Path) -> Result<(String, bool), String> {
    let mcp = edit::read_json(&dir.join("mcp_config.json"))?;
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
        let found = hooks[SERVER][event].as_array().is_some_and(|handlers| {
            handlers.iter().any(|h| {
                h["command"]
                    .as_str()
                    .is_some_and(|c| hooks_json::is_ours(c, AGENT))
            })
        });
        if !found {
            missing.push(format!("the {event} hook"));
        }
    }
    if hooks[SERVER]["enabled"] == json!(false) {
        missing.push("its hooks are disabled (\"enabled\": false)".to_string());
    }
    Ok(match missing.is_empty() {
        true => ("set up".to_string(), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent antigravity`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(dir: &Path) -> Result<String, String> {
    let mcp_file = dir.join("mcp_config.json");
    let mut mcp = edit::read_json(&mcp_file)?;
    let had_server = mcp["mcpServers"]
        .as_object_mut()
        .is_some_and(|servers| servers.remove(SERVER).is_some());
    hooks_json::drop_if_empty(&mut mcp, "mcpServers");
    let hooks_file = dir.join("hooks.json");
    let mut hooks = edit::read_json(&hooks_file)?;
    let had_hooks = hooks
        .as_object_mut()
        .is_some_and(|named| named.remove(SERVER).is_some());
    let mut changed = false;
    if had_server {
        changed |= edit::write_json(&mcp_file, &mcp)?;
    }
    if had_hooks {
        changed |= edit::write_json(&hooks_file, &hooks)?;
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
            dir.join("mcp_config.json"),
            r#"{"mcpServers": {"other": {"serverUrl": "http://localhost:1"}}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("hooks.json"),
            r#"{"lint": {"PostToolUse": [{"matcher": "*", "hooks": [{"command": "lint"}]}]}}"#,
        )
        .unwrap();
        let exe = Path::new("/opt/pp/petit-poucet");

        assert!(setup(dir, exe).unwrap().starts_with("set up"));
        let mcp = read(dir, "mcp_config.json");
        assert_eq!(
            mcp["mcpServers"]["other"]["serverUrl"],
            "http://localhost:1"
        );
        assert_eq!(
            mcp["mcpServers"]["petit-poucet"],
            json!({"command": "/opt/pp/petit-poucet", "args": ["serve"]})
        );
        let hooks = read(dir, "hooks.json");
        assert_eq!(
            hooks["lint"]["PostToolUse"][0]["hooks"][0]["command"],
            "lint"
        );
        assert_eq!(
            hooks["petit-poucet"],
            json!({
                "PreInvocation": [{"type": "command", "command": "\"/opt/pp/petit-poucet\" hook session-start --agent antigravity", "timeout": 30}],
                "Stop": [{"type": "command", "command": "\"/opt/pp/petit-poucet\" hook stop --agent antigravity", "timeout": 30}],
            })
        );
        assert!(edit::backup_path(&dir.join("hooks.json")).is_file());

        assert_eq!(setup(dir, exe).unwrap(), "already set up");
        setup(dir, Path::new("/usr/local/bin/pp")).unwrap();
        let moved = read(dir, "hooks.json");
        assert_eq!(
            moved["petit-poucet"]["Stop"].as_array().unwrap().len(),
            1,
            "replaced, not added"
        );
    }

    #[test]
    fn a_file_it_cannot_read_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("hooks.json"), "{ not json").unwrap();
        assert!(setup(tmp.path(), Path::new("/bin/pp")).is_err());
        assert_eq!(
            fs::read_to_string(tmp.path().join("hooks.json")).unwrap(),
            "{ not json"
        );
        assert!(
            !tmp.path().join("mcp_config.json").exists(),
            "nothing written"
        );
    }

    #[test]
    fn check_names_what_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert_eq!(
            check(dir).unwrap(),
            (
                "missing the MCP server, the PreInvocation hook, the Stop hook: run `petit-poucet setup --agent antigravity`".to_string(),
                false
            )
        );
        let exe = dir.join("petit-poucet");
        fs::write(&exe, "").unwrap();
        setup(dir, &exe).unwrap();
        assert_eq!(check(dir).unwrap(), ("set up".to_string(), true));

        let mut hooks = read(dir, "hooks.json");
        hooks["petit-poucet"]["enabled"] = json!(false);
        fs::write(dir.join("hooks.json"), hooks.to_string()).unwrap();
        fs::remove_file(&exe).unwrap();
        let (line, ok) = check(dir).unwrap();
        assert!(
            !ok && line.contains("(the MCP server's binary)") && line.contains("disabled"),
            "{line}"
        );
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mcp = r#"{"mcpServers": {"other": {"serverUrl": "http://localhost:1"}}}"#;
        let hooks = r#"{"lint": {"Stop": [{"command": "lint"}]}}"#;
        fs::write(dir.join("mcp_config.json"), mcp).unwrap();
        fs::write(dir.join("hooks.json"), hooks).unwrap();
        setup(dir, Path::new("/bin/pp")).unwrap();

        assert_eq!(uninstall(dir).unwrap(), "removed (MCP server and hooks)");
        assert_eq!(
            read(dir, "mcp_config.json"),
            serde_json::from_str::<Value>(mcp).unwrap()
        );
        assert_eq!(
            read(dir, "hooks.json"),
            serde_json::from_str::<Value>(hooks).unwrap()
        );
        assert_eq!(uninstall(dir).unwrap(), "nothing to remove");
    }

    #[test]
    fn uninstall_drops_the_servers_section_it_alone_used() {
        let tmp = tempfile::tempdir().unwrap();
        setup(tmp.path(), Path::new("/bin/pp")).unwrap();
        uninstall(tmp.path()).unwrap();
        assert_eq!(read(tmp.path(), "mcp_config.json"), json!({}));
        assert_eq!(read(tmp.path(), "hooks.json"), json!({}));
    }
}
