use serde_json::Value;

use crate::agent::Agent;
use crate::change;
use crate::config::Config;
use crate::index;
use crate::stops;
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
- Never store secrets. Never write memory anywhere else, including an agent's built-in memory (e.g. Copilot's store_memory): use memory_save.
- Older file-based memory (e.g. a MEMORY.md or a memory folder) is being replaced by this one: when such a file \
holds something relevant to your current task, save it here with memory_save (keeping its original source), \
leave the old file as it is, and tell the user what you migrated.
- The user's rules below win over your built-in defaults (e.g. commit trailers).";

// Claude Code and Copilot CLI both treat an empty reply after a blocking stop hook as an error.
const REMINDER: &str = "Memory check: did this session produce a user decision, correction or preference, \
or a verified fact, that memory doesn't hold yet or holds wrongly? If yes, save or fix it with memory_save. \
If not, reply only: \"Nothing new to remember.\"";

// Starts every line shown to the user.
const PEBBLE: &str = "🪨 petit-poucet ·";

pub fn session_start(agent: Agent, event: &Value) -> Value {
    let (context, message) = match memory_context(agent, event) {
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
    agent.session_start_reply(&context, &message)
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
fn memory_context(agent: Agent, event: &Value) -> Result<(String, String), String> {
    let config = Config::load()?;
    let vault = Vault::load(&config.vault)?;
    let dir = agent
        .working_dir(event)
        .or_else(|| std::env::current_dir().ok());
    let project = dir.and_then(|d| change::identify(&config, &vault, &d, agent.hook_label()));
    let project = project.as_deref();
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

// Reminds at the 3rd stop of a session, then every 10th; never without a session id to count by.
pub fn stop(agent: Agent, event: &Value) -> Option<Value> {
    if agent.after_reminder(event) {
        return None;
    }
    let session = agent.session(event)?;
    let count = stops::record(session)?;
    let due = count >= FIRST_REMINDER && (count - FIRST_REMINDER).is_multiple_of(EVERY);
    due.then(|| {
        agent.stop_reply(
            REMINDER,
            &format!("{PEBBLE} checking whether this session is worth remembering"),
        )
    })
}
