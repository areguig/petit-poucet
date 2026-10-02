use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::{edit, hook_command};

const SERVER: &str = "petit-poucet";
const AGENT: &str = "copilot";
// Copilot event names, with the `petit-poucet hook` event each one runs.
const EVENTS: [(&str, &str); 2] = [("sessionStart", "session-start"), ("agentStop", "stop")];

// Copilot CLI and its IDE hosts (VS Code, IntelliJ, the desktop app) all read this folder.
pub fn dir(home: &Path) -> PathBuf {
    std::env::var_os("COPILOT_HOME").map_or_else(|| home.join(".copilot"), PathBuf::from)
}

// petit-poucet's own file: nothing else lives in it.
fn hooks_file(dir: &Path) -> PathBuf {
    dir.join("hooks/petit-poucet.json")
}

// Copilot's IDE hosts run `bash` or `powershell` entries; its `exec` form is CLI-only.
fn hook(exe: &Path, event: &str) -> Value {
    json!({
        "type": "command",
        "bash": hook_command::posix(exe, event, AGENT),
        "powershell": hook_command::powershell(exe, event, AGENT),
        "timeoutSec": 30,
    })
}

pub fn setup(dir: &Path, exe: &Path) -> Result<String, String> {
    let mcp_file = dir.join("mcp-config.json");
    let mut mcp = edit::read_json(&mcp_file)?;
    if !mcp["mcpServers"].is_object() {
        mcp["mcpServers"] = json!({});
    }
    // What `copilot mcp add` writes.
    mcp["mcpServers"][SERVER] = json!({"tools": ["*"], "type": "local", "command": exe.display().to_string(), "args": ["serve"]});

    let hooks = EVENTS.map(|(event, ours)| (event.to_string(), json!([hook(exe, ours)])));
    let hooks = json!({"version": 1, "hooks": Value::Object(hooks.into_iter().collect())});
    let hooks_file = hooks_file(dir);
    fs::create_dir_all(hooks_file.parent().unwrap()).map_err(|e| e.to_string())?;

    let changed = edit::write_json(&mcp_file, &mcp)? | edit::write_json(&hooks_file, &hooks)?;
    Ok(match changed {
        true => {
            "set up (MCP server in mcp-config.json, hooks in hooks/petit-poucet.json)".to_string()
        }
        false => "already set up".to_string(),
    })
}

pub fn check(dir: &Path) -> Result<(String, bool), String> {
    let mcp = edit::read_json(&dir.join("mcp-config.json"))?;
    let hooks = edit::read_json(&hooks_file(dir))?;
    let mut missing = Vec::new();
    match mcp["mcpServers"][SERVER]["command"].as_str() {
        None => missing.push("the MCP server".to_string()),
        Some(path) if !Path::new(path).is_file() => {
            missing.push(format!("{path} (the MCP server's binary)"))
        }
        Some(_) => {}
    }
    for (event, _) in EVENTS {
        let found = hooks["hooks"][event][0]["bash"]
            .as_str()
            .is_some_and(|c| hook_command::is_ours(c, AGENT));
        if !found {
            missing.push(format!("the {event} hook"));
        }
    }
    Ok(match missing.is_empty() {
        true => ("set up".to_string(), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent copilot`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(dir: &Path) -> Result<String, String> {
    let mcp_file = dir.join("mcp-config.json");
    let mut mcp = edit::read_json(&mcp_file)?;
    let mut changed = false;
    if mcp["mcpServers"]
        .as_object_mut()
        .is_some_and(|servers| servers.remove(SERVER).is_some())
    {
        changed |= edit::write_json(&mcp_file, &mcp)?;
    }
    let hooks_file = hooks_file(dir);
    if hooks_file.is_file() {
        fs::remove_file(&hooks_file).map_err(|e| format!("{}: {e}", hooks_file.display()))?;
        changed = true;
    }
    Ok(match changed {
        true => "removed (MCP server and hooks)".to_string(),
        false => "nothing to remove".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path, file: &str) -> Value {
        serde_json::from_str(&fs::read_to_string(dir.join(file)).unwrap()).unwrap()
    }

    #[test]
    fn setup_adds_its_entries_and_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(
            dir.join("mcp-config.json"),
            r#"{"mcpServers": {"other": {"type": "http", "url": "http://localhost:1"}}}"#,
        )
        .unwrap();
        let exe = Path::new("/opt/it's/petit-poucet");

        assert!(setup(dir, exe).unwrap().starts_with("set up"));
        let mcp = read(dir, "mcp-config.json");
        assert_eq!(mcp["mcpServers"]["other"]["url"], "http://localhost:1");
        assert_eq!(
            mcp["mcpServers"]["petit-poucet"]["command"],
            "/opt/it's/petit-poucet"
        );
        let hooks = read(dir, "hooks/petit-poucet.json");
        assert_eq!(hooks["version"], 1);
        assert_eq!(
            hooks["hooks"]["sessionStart"][0]["bash"],
            hook_command::posix(exe, "session-start", AGENT)
        );
        assert_eq!(
            hooks["hooks"]["agentStop"][0]["powershell"],
            hook_command::powershell(exe, "stop", AGENT)
        );
        assert_eq!(setup(dir, exe).unwrap(), "already set up");
    }

    #[test]
    fn check_names_what_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert_eq!(
            check(dir).unwrap(),
            (
                "missing the MCP server, the sessionStart hook, the agentStop hook: run `petit-poucet setup --agent copilot`".to_string(),
                false
            )
        );
        let exe = dir.join("petit-poucet");
        fs::write(&exe, "").unwrap();
        setup(dir, &exe).unwrap();
        assert_eq!(check(dir).unwrap(), ("set up".to_string(), true));
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mcp = r#"{"mcpServers": {"other": {"type": "http", "url": "http://localhost:1"}}}"#;
        fs::write(dir.join("mcp-config.json"), mcp).unwrap();
        fs::create_dir(dir.join("hooks")).unwrap();
        fs::write(dir.join("hooks/mine.json"), "{}").unwrap();
        setup(dir, Path::new("/bin/pp")).unwrap();

        assert_eq!(uninstall(dir).unwrap(), "removed (MCP server and hooks)");
        assert_eq!(
            read(dir, "mcp-config.json"),
            serde_json::from_str::<Value>(mcp).unwrap()
        );
        assert!(!dir.join("hooks/petit-poucet.json").exists());
        assert!(dir.join("hooks/mine.json").is_file());
        assert_eq!(uninstall(dir).unwrap(), "nothing to remove");
    }
}
