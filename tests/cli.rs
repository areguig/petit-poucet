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
Topics/homelab/wrong-scope.md: error: scope is `all repos`, expected `homelab`
scratch.md: error: not in Preferences/, Projects/<project>/ or Topics/<topic>/
notes: 20, errors: 17, warnings: 1
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
    let vault = dunce::canonicalize(vault).unwrap();
    let config: toml::Table = toml::from_str(&config).unwrap();
    assert_eq!(config["vault"].as_str(), vault.to_str());
    assert_eq!(config["git_autocommit"].as_bool(), Some(true));
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
    assert!(stdout(&output).ends_with("notes: 20, errors: 17, warnings: 1\n"));
}

#[test]
fn init_does_not_commit_when_autocommit_is_off() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let config_dir = home.path().join(".config/petit-poucet");
    fs::create_dir_all(&config_dir).unwrap();
    let vault = dunce::canonicalize(vault).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        format!(
            "vault = {}\ngit_autocommit = false\n",
            toml::Value::from(vault.to_str().unwrap())
        ),
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
    assert_eq!(
        read(".gitignore"),
        ".obsidian/\n.trash/\n.DS_Store\n.petit-poucet/\n"
    );

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

// Stop counters go to the temp folder: one per test.
fn hook(home: &Path, args: &[&str], input: &str) -> String {
    let output = petit_poucet(home)
        .env("TMPDIR", home)
        .env("TMP", home)
        .env("TEMP", home)
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
    assert!(!context.contains("[[Topics/"), "topic notes are not loaded");
    assert!(
        context.ends_with("memory_index with `topic` lists one): homelab (2)\n"),
        "{context}"
    );

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
fn session_start_ignores_a_project_of_another_repo_with_the_same_folder_name() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    copy_dir(Path::new(FIXTURE), &vault);
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    // The fixture's `alpha` is github.com/example/alpha.
    let other_alpha = home.path().join("alpha");
    fs::create_dir(&other_alpha).unwrap();
    for args in [
        &["init", "--quiet"][..],
        &[
            "remote",
            "add",
            "origin",
            "git@gitlab.com:someone-else/alpha.git",
        ],
    ] {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&other_alpha)
            .status()
            .unwrap();
        assert!(status.success());
    }
    let event = serde_json::json!({"cwd": other_alpha}).to_string();
    let claude: serde_json::Value = serde_json::from_str(&hook(
        home.path(),
        &["session-start", "--agent", "claude"],
        &event,
    ))
    .unwrap();
    let context = claude["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(!context.contains("Projects/alpha"), "{context}");
    assert!(
        claude["systemMessage"]
            .as_str()
            .unwrap()
            .ends_with("(preferences)")
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
}

#[test]
fn stop_never_reminds_without_a_session_id_and_keeps_counters_in_their_folder() {
    let home = TempDir::new().unwrap();
    for event in ["{}", r#"{"session_id": "../"}"#, "not json"] {
        let stops: Vec<String> = (1..=5)
            .map(|_| hook(home.path(), &["stop", "--agent", "claude"], event))
            .collect();
        assert!(stops.iter().all(String::is_empty), "{event}");
    }
    let event = serde_json::json!({"session_id": "../escape"}).to_string();
    hook(home.path(), &["stop", "--agent", "claude"], &event);
    assert_eq!(
        fs::read_to_string(home.path().join("petit-poucet-stops/escape")).unwrap(),
        "1"
    );
    assert!(!home.path().parent().unwrap().join("escape").exists());
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
    }
}

