use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::edit;

const SERVER: &str = "petit-poucet";
// Codex event names, with the `petit-poucet hook` event each one runs.
const EVENTS: [(&str, &str); 2] = [("SessionStart", "session-start"), ("Stop", "stop")];
const TRUST: &str = "open Codex and trust its hooks once with /hooks";

pub fn dir(home: &Path) -> PathBuf {
    std::env::var_os("CODEX_HOME").map_or_else(|| home.join(".codex"), PathBuf::from)
}

fn hook_command(exe: &Path, event: &str) -> String {
    format!("\"{}\" hook {event} --agent codex", exe.display())
}

// Ours whatever the binary is called or where it lives: matched by the arguments `hook_command` writes.
fn is_ours(group: &Value) -> bool {
    group["hooks"].as_array().is_some_and(|hooks| {
        hooks.iter().any(|h| {
            h["command"].as_str().is_some_and(|c| {
                EVENTS
                    .iter()
                    .any(|(_, ours)| c.ends_with(&format!(" hook {ours} --agent codex")))
            })
        })
    })
}

fn read_config(dir: &Path) -> Result<DocumentMut, String> {
    let path = dir.join("config.toml");
    let text = fs::read_to_string(&path).unwrap_or_default();
    text.parse().map_err(|e| format!("{}: {e}", path.display()))
}

// Never overwrites a hooks.json it can't read: the user's hooks would be lost.
fn read_hooks(dir: &Path) -> Result<Value, String> {
    let path = dir.join("hooks.json");
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(_) => Ok(json!({})),
    }
}

fn write_hooks(dir: &Path, hooks: &Value) -> Result<bool, String> {
    let text = serde_json::to_string_pretty(hooks).map_err(|e| e.to_string())? + "\n";
    edit::replace(&dir.join("hooks.json"), &text)
}

fn without_ours(hooks: &mut Value) {
    if let Some(events) = hooks["hooks"].as_object_mut() {
        for groups in events.values_mut().filter_map(Value::as_array_mut) {
            groups.retain(|g| !is_ours(g));
        }
        events.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
    }
}

pub fn setup(dir: &Path, exe: &Path) -> Result<String, String> {
    let mut config = read_config(dir)?;
    let mut server = Table::new();
    server["command"] = value(exe.display().to_string());
    server["args"] = value(toml_edit::Array::from_iter(["serve"]));
    if !config.contains_table("mcp_servers") {
        let mut servers = Table::new();
        servers.set_implicit(true);
        config["mcp_servers"] = Item::Table(servers);
    }
    config["mcp_servers"][SERVER] = Item::Table(server);

    let mut hooks = read_hooks(dir)?;
    without_ours(&mut hooks);
    if !hooks["hooks"].is_object() {
        hooks["hooks"] = json!({});
    }
    for (event, ours) in EVENTS {
        let group = json!({"hooks": [{"type": "command", "command": hook_command(exe, ours), "timeout": 30}]});
        match hooks["hooks"][event].as_array_mut() {
            Some(groups) => groups.push(group),
            None => hooks["hooks"][event] = json!([group]),
        }
    }

    let changed =
        edit::replace(&dir.join("config.toml"), &config.to_string())? | write_hooks(dir, &hooks)?;
    Ok(match changed {
        true => format!("set up (MCP server in config.toml, hooks in hooks.json): {TRUST}"),
        false => "already set up".to_string(),
    })
}

