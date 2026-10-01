use std::path::Path;

use serde_json::{Value, json};

use crate::{edit, hook_command};

// The hooks layout Claude Code and Codex share:
// {"hooks": {Event: [{"hooks": [{"type": "command", "command": …}]}]}}.

// Run without a shell, so the same entry works on every OS.
pub fn exec(exe: &Path, event: &str, agent: &str) -> Value {
    json!({"type": "command", "command": exe.display().to_string(), "args": hook_command::args(event, agent)})
}

fn entry_is_ours(entry: &Value, agent: &str) -> bool {
    let shell = entry["command"]
        .as_str()
        .is_some_and(|c| hook_command::is_ours(c, agent));
    let exec = hook_command::EVENTS
        .iter()
        .any(|event| entry["args"] == json!(hook_command::args(event, agent)));
    shell || exec
}

fn group_is_ours(group: &Value, agent: &str) -> bool {
    group["hooks"]
        .as_array()
        .is_some_and(|hooks| hooks.iter().any(|h| entry_is_ours(h, agent)))
}

pub fn remove(settings: &mut Value, agent: &str) {
    // `settings["hooks"]` would add a null `hooks` to settings without one.
    if let Some(events) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        for groups in events.values_mut().filter_map(Value::as_array_mut) {
            groups.retain(|g| !group_is_ours(g, agent));
        }
        events.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
    }
    edit::drop_if_empty(settings, "hooks");
}

pub fn add(settings: &mut Value, event: &str, hook: Value) {
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }
    let group = json!({"hooks": [hook]});
    match settings["hooks"][event].as_array_mut() {
        Some(groups) => groups.push(group),
        None => settings["hooks"][event] = json!([group]),
    }
}

pub fn has(settings: &Value, event: &str, agent: &str) -> bool {
    settings["hooks"][event]
        .as_array()
        .is_some_and(|groups| groups.iter().any(|g| group_is_ours(g, agent)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_from_settings_without_hooks_changes_nothing() {
        let mut settings = json!({"model": "opus"});
        remove(&mut settings, "claude");
        assert_eq!(settings, json!({"model": "opus"}));
    }

    #[test]
    fn exec_entries_are_recognised_by_their_arguments() {
        let mut settings = json!({});
        add(
            &mut settings,
            "Stop",
            exec(Path::new("/any/pp"), "stop", "claude"),
        );
        assert!(has(&settings, "Stop", "claude") && !has(&settings, "Stop", "codex"));
        remove(&mut settings, "claude");
        assert_eq!(settings, json!({}));
    }

    #[test]
    fn adds_and_removes_only_its_groups() {
        let mine =
            json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify"}]}]}});
        let mut settings = mine.clone();
        let hook = |event| json!({"type": "command", "command": hook_command::shell(Path::new("/pp"), event, "codex")});
        add(&mut settings, "SessionStart", hook("session-start"));
        add(&mut settings, "Stop", hook("stop"));
        assert!(has(&settings, "SessionStart", "codex") && has(&settings, "Stop", "codex"));
        assert!(!has(&settings, "Stop", "cursor"));
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 2);

        remove(&mut settings, "codex");
        assert_eq!(settings, mine);

        let mut only_ours = json!({"theme": "x"});
        add(&mut only_ours, "Stop", hook("stop"));
        remove(&mut only_ours, "codex");
        assert_eq!(
            only_ours,
            json!({"theme": "x"}),
            "no empty `hooks` left behind"
        );
    }
}
