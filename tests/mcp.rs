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
    let (is_error, text) = client.call("memory_save", save.clone());
    assert!(is_error && text.contains("already exists"), "{text}");
    let mut symbols_only = save;
    symbols_only["title"] = json!("???");
    let (is_error, text) = client.call("memory_save", symbols_only);
    assert!(is_error && text.contains("title needs"), "{text}");

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
        review.contains("## Preferences\n- commit-rules | feedback | ")
            && review.contains(" | 1r "),
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
    let vault = dunce::canonicalize(vault).unwrap();
    assert_eq!(
        git_log(&vault),
        "create: Topics/homelab/nas-backups (test-agent)\n\
         delete: Preferences/commit-rules (test-agent)\n\
         move: Projects/my-repo/deploy-steps -> Projects/my-repo/release (test-agent)\n\
         create: Projects/my-repo/deploy-steps (test-agent)\n\
         cleanup: memory reviewed (test-agent)\n\
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

fn checkout(dir: &Path, remote: &str) {
    std::fs::create_dir_all(dir).unwrap();
    for args in [
        &["init", "--quiet"][..],
        &["remote", "add", "origin", remote],
    ] {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success());
    }
}

#[test]
fn repos_sharing_a_folder_name_keep_separate_projects() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let mine = home.path().join("mine/api");
    let theirs = home.path().join("theirs/api");
    checkout(&mine, "git@github.com:me/api.git");
    checkout(&theirs, "git@gitlab.com:someone-else/api.git");
    let (mut child, mut client, _) = start(home.path());
    let note = |dir: &Path, title: &str| {
        json!({
            "type": "project", "project_dir": dir, "title": title,
            "summary": format!("{title} summary"), "fact": "A fact.",
            "source": "test on 2026-09-30", "how_to_apply": "Test.",
        })
    };

    assert_eq!(
        client.call("memory_save", note(&mine, "Mine")).1,
        "saved Projects/api/mine"
    );
    assert_eq!(
        client.call("memory_save", note(&theirs, "Theirs")).1,
        "saved Projects/someone-else-api/theirs"
    );
    let (_, index) = client.call("memory_index", json!({"project_dir": theirs}));
    assert!(
        index.contains("[[Projects/someone-else-api/theirs]]") && !index.contains("Projects/api/"),
        "{index}"
    );

    drop(client);
    assert!(child.wait().unwrap().success());
}

// What Syncthing would bring to another machine: the files with their times, not git or petit-poucet's own state.
fn sync(from: &Path, to: &Path) {
    let entries = walkdir::WalkDir::new(from)
        .into_iter()
        .filter_entry(|e| ![".git", ".petit-poucet"].contains(&e.file_name().to_str().unwrap()));
    for entry in entries {
        let entry = entry.unwrap();
        let target = to.join(entry.path().strip_prefix(from).unwrap());
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
            let modified = entry.metadata().unwrap().modified().unwrap();
            let file = std::fs::File::options().write(true).open(&target).unwrap();
            file.set_modified(modified).unwrap();
        }
    }
}

