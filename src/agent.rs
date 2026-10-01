use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde_json::{Value, json};

use crate::{antigravity, codex, copilot};

// Every agent petit-poucet knows: how its hooks reply, how to find it, how it gets set up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Agent {
    Claude,
    Copilot,
    Codex,
    Cursor,
    Antigravity,
}

impl Agent {
    pub const ALL: [Agent; 5] = [
        Agent::Claude,
        Agent::Copilot,
        Agent::Codex,
        Agent::Cursor,
        Agent::Antigravity,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Copilot => "GitHub Copilot",
            Agent::Codex => "Codex",
            Agent::Cursor => "Cursor",
            Agent::Antigravity => "Antigravity CLI",
        }
    }

    // The value of `--agent`.
    pub fn flag(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Copilot => "copilot",
            Agent::Codex => "codex",
            Agent::Cursor => "cursor",
            Agent::Antigravity => "antigravity",
        }
    }

    // Names the agent in commit messages.
    pub fn hook_label(self) -> &'static str {
        match self {
            Agent::Claude => "claude-code hook",
            Agent::Copilot => "copilot hook",
            Agent::Codex => "codex hook",
            Agent::Cursor => "cursor hook",
            Agent::Antigravity => "antigravity hook",
        }
    }

    // What each agent's hooks receive: Cursor and Antigravity name things their own way, Copilot CLI in camelCase.
    pub fn working_dir(self, event: &Value) -> Option<PathBuf> {
        let dir = match self {
            Agent::Cursor => &event["workspace_roots"][0],
            Agent::Antigravity => &event["workspacePaths"][0],
            _ => &event["cwd"],
        };
        dir.as_str().map(PathBuf::from)
    }

    pub fn session(self, event: &Value) -> Option<&str> {
        let keys: &[&str] = match self {
            Agent::Cursor => &["conversation_id", "session_id"],
            Agent::Antigravity => &["conversationId"],
            _ => &["session_id", "sessionId"],
        };
        keys.iter().find_map(|key| event[*key].as_str())
    }

    // True when this stop follows our own reminder: reminding again would loop.
    pub fn after_reminder(self, event: &Value) -> bool {
        match self {
            Agent::Cursor => event["loop_count"].as_u64().is_some_and(|n| n > 0),
            Agent::Antigravity => event["executionNum"].as_u64().is_some_and(|n| n > 0),
            _ => ["stop_hook_active", "stopHookActive"]
                .iter()
                .any(|key| event[*key] == json!(true)),
        }
    }

    // Codex keeps a resumed session's memory in its history but runs the hook again: answering would repeat it.
    pub fn holds_memory_already(self, event: &Value) -> bool {
        self == Agent::Codex && event["source"] == "resume"
    }

    // Copilot CLI, Cursor and Antigravity hooks have no line for the user, so `message` goes to Claude Code and Codex only.
    pub fn session_start_reply(self, context: &str, message: &str) -> Value {
        match self {
            Agent::Claude | Agent::Codex => json!({
                "systemMessage": message,
                "hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": context},
            }),
            Agent::Copilot => json!({"additionalContext": context}),
            Agent::Cursor => json!({"additional_context": context}),
            // Sent before every model call; an ephemeral message is never kept in the history.
            Agent::Antigravity => json!({"injectSteps": [{"ephemeralMessage": context}]}),
        }
    }

    pub fn stop_reply(self, reason: &str, message: &str) -> Value {
        match self {
            Agent::Claude | Agent::Codex => {
                json!({"decision": "block", "reason": reason, "systemMessage": message})
            }
            Agent::Copilot => json!({"decision": "block", "reason": reason}),
            // Cursor sends it as the user's next message.
            Agent::Cursor => json!({"followup_message": reason}),
            Agent::Antigravity => json!({"decision": "continue", "reason": reason}),
        }
    }

    pub fn installed(self, home: &Path) -> bool {
        match self {
            Agent::Claude => home.join(".claude").is_dir(),
            Agent::Copilot => copilot::dir(home).is_dir(),
            Agent::Codex => codex::dir(home).is_dir(),
            Agent::Cursor => home.join(".cursor").is_dir(),
            Agent::Antigravity => antigravity::dir(home).is_dir(),
        }
    }

    // petit-poucet 0.2 came as a plugin for these two: setup removes it with the agent's own command.
    pub fn old_plugin(self) -> Option<&'static [&'static str]> {
        match self {
            Agent::Claude => Some(&["claude", "plugin", "uninstall", "petit-poucet@petit-poucet"]),
            Agent::Copilot => Some(&["copilot", "plugin", "uninstall", "petit-poucet"]),
            Agent::Codex | Agent::Cursor | Agent::Antigravity => None,
        }
    }

    pub fn plugin_installed(self, home: &Path) -> bool {
        match self {
            Agent::Claude => enabled_in(&home.join(".claude/settings.json")),
            // A local install is only listed in the settings; a marketplace install is also copied.
            Agent::Copilot => {
                enabled_in(&copilot::dir(home).join("settings.json"))
                    || fs::read_dir(copilot::dir(home).join("installed-plugins"))
                        .into_iter()
                        .flatten()
                        .flatten()
                        .any(|marketplace| marketplace.path().join("petit-poucet").is_dir())
            }
            Agent::Codex | Agent::Cursor | Agent::Antigravity => false,
        }
    }
}

