#![allow(dead_code, reason = "each test file uses a different subset")]

use std::fs;
use std::path::Path;

use assert_cmd::Command;

pub const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/vault");

// Windows needs these to start processes and find its temp folder.
const SYSTEM: [&str; 6] = ["PATH", "SystemRoot", "ComSpec", "PATHEXT", "TEMP", "TMP"];

// A fake home (HOME, USERPROFILE on Windows) keeps the real config untouched, git's global config included.
// Tests stay offline: no update check unless a test turns it back on.
pub fn command(home: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_petit-poucet"));
    cmd.env_clear();
    for key in SYSTEM {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .env("PETIT_POUCET_NO_UPDATE_CHECK", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");
    cmd
}

pub fn petit_poucet(home: &Path) -> Command {
    Command::from_std(command(home))
}

pub fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn git_log(vault: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["-C", vault.to_str().unwrap(), "log", "--format=%s"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn copy_dir(from: &Path, to: &Path) {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.unwrap();
        let target = to.join(entry.path().strip_prefix(from).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}
