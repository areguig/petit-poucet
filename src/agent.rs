use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde_json::{Value, json};

use crate::{antigravity, omp, opencode};

// Where the agents keep their files. The environment is read here only, so a test names every folder itself.
pub struct Dirs {
    pub home: PathBuf,
    // Copilot CLI and its IDE hosts (VS Code, IntelliJ, the desktop app) all read this folder.
    pub copilot: PathBuf,
    pub codex: PathBuf,
    pub hermes: PathBuf,
}

impl Dirs {
    pub fn from_env(home: PathBuf) -> Dirs {
        Dirs::resolve(home, |name| std::env::var_os(name).map(PathBuf::from))
    }

    // Every agent in its default place under `home`, whatever the environment says.
    #[cfg(test)]
    pub fn under(home: &Path) -> Dirs {
        Dirs::resolve(home.to_path_buf(), |_| None)
    }

    fn resolve(home: PathBuf, var: impl Fn(&str) -> Option<PathBuf>) -> Dirs {
        let hermes = match cfg!(windows) {
            true => var("LOCALAPPDATA")
                .unwrap_or_else(|| home.join("AppData/Local"))
                .join("hermes"),
            false => home.join(".hermes"),
        };
        Dirs {
            copilot: var("COPILOT_HOME").unwrap_or_else(|| home.join(".copilot")),
            codex: var("CODEX_HOME").unwrap_or_else(|| home.join(".codex")),
            hermes: var("HERMES_HOME").unwrap_or(hermes),
            home,
        }
    }
}

// Every agent petit-poucet knows: how its hooks reply, how to find it, how it gets set up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Agent {
    Claude,
    Copilot,
    Codex,
    Cursor,
    Antigravity,
    Opencode,
    Omp,
    Hermes,
}

