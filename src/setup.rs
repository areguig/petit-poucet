use std::path::Path;

use crate::agent::Agent;
use crate::config::Config;
use crate::init;
use crate::vault::Vault;
use crate::{antigravity, claude, codex, copilot, cursor, opencode, skills};

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
        let (agent_lines, agent_ok) = report(agent, mode, &home, only.is_some());
        ok &= agent_ok;
        lines.extend(agent_lines);
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
                let lines = created.lines().enumerate().map(|(i, line)| match i {
                    0 => format!("vault: created, {line}"),
                    _ => format!("vault: {line}"),
                });
                Ok((lines.collect(), true))
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
fn report(agent: Agent, mode: Mode, home: &Path, named: bool) -> (Vec<String>, bool) {
    let name = agent.name();
    if !agent.installed(home) {
        let line = named.then(|| format!("{name}: not found on this machine"));
        return (line.into_iter().collect(), true);
    }
    let mut lines = Vec::new();
    if let Some((line, ok)) = old_plugin(agent, mode, home) {
        lines.push(format!("{name}: {line}"));
        // Its hooks would run next to the new ones.
        if !ok {
            return (lines, false);
        }
    }
    let (line, ok) = by_config(agent, mode, home).unwrap_or_else(|e| (e, false));
    let (skills, skills_ok) = by_skills(agent, mode, home).unwrap_or_else(|e| (e, false));
    lines.push(format!("{name}: {line}"));
    lines.push(format!("{name} skills: {skills}"));
    (lines, ok && skills_ok)
}

fn old_plugin(agent: Agent, mode: Mode, home: &Path) -> Option<(String, bool)> {
    let command = agent.old_plugin()?;
    if !agent.plugin_installed(home) {
        return None;
    }
    if mode == Mode::Check {
        let fix = format!("run `petit-poucet setup --agent {}`", agent.flag());
        return Some((
            format!("its old petit-poucet plugin is still installed: {fix}"),
            false,
        ));
    }
    let removed = std::process::Command::new(command[0])
        .args(&command[1..])
        .output()
        .map_err(|e| e.to_string())
        .and_then(|out| match out.status.success() {
            true => Ok(()),
            false => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        });
    Some(match removed {
        Ok(()) => ("removed its old petit-poucet plugin".to_string(), true),
        Err(e) => (
            format!(
                "couldn't remove its old petit-poucet plugin ({e}): run `{}`, then `petit-poucet setup` again",
                command.join(" ")
            ),
            false,
        ),
    })
}

fn by_config(agent: Agent, mode: Mode, home: &Path) -> Result<(String, bool), String> {
    let exe = || std::env::current_exe().map_err(|e| e.to_string());
    let (setup, check, uninstall, dir): (Setup, Check, Uninstall, _) = match agent {
        Agent::Codex => (
            codex::setup,
            codex::check,
            codex::uninstall,
            codex::dir(home),
        ),
        Agent::Cursor => (
            cursor::setup,
            cursor::check,
            cursor::uninstall,
            home.join(".cursor"),
        ),
        Agent::Antigravity => (
            antigravity::setup,
            antigravity::check,
            antigravity::uninstall,
            antigravity::dir(home),
        ),
        Agent::Claude => (
            claude::setup,
            claude::check,
            claude::uninstall,
            home.to_path_buf(),
        ),
        Agent::Copilot => (
            copilot::setup,
            copilot::check,
            copilot::uninstall,
            copilot::dir(home),
        ),
        Agent::Opencode => (
            opencode::setup,
            opencode::check,
            opencode::uninstall,
            opencode::dir(home),
        ),
    };
    match mode {
        Mode::Setup => Ok((setup(&dir, &exe()?)?, true)),
        Mode::Check => check(&dir),
        Mode::Uninstall => Ok((uninstall(&dir)?, true)),
    }
}

fn by_skills(agent: Agent, mode: Mode, home: &Path) -> Result<(String, bool), String> {
    match mode {
        Mode::Setup => Ok((skills::setup(agent, home)?, true)),
        Mode::Check => Ok(skills::check(agent, home)),
        Mode::Uninstall => Ok((skills::uninstall(agent, home)?, true)),
    }
}

type Setup = fn(&Path, &Path) -> Result<String, String>;
type Check = fn(&Path) -> Result<(String, bool), String>;
type Uninstall = fn(&Path) -> Result<String, String>;
