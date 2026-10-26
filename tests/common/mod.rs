#![allow(dead_code, reason = "each test file uses a different subset")]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Stdio};

use assert_cmd::Command;
use serde_json::{Value, json};

pub const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/vault");

// Windows needs these to start processes and find its temp folder.
const SYSTEM: [&str; 6] = ["PATH", "SystemRoot", "ComSpec", "PATHEXT", "TEMP", "TMP"];

// A fake home (HOME, USERPROFILE on Windows) keeps the real config untouched, git's global config included.
// Tests stay offline: no update check unless a test turns it back on.
pub fn command(home: &Path) -> std::process::Command {
    command_for(Path::new(env!("CARGO_BIN_EXE_petit-poucet")), home)
}

// The same, for a copy of the binary.
pub fn command_for(exe: &Path, home: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
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

pub struct Client {
    stdin: std::process::ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    pub fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").unwrap();
    }

    // Every stdout line must be a JSON-RPC message: logs on stdout would break the protocol.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).unwrap();
            let message: Value = serde_json::from_str(&line).expect("stdout carries only JSON-RPC");
            if message["id"] == id {
                return message["result"].clone();
            }
        }
    }

    pub fn call(&mut self, tool: &str, arguments: Value) -> (bool, String) {
        let result = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        let text = result["content"][0]["text"].as_str().unwrap().to_string();
        (result["isError"] == true, text)
    }
}

// One agent session: a `serve` process after the MCP handshake.
pub fn start(home: &Path) -> (Child, Client, Value) {
    let mut child = command(home)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut client = Client {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()),
        next_id: 0,
    };
    let init = client.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test-agent", "version": "1"}}),
    );
    client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    (child, client, init)
}

// Reads every page of memory_review; returns the pages and the project note lines they hold (#77).
pub fn review_pages(client: &mut Client, full: bool) -> (Vec<String>, usize) {
    let mut pages = Vec::new();
    loop {
        let page = pages.len() + 1;
        let (is_error, text) = client.call("memory_review", json!({"page": page, "full": full}));
        // Antigravity saves a tool result over about 4 KB to a file the agent has to page through.
        assert!(
            !is_error && text.len() <= 3500,
            "page {page}: {} bytes",
            text.len()
        );
        let more = text.contains(&format!("more: call memory_review with page={}", page + 1));
        pages.push(text);
        if !more {
            let notes = pages
                .iter()
                .map(|p| p.matches(" | project | ").count())
                .sum();
            return (pages, notes);
        }
    }
}

pub fn session_start(home: &Path, agent: &str) -> (String, String) {
    let event = json!({"cwd": home, "session_id": format!("{agent}-1"), "source": "startup"});
    let mut child = command(home)
        .args(["hook", "session-start", "--agent", agent])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "{event}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    (
        stdout(&output),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

pub fn git_status(vault: &Path) -> String {
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(vault)
        .output()
        .unwrap();
    String::from_utf8_lossy(&status.stdout).into_owned()
}
