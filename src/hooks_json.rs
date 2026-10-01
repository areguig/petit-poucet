use std::path::Path;

use serde_json::{Value, json};

// The `petit-poucet hook` events an agent's hooks run.
const OURS: [&str; 2] = ["session-start", "stop"];

pub fn command(exe: &Path, event: &str, agent: &str) -> String {
    format!("\"{}\" hook {event} --agent {agent}", exe.display())
}

// Ours whatever the binary is called or where it lives: matched by the arguments `command` writes.
pub fn is_ours(command: &str, agent: &str) -> bool {
    OURS.iter()
        .any(|event| command.ends_with(&format!(" hook {event} --agent {agent}")))
}

// The layout Codex and Gemini CLI share with Claude Code:
// {"hooks": {Event: [{"hooks": [{"type": "command", "command": …}]}]}}.
fn group_is_ours(group: &Value, agent: &str) -> bool {
    group["hooks"].as_array().is_some_and(|hooks| {
        hooks
            .iter()
            .any(|h| h["command"].as_str().is_some_and(|c| is_ours(c, agent)))
    })
}

pub fn remove(settings: &mut Value, agent: &str) {
    if let Some(events) = settings["hooks"].as_object_mut() {
        for groups in events.values_mut().filter_map(Value::as_array_mut) {
            groups.retain(|g| !group_is_ours(g, agent));
        }
        events.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
    }
    drop_if_empty(settings, "hooks");
}

// After an uninstall, a section only petit-poucet used is left out rather than empty.
pub fn drop_if_empty(settings: &mut Value, key: &str) {
    if let Some(object) = settings.as_object_mut()
        && object
            .get(key)
            .and_then(Value::as_object)
            .is_some_and(|o| o.is_empty())
    {
        object.remove(key);
    }
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
    fn our_commands_are_recognised_by_their_arguments_only() {
        let ours = command(Path::new("/any/name"), "stop", "codex");
        assert_eq!(ours, "\"/any/name\" hook stop --agent codex");
        assert!(is_ours(&ours, "codex"));
        assert!(!is_ours(&ours, "gemini"), "another agent's entry");
        assert!(!is_ours("notify --agent codex", "codex"));
    }

    #[test]
    fn adds_and_removes_only_its_groups() {
        let mine =
            json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify"}]}]}});
        let mut settings = mine.clone();
        let hook = |event| json!({"type": "command", "command": command(Path::new("/pp"), event, "codex")});
        add(&mut settings, "SessionStart", hook("session-start"));
        add(&mut settings, "Stop", hook("stop"));
        assert!(has(&settings, "SessionStart", "codex") && has(&settings, "Stop", "codex"));
        assert!(!has(&settings, "Stop", "gemini"));
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
