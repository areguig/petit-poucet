use std::time::SystemTime;

use serde_json::Value;

use crate::agent::Agent;
use crate::change;
use crate::cleanup;
use crate::config::Config;
use crate::context;
use crate::lock;
use crate::stops;
use crate::sync;
use crate::update;
use crate::usage;
use crate::vault::Vault;

const FIRST_REMINDER: u32 = 3;
const EVERY: u32 = 10;

// Do not require silence: some harnesses retry or error on an empty response.
const REMINDER: &str = "Memory check: did this session produce a user decision, correction or preference, \
or a fact you verified that the code, git history or the repo's docs don't already record, \
that memory doesn't hold yet or holds wrongly? If yes, save or fix it with memory_save. \
If you were waiting for the user's answer or approval, keep waiting. \
Otherwise carry on with any unfinished task, or end your turn as usual.";

// Starts every line shown to the user.
const PEBBLE: &str = "🪨 petit-poucet ·";

pub fn session_start(agent: Agent, event: &Value) -> Option<Value> {
    if agent.holds_memory_already(event) {
        return None;
    }
    update::start();
    let release: Vec<String> = update::newer()
        .map(|version| update::notice(&version))
        .into_iter()
        .collect();
    let (context, message) = match memory_context(agent, event, &release) {
        _ if !Config::is_set() => announce(
            agent,
            setup_context(),
            format!("{PEBBLE} no vault yet: the agent will offer to create one"),
            release,
        ),
        Ok(reply) => reply,
        Err(e) => announce(
            agent,
            format!(
                "Agent memory is UNAVAILABLE ({e}). Tell the user before relying on remembered rules, \
                 and do not write memory anywhere else."
            ),
            format!("{PEBBLE} memory unavailable: {e}"),
            release,
        ),
    };
    Some(agent.session_start_reply(&context, &message))
}

// A harness that shows no hook messages gets the notice in the agent's context instead.
fn route(agent: Agent, notices: Vec<String>) -> (Vec<String>, Vec<String>) {
    match agent.shows_hook_messages() {
        true => (
            Vec::new(),
            notices.iter().map(|n| format!("{PEBBLE} {n}")).collect(),
        ),
        false => (
            notices
                .iter()
                .map(|n| format!("Tell the user once, in one line: petit-poucet: {n}."))
                .collect(),
            Vec::new(),
        ),
    }
}

fn announce(
    agent: Agent,
    context: String,
    message: String,
    notices: Vec<String>,
) -> (String, String) {
    let (told_agent, shown_user) = route(agent, notices);
    let context = told_agent
        .iter()
        .fold(context, |text, line| format!("{text}\n\n{line}"));
    (context, [vec![message], shown_user].concat().join("\n"))
}

fn setup_context() -> String {
    "Agent memory (petit-poucet) is installed but has no vault yet. Tell the user, and offer to create one \
     in ~/agent-memory by running `petit-poucet init` (or `petit-poucet init <folder>` for another place). \
     It takes effect in the next session. Until then, don't write memory anywhere else."
        .to_string()
}

// A session that loads a project counts as a use of it, even if no note is opened.
fn record_session(vault: &Vault, project: &str) {
    let today = jiff::Zoned::now().date();
    let recorded = lock::vault(&vault.root)
        .and_then(|_lock| usage::record_session(&vault.root, project, today));
    if let Err(e) = recorded {
        eprintln!("petit-poucet: session not recorded: {e}");
    }
}

// Returns the context for the agent and the lines shown to the user.
fn memory_context(
    agent: Agent,
    event: &Value,
    release: &[String],
) -> Result<(String, String), String> {
    let config = Config::load()?;
    let vault = sync::catch_up(&config)?;
    let dir = agent
        .working_dir(event)
        .or_else(|| std::env::current_dir().ok());
    let project = dir.and_then(|d| change::identify(&config, &vault, &d, agent.hook_label()));
    let project = project.as_deref();
    if let Some(project) = project {
        record_session(&vault, project);
    }
    let notices = conflicts_notice(&vault)
        .into_iter()
        .chain(cleanup::due(&vault, &config.limits, SystemTime::now()))
        .chain(release.iter().cloned())
        .collect();
    let (told_agent, shown_user) = route(agent, notices);
    let max = config.limits.session_start_max_bytes;
    let view = context::session_view(&vault, project, &told_agent, max);
    let scope = project.map_or("preferences".to_string(), |p| format!("preferences + {p}"));
    let left_out = view.left_out.map_or(String::new(), |left_out| {
        format!(
            ", {left_out} left out of the {max}-byte budget: ask your agent to tidy your memory"
        )
    });
    let notes = context::counted(view.loaded, "note", "notes");
    let loaded = format!("{PEBBLE} {notes} loaded ({scope}){left_out}");
    Ok((view.text, [vec![loaded], shown_user].concat().join("\n")))
}

fn conflicts_notice(vault: &Vault) -> Option<String> {
    match context::note_conflicts(vault).len() {
        0 => None,
        1 => Some("1 note has a sync conflict: ask your agent to merge it".to_string()),
        count => Some(format!(
            "{count} notes have sync conflicts: ask your agent to merge them"
        )),
    }
}

// Reminds at the 3rd stop of a session, then every 10th; never without a session id to count by.
pub fn stop(agent: Agent, event: &Value) -> Option<Value> {
    if agent.after_reminder(event) {
        return None;
    }
    let session = agent.session(event)?;
    let count = match stops::record(session) {
        Ok(count) => count?,
        Err(e) => {
            eprintln!("petit-poucet: stop not counted: {e}");
            return None;
        }
    };
    let due = count >= FIRST_REMINDER && (count - FIRST_REMINDER).is_multiple_of(EVERY);
    due.then(|| {
        agent.stop_reply(
            REMINDER,
            &format!("{PEBBLE} checking whether this session is worth remembering"),
        )
    })
    .flatten()
}