// Reads every page of memory_review; returns the pages and the project note lines they hold (#77).
fn review_pages(client: &mut Client, full: bool) -> (Vec<String>, usize) {
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

// Lowers a limit in a config `init` wrote.
fn set_review_max_pages(home: &Path, pages: usize) {
    let path = home.join(".config/petit-poucet/config.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("review_max_pages = 10\n"), "{text}");
    let text = text.replace(
        "review_max_pages = 10\n",
        &format!("review_max_pages = {pages}\n"),
    );
    std::fs::write(&path, text).unwrap();
}

fn joined(pages: &[String]) -> String {
    pages.concat()
}

// Over 300 notes, a later cleanup sends preferences, changed and recently used folders whole, then the rest while pages last (#89).
#[test]
fn a_cleanup_reviews_the_whole_vault_first_then_folders_by_priority() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    for project in ["api", "web", "infra"] {
        let dir = vault.join("Projects").join(project);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("_project.md"),
            format!("---\ntype: project-identity\nremotes: []\nfolders: [{project}]\n---\n"),
        )
        .unwrap();
        for i in 0..100 {
            std::fs::write(
                dir.join(format!("fact-{i}.md")),
                format!("---\ntype: project\nscope: {project}\nsummary: fact {project}{i} detail{i}{project}\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n# Fact {i}\n\n**Why:** test.\n"),
            )
            .unwrap();
        }
    }
    let (mut child, mut client, _) = start(home.path());
    // Any change rewrites the Index, so the vault has no problem left.
    let (_, saved) = client.call(
        "memory_save",
        json!({"type": "user", "scope": "all repos", "title": "Tabs", "summary": "the user prefers tabs",
               "fact": "Tabs.", "source": "the user on 2026-10-04", "how_to_apply": "When indenting."}),
    );
    assert_eq!(saved, "saved Preferences/tabs");

    let (first, notes) = review_pages(&mut client, false);
    assert!(
        first[0].contains("\nfirst cleanup: all 301 notes"),
        "{}",
        first[0]
    );
    assert!(first.len() > 5, "{} pages", first.len());
    assert_eq!(notes, 300);
    assert!(
        !first.iter().any(|p| p.contains("## check")),
        "no problem in this vault"
    );
    // The cleanup is recorded in the vault and committed, so it syncs with the notes.
    assert!(vault.join(".last-cleanup").is_file());
    let log = git_log(&vault);
    assert!(
        log.starts_with("cleanup: memory reviewed (test-agent)\n"),
        "{log}"
    );

    // Another machine, with room for 3 pages, gets the vault through a sync that keeps file times, then edits one note.
    let other = TempDir::new().unwrap();
    let synced = other.path().join("vault");
    sync(&vault, &synced);
    petit_poucet(other.path())
        .args(["init", synced.to_str().unwrap()])
        .assert()
        .success();
    set_review_max_pages(other.path(), 3);
    let note = synced.join("Projects/infra/fact-3.md");
    let text = std::fs::read_to_string(&note).unwrap();
    std::fs::write(
        &note,
        text.replace("**Why:** test.", "**Why:** edited there."),
    )
    .unwrap();
    let (mut other_child, mut other_client, _) = start(other.path());
    let (there, notes) = review_pages(&mut other_client, false);
    let text = joined(&there);
    assert!(text.contains("\n101 of 301 notes: preferences, "), "{text}");
    assert_eq!(notes, 100, "the changed note's whole folder");
    assert!(text.contains("## Preferences\n- tabs | "), "{text}");
    assert!(text.contains("## Projects/infra\n"), "{text}");
    assert!(
        !text.contains("## Projects/api") && !text.contains("## Projects/web"),
        "no room left: {text}"
    );

    // A read makes its folder one of the recently used ones.
    other_client.call("memory_read", json!({"path": "Projects/web/fact-1"}));
    let (next, notes) = review_pages(&mut other_client, false);
    let text = joined(&next);
    assert_eq!(notes, 100);
    assert!(text.contains("## Projects/web\n"), "{text}");
    assert!(
        !text.contains("## Projects/infra"),
        "reviewed already: {text}"
    );

    // A whole review on demand.
    let (full, notes) = review_pages(&mut other_client, true);
    assert!(
        full[0].contains("\nall 301 notes, 0 changed since the last cleanup ("),
        "{}",
        full[0]
    );
    assert_eq!(notes, 300);
    drop(other_client);
    assert!(other_child.wait().unwrap().success());

    // With the default 10 pages, everything fits again.
    for file in ["Projects/api/fact-7.md", "Projects/web/fact-42.md"] {
        let path = vault.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            &path,
            text.replace("**Why:** test.", "**Why:** checked again."),
        )
        .unwrap();
    }
    let (second, notes) = review_pages(&mut client, false);
    assert!(
        second[0].contains("\nall 301 notes, 2 changed since the last cleanup ("),
        "{}",
        second[0]
    );
    assert_eq!(notes, 300);

    let (is_error, _) = client.call("memory_review", json!({"page": second.len() + 1}));
    assert!(is_error);

    // An agent saves what an old note already says: the review names the pair once (#36).
    let (_, saved) = client.call(
        "memory_save",
        json!({"type": "user", "scope": "all repos", "title": "API fact", "summary": "fact api7 detail7api again",
               "fact": "Again.", "source": "the user on 2026-10-04", "how_to_apply": "Never."}),
    );
    assert!(saved.starts_with("saved Preferences/api-fact"), "{saved}");
    let (third, _) = review_pages(&mut client, false);
    assert_eq!(
        joined(&third)
            .matches(
                "- Projects/api/fact-7.md: warning: near-duplicate of [[Preferences/api-fact]]"
            )
            .count(),
        1,
        "{third:?}"
    );
    drop(client);
    assert!(child.wait().unwrap().success());
}