pub fn check(dir: &Path) -> Result<(String, bool), String> {
    let config = read_config(dir)?;
    let hooks = read_hooks(dir)?;
    let mut missing = Vec::new();
    let command = config
        .get("mcp_servers")
        .and_then(|s| s.get(SERVER))
        .and_then(|s| s.get("command"))
        .and_then(Item::as_str);
    match command {
        None => missing.push("the MCP server".to_string()),
        Some(path) if !Path::new(path).is_file() => {
            missing.push(format!("{path} (the MCP server's binary)"))
        }
        Some(_) => {}
    }
    for (event, _) in EVENTS {
        if !hooks["hooks"][event]
            .as_array()
            .is_some_and(|g| g.iter().any(is_ours))
        {
            missing.push(format!("the {event} hook"));
        }
    }
    for flag in ["hooks", "codex_hooks"] {
        let off = config
            .get("features")
            .and_then(|f| f.get(flag))
            .and_then(Item::as_bool)
            == Some(false);
        if off {
            missing.push(format!("hooks are turned off (features.{flag} = false)"));
        }
    }
    Ok(match missing.is_empty() {
        true => (format!("set up (if memory doesn't load, {TRUST})"), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent codex`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(dir: &Path) -> Result<String, String> {
    let mut config = read_config(dir)?;
    let had_server = config
        .get_mut("mcp_servers")
        .and_then(Item::as_table_like_mut)
        .is_some_and(|servers| servers.remove(SERVER).is_some());
    let mut hooks = read_hooks(dir)?;
    without_ours(&mut hooks);
    let mut changed = write_hooks_if_present(dir, &hooks)?;
    if had_server {
        changed |= edit::replace(&dir.join("config.toml"), &config.to_string())?;
    }
    Ok(match changed {
        true => "removed (MCP server and hooks)".to_string(),
        false => "nothing to remove".to_string(),
    })
}

fn write_hooks_if_present(dir: &Path, hooks: &Value) -> Result<bool, String> {
    match dir.join("hooks.json").exists() {
        true => write_hooks(dir, hooks),
        false => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path, file: &str) -> String {
        fs::read_to_string(dir.join(file)).unwrap()
    }

    #[test]
    fn setup_adds_its_entries_and_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(
            dir.join("config.toml"),
            "# my settings\nmodel = \"gpt-6\"\n\n[mcp_servers.other]\ncommand = \"other\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("hooks.json"),
            r#"{"description": "mine", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify"}]}]}}"#,
        )
        .unwrap();
        let exe = Path::new("/opt/pp/petit-poucet");

        assert!(setup(dir, exe).unwrap().starts_with("set up"));
        let config = read(dir, "config.toml");
        assert!(
            config.starts_with("# my settings\nmodel = \"gpt-6\"\n"),
            "{config}"
        );
        assert!(
            config.contains("[mcp_servers.other]\ncommand = \"other\"\n"),
            "{config}"
        );
        assert!(
            config.contains("[mcp_servers.petit-poucet]\ncommand = \"/opt/pp/petit-poucet\"\nargs = [\"serve\"]\n"),
            "{config}"
        );
        let hooks: Value = serde_json::from_str(&read(dir, "hooks.json")).unwrap();
        assert_eq!(hooks["description"], "mine");
        assert_eq!(hooks["hooks"]["Stop"][0]["hooks"][0]["command"], "notify");
        assert_eq!(
            hooks["hooks"]["Stop"][1]["hooks"][0]["command"],
            "\"/opt/pp/petit-poucet\" hook stop --agent codex"
        );
        assert_eq!(
            hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "\"/opt/pp/petit-poucet\" hook session-start --agent codex"
        );
        assert!(edit::backup_path(&dir.join("config.toml")).is_file());
        assert!(edit::backup_path(&dir.join("hooks.json")).is_file());

        assert_eq!(setup(dir, exe).unwrap(), "already set up");
        let moved = Path::new("/usr/local/bin/petit-poucet");
        setup(dir, moved).unwrap();
        let hooks: Value = serde_json::from_str(&read(dir, "hooks.json")).unwrap();
        assert_eq!(
            hooks["hooks"]["Stop"].as_array().unwrap().len(),
            2,
            "replaced, not added"
        );
        assert!(read(dir, "config.toml").contains("command = \"/usr/local/bin/petit-poucet\""));
    }

    #[test]
    fn setup_on_a_fresh_codex_writes_both_files() {
        let tmp = tempfile::tempdir().unwrap();
        setup(tmp.path(), Path::new("/bin/pp")).unwrap();
        assert_eq!(
            read(tmp.path(), "config.toml"),
            "[mcp_servers.petit-poucet]\ncommand = \"/bin/pp\"\nargs = [\"serve\"]\n"
        );
        let hooks: Value = serde_json::from_str(&read(tmp.path(), "hooks.json")).unwrap();
        assert_eq!(hooks["hooks"].as_object().unwrap().len(), 2);
    }

    #[test]
    fn a_hooks_file_it_cannot_read_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("hooks.json"), "{ not json").unwrap();
        assert!(setup(tmp.path(), Path::new("/bin/pp")).is_err());
        assert_eq!(read(tmp.path(), "hooks.json"), "{ not json");
        assert!(!tmp.path().join("config.toml").exists(), "nothing written");
    }

    #[test]
    fn check_names_what_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let (line, ok) = check(dir).unwrap();
        assert!(!ok);
        assert_eq!(
            line,
            "missing the MCP server, the SessionStart hook, the Stop hook: run `petit-poucet setup --agent codex`"
        );

        let exe = tmp.path().join("petit-poucet");
        fs::write(&exe, "").unwrap();
        setup(dir, &exe).unwrap();
        assert!(check(dir).unwrap().1);

        let config = read(dir, "config.toml") + "\n[features]\nhooks = false\n";
        fs::write(dir.join("config.toml"), config).unwrap();
        fs::remove_file(&exe).unwrap();
        let (line, ok) = check(dir).unwrap();
        assert!(!ok);
        assert!(
            line.contains("(the MCP server's binary)") && line.contains("features.hooks = false"),
            "{line}"
        );
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mine = "model = \"gpt-6\"\n";
        fs::write(dir.join("config.toml"), mine).unwrap();
        let hooks =
            r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify"}]}]}}"#;
        fs::write(dir.join("hooks.json"), hooks).unwrap();
        setup(dir, Path::new("/bin/pp")).unwrap();

        assert_eq!(uninstall(dir).unwrap(), "removed (MCP server and hooks)");
        assert_eq!(read(dir, "config.toml"), mine);
        let after: Value = serde_json::from_str(&read(dir, "hooks.json")).unwrap();
        assert_eq!(after, serde_json::from_str::<Value>(hooks).unwrap());
        assert_eq!(uninstall(dir).unwrap(), "nothing to remove");
    }
}
