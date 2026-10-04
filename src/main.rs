mod agent;
mod antigravity;
mod change;
mod check;
mod claude;
mod cleanup;
mod codex;
mod config;
mod copilot;
mod cursor;
mod delete;
mod duplicates;
mod edit;
mod git;
mod guard;
mod hook;
mod hook_command;
mod hooks_json;
mod index;
mod init;
mod lock;
mod migrate;
mod move_note;
mod note;
mod project;
mod review;
mod save;
mod search;
mod secrets;
mod server;
mod setup;
mod skills;
mod state;
mod stops;
mod update;
mod usage;
mod vault;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::Config;
use crate::vault::Vault;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the MCP tools over stdio
    Serve,
    /// Run an agent hook
    Hook {
        event: HookEvent,
        #[arg(long)]
        agent: agent::Agent,
    },
    /// Validate the vault
    Check,
    /// Create a vault (default: ~/agent-memory) and the config file
    Init { path: Option<PathBuf> },
    /// Upgrade an existing vault to the current format
    Migrate,
    /// Set up memory for the agents on this machine (and create the vault if there's none)
    Setup {
        /// Only this agent
        #[arg(long)]
        agent: Option<agent::Agent>,
        /// Report what's set up, without changing anything; fails if something is missing
        #[arg(long, conflicts_with = "uninstall")]
        check: bool,
        /// Remove petit-poucet from the agents (the vault is kept)
        #[arg(long)]
        uninstall: bool,
    },
    /// Record the latest release's version: session start runs it in the background
    #[command(hide = true)]
    UpdateCheck,
}

#[derive(Clone, ValueEnum)]
enum HookEvent {
    SessionStart,
    Stop,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Check => run_check(),
        Command::Init { path } => init::init(path).map(|msg| {
            println!("{msg}");
            true
        }),
        Command::Serve => Config::load().and_then(server::serve).map(|()| true),
        Command::Migrate => Config::load()
            .and_then(|config| migrate::migrate(&config))
            .map(|report| {
                println!("{report}");
                true
            }),
        Command::Setup {
            agent,
            check,
            uninstall,
        } => run_setup(agent, check, uninstall),
        Command::Hook { event, agent } => {
            run_hook(event, agent);
            Ok(true)
        }
        Command::UpdateCheck => {
            update::fetch();
            Ok(true)
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("petit-poucet: {e}");
            ExitCode::FAILURE
        }
    }
}

// Hooks never fail the agent's session: problems are reported inside the output.
fn run_hook(event: HookEvent, agent: agent::Agent) {
    let input = serde_json::from_reader(std::io::stdin()).unwrap_or(serde_json::Value::Null);
    let output = match event {
        HookEvent::SessionStart => hook::session_start(agent, &input),
        HookEvent::Stop => hook::stop(agent, &input),
    };
    if let Some(output) = output {
        println!("{output}");
    }
}

fn run_setup(agent: Option<agent::Agent>, check: bool, uninstall: bool) -> Result<bool, String> {
    let mode = match (check, uninstall) {
        (true, _) => setup::Mode::Check,
        (_, true) => setup::Mode::Uninstall,
        _ => setup::Mode::Setup,
    };
    let (lines, ok) = setup::run(mode, agent)?;
    for line in lines {
        println!("{line}");
    }
    Ok(ok || mode != setup::Mode::Check)
}

fn run_check() -> Result<bool, String> {
    let vault = Vault::load(&Config::load()?.vault)?;
    let issues = check::check(&vault);
    for issue in &issues {
        println!("{issue}");
    }
    let errors = issues
        .iter()
        .filter(|i| i.level == check::Level::Error)
        .count();
    println!(
        "notes: {}, errors: {errors}, warnings: {}",
        vault.notes.len(),
        issues.len() - errors
    );
    if let Some(line) = update::report() {
        println!("{line}");
    }
    Ok(errors == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_valid() {
        Cli::command().debug_assert();
    }
}
