mod common;

use std::fs;
use std::path::Path;

use common::{FIXTURE, copy_dir, git_log, petit_poucet, stdout};
use tempfile::TempDir;

#[test]
fn check_reports_every_bad_note_of_the_fixture() {
    let home = TempDir::new().unwrap();
    let output = petit_poucet(home.path())
        .env("PETIT_POUCET_VAULT", FIXTURE)
        .arg("check")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        stdout(&output),
        "\
Index.md: error: does not list [[Projects/beta/too-long]]
Index.md: error: lists missing note [[Preferences/deleted-note]]
Preferences/bad-type.md: error: frontmatter: line 1 column 7: unknown variant `opinion`, expected one of user, feedback, project, reference
Preferences/no-summary.md: error: missing summary
Preferences/wrong-scope.md: error: scope is `alpha`, expected `all repos`
Projects/alpha/_project.md: error: remote github.com/example/alpha is claimed by projects alpha, delta
Projects/alpha/broken-link.md: error: broken link [[Projects/alpha/nowhere]]
Projects/alpha/missing-tag.md: error: tags must include `agent-memory`
Projects/alpha/no-why.md: error: missing **Why:** line
Projects/beta/bad-dates.md: error: updated is before created
Projects/beta/bad-yaml.md: error: frontmatter: line 3 column 10: expected string scalar
Projects/beta/no-frontmatter.md: error: frontmatter: no frontmatter
Projects/beta/secret.md: error: looks like a secret (AWS access key)
Projects/beta/too-long.md: warning: longer than 1500 characters: one short fact per note
Projects/delta/_project.md: error: remote github.com/example/alpha is claimed by projects alpha, delta
Projects/gamma/_project.md: error: missing
scratch.md: error: not in Preferences/ or Projects/<project>/
notes: 18, errors: 16, warnings: 1
"
    );
}

#[test]
fn check_without_a_configured_vault_says_how_to_create_one() {
    let home = TempDir::new().unwrap();
    let output = petit_poucet(home.path()).arg("check").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("run `petit-poucet init <path>`"));
}

#[test]
fn init_creates_a_valid_vault_a_config_and_a_commit() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();

    assert!(vault.join("Preferences").is_dir() && vault.join("Projects").is_dir());
    let config = fs::read_to_string(home.path().join(".config/petit-poucet/config.toml")).unwrap();
    let vault = vault.canonicalize().unwrap();
    assert!(
        config.contains(&format!("vault = \"{}\"", vault.display())),
        "{config}"
    );
    assert!(config.contains("git_autocommit = true"), "{config}");
    assert_eq!(git_log(&vault), "init: vault\n");

    let output = petit_poucet(home.path()).arg("check").output().unwrap();
    assert!(output.status.success());
    assert_eq!(stdout(&output), "notes: 0, errors: 0, warnings: 0\n");
}

#[test]
fn init_on_an_existing_vault_regenerates_the_index_and_keeps_the_notes() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    copy_dir(Path::new(FIXTURE), &vault);
    let note = vault.join("Preferences/user-commits-themselves.md");
    let before = fs::read_to_string(&note).unwrap();

    for _ in 0..2 {
        petit_poucet(home.path())
            .args(["init", vault.to_str().unwrap()])
            .assert()
            .success();
    }

    assert_eq!(fs::read_to_string(&note).unwrap(), before);
    let index = fs::read_to_string(vault.join("Index.md")).unwrap();
    assert!(index.contains("- [[Projects/beta/too-long]] — several facts in one note"));
    assert!(!index.contains("deleted-note"));
    assert_eq!(
        git_log(&vault),
        "init: vault\n",
        "a second init has nothing to commit"
    );
}

#[test]
fn init_refuses_to_switch_the_configured_vault() {
    let home = TempDir::new().unwrap();
    let init = |name: &str| {
        petit_poucet(home.path())
            .args(["init", home.path().join(name).to_str().unwrap()])
            .output()
            .unwrap()
    };
    assert!(init("first").status.success());
    let second = init("second");
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("already points to"));
}

