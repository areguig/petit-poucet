use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

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
    let cli = Cli::parse();
    let name = match cli.command {
        Command::Serve => "serve",
        Command::Hook { .. } => "hook",
        Command::Check => "check",
        Command::Init { .. } => "init",
        Command::Migrate => "migrate",
    };
    eprintln!("{name}: not implemented yet");
    ExitCode::FAILURE
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
