use std::fs;
use std::path::Path;

use clap::ValueEnum;
use serde_json::{Value, json};

use crate::codex;

// Every agent petit-poucet knows: how its hooks reply, how to find it, how it gets set up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Agent {
    Claude,
    Copilot,
    Codex,
}

// Agents with a petit-poucet plugin are set up by it; the others by `petit-poucet setup`.
pub struct Plugin {
    pub install: &'static str,
    pub uninstall: &'static str,
}

impl Agent {
    pub const ALL: [Agent; 3] = [Agent::Claude, Agent::Copilot, Agent::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Copilot => "GitHub Copilot",
            Agent::Codex => "Codex",
        }
    }

    // Names the agent in commit messages.
    pub fn hook_label(self) -> &'static str {
        match self {
            Agent::Claude => "claude-code hook",
            Agent::Copilot => "copilot hook",
            Agent::Codex => "codex hook",
        }
    }

    // Copilot CLI hooks have no line for the user, so `message` goes to Claude Code and Codex only.
    pub fn session_start_reply(self, context: &str, message: &str) -> Value {
        match self {
            Agent::Claude | Agent::Codex => json!({
                "systemMessage": message,
                "hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": context},
            }),
            Agent::Copilot => json!({"additionalContext": context}),
        }
    }

    pub fn stop_reply(self, reason: &str, message: &str) -> Value {
        match self {
            Agent::Claude | Agent::Codex => {
                json!({"decision": "block", "reason": reason, "systemMessage": message})
            }
            Agent::Copilot => json!({"decision": "block", "reason": reason}),
        }
    }

    pub fn installed(self, home: &Path) -> bool {
        match self {
            Agent::Claude => home.join(".claude").is_dir(),
            Agent::Copilot => home.join(".copilot").is_dir(),
            Agent::Codex => codex::dir(home).is_dir(),
        }
    }

    pub fn plugin(self) -> Option<Plugin> {
        match self {
            Agent::Claude => Some(Plugin {
                install: "claude plugin marketplace add https://github.com/areguig/petit-poucet && claude plugin install petit-poucet@petit-poucet",
                uninstall: "claude plugin uninstall petit-poucet@petit-poucet",
            }),
            Agent::Copilot => Some(Plugin {
                install: "copilot plugin marketplace add areguig/petit-poucet && copilot plugin install petit-poucet@petit-poucet",
                uninstall: "copilot plugin uninstall petit-poucet",
            }),
            Agent::Codex => None,
        }
    }

    pub fn plugin_installed(self, home: &Path) -> bool {
        match self {
            Agent::Claude => claude_plugin_enabled(home),
            Agent::Copilot => fs::read_dir(home.join(".copilot/installed-plugins"))
                .into_iter()
                .flatten()
                .flatten()
                .any(|marketplace| marketplace.path().join("petit-poucet").is_dir()),
            Agent::Codex => false,
        }
    }
}

// Claude Code lists enabled plugins in its settings as `name@marketplace: true`.
fn claude_plugin_enabled(home: &Path) -> bool {
    let Ok(text) = fs::read_to_string(home.join(".claude/settings.json")) else {
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