impl Agent {
    pub const ALL: [Agent; 8] = [
        Agent::Claude,
        Agent::Copilot,
        Agent::Codex,
        Agent::Cursor,
        Agent::Antigravity,
        Agent::Opencode,
        Agent::Omp,
        Agent::Hermes,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Copilot => "GitHub Copilot",
            Agent::Codex => "Codex",
            Agent::Cursor => "Cursor",
            Agent::Antigravity => "Antigravity CLI",
            Agent::Opencode => "OpenCode",
            Agent::Omp => "omp",
            Agent::Hermes => "Hermes Agent",
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
            Agent::Opencode => "opencode",
            Agent::Omp => "omp",
            Agent::Hermes => "hermes",
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
            Agent::Opencode => "opencode hook",
            Agent::Omp => "omp hook",
            Agent::Hermes => "hermes hook",
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
    // Hermes runs it before every model call, and keeps what the first turn's call received.
    pub fn holds_memory_already(self, event: &Value) -> bool {
        match self {
            Agent::Codex => event["source"] == "resume",
            Agent::Hermes => event["extra"]["is_first_turn"] == json!(false),
            _ => false,
        }
    }

    pub fn shows_hook_messages(self) -> bool {
        matches!(self, Agent::Claude | Agent::Codex)
    }

    // Copilot CLI, Cursor, Antigravity, OpenCode, omp and Hermes hooks have no line for the user, so `message` goes to Claude Code and Codex only.
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
            // Read by petit-poucet's own plugin, which adds it to every model call.
            Agent::Opencode | Agent::Omp => json!({"context": context}),
            // Added to the user's message.
            Agent::Hermes => json!({"context": context}),
        }
    }

    // None for Hermes: it has no hook after an answer, where the reminder belongs.
    pub fn stop_reply(self, reason: &str, message: &str) -> Option<Value> {
        match self {
            Agent::Claude | Agent::Codex => {
                Some(json!({"decision": "block", "reason": reason, "systemMessage": message}))
            }
            Agent::Copilot => Some(json!({"decision": "block", "reason": reason})),
            // Cursor sends it as the user's next message.
            Agent::Cursor => Some(json!({"followup_message": reason})),
            Agent::Antigravity => Some(json!({"decision": "continue", "reason": reason})),
            // The plugin sends it back to the agent.
            Agent::Opencode | Agent::Omp => Some(json!({"reason": reason})),
            Agent::Hermes => None,
        }
    }

    pub fn installed(self, dirs: &Dirs) -> bool {
        let home = &dirs.home;
        match self {
            Agent::Claude => home.join(".claude").is_dir(),
            Agent::Copilot => dirs.copilot.is_dir(),
            Agent::Codex => dirs.codex.is_dir(),
            Agent::Cursor => home.join(".cursor").is_dir(),
            Agent::Antigravity => antigravity::dir(home).is_dir(),
            Agent::Opencode => opencode::dir(home).is_dir(),
            Agent::Omp => omp::dir(home).is_dir(),
            Agent::Hermes => dirs.hermes.is_dir(),
        }
    }

    // petit-poucet 0.2 came as a plugin for these two: setup removes it with the agent's own command.
    pub fn old_plugin(self) -> Option<&'static [&'static str]> {
        match self {
            Agent::Claude => Some(&["claude", "plugin", "uninstall", "petit-poucet@petit-poucet"]),
            Agent::Copilot => Some(&["copilot", "plugin", "uninstall", "petit-poucet"]),
            Agent::Codex
            | Agent::Cursor
            | Agent::Antigravity
            | Agent::Opencode
            | Agent::Omp
            | Agent::Hermes => None,
        }
    }

    pub fn plugin_installed(self, dirs: &Dirs) -> bool {
        match self {
            Agent::Claude => enabled_in(&dirs.home.join(".claude/settings.json")),
            // A local install is only listed in the settings; a marketplace install is also copied.
            Agent::Copilot => {
                enabled_in(&dirs.copilot.join("settings.json"))
                    || fs::read_dir(dirs.copilot.join("installed-plugins"))
                        .into_iter()
                        .flatten()
                        .flatten()
                        .any(|marketplace| marketplace.path().join("petit-poucet").is_dir())
            }
            Agent::Codex
            | Agent::Cursor
            | Agent::Antigravity
            | Agent::Opencode
            | Agent::Omp
            | Agent::Hermes => false,
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
    fn dirs_for_tests_stay_under_their_home_whatever_the_environment() {
        let home = Path::new("/test/home");
        let dirs = Dirs::under(home);
        for dir in [&dirs.home, &dirs.copilot, &dirs.codex, &dirs.hermes] {
            assert!(dir.starts_with(home), "{}", dir.display());
        }
    }

    #[test]
    fn dirs_follow_the_agents_environment_variables() {
        let var = |name: &str| match name {
            "COPILOT_HOME" => Some(PathBuf::from("/c")),
            "CODEX_HOME" => Some(PathBuf::from("/x")),
            "HERMES_HOME" => Some(PathBuf::from("/h")),
            _ => None,
        };
        let dirs = Dirs::resolve(PathBuf::from("/home"), var);
        assert_eq!(
            [dirs.copilot, dirs.codex, dirs.hermes],
            [Path::new("/c"), Path::new("/x"), Path::new("/h")]
        );
        let local = |name: &str| (name == "LOCALAPPDATA").then(|| PathBuf::from("/local"));
        let hermes = Dirs::resolve(PathBuf::from("/home"), local).hermes;
        match cfg!(windows) {
            true => assert_eq!(hermes, Path::new("/local/hermes")),
            false => assert_eq!(hermes, Path::new("/home/.hermes")),
        }
    }

    #[test]
    fn hermes_gets_the_memory_on_its_first_turn_only() {
        let first = json!({"extra": {"is_first_turn": true}});
        let later = json!({"extra": {"is_first_turn": false}});
        assert!(!Agent::Hermes.holds_memory_already(&first));
        assert!(Agent::Hermes.holds_memory_already(&later));
        assert!(!Agent::Codex.holds_memory_already(&later));
        assert_eq!(
            Agent::Hermes.stop_reply("r", "m"),
            None,
            "no hook after an answer"
        );
    }

    #[test]
    fn finds_agents_by_their_home_folder() {
        let home = tempfile::tempdir().unwrap();
        assert!(!Agent::Claude.installed(&Dirs::under(home.path())));
        fs::create_dir(home.path().join(".claude")).unwrap();
        assert!(Agent::Claude.installed(&Dirs::under(home.path())));
        assert!(!Agent::Copilot.installed(&Dirs::under(home.path())));
    }

    #[test]
    fn the_claude_plugin_counts_only_when_enabled() {
        let home = tempfile::tempdir().unwrap();
        assert!(
            !Agent::Claude.plugin_installed(&Dirs::under(home.path())),
            "no settings"
        );
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
                Agent::Claude.plugin_installed(&Dirs::under(home.path())),
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
        assert!(Agent::Copilot.plugin_installed(&Dirs::under(home.path())));
        assert!(
            !Agent::Claude.plugin_installed(&Dirs::under(home.path())),
            "each agent has its own settings"
        );
    }

    #[test]
    fn the_copilot_plugin_is_found_under_any_marketplace() {
        let home = tempfile::tempdir().unwrap();
        assert!(!Agent::Copilot.plugin_installed(&Dirs::under(home.path())));
        write(
            home.path(),
            ".copilot/installed-plugins/other/tool/plugin.json",
            "{}",
        );
        assert!(!Agent::Copilot.plugin_installed(&Dirs::under(home.path())));
        write(
            home.path(),
            ".copilot/installed-plugins/some-marketplace/petit-poucet/plugin.json",
            "{}",
        );
        assert!(Agent::Copilot.plugin_installed(&Dirs::under(home.path())));
    }
}