// Usage lives in the vault: a read never commits alone, and every machine's reads add up (#88).
#[test]
fn usage_rides_with_the_next_commit_and_adds_up_across_machines() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let note = |title: &str| {
        json!({"type": "user", "scope": "all repos", "title": title, "summary": format!("about {title}"),
               "fact": "A fact.", "source": "test on 2026-10-04", "how_to_apply": "Test."})
    };
    let (mut child, mut client, _) = start(home.path());
    client.call("memory_save", note("tabs"));
    let commits = git_log(&vault).lines().count();
    client.call("memory_read", json!({"path": "Preferences/tabs"}));
    client.call("memory_read", json!({"path": "Preferences/tabs"}));
    assert_eq!(
        git_log(&vault).lines().count(),
        commits,
        "a read never commits"
    );
    client.call("memory_save", note("spaces"));
    let files = std::process::Command::new("git")
        .args(["show", "--name-only", "--format=", "HEAD"])
        .current_dir(&vault)
        .output()
        .unwrap();
    let files = String::from_utf8_lossy(&files.stdout).into_owned();
    assert!(
        files.contains(".usage/"),
        "usage rides with the save: {files}"
    );
    drop(client);
    assert!(child.wait().unwrap().success());

    // Another machine gets the vault by sync, reads the note once, and sees every machine's reads.
    let other = TempDir::new().unwrap();
    let synced = other.path().join("vault");
    sync(&vault, &synced);
    petit_poucet(other.path())
        .args(["init", synced.to_str().unwrap()])
        .assert()
        .success();
    let (mut other_child, mut other_client, _) = start(other.path());
    other_client.call("memory_read", json!({"path": "Preferences/tabs"}));
    let (pages, _) = review_pages(&mut other_client, false);
    assert!(pages[0].contains("- tabs | user | 2026-"), "{}", pages[0]);
    assert!(
        pages[0].contains(" | 3r "),
        "2 reads there + 1 here: {}",
        pages[0]
    );
    // A small vault: a later cleanup still lists every note, changed or not (#89).
    let (again, _) = review_pages(&mut other_client, false);
    assert!(
        again[0].contains("\nall 2 notes, 0 changed since the last cleanup (")
            && again[0].contains("- tabs | ")
            && again[0].contains("- spaces | "),
        "{}",
        again[0]
    );
    assert_eq!(std::fs::read_dir(synced.join(".usage")).unwrap().count(), 2);
    drop(other_client);
    assert!(other_child.wait().unwrap().success());
}

