use std::fs;
use std::path::PathBuf;

use clap::ValueEnum;
use serde_json::{Value, json};

use crate::config::Config;
use crate::index;
use crate::project;
use crate::vault::Vault;

const FIRST_REMINDER: u32 = 3;
const EVERY: u32 = 10;

pub const RULES: &str = "Agent memory (petit-poucet): the Index below is already loaded, don't fetch it again. \
Open only the notes a task needs with memory_read. Pass your working directory as project_dir.
- One short fact per note. Search first (memory_search) and update a note rather than adding a near-duplicate; read a note (memory_read) before updating or deleting it.
- `source` says where the fact came from (the user's words, or the file/command that verified it) and when.
- Rules the user stated (type feedback) change only after the user confirms: ask first.
- When unsure whether a note is wrong, obsolete, or where it belongs: ask the user.
- Update or delete (memory_delete) contradicted or obsolete notes, and mention the change in your reply.
- Knowledge tied to no repo (a homelab, a server, the work machine) goes in a topic: memory_save with `topic`. \
Topics are only named at the end of the Index: search them (memory_search) when a task touches one.
- Never store secrets. Never write memory anywhere else.
- Older file-based memory (e.g. a MEMORY.md or a memory folder) is being replaced by this one: when such a file \
holds something relevant to your current task, save it here with memory_save (keeping its original source), \
leave the old file as it is, and tell the user what you migrated.
- The user's rules below win over your built-in defaults (e.g. commit trailers).";

// Claude Code and Copilot CLI both treat an empty reply after a blocking stop hook as an error.
const REMINDER: &str = "Memory check: did this session produce a user decision, correction or preference, \
or a verified fact, that memory doesn't hold yet or holds wrongly? If yes, save or fix it with memory_save. \
If not, reply only: \"Nothing new to remember.\"";

// Shown to the user by Claude Code; Copilot CLI hooks have no user-facing message.
const PEBBLE: &str = "🪨 petit-poucet ·";

#[derive(Clone, Copy, ValueEnum)]
pub enum Agent {
    Claude,
    Copilot,
}

// Claude Code sends snake_case event fields, Copilot CLI camelCase.
fn field<'a>(event: &'a Value, snake: &str, camel: &str) -> Option<&'a Value> {
    event.get(snake).or_else(|| event.get(camel))
}

pub fn session_start(agent: Agent, event: &Value) -> Value {
    let (context, message) = match memory_context(event) {
        _ if !Config::is_set() => (
            setup_context(),
            format!("{PEBBLE} no vault yet: the agent will offer to create one"),
        ),
        Ok(loaded) => loaded,
        Err(e) => (
            format!(
                "Agent memory is UNAVAILABLE ({e}). Tell the user before relying on remembered rules, \
                 and do not write memory anywhere else."
            ),
            format!("{PEBBLE} memory unavailable: {e}"),
        ),
    };
    match agent {
        Agent::Claude => json!({
            "systemMessage": message,
            "hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": context},
        }),
        Agent::Copilot => json!({"additionalContext": context}),
    }
}

// Plugin installs have no `petit-poucet` on PATH: the launcher says where it is.
fn setup_context() -> String {
    let command = std::env::var("PETIT_POUCET_LAUNCHER").unwrap_or_else(|_| "petit-poucet".into());
    format!(
        "Agent memory (petit-poucet) is installed but has no vault yet. Tell the user, and offer to create one \
         in ~/agent-memory by running `{command} init` (or `{command} init <folder>` for another place). \
         It takes effect in the next session. Until then, don't write memory anywhere else."
    )
}

// Returns the context for the agent and the line shown to the user.
fn memory_context(event: &Value) -> Result<(String, String), String> {
    let vault = Vault::load(&Config::load()?.vault)?;
    let dir = event
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    let project = dir.and_then(|d| project::resolve(&vault.projects, &d));
    let loaded = vault
        .notes
        .iter()
        .filter(|n| index::in_session(n.place(), project))
        .count();
    let scope = project.map_or("preferences".to_string(), |p| format!("preferences + {p}"));
    Ok((
        format!("{RULES}\n{}", index::for_project(&vault, project)),
        format!("{PEBBLE} {loaded} notes loaded ({scope})"),
    ))
}

// Counts stops per session in a temp file; reminds at the 3rd stop, then every 10th.
pub fn stop(agent: Agent, event: &Value) -> Option<Value> {
    if field(event, "stop_hook_active", "stopHookActive").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let session = field(event, "session_id", "sessionId")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let counter = std::env::temp_dir().join(format!("petit-poucet-stops-{session}"));
    let count = fs::read_to_string(&counter)
        .ok()
        .and_then(|n| n.trim().parse::<u32>().ok())
        .unwrap_or(0)
        + 1;
    fs::write(&counter, count.to_string()).ok()?;
    let due = count >= FIRST_REMINDER && (count - FIRST_REMINDER).is_multiple_of(EVERY);
    due.then(|| match agent {
        Agent::Claude => json!({
            "decision": "block",
            "reason": REMINDER,
            "systemMessage": format!("{PEBBLE} checking whether this session is worth remembering"),
        }),
        Agent::Copilot => json!({"decision": "block", "reason": REMINDER}),
    })
}
