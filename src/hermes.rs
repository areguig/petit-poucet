use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::config::{Config, Limits};
use crate::vault::write_atomic;
use crate::{edit, hook, hook_command, note};

const AGENT: &str = "hermes";
const SERVER_KEY: &str = "mcp_servers.petit-poucet";
const HOOKS_KEY: &str = "hooks.pre_llm_call";
// Run before each model call, it answers only the first; Hermes has no hook after an answer for `stop`.
const EVENT: &str = "session-start";
const SPILL_KEY: &str = "hooks.output_spill.max_chars";
// Hermes's own default when the config sets none.
const DEFAULT_SPILL: u64 = 10_000;
// A generous note path in an Index line, before its summary.
const PATH_CHARS: usize = 100;
const APPROVE: &str = "open `hermes` once and approve petit-poucet's hook when it asks";

pub fn dir(home: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("HERMES_HOME") {
        return PathBuf::from(dir);
    }
    match cfg!(windows) {
        true => std::env::var_os("LOCALAPPDATA")
            .map_or_else(|| home.join("AppData/Local"), PathBuf::from)
            .join("hermes"),
        false => home.join(".hermes"),
    }
}

#[derive(Debug, PartialEq)]
enum Change {
    Set(&'static str, Value),
    Unset(&'static str),
}

// Only read here: Hermes writes its own file, keeping the user's comments.
fn read_config(dir: &Path) -> Result<Value, String> {
    let path = dir.join("config.yaml");
    match fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => {
            note::parse_yaml::<Value>(&text).map_err(|e| format!("{}: {e}", path.display()))
        }
        _ => Ok(json!({})),
    }
}

fn our_hook(exe: &Path) -> Value {
    json!({"command": hook_command::argv_line(exe, EVENT, AGENT), "timeout": 30})
}

fn hooks(config: &Value) -> &[Value] {
    config["hooks"]["pre_llm_call"]
        .as_array()
        .map_or(&[], Vec::as_slice)
}

fn is_ours(hook: &Value) -> bool {
    hook["command"]
        .as_str()
        .is_some_and(|command| hook_command::is_ours(command, AGENT))
}

fn others_hooks(config: &Value) -> Vec<Value> {
    hooks(config)
        .iter()
        .filter(|h| !is_ours(h))
        .cloned()
        .collect()
}

// Room for the rules and twice an Index at its limit: an Index passes its limit until the user tidies it,
// and Hermes cuts a longer hook output down to a preview.
fn context_max_chars(limits: &Limits) -> u64 {
    let line = PATH_CHARS + limits.summary_max_chars;
    (hook::RULES.len() + 2 * limits.index_max_notes * line) as u64
}

fn spill(config: &Value) -> u64 {
    config["hooks"]["output_spill"]["max_chars"]
        .as_u64()
        .unwrap_or(DEFAULT_SPILL)
}

fn setup_changes(config: &Value, exe: &Path, limits: &Limits) -> Vec<Change> {
    let mut changes = Vec::new();
    let server = json!({"command": exe.display().to_string(), "args": ["serve"]});
    if config["mcp_servers"]["petit-poucet"] != server {
        changes.push(Change::Set(SERVER_KEY, server));
    }
    let mut wanted = others_hooks(config);
    wanted.push(our_hook(exe));
    if hooks(config) != wanted {
        changes.push(Change::Set(HOOKS_KEY, json!(wanted)));
    }
    let needed = context_max_chars(limits);
    if spill(config) < needed {
        changes.push(Change::Set(SPILL_KEY, json!(needed)));
    }
    changes
}

// The raised output limit stays: it changes nothing without a hook that needs it.
fn uninstall_changes(config: &Value) -> Vec<Change> {
    let mut changes = Vec::new();
    if !config["mcp_servers"]["petit-poucet"].is_null() {
        changes.push(Change::Unset(SERVER_KEY));
    }
    let others = others_hooks(config);
    match (hooks(config).iter().any(is_ours), others.is_empty()) {
        (false, _) => {}
        (true, true) => changes.push(Change::Unset(HOOKS_KEY)),
        (true, false) => changes.push(Change::Set(HOOKS_KEY, json!(others))),
    }
    changes
}

// `hermes config set` takes a JSON value as it is.
fn run(change: &Change) -> Result<(), String> {
    let mut command = Command::new("hermes");
    let key = match change {
        Change::Set(key, value) => {
            command.args(["config", "set", key, &value.to_string()]);
            key
        }
        Change::Unset(key) => {
            command.args(["config", "unset", key]);
            key
        }
    };
    let out = command
        .output()
        .map_err(|e| format!("can't run `hermes`: {e}"))?;
    match out.status.success() {
        true => Ok(()),
        false => Err(format!(
            "`hermes config` failed on {key}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

fn apply(dir: &Path, changes: &[Change]) -> Result<(), String> {
    let config = dir.join("config.yaml");
    if let (false, Ok(old)) = (changes.is_empty(), fs::read_to_string(&config)) {
        write_atomic(&edit::backup_path(&config), &old)?;
    }
    changes.iter().try_for_each(run)
}

pub fn setup(dir: &Path, exe: &Path) -> Result<String, String> {
    let limits = Config::load()?.limits;
    let changes = setup_changes(&read_config(dir)?, exe, &limits);
    apply(dir, &changes)?;
    Ok(match changes.is_empty() {
        true => "already set up".to_string(),
        false => format!("set up (MCP server and hook in config.yaml): {APPROVE}"),
    })
}

pub fn check(dir: &Path) -> Result<(String, bool), String> {
    let config = read_config(dir)?;
    let limits = Config::load()?.limits;
    let mut missing = Vec::new();
    match config["mcp_servers"]["petit-poucet"]["command"].as_str() {
        None => missing.push("the MCP server".to_string()),
        Some(path) if !Path::new(path).is_file() => {
            missing.push(format!("{path} (the MCP server's binary)"))
        }
        Some(_) => {}
    }
    if !hooks(&config).iter().any(is_ours) {
        missing.push("the pre_llm_call hook".to_string());
    }
    if spill(&config) < context_max_chars(&limits) {
        missing.push(format!("room for the memory ({SPILL_KEY})"));
    }
    Ok(match missing.is_empty() {
        true => ("set up".to_string(), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent {AGENT}`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(dir: &Path) -> Result<String, String> {
    let changes = uninstall_changes(&read_config(dir)?);
    apply(dir, &changes)?;
    Ok(match changes.is_empty() {
        true => "nothing to remove".to_string(),
        false => "removed (MCP server and hook)".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Config::new(PathBuf::from("/v")).limits
    }

    #[test]
    fn setup_adds_its_server_and_hook_and_keeps_the_user_s_hooks() {
        let exe = Path::new("/opt/pp/petit-poucet");
        let theirs = json!({"command": "/usr/bin/theirs"});
        let ours = our_hook(exe);
        let server = json!({"command": "/opt/pp/petit-poucet", "args": ["serve"]});
        let needed = json!(context_max_chars(&limits()));
        assert_eq!(
            setup_changes(
                &json!({"hooks": {"pre_llm_call": [theirs]}}),
                exe,
                &limits()
            ),
            [
                Change::Set(SERVER_KEY, server.clone()),
                Change::Set(HOOKS_KEY, json!([theirs, ours])),
                Change::Set(SPILL_KEY, needed.clone()),
            ]
        );

        let set_up = json!({
            "mcp_servers": {"petit-poucet": server},
            "hooks": {"pre_llm_call": [theirs, ours], "output_spill": {"max_chars": needed}}
        });
        assert_eq!(setup_changes(&set_up, exe, &limits()), []);
        let moved = Path::new("/new/petit-poucet");
        assert!(
            setup_changes(&set_up, moved, &limits())
                .contains(&Change::Set(HOOKS_KEY, json!([theirs, our_hook(moved)])))
        );
    }

    #[test]
    fn setup_keeps_a_larger_output_limit() {
        let config = json!({"hooks": {"output_spill": {"max_chars": 1_000_000}}});
        let changes = setup_changes(&config, Path::new("/pp"), &limits());
        assert!(
            !changes
                .iter()
                .any(|c| matches!(c, Change::Set(SPILL_KEY, _)))
        );
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let exe = Path::new("/pp");
        let theirs = json!({"command": "/usr/bin/theirs"});
        let config = json!({
            "mcp_servers": {"petit-poucet": {"command": "/pp", "args": ["serve"]}},
            "hooks": {"pre_llm_call": [theirs, our_hook(exe)]}
        });
        assert_eq!(
            uninstall_changes(&config),
            [
                Change::Unset(SERVER_KEY),
                Change::Set(HOOKS_KEY, json!([theirs]))
            ]
        );
        let alone = json!({"hooks": {"pre_llm_call": [our_hook(exe)]}});
        assert_eq!(uninstall_changes(&alone), [Change::Unset(HOOKS_KEY)]);
        assert_eq!(uninstall_changes(&json!({})), []);
    }

    #[test]
    fn the_output_limit_holds_an_index_at_its_limit() {
        let limits = limits();
        let index = limits.index_max_notes * (PATH_CHARS + limits.summary_max_chars);
        assert!(context_max_chars(&limits) as usize > hook::RULES.len() + index);
        assert!(context_max_chars(&limits) > DEFAULT_SPILL);
    }
}