#[test]
fn env_var_overrides_the_configured_vault() {
    let home = TempDir::new().unwrap();
    petit_poucet(home.path())
        .args(["init", home.path().join("configured").to_str().unwrap()])
        .assert()
        .success();
    let output = petit_poucet(home.path())
        .env("PETIT_POUCET_VAULT", FIXTURE)
        .arg("check")
        .output()
        .unwrap();
    assert!(stdout(&output).ends_with("notes: 18, errors: 16, warnings: 1\n"));
}

#[test]
fn init_does_not_commit_when_autocommit_is_off() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let config_dir = home.path().join(".config/petit-poucet");
    fs::create_dir_all(&config_dir).unwrap();
    let vault = vault.canonicalize().unwrap();
    fs::write(
        config_dir.join("config.toml"),
        format!("vault = \"{}\"\ngit_autocommit = false\n", vault.display()),
    )
    .unwrap();

    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    assert!(vault.join("Index.md").is_file());
    assert_eq!(git_log(&vault), "");
}

#[test]
fn migrate_upgrades_a_hand_maintained_vault_once() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    copy_dir(&Path::new(FIXTURE).with_file_name("legacy-vault"), &vault);
    let migrate = || {
        petit_poucet(home.path())
            .env("PETIT_POUCET_VAULT", &vault)
            .arg("migrate")
            .output()
            .unwrap()
    };

    let first = migrate();
    assert!(first.status.success());
    assert_eq!(
        stdout(&first),
        format!(
            "migrated {}: 4 summaries added, 2 links rewritten, 2 projects identified\n\
             Projects/beta/unlisted.md: no Index line to take a summary from\n",
            vault.display()
        )
    );
    let read = |path: &str| fs::read_to_string(vault.join(path)).unwrap();
    assert!(
        read("Preferences/commit-rules.md")
            .contains("summary: \"commit locally per step: never push\"")
    );
    assert!(
        read("Preferences/commit-rules.md")
            .contains("[[Projects/alpha/design|the design]] and [[Preferences/setup]]")
    );
    assert!(read("Projects/alpha/design.md").contains("[[Preferences/commit-rules#Details]]"));
    assert!(read("Projects/alpha/setup.md").contains("Ambiguous [[setup]]"));
    assert!(read("Projects/alpha/_project.md").contains("type: project-identity"));
    assert_eq!(read(".gitignore"), ".obsidian/\n.trash/\n.DS_Store\n");

    let check = petit_poucet(home.path())
        .env("PETIT_POUCET_VAULT", &vault)
        .arg("check")
        .output()
        .unwrap();
    assert_eq!(
        stdout(&check),
        "Projects/alpha/setup.md: error: broken link [[setup]]\n\
         Projects/beta/unlisted.md: error: missing summary\n\
         notes: 5, errors: 2, warnings: 0\n"
    );

    assert!(
        stdout(&migrate()).contains("0 summaries added, 0 links rewritten, 0 projects identified")
    );
    assert_eq!(git_log(&vault), "migrate: vault\n");
}

fn hook(home: &Path, args: &[&str], input: &str) -> String {
    let output = petit_poucet(home)
        .arg("hook")
        .args(args)
        .write_stdin(input)
        .output()
        .unwrap();
    assert!(output.status.success(), "hooks never fail the session");
    stdout(&output)
}

#[test]
fn session_start_injects_rules_and_the_project_index() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    copy_dir(Path::new(FIXTURE), &vault);
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let alpha_checkout = home.path().join("alpha");
    fs::create_dir(&alpha_checkout).unwrap();
    let event = serde_json::json!({"cwd": alpha_checkout}).to_string();

    let claude: serde_json::Value = serde_json::from_str(&hook(
        home.path(),
        &["session-start", "--agent", "claude"],
        &event,
    ))
    .unwrap();
    assert_eq!(
        claude["hookSpecificOutput"]["hookEventName"],
        "SessionStart"
    );
    let context = claude["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(
        context.starts_with("Agent memory (petit-poucet)"),
        "{context}"
    );
    assert!(context.contains("Older file-based memory"), "{context}");
    assert!(context.contains("[[Preferences/user-commits-themselves]]"));
    assert!(context.contains("[[Projects/alpha/plugin-design]]"));
    assert!(!context.contains("Projects/beta"));

    let copilot: serde_json::Value = serde_json::from_str(&hook(
        home.path(),
        &["session-start", "--agent", "copilot"],
        &event,
    ))
    .unwrap();
    assert_eq!(copilot["additionalContext"].as_str(), Some(context));
    assert_eq!(
        claude["systemMessage"],
        "🪨 petit-poucet · 10 notes loaded (preferences + alpha)"
    );
    assert!(
        copilot.get("systemMessage").is_none(),
        "Copilot has no user message"
    );
}