#[test]
fn two_agents_writing_at_once_lose_nothing() {
    const EACH: usize = 15;
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let note = |title: String| {
        json!({
            "type": "reference", "scope": "all repos", "title": title,
            "summary": format!("about {title}"), "fact": "A fact.",
            "source": "test on 2026-09-30", "how_to_apply": "Test.",
        })
    };
    let (mut child, mut client, _) = start(home.path());
    client.call("memory_save", note("shared".into()));
    drop(client);
    child.wait().unwrap();

    let agents: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|agent| {
            let home = home.path().to_path_buf();
            std::thread::spawn(move || {
                let (mut child, mut client, _) = start(&home);
                for i in 0..EACH {
                    // One word each, so saves never report each other as similar.
                    let title = format!("{agent}{i}");
                    let (_, reply) = client.call("memory_save", note(title));
                    assert_eq!(reply, format!("saved Preferences/{agent}{i}"));
                    client.call("memory_read", json!({"path": "Preferences/shared"}));
                }
                drop(client);
                assert!(child.wait().unwrap().success());
            })
        })
        .collect();
    for agent in agents {
        agent.join().unwrap();
    }

    let vault = dunce::canonicalize(vault).unwrap();
    assert_eq!(git_log(&vault).lines().count(), 2 + 2 * EACH);
    let index = std::fs::read_to_string(vault.join("Index.md")).unwrap();
    assert_eq!(index.matches("- [[Preferences/").count(), 1 + 2 * EACH);
    // One machine, so one usage file in the vault.
    let files: Vec<_> = std::fs::read_dir(vault.join(".usage")).unwrap().collect();
    assert_eq!(files.len(), 1);
    let usage = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
    let usage: Value = serde_json::from_str(&usage).unwrap();
    assert_eq!(usage["notes"]["Preferences/shared"]["reads"], 2 * EACH);
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&vault)
        .output()
        .unwrap();
    // A read never commits on its own: only the last reads' usage may wait for the next commit.
    let pending: Vec<String> = String::from_utf8_lossy(&status.stdout)
        .lines()
        .filter(|l| !l.contains(".usage/"))
        .map(str::to_string)
        .collect();
    assert!(pending.is_empty(), "all committed: {pending:?}");
}

#[test]
fn an_update_keeps_tags_and_is_committed_as_an_update() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    std::fs::write(
        vault.join("Preferences/editor.md"),
        "---\ntype: user\nscope: all repos\nsummary: s\ncreated: 2026-09-14\ntags: [agent-memory, from-obsidian]\n---\n\n# Editor\n",
    )
    .unwrap();
    std::fs::write(vault.join("Preferences/broken.md"), "no frontmatter\n").unwrap();
    let (mut child, mut client, _) = start(home.path());
    for (path, title) in [
        ("Preferences/editor", "Editor"),
        ("Preferences/broken", "Broken"),
    ] {
        client.call("memory_read", json!({"path": path}));
        let (is_error, text) = client.call(
            "memory_save",
            json!({
                "path": path, "type": "user", "title": title,
                "summary": "the user edits notes in Obsidian", "fact": "A fact.",
                "source": "test on 2026-09-30", "how_to_apply": "Test.",
            }),
        );
        assert!(!is_error, "{text}");
    }
    let (_, editor) = client.call("memory_read", json!({"path": "Preferences/editor"}));
    assert!(editor.contains("- from-obsidian\n"), "{editor}");

    drop(client);
    assert!(child.wait().unwrap().success());
    let log = git_log(&dunce::canonicalize(vault).unwrap());
    assert!(
        log.starts_with(
            "update: Preferences/broken (test-agent)\nupdate: Preferences/editor (test-agent)\n"
        ),
        "{log}"
    );
}

#[test]
fn search_finds_two_letter_names_as_whole_words() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let (mut child, mut client, _) = start(home.path());
    client.call(
        "memory_save",
        json!({
            "type": "reference", "scope": "all repos", "title": "CI runners",
            "summary": "CI runs on self-hosted runners, see the feedback channel", "fact": "Self-hosted.",
            "source": "test on 2026-09-30", "how_to_apply": "When CI is slow.",
        }),
    );
    let search = |client: &mut Client, query: &str| {
        client
            .call(
                "memory_search",
                json!({"query": query, "project_dir": home.path()}),
            )
            .1
    };
    assert!(search(&mut client, "CI").starts_with("- [[Preferences/ci-runners]]"));
    assert_eq!(search(&mut client, "db"), "no matches");
    assert_eq!(search(&mut client, "is it on"), "no matches");

    drop(client);
    assert!(child.wait().unwrap().success());
}
