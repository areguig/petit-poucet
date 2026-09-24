mod change;
mod check;
mod config;
mod delete;
mod git;
mod guard;
mod hook;
mod index;
mod init;
mod migrate;
mod move_note;
mod note;
mod project;
mod save;
mod search;
mod secrets;
mod server;
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
        agent: hook::Agent,
    },
    /// Validate the vault
    Check,
    /// Create a vault (default: ~/agent-memory) and the config file
    Init { path: Option<PathBuf> },
    /// Upgrade an existing vault to the current format
    Migrate,
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
        Command::Hook { event, agent } => {
            run_hook(event, agent);
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
fn run_hook(event: HookEvent, agent: hook::Agent) {
    let input = serde_json::from_reader(std::io::stdin()).unwrap_or(serde_json::Value::Null);
    let output = match event {
        HookEvent::SessionStart => Some(hook::session_start(agent, &input)),
        HookEvent::Stop => hook::stop(&input),
    };
    if let Some(output) = output {
        println!("{output}");
    }
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
