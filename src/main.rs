mod check;
mod config;
mod git;
mod index;
mod init;
mod note;
mod project;
mod secrets;
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
        agent: Agent,
    },
    /// Validate the vault
    Check,
    /// Create a vault and the config file
    Init { path: PathBuf },
    /// Upgrade an existing vault to the current format
    Migrate,
}

#[derive(Clone, ValueEnum)]
enum HookEvent {
    SessionStart,
    Stop,
}

#[derive(Clone, ValueEnum)]
enum Agent {
    Claude,
    Copilot,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Check => run_check(),
        Command::Init { path } => init::init(&path).map(|msg| {
            println!("{msg}");
            true
        }),
        Command::Serve | Command::Hook { .. } | Command::Migrate => {
            Err("not implemented yet".to_string())
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