#[test]
fn session_start_says_when_memory_is_unavailable() {
    let home = TempDir::new().unwrap();
    let broken = home.path().join("gone");
    let output = petit_poucet(home.path())
        .env("PETIT_POUCET_VAULT", &broken)
        .args(["hook", "session-start", "--agent", "copilot"])
        .output()
        .unwrap();
    let context: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let context = context["additionalContext"].as_str().unwrap();
    assert!(
        context.starts_with("Agent memory is UNAVAILABLE (vault not found"),
        "{context}"
    );
}

#[test]
fn session_start_without_a_vault_offers_to_create_one() {
    let home = TempDir::new().unwrap();
    let output = petit_poucet(home.path())
        .env(
            "PETIT_POUCET_LAUNCHER",
            "/plugins/petit-poucet/bin/petit-poucet",
        )
        .args(["hook", "session-start", "--agent", "claude"])
        .write_stdin("not json")
        .output()
        .unwrap();
    let context: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let context = context["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("has no vault yet"), "{context}");
    let message: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        message["systemMessage"],
        "🪨 petit-poucet · no vault yet: the agent will offer to create one"
    );
    assert!(
        context.contains("`/plugins/petit-poucet/bin/petit-poucet init`"),
        "{context}"
    );
}

#[test]
fn stop_reminds_at_the_third_stop_then_every_tenth() {
    let home = TempDir::new().unwrap();
    let session = home
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let event = serde_json::json!({"sessionId": session}).to_string();
    let reminded: Vec<u32> = (1..=23)
        .filter(|_| !hook(home.path(), &["stop", "--agent", "copilot"], &event).is_empty())
        .collect();
    assert_eq!(reminded, [3, 13, 23]);

    let active = serde_json::json!({"session_id": session, "stop_hook_active": true}).to_string();
    assert_eq!(
        hook(home.path(), &["stop", "--agent", "claude"], &active),
        ""
    );
    let _ = fs::remove_file(std::env::temp_dir().join(format!("petit-poucet-stops-{session}")));
}

#[test]
fn stop_reminder_shows_a_line_to_claude_code_users_only() {
    let home = TempDir::new().unwrap();
    for agent in ["claude", "copilot"] {
        let session = format!(
            "{}-{agent}",
            home.path().file_name().unwrap().to_str().unwrap()
        );
        let event = serde_json::json!({"session_id": session}).to_string();
        let stop = || hook(home.path(), &["stop", "--agent", agent], &event);
        let _ = (stop(), stop());
        let third = stop();
        let reminder: serde_json::Value = serde_json::from_str(&third).unwrap();
        assert_eq!(reminder["decision"], "block");
        let expected = (agent == "claude")
            .then_some("🪨 petit-poucet · checking whether this session is worth remembering");
        assert_eq!(reminder["systemMessage"].as_str(), expected, "{agent}");
        let _ = fs::remove_file(std::env::temp_dir().join(format!("petit-poucet-stops-{session}")));
    }
}

#[test]
fn init_without_a_path_uses_agent_memory_in_home_and_says_how_to_change_it() {
    let home = TempDir::new().unwrap();
    let output = petit_poucet(home.path()).arg("init").output().unwrap();
    assert!(output.status.success());
    let vault = home.path().canonicalize().unwrap().join("agent-memory");
    assert!(vault.join("Index.md").is_file());
    let config = home.path().join(".config/petit-poucet/config.toml");
    assert_eq!(
        stdout(&output),
        format!(
            "vault ready at {} (0 notes)\nto use another folder, change `vault` in {}\n",
            vault.display(),
            config.display()
        )
    );
}
