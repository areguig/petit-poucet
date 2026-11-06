// site/install.md is run by agents: each step's command must reach the binary that step 1 found.
mod common;

use std::env::consts::EXE_SUFFIX;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use tempfile::TempDir;

const GUIDE: &str = include_str!("../site/install.md");
const VERSION: &str = env!("CARGO_PKG_VERSION");

enum Shell {
    #[cfg(unix)]
    Sh,
    #[cfg(windows)]
    PowerShell(&'static str),
}

#[cfg(unix)]
const SHELLS: &[Shell] = &[Shell::Sh];

#[cfg(windows)]
const SHELLS: &[Shell] = &[Shell::PowerShell("powershell"), Shell::PowerShell("pwsh")];

// The guide's commands: its inline code and the lines of its code blocks.
fn commands() -> Vec<&'static str> {
    let mut commands = Vec::new();
    let mut in_block = false;
    for line in GUIDE.lines() {
        if line.starts_with("```") {
            in_block = !in_block;
        } else if in_block {
            commands.push(line.trim());
        } else {
            commands.extend(line.split('`').skip(1).step_by(2));
        }
    }
    commands
}

impl Shell {
    fn name(&self) -> &'static str {
        match self {
            #[cfg(unix)]
            Shell::Sh => "sh",
            #[cfg(windows)]
            Shell::PowerShell(program) => program,
        }
    }

    fn writes(&self, command: &str) -> bool {
        let full_path = match self {
            #[cfg(unix)]
            Shell::Sh => "~/",
            #[cfg(windows)]
            Shell::PowerShell(_) => "& ",
        };
        command.starts_with("petit-poucet ") || command.starts_with(full_path)
    }

    fn run(&self, home: &Path, path: &OsStr, command: &str) -> Output {
        let mut cmd = common::command_for(Path::new(self.name()), home);
        match self {
            #[cfg(unix)]
            Shell::Sh => cmd.args(["-c", command]),
            #[cfg(windows)]
            Shell::PowerShell(_) => {
                // A command that isn't found must fail the script, and a native command's status must be its own.
                let script = home.join("step.ps1");
                fs::write(
                    &script,
                    format!("$ErrorActionPreference = 'Stop'\n{command}\nexit $LASTEXITCODE\n"),
                )
                .unwrap();
                cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                    .arg(script)
            }
        };
        cmd.env("PATH", path)
            .env("PETIT_POUCET_LATEST_RELEASE", latest_release(home))
            .env("PETIT_POUCET_RELEASES", home.join("no-releases"))
            .output()
            .unwrap()
    }

    // Step 1: the first way to run petit-poucet that answers.
    fn invocation(&self, home: &Path, path: &OsStr) -> &'static str {
        commands()
            .into_iter()
            .filter(|command| self.writes(command))
            .filter_map(|command| command.strip_suffix(" --version"))
            .find(|invocation| {
                self.run(home, path, &format!("{invocation} --version"))
                    .status
                    .success()
            })
            .unwrap_or_else(|| panic!("{}: no command in step 1 finds the binary", self.name()))
    }
}

// What the guide says to run for `subcommand`, written the way that worked; an agent shown no such form runs the bare one.
fn guide_command(invocation: &str, subcommand: &str) -> String {
    let wanted = format!("{invocation} {subcommand}");
    match commands().contains(&wanted.as_str()) {
        true => wanted,
        false => format!("petit-poucet {subcommand}"),
    }
}

// update reads the latest release here, and finds it is this build: it downloads nothing.
fn latest_release(home: &Path) -> String {
    let file = home.join("latest.json");
    fs::write(&file, format!(r#"{{"tag_name": "v{VERSION}"}}"#)).unwrap();
    let path = file.display().to_string().replace('\\', "/");
    format!(
        "file://{}{path}",
        if path.starts_with('/') { "" } else { "/" }
    )
}

// A home where the install script put the binary, in ~/.local/bin.
fn installed_home() -> (TempDir, PathBuf) {
    let home = TempDir::new().unwrap();
    let bin = home.path().join(".local").join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_petit-poucet"),
        bin.join(format!("petit-poucet{EXE_SUFFIX}")),
    )
    .unwrap();
    (home, bin)
}

// This machine's PATH, without any petit-poucet already installed on it, and with `bin` first when given.
fn search_path(bin: Option<&Path>) -> OsString {
    let system = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::env::split_paths(&system)
        .filter(|dir| !dir.join(format!("petit-poucet{EXE_SUFFIX}")).exists());
    std::env::join_paths(bin.map(Path::to_path_buf).into_iter().chain(dirs)).unwrap()
}

// Runs `subcommands` the way the guide says, and checks the binary answered each.
fn follow(on_path: bool, subcommands: &[(&str, &str)]) {
    for shell in SHELLS {
        let (home, bin) = installed_home();
        let path = search_path(on_path.then_some(bin.as_path()));
        let invocation = shell.invocation(home.path(), &path);
        for (subcommand, answer) in subcommands {
            let command = guide_command(invocation, subcommand);
            let output = shell.run(home.path(), &path, &command);
            let text = format!(
                "{}{}",
                common::stdout(&output),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                text.contains(answer),
                "{}: `{command}` (exit {:?}): {text}",
                shell.name(),
                output.status.code()
            );
        }
    }
}

// An empty home has nothing set up: `setup --check` answering at all is what counts.
const UPDATE: [(&str, &str); 2] = [
    ("update", "is already up to date"),
    ("setup --check", "vault: "),
];

#[test]
fn an_install_on_path_updates_and_checks_with_the_bare_command() {
    follow(true, &UPDATE);
}

#[test]
fn an_install_off_path_updates_and_checks_with_the_full_path() {
    follow(false, &UPDATE);
}

// The install script prints this same full path when its folder isn't on PATH (tests/install.rs).
#[test]
fn a_fresh_install_off_path_sets_up_and_checks_with_the_full_path() {
    follow(false, &[("setup", "vault: "), ("setup --check", "vault: ")]);
}
