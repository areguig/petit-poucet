mod common;

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Stdio};

use common::{command, git_log, petit_poucet};
use serde_json::{Value, json};
use tempfile::TempDir;

struct Client {
    stdin: std::process::ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").unwrap();
    }

    // Every stdout line must be a JSON-RPC message: logs on stdout would break the protocol.
    fn request(&mut self, method: &str, params: Value) -> Value {
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

    fn call(&mut self, tool: &str, arguments: Value) -> (bool, String) {
        let result = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        let text = result["content"][0]["text"].as_str().unwrap().to_string();
        (result["isError"] == true, text)
    }
}

// One agent session: a `serve` process after the MCP handshake.
fn start(home: &Path) -> (Child, Client) {
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
    client.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test-agent", "version": "1"}}),
    );
    client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    (child, client)
}

#[test]
fn an_agent_saves_finds_and_reads_a_note() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();

    let (mut child, mut client) = start(home.path());

    let tools = client.request("tools/list", json!({}));
    let mut names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "memory_delete",
            "memory_index",
            "memory_move",
            "memory_read",
            "memory_save",
            "memory_search"
        ]
    );

    assert_eq!(
        client.call("memory_index", json!({})),
        (false, "no notes yet".into())
    );

    let save = json!({
        "type": "feedback", "scope": "all repos", "title": "Commit rules",
        "summary": "commit locally per step, never push", "fact": "Commit each step locally.",
        "source": "the user said so on 2026-09-24", "how_to_apply": "After each step.",
    });
    assert_eq!(
        client.call("memory_save", save.clone()),
        (false, "saved Preferences/commit-rules".into())
    );
    let (is_error, text) = client.call("memory_save", save);
    assert!(is_error && text.contains("already exists"), "{text}");

    assert_eq!(
        client.call("memory_index", json!({})).1,
        "## Preferences (all repos)\n- [[Preferences/commit-rules]] — commit locally per step, never push"
    );
    assert_eq!(
        client.call("memory_search", json!({"query": "push"})).1,
        "- [[Preferences/commit-rules]] — commit locally per step, never push"
    );
    assert_eq!(
        client.call("memory_search", json!({"query": "database"})).1,
        "no matches"
    );
    let (_, note) = client.call("memory_read", json!({"path": "Preferences/commit-rules"}));
    assert!(
        note.contains("**Why:** the user said so on 2026-09-24"),
        "{note}"
    );
    let (is_error, _) = client.call("memory_read", json!({"path": "../../etc/passwd"}));
    assert!(is_error);

    let repo = home.path().join("my-repo");
    std::fs::create_dir(&repo).unwrap();
    let project_note = json!({
        "type": "project", "project_dir": repo, "title": "Deploy steps",
        "summary": "deploy with make release", "fact": "Run make release.",
        "source": "verified with make -n release on 2026-09-24", "how_to_apply": "When deploying.",
    });
    assert_eq!(
        client.call("memory_save", project_note).1,
        "saved Projects/my-repo/deploy-steps"
    );
    let (_, here) = client.call("memory_index", json!({"project_dir": repo}));
    assert!(
        here.contains("[[Preferences/commit-rules]]")
            && here.contains("[[Projects/my-repo/deploy-steps]]"),
        "{here}"
    );
    let elsewhere = json!({"query": "deploy", "project_dir": home.path()});
    assert_eq!(client.call("memory_search", elsewhere).1, "no matches");
    let (_, found) = client.call(
        "memory_search",
        json!({"query": "deploy", "project_dir": repo}),
    );
    assert_eq!(
        found,
        "- [[Projects/my-repo/deploy-steps]] — deploy with make release"
    );

    let (_, moved) = client.call(
        "memory_move",
        json!({"path": "Projects/my-repo/deploy-steps", "new_path": "Projects/my-repo/release"}),
    );
    assert_eq!(
        moved,
        "moved Projects/my-repo/deploy-steps to Projects/my-repo/release"
    );
    let (is_error, text) = client.call(
        "memory_delete",
        json!({"path": "Preferences/commit-rules", "reason": "test"}),
    );
    assert!(is_error && text.contains("user_confirmed"), "{text}");
    let (_, deleted) = client.call(
        "memory_delete",
        json!({"path": "Preferences/commit-rules", "reason": "replaced", "user_confirmed": true}),
    );
    assert_eq!(deleted, "deleted Preferences/commit-rules");

    drop(client);
    assert!(child.wait().unwrap().success());
    let vault = vault.canonicalize().unwrap();
    assert_eq!(
        git_log(&vault),
        "delete: Preferences/commit-rules (test-agent)\n\
         move: Projects/my-repo/deploy-steps -> Projects/my-repo/release (test-agent)\n\
         create: Projects/my-repo/deploy-steps (test-agent)\n\
         create: Preferences/commit-rules (test-agent)\n\
         init: vault\n"
    );
}

#[test]
fn a_write_never_replaces_text_the_agent_has_not_seen() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let (mut child, mut client) = start(home.path());
    let note = |fact: &str| {
        json!({
            "path": "Preferences/editor", "type": "user", "title": "Editor",
            "summary": "the user edits notes in Obsidian", "fact": fact,
            "source": "the user said so on 2026-09-24", "how_to_apply": "Expect edits.",
        })
    };
    let mut create = note("Uses Obsidian.");
    create.as_object_mut().unwrap().remove("path");
    create["scope"] = json!("all repos");
    client.call("memory_save", create);
    let (is_error, _) = client.call("memory_save", note("Uses Obsidian daily."));
    assert!(!is_error, "the agent wrote it, so it has seen it");

    let file = vault.join("Preferences/editor.md");
    let edited = std::fs::read_to_string(&file)
        .unwrap()
        .replace("daily", "every day");
    std::fs::write(&file, edited).unwrap();
    for (tool, args) in [
        ("memory_save", note("Overwrite.")),
        (
            "memory_delete",
            json!({"path": "Preferences/editor", "reason": "test"}),
        ),
    ] {
        let (is_error, text) = client.call(tool, args);
        assert!(
            is_error && text.contains("changed since you read it"),
            "{tool}: {text}"
        );
    }
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("every day")
    );

    client.call("memory_read", json!({"path": "Preferences/editor"}));
    assert!(
        !client
            .call("memory_save", note("Uses Obsidian every day."))
            .0
    );

    let (mut other_child, mut other) = start(home.path());
    let (is_error, text) = other.call("memory_save", note("Other agent."));
    assert!(
        is_error && text.contains("read Preferences/editor first"),
        "{text}"
    );

    drop(client);
    drop(other);
    assert!(child.wait().unwrap().success() && other_child.wait().unwrap().success());
}