#[test]
fn init_without_a_path_uses_agent_memory_in_home_and_says_how_to_change_it() {
    let home = TempDir::new().unwrap();
    let output = petit_poucet(home.path()).arg("init").output().unwrap();
    assert!(output.status.success());
    let vault = dunce::canonicalize(home.path())
        .unwrap()
        .join("agent-memory");
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

fn setup(home: &Path, args: &[&str]) -> (bool, String) {
    let output = petit_poucet(home).arg("setup").args(args).output().unwrap();
    (output.status.success(), stdout(&output))
}

fn write(home: &Path, path: &str, text: &str) {
    let path = home.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn setup_creates_the_vault_once_and_reports_each_agent() {
    let home = TempDir::new().unwrap();
    let (ok, out) = setup(home.path(), &[]);
    assert!(ok, "{out}");
    let vault = dunce::canonicalize(home.path().join("agent-memory")).unwrap();
    assert!(
        out.starts_with(&format!(
            "vault: created, vault ready at {}",
            vault.display()
        )),
        "{out}"
    );
    assert!(
        out.ends_with(
            "agents: none found (supported: Claude Code, GitHub Copilot, Codex, Cursor, Antigravity CLI)\n"
        ),
        "{out}"
    );
    assert!(vault.join("Index.md").is_file());

    write(
        home.path(),
        ".claude/settings.json",
        r#"{"enabledPlugins": {"petit-poucet@petit-poucet": true}}"#,
    );
    fs::create_dir(home.path().join(".copilot")).unwrap();
    let (ok, again) = setup(home.path(), &[]);
    assert!(ok, "setup reports a missing plugin, it doesn't fail on it");
    assert_eq!(
        again,
        format!(
            "vault: {} (0 notes)\n\
             Claude Code: set up by its plugin\n\
             GitHub Copilot: install its plugin: `copilot plugin marketplace add areguig/petit-poucet && copilot plugin install petit-poucet@petit-poucet`\n",
            vault.display()
        )
    );
    assert_eq!(
        git_log(&vault),
        "init: vault\n",
        "the vault is created once"
    );
}

#[test]
fn setup_check_fails_until_every_agent_found_is_set_up() {
    let home = TempDir::new().unwrap();
    let (ok, out) = setup(home.path(), &["--check"]);
    assert!(!ok);
    assert!(
        out.starts_with("vault: none yet: run `petit-poucet setup`\n"),
        "{out}"
    );
    assert!(
        !home.path().join("agent-memory").exists(),
        "check changes nothing"
    );

    setup(home.path(), &[]);
    fs::create_dir(home.path().join(".copilot")).unwrap();
    assert!(
        !setup(home.path(), &["--check"]).0,
        "Copilot plugin missing"
    );
    write(
        home.path(),
        ".copilot/installed-plugins/petit-poucet/petit-poucet/plugin.json",
        "{}",
    );
    let (ok, out) = setup(home.path(), &["--check"]);
    assert!(ok, "{out}");
    assert!(
        out.ends_with("GitHub Copilot: set up by its plugin\n"),
        "{out}"
    );
}

#[test]
fn setup_for_one_agent_and_uninstall_keep_the_vault() {
    let home = TempDir::new().unwrap();
    setup(home.path(), &[]);
    let (_, one) = setup(home.path(), &["--agent", "claude"]);
    assert!(
        one.ends_with("Claude Code: not found on this machine\n"),
        "{one}"
    );

    write(
        home.path(),
        ".claude/settings.json",
        r#"{"enabledPlugins": {"petit-poucet@petit-poucet": true}}"#,
    );
    let (ok, out) = setup(home.path(), &["--uninstall"]);
    let vault = dunce::canonicalize(home.path().join("agent-memory")).unwrap();
    assert!(ok);
    assert_eq!(
        out,
        format!(
            "vault: kept at {}\nClaude Code: remove the plugin with `claude plugin uninstall petit-poucet@petit-poucet`\n",
            vault.display()
        )
    );
    assert!(vault.join("Index.md").is_file());

    let conflict = petit_poucet(home.path())
        .args(["setup", "--check", "--uninstall"])
        .output()
        .unwrap();
    assert!(!conflict.status.success());
}

#[test]
fn setup_wires_codex_in_and_out() {
    let home = TempDir::new().unwrap();
    let codex = home.path().join(".codex");
    write(home.path(), ".codex/config.toml", "model = \"gpt-6\"\n");
    let exe = dunce::canonicalize(Path::new(env!("CARGO_BIN_EXE_petit-poucet"))).unwrap();

    let (ok, out) = setup(home.path(), &[]);
    assert!(ok, "{out}");
    assert!(
        out.ends_with("Codex: set up (MCP server in config.toml, hooks in hooks.json): open Codex and trust its hooks once with /hooks\n"),
        "{out}"
    );
    let config = fs::read_to_string(codex.join("config.toml")).unwrap();
    assert!(config.starts_with("model = \"gpt-6\"\n"), "{config}");
    let parsed: toml::Table = toml::from_str(&config).unwrap();
    assert_eq!(
        parsed["mcp_servers"]["petit-poucet"]["command"].as_str(),
        exe.to_str()
    );
    let hooks: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(codex.join("hooks.json")).unwrap()).unwrap();
    let start = hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert_eq!(
        start,
        format!("\"{}\" hook session-start --agent codex", exe.display())
    );

    assert!(setup(home.path(), &["--check"]).0);
    assert!(
        setup(home.path(), &[])
            .1
            .ends_with("Codex: already set up\n")
    );
    let (ok, out) = setup(home.path(), &["--uninstall"]);
    assert!(
        ok && out.ends_with("Codex: removed (MCP server and hooks)\n"),
        "{out}"
    );
    assert_eq!(
        fs::read_to_string(codex.join("config.toml")).unwrap(),
        "model = \"gpt-6\"\n"
    );
    let (ok, out) = setup(home.path(), &["--check"]);
    assert!(
        !ok && out.contains("Codex: missing the MCP server"),
        "{out}"
    );
}

#[test]
fn codex_hooks_reply_in_codex_format() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    copy_dir(Path::new(FIXTURE), &vault);
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let event =
        serde_json::json!({"cwd": home.path(), "session_id": "codex-1", "source": "startup"});
    let start: serde_json::Value = serde_json::from_str(&hook(
        home.path(),
        &["session-start", "--agent", "codex"],
        &event.to_string(),
    ))
    .unwrap();
    assert_eq!(start["hookSpecificOutput"]["hookEventName"], "SessionStart");
    assert!(
        start["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .starts_with("Agent memory (petit-poucet)")
    );
    assert!(
        start["systemMessage"]
            .as_str()
            .unwrap()
            .starts_with("🪨 petit-poucet ·")
    );

    let stop = serde_json::json!({"session_id": "codex-1", "stop_hook_active": false}).to_string();
    let replies: Vec<String> = (0..3)
        .map(|_| hook(home.path(), &["stop", "--agent", "codex"], &stop))
        .collect();
    let third: serde_json::Value = serde_json::from_str(&replies[2]).unwrap();
    assert_eq!(third["decision"], "block");
    assert!(
        third["reason"]
            .as_str()
            .unwrap()
            .starts_with("Memory check")
    );
}

// A fresh machine or a container often has no git identity, and git must not guess one.
fn without_git_identity(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    for var in [
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ] {
        cmd.env_remove(var);
    }
    cmd.env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "user.useConfigOnly")
        .env("GIT_CONFIG_VALUE_0", "true")
}

