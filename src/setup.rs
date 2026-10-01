use std::path::Path;

use crate::agent::{Agent, Plugin};
use crate::codex;
use crate::config::Config;
use crate::init;
use crate::vault::Vault;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Setup,
    Check,
    Uninstall,
}

// One line per item; false when something needs the user (check fails on it, setup only reports it).
pub fn run(mode: Mode, only: Option<Agent>) -> Result<(Vec<String>, bool), String> {
    let home = std::env::home_dir().ok_or("cannot find the home folder")?;
    let (mut lines, mut ok) = vault(mode)?;
    let before = lines.len();
    for agent in only.map_or(Agent::ALL.to_vec(), |a| vec![a]) {
        let (line, agent_ok) = report(agent, mode, &home, only.is_some());
        ok &= agent_ok;
        lines.extend(line);
    }
    if lines.len() == before {
        let names: Vec<&str> = Agent::ALL.iter().map(|a| a.name()).collect();
        lines.push(format!(
            "agents: none found (supported: {})",
            names.join(", ")
        ));
    }
    Ok((lines, ok))
}

fn vault(mode: Mode) -> Result<(Vec<String>, bool), String> {
    if !Config::is_set() {
        return match mode {
            Mode::Setup => {
                let created = init::init(None)?;
                let first = created.lines().next().unwrap_or_default();
                Ok((vec![format!("vault: created, {first}")], true))
            }
            Mode::Check => Ok((
                vec!["vault: none yet: run `petit-poucet setup`".into()],
                false,
            )),
            Mode::Uninstall => Ok((vec![], true)),
        };
    }
    let config = Config::load()?;
    if mode == Mode::Uninstall {
        let kept = format!("vault: kept at {}", config.vault.display());
        return Ok((vec![kept], true));
    }
    Ok(match Vault::load(&config.vault) {
        Ok(vault) => (
            vec![format!(
                "vault: {} ({} notes)",
                config.vault.display(),
                vault.notes.len()
            )],
            true,
        ),
        Err(e) => (vec![format!("vault: {e}")], false),
    })
}

// An agent not on this machine is only mentioned when the user named it.
fn report(agent: Agent, mode: Mode, home: &Path, named: bool) -> (Option<String>, bool) {
    let name = agent.name();
    if !agent.installed(home) {
        return (
            named.then(|| format!("{name}: not found on this machine")),
            true,
        );
    }
    let (line, ok) = match agent.plugin() {
        Some(plugin) => by_plugin(agent, &plugin, mode, home),
        None => by_config(mode, home).unwrap_or_else(|e| (e, false)),
    };
    (Some(format!("{name}: {line}")), ok)
}

fn by_plugin(agent: Agent, plugin: &Plugin, mode: Mode, home: &Path) -> (String, bool) {
    let installed = agent.plugin_installed(home);
    let line = match (mode, installed) {
        (Mode::Uninstall, true) => format!("remove the plugin with `{}`", plugin.uninstall),
        (Mode::Uninstall, false) => "nothing to remove".to_string(),
        (_, true) => "set up by its plugin".to_string(),
        (_, false) => format!("install its plugin: `{}`", plugin.install),
    };
    (line, installed || mode == Mode::Uninstall)
}

// Only Codex is set up this way so far.
fn by_config(mode: Mode, home: &Path) -> Result<(String, bool), String> {
    let dir = codex::dir(home);
    match mode {
        Mode::Setup => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            Ok((codex::setup(&dir, &exe)?, true))
        }
        Mode::Check => codex::check(&dir),
        Mode::Uninstall => Ok((codex::uninstall(&dir)?, true)),
    }
}