// Claude Code and Copilot list enabled plugins in their settings as `name@marketplace: true`.
fn enabled_in(settings: &Path) -> bool {
    let Ok(text) = fs::read_to_string(settings) else {
        return false;
    };
    let settings: Value = serde_json::from_str(&text).unwrap_or_default();
    settings["enabledPlugins"]
        .as_object()
        .is_some_and(|plugins| {
            plugins
                .iter()
                .any(|(name, on)| name.starts_with("petit-poucet@") && on == &json!(true))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(home: &Path, path: &str, text: &str) {
        let path = home.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn finds_agents_by_their_home_folder() {
        let home = tempfile::tempdir().unwrap();
        assert!(!Agent::Claude.installed(home.path()));
        fs::create_dir(home.path().join(".claude")).unwrap();
        assert!(Agent::Claude.installed(home.path()));
        assert!(!Agent::Copilot.installed(home.path()));
    }

    #[test]
    fn the_claude_plugin_counts_only_when_enabled() {
        let home = tempfile::tempdir().unwrap();
        assert!(!Agent::Claude.plugin_installed(home.path()), "no settings");
        for (settings, enabled) in [
            ("not json", false),
            (r#"{"enabledPlugins": {"other@x": true}}"#, false),
            (
                r#"{"enabledPlugins": {"petit-poucet@petit-poucet": false}}"#,
                false,
            ),
            (
                r#"{"enabledPlugins": {"petit-poucet@petit-poucet": true}}"#,
                true,
            ),
        ] {
            write(home.path(), ".claude/settings.json", settings);
            assert_eq!(
                Agent::Claude.plugin_installed(home.path()),
                enabled,
                "{settings}"
            );
        }
    }

    #[test]
    fn the_copilot_plugin_counts_when_enabled_in_its_settings() {
        let home = tempfile::tempdir().unwrap();
        write(
            home.path(),
            ".copilot/settings.json",
            r#"{"enabledPlugins": {"petit-poucet@petit-poucet": true}}"#,
        );
        assert!(Agent::Copilot.plugin_installed(home.path()));
        assert!(
            !Agent::Claude.plugin_installed(home.path()),
            "each agent has its own settings"
        );
    }

    #[test]
    fn the_copilot_plugin_is_found_under_any_marketplace() {
        let home = tempfile::tempdir().unwrap();
        assert!(!Agent::Copilot.plugin_installed(home.path()));
        write(
            home.path(),
            ".copilot/installed-plugins/other/tool/plugin.json",
            "{}",
        );
        assert!(!Agent::Copilot.plugin_installed(home.path()));
        write(
            home.path(),
            ".copilot/installed-plugins/some-marketplace/petit-poucet/plugin.json",
            "{}",
        );
        assert!(Agent::Copilot.plugin_installed(home.path()));
    }
}
