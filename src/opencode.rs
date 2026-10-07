use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::edit;

const SERVER: &str = "petit-poucet";
const PLUGIN: &str = include_str!("../plugins/opencode.js");
const PLUGIN_EXE: &str = "__PETIT_POUCET__";

// The same folder on every OS, Windows included.
pub fn dir(home: &Path) -> PathBuf {
    home.join(".config/opencode")
}

fn plugin_file(dir: &Path) -> PathBuf {
    dir.join(format!("plugins/{SERVER}.js"))
}

// A JSON string is a valid JavaScript string, quotes and backslashes of Windows paths included.
fn plugin(exe: &Path) -> String {
    let quoted = serde_json::to_string(&exe.display().to_string()).unwrap();
    PLUGIN.replace(PLUGIN_EXE, &quoted)
}

pub fn setup(dir: &Path, exe: &Path) -> Result<String, String> {
    let config_file = dir.join("opencode.json");
    let mut config = edit::read_json(&config_file)?;
    if !config["mcp"]["servers"].is_object() {
        config["mcp"]["servers"] = json!({});
    }
    config["mcp"]["servers"][SERVER] =
        json!({"type": "local", "command": [exe.display().to_string(), "serve"]});
    let changed =
        edit::write_json(&config_file, &config)? | edit::replace(&plugin_file(dir), &plugin(exe))?;
    Ok(match changed {
        true => "set up (MCP server in opencode.json, plugin in plugins/petit-poucet.js): restart OpenCode".to_string(),
        false => "already set up".to_string(),
    })
}

pub fn check(dir: &Path) -> Result<(String, bool), String> {
    let config = edit::read_json(&dir.join("opencode.json"))?;
    let mut missing = Vec::new();
    match config["mcp"]["servers"][SERVER]["command"][0].as_str() {
        None => missing.push("the MCP server".to_string()),
        Some(path) if !Path::new(path).is_file() => {
            missing.push(format!("{path} (the MCP server's binary)"))
        }
        Some(_) => {}
    }
    if !plugin_file(dir).is_file() {
        missing.push("the plugin".to_string());
    }
    Ok(match missing.is_empty() {
        true => ("set up".to_string(), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent opencode`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(dir: &Path) -> Result<String, String> {
    let config_file = dir.join("opencode.json");
    let mut config = edit::read_json(&config_file)?;
    let had_server = config
        .pointer_mut("/mcp/servers")
        .and_then(Value::as_object_mut)
        .is_some_and(|servers| servers.remove(SERVER).is_some());
    let mut changed = false;
    if had_server {
        edit::drop_if_empty(&mut config["mcp"], "servers");
        edit::drop_if_empty(&mut config, "mcp");
        changed |= edit::write_json(&config_file, &config)?;
    }
    let plugin = plugin_file(dir);
    if plugin.is_file() {
        fs::remove_file(&plugin).map_err(|e| format!("{}: {e}", plugin.display()))?;
        changed = true;
    }
    Ok(match changed {
        true => "removed (MCP server and plugin)".to_string(),
        false => "nothing to remove".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(dir.join("opencode.json")).unwrap()).unwrap()
    }

    #[test]
    fn setup_adds_its_entries_and_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(
            dir.join("opencode.json"),
            r#"{"model": "anthropic/x", "mcp": {"servers": {"other": {"type": "remote", "url": "http://localhost:1"}}}}"#,
        )
        .unwrap();
        let exe = Path::new(r"C:\Users\a b\petit-poucet.exe");

        assert!(setup(dir, exe).unwrap().starts_with("set up"));
        let config = read(dir);
        assert_eq!(config["model"], "anthropic/x");
        assert_eq!(
            config["mcp"]["servers"]["other"]["url"],
            "http://localhost:1"
        );
        assert_eq!(
            config["mcp"]["servers"]["petit-poucet"],
            json!({"type": "local", "command": [r"C:\Users\a b\petit-poucet.exe", "serve"]})
        );
        assert!(edit::backup_path(&dir.join("opencode.json")).is_file());
        let plugin = fs::read_to_string(plugin_file(dir)).unwrap();
        assert!(plugin.contains(r#"const EXE = "C:\\Users\\a b\\petit-poucet.exe";"#));
        assert!(!plugin.contains(PLUGIN_EXE));

        assert_eq!(setup(dir, exe).unwrap(), "already set up");
    }

    #[test]
    fn a_file_it_cannot_read_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("opencode.json"), "{ // a comment\n}").unwrap();
        assert!(setup(tmp.path(), Path::new("/bin/pp")).is_err());
        assert_eq!(
            fs::read_to_string(tmp.path().join("opencode.json")).unwrap(),
            "{ // a comment\n}"
        );
        assert!(!plugin_file(tmp.path()).exists(), "nothing written");
    }

    #[test]
    fn check_names_what_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert_eq!(
            check(dir).unwrap(),
            (
                "missing the MCP server, the plugin: run `petit-poucet setup --agent opencode`"
                    .to_string(),
                false
            )
        );
        let exe = dir.join("petit-poucet");
        fs::write(&exe, "").unwrap();
        setup(dir, &exe).unwrap();
        assert_eq!(check(dir).unwrap(), ("set up".to_string(), true));
        fs::remove_file(plugin_file(dir)).unwrap();
        assert!(!check(dir).unwrap().1);
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("opencode.json"), r#"{"model": "anthropic/x"}"#).unwrap();
        setup(dir, Path::new("/opt/pp/petit-poucet")).unwrap();

        assert_eq!(uninstall(dir).unwrap(), "removed (MCP server and plugin)");
        assert_eq!(read(dir), json!({"model": "anthropic/x"}));
        assert!(!plugin_file(dir).exists());
        assert_eq!(uninstall(dir).unwrap(), "nothing to remove");
    }
}