#[test]
fn init_and_setup_work_without_a_git_identity() {
    let home = TempDir::new().unwrap();
    let output = without_git_identity(&mut petit_poucet(home.path()))
        .arg("setup")
        .output()
        .unwrap();
    let (out, err) = (stdout(&output), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "{out}{err}");
    assert!(out.contains("\nvault: not committed: "), "{out}");
    assert!(out.contains("vault: set your git identity"), "{out}");
    assert!(
        out.ends_with(
            "agents: none found (supported: Claude Code, GitHub Copilot, Codex, Cursor, Antigravity CLI)\n"
        ),
        "{out}"
    );
    assert!(home.path().join("agent-memory/Index.md").is_file());

    // A home of its own: no config left by the setup above.
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    let output = without_git_identity(&mut petit_poucet(home.path()))
        .args(["init", vault.to_str().unwrap()])
        .output()
        .unwrap();
    let (out, err) = (stdout(&output), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "{out}{err}");
    assert!(out.contains("\nnot committed: "), "{out}");
    assert!(vault.join("Index.md").is_file());
}

// Copilot CLI 1.0.90 installs from a local folder without copying it: only its settings say so (#54).
#[test]
fn setup_sees_a_copilot_plugin_installed_from_a_local_folder() {
    let home = TempDir::new().unwrap();
    setup(home.path(), &[]);
    write(
        home.path(),
        ".copilot/settings.json",
        r#"{"enabledPlugins": {"petit-poucet@petit-poucet": true}}"#,
    );
    let (ok, out) = setup(home.path(), &["--check"]);
    assert!(ok, "{out}");
    assert!(
        out.ends_with("GitHub Copilot: set up by its plugin\n"),
        "{out}"
    );
}

