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
fn start(home: &Path) -> (Child, Client, Value) {
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

#[test]
fn an_agent_saves_finds_and_reads_a_note() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();

    let (mut child, mut client, init) = start(home.path());
    let instructions = init["instructions"].as_str().unwrap();
    assert!(instructions.contains("call memory_index"), "{instructions}");

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
            "memory_review",
            "memory_save",
            "memory_search"
        ]
    );

    let (is_error, missing) = client.call("memory_index", json!({}));
    assert!(
        is_error && missing.contains("missing field `project_dir`"),
        "{missing}"
    );
    let (_, empty) = client.call("memory_index", json!({"project_dir": home.path()}));
    assert!(
        empty.starts_with("Agent memory (petit-poucet)") && empty.ends_with("\n\nno notes yet"),
        "rules come with the Index for clients without hooks: {empty}"
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

    assert!(client.call("memory_index", json!({"project_dir": home.path()})).1.ends_with(
        "\n\n## Preferences (all repos)\n- [[Preferences/commit-rules]] — commit locally per step, never push"
    ));
    assert_eq!(
        client
            .call(
                "memory_search",
                json!({"query": "push", "project_dir": home.path()})
            )
            .1,
        "- [[Preferences/commit-rules]] — commit locally per step, never push"
    );
    assert_eq!(
        client
            .call(
                "memory_search",
                json!({"query": "database", "project_dir": home.path()})
            )
            .1,
        "no matches"
    );
    let (_, note) = client.call("memory_read", json!({"path": "Preferences/commit-rules"}));
    assert!(
        note.contains("**Why:** the user said so on 2026-09-24"),
        "{note}"
    );
    let (_, review) = client.call("memory_review", json!({}));
    assert!(
        review.contains("- Preferences/commit-rules | feedback | ")
            && review.contains(" | 1 reads, last "),
        "{review}"
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
    // From a folder that is no known project, every project's notes are searched.
    let elsewhere = json!({"query": "deploy", "project_dir": home.path()});
    assert_eq!(
        client.call("memory_search", elsewhere).1,
        "- [[Projects/my-repo/deploy-steps]] — deploy with make release"
    );
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

    let topic_note = json!({
        "type": "reference", "topic": "homelab", "title": "NAS backups",
        "summary": "nightly NAS backups go to the offsite disk", "fact": "Nightly at 02:00.",
        "source": "the user said so on 2026-09-24", "how_to_apply": "When touching backups.",
    });
    assert_eq!(
        client.call("memory_save", topic_note).1,
        "saved Topics/homelab/nas-backups"
    );
    let (_, found) = client.call(
        "memory_search",
        json!({"query": "backups", "project_dir": repo}),
    );
    assert_eq!(
        found,
        "- [[Topics/homelab/nas-backups]] — nightly NAS backups go to the offsite disk"
    );
    let (_, index) = client.call("memory_index", json!({"project_dir": repo}));
    assert!(
        index.ends_with("homelab (1)") && !index.contains("[[Topics/"),
        "{index}"
    );
    let (_, topic) = client.call(
        "memory_index",
        json!({"topic": "homelab", "project_dir": repo}),
    );
    assert_eq!(
        topic,
        "## Topics / homelab\n- [[Topics/homelab/nas-backups]] — nightly NAS backups go to the offsite disk"
    );

    drop(client);
    assert!(child.wait().unwrap().success());
    let vault = vault.canonicalize().unwrap();
    assert_eq!(
        git_log(&vault),
        "create: Topics/homelab/nas-backups (test-agent)\n\
         delete: Preferences/commit-rules (test-agent)\n\
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
    let (mut child, mut client, _) = start(home.path());
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

    let (mut other_child, mut other, _) = start(home.path());
    let (is_error, text) = other.call("memory_save", note("Other agent."));
    assert!(
        is_error && text.contains("read Preferences/editor first"),
        "{text}"
    );

    drop(client);
    drop(other);
    assert!(child.wait().unwrap().success() && other_child.wait().unwrap().success());
}

#[test]
fn a_session_is_not_blocked_by_its_own_link_rewrites() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let (mut child, mut client, _) = start(home.path());
    let note = |title: &str, fact: &str| {
        json!({
            "type": "reference", "scope": "all repos", "title": title,
            "summary": format!("{title} summary"), "fact": fact,
            "source": "test on 2026-09-24", "how_to_apply": "Test.",
        })
    };
    client.call("memory_save", note("Target", "The target."));
    client.call("memory_save", note("Linker", "See [[Preferences/target]]."));
    client.call("memory_save", note("Unseen", "See [[Preferences/target]]."));
    // A new session: the only notes it has seen are the ones it reads.
    drop(client);
    child.wait().unwrap();
    let (mut child, mut client, _) = start(home.path());
    client.call("memory_read", json!({"path": "Preferences/linker"}));

    let (_, moved) = client.call(
        "memory_move",
        json!({"path": "Preferences/target", "new_path": "Preferences/renamed"}),
    );
    assert!(moved.contains("links updated in"), "{moved}");

    let mut update = note("Linker", "See [[Preferences/renamed]], updated.");
    update["path"] = json!("Preferences/linker");
    let (is_error, text) = client.call("memory_save", update);
    assert!(!is_error, "{text}");
    let mut unseen = note("Unseen", "Changed.");
    unseen["path"] = json!("Preferences/unseen");
    let (is_error, text) = client.call("memory_save", unseen);
    assert!(
        is_error && text.contains("read Preferences/unseen first"),
        "{text}"
    );

    drop(client);
    assert!(child.wait().unwrap().success());
}