#[test]
fn setup_wires_cursor_in_and_out() {
    let home = TempDir::new().unwrap();
    let cursor = home.path().join(".cursor");
    write(
        home.path(),
        ".cursor/mcp.json",
        r#"{"mcpServers": {"other": {"url": "http://localhost:1"}}}"#,
    );
    let exe = dunce::canonicalize(Path::new(env!("CARGO_BIN_EXE_petit-poucet"))).unwrap();

    let (ok, out) = setup(home.path(), &[]);
    assert!(ok, "{out}");
    assert!(
        out.ends_with(
            "Cursor: set up (MCP server in mcp.json, hooks in hooks.json): restart Cursor\n"
        ),
        "{out}"
    );
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(cursor.join("mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["other"]["url"], "http://localhost:1");
    assert_eq!(
        mcp["mcpServers"]["petit-poucet"]["command"],
        exe.display().to_string()
    );

    assert!(setup(home.path(), &["--check"]).0);
    let (ok, out) = setup(home.path(), &["--uninstall"]);
    assert!(
        ok && out.ends_with("Cursor: removed (MCP server and hooks)\n"),
        "{out}"
    );
    let (ok, out) = setup(home.path(), &["--check"]);
    assert!(
        !ok && out.contains("Cursor: missing the MCP server"),
        "{out}"
    );
}

#[test]
fn cursor_hooks_reply_in_cursor_format() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    copy_dir(Path::new(FIXTURE), &vault);
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    // Cursor gives the project folder only as workspace_roots: the fixture's `alpha` is matched by folder name.
    let alpha = home.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    let event = serde_json::json!({"conversation_id": "c-1", "workspace_roots": [alpha]});
    let start: serde_json::Value = serde_json::from_str(&hook(
        home.path(),
        &["session-start", "--agent", "cursor"],
        &event.to_string(),
    ))
    .unwrap();
    let context = start["additional_context"].as_str().unwrap();
    assert!(
        context.contains("[[Projects/alpha/plugin-design]]"),
        "{context}"
    );
    assert_eq!(start.as_object().unwrap().len(), 1, "{start}");

    let stop = |loop_count: u32| {
        let event = serde_json::json!({"conversation_id": "c-1", "status": "completed", "loop_count": loop_count});
        hook(
            home.path(),
            &["stop", "--agent", "cursor"],
            &event.to_string(),
        )
    };
    let replies: Vec<String> = (0..3).map(|_| stop(0)).collect();
    assert_eq!(&replies[..2], ["", ""]);
    let third: serde_json::Value = serde_json::from_str(&replies[2]).unwrap();
    assert!(
        third["followup_message"]
            .as_str()
            .unwrap()
            .starts_with("Memory check")
    );
    assert_eq!(stop(1), "", "never reminds again inside its own follow-up");
}

#[test]
fn setup_wires_antigravity_in_and_out() {
    let home = TempDir::new().unwrap();
    let config = home.path().join(".gemini/config");
    let mine = r#"{"mcpServers": {"other": {"serverUrl": "http://localhost:1"}}}"#;
    write(home.path(), ".gemini/config/mcp_config.json", mine);
    let exe = dunce::canonicalize(Path::new(env!("CARGO_BIN_EXE_petit-poucet"))).unwrap();

    let (ok, out) = setup(home.path(), &[]);
    assert!(ok, "{out}");
    assert!(
        out.ends_with(
            "Antigravity CLI: set up (MCP server in mcp_config.json, hooks in hooks.json)\n"
        ),
        "{out}"
    );
    let read = |file: &str| -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(config.join(file)).unwrap()).unwrap()
    };
    let mcp = read("mcp_config.json");
    assert_eq!(
        mcp["mcpServers"]["other"]["serverUrl"],
        "http://localhost:1"
    );
    assert_eq!(
        mcp["mcpServers"]["petit-poucet"]["command"],
        exe.display().to_string()
    );
    assert!(read("hooks.json")["petit-poucet"]["PreInvocation"].is_array());

    assert!(setup(home.path(), &["--check"]).0);
    let (ok, out) = setup(home.path(), &["--uninstall"]);
    assert!(
        ok && out.ends_with("Antigravity CLI: removed (MCP server and hooks)\n"),
        "{out}"
    );
    assert_eq!(
        read("mcp_config.json"),
        serde_json::from_str::<serde_json::Value>(mine).unwrap()
    );
}

#[test]
fn antigravity_hooks_reply_in_antigravity_format() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let event = serde_json::json!({"conversationId": "a-1", "workspacePaths": [home.path()], "invocationNum": 0});
    let start: serde_json::Value = serde_json::from_str(&hook(
        home.path(),
        &["session-start", "--agent", "antigravity"],
        &event.to_string(),
    ))
    .unwrap();
    assert!(
        start["injectSteps"][0]["ephemeralMessage"]
            .as_str()
            .unwrap()
            .starts_with("Agent memory (petit-poucet)"),
        "{start}"
    );

    let stop = |execution: u64| {
        let event = serde_json::json!({"conversationId": "a-1", "executionNum": execution, "terminationReason": "NO_TOOL_CALL"});
        hook(
            home.path(),
            &["stop", "--agent", "antigravity"],
            &event.to_string(),
        )
    };
    let replies: Vec<String> = (0..3).map(|_| stop(0)).collect();
    let third: serde_json::Value = serde_json::from_str(&replies[2]).unwrap();
    assert_eq!(third["decision"], "continue");
    assert!(
        third["reason"]
            .as_str()
            .unwrap()
            .starts_with("Memory check")
    );
    assert_eq!(
        stop(1),
        "",
        "the stop after our reminder lets the agent stop"
    );
}
