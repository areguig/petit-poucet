mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::stdout;
use tempfile::TempDir;

struct Install {
    home: TempDir,
    bin: PathBuf,
}

impl Install {
    fn fresh() -> Self {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        fs::copy(
            env!("CARGO_BIN_EXE_petit-poucet"),
            bin.join(format!("petit-poucet{}", std::env::consts::EXE_SUFFIX)),
        )
        .unwrap();
        Self { home, bin }
    }

    fn command(&self) -> Command {
        let mut command = common::command_for(
            &self
                .bin
                .join(format!("petit-poucet{}", std::env::consts::EXE_SUFFIX)),
            self.home.path(),
        );
        let path = std::env::join_paths(std::iter::once(self.bin.clone()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap();
        command.current_dir(self.home.path()).env("PATH", path);
        command
    }

    fn ready() -> Self {
        let install = Self::fresh();
        fs::create_dir(install.home.path().join(".claude")).unwrap();
        let output = install
            .command()
            .args(["setup", "--agent", "claude"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        install
    }

    fn doctor(&self) -> Output {
        self.command().arg("doctor").output().unwrap()
    }

    fn config(&self) -> PathBuf {
        self.home.path().join(".config/petit-poucet/config.toml")
    }

    fn vault(&self) -> PathBuf {
        self.home.path().join("agent-memory")
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            (
                e.path().strip_prefix(root).unwrap().to_path_buf(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_fresh_home_fails_then_setup_fixes_the_missing_installation() {
    let install = Install::fresh();
    let before = snapshot(install.home.path());
    let missing = install.doctor();
    assert_eq!(missing.status.code(), Some(1));
    let text = stdout(&missing);
    assert!(
        text.contains("config: error:") && text.contains("petit-poucet setup"),
        "{text}"
    );
    assert!(text.contains("Claude Code: info: not installed"), "{text}");
    assert_eq!(snapshot(install.home.path()), before);
    let setup = install.command().arg("setup").output().unwrap();
    assert!(setup.status.success());
    let ready = install.doctor();
    assert!(
        ready.status.success(),
        "{}\n{}",
        stdout(&ready),
        String::from_utf8_lossy(&ready.stderr)
    );
}

#[test]
fn a_wired_installation_reports_each_agent_and_payload_without_writing() {
    let install = Install::ready();
    let before = snapshot(install.home.path());
    let output = install.doctor();
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let text = stdout(&output);
    for line in [
        "binary: ok:",
        "binary PATH: ok:",
        "config: ok:",
        "vault: ok:",
        "git identity: ok:",
        "vault check: ok: 0 errors, 0 warnings",
        "Claude Code: ok: wired",
        "Claude Code payload: ok:",
        "8000 bytes",
        "left out: none",
        "sync conflicts: ok: 0",
        "sync usage:",
    ] {
        assert!(text.contains(line), "missing {line:?}: {text}");
    }
    assert_eq!(snapshot(install.home.path()), before);
}

#[test]
fn invalid_and_secret_config_values_are_not_echoed() {
    let install = Install::ready();
    let config = install.config();
    let valid = fs::read_to_string(&config).unwrap();
    for invalid in [
        "password = 'hunter22hunter'\nvault = [\n".to_string(),
        valid.replace("git_autocommit = true", "git_autocommit = 'hunter22hunter'"),
    ] {
        fs::write(&config, &invalid).unwrap();
        let before = snapshot(install.home.path());
        let output = install.doctor();
        assert_eq!(output.status.code(), Some(1));
        let text = stdout(&output);
        assert!(
            text.contains("config: error:")
                && text.contains("config.toml")
                && text.contains("edit"),
            "{text}"
        );
        if invalid.contains("git_autocommit") {
            assert!(text.contains("git_autocommit"), "{text}");
        }
        assert!(!text.contains("hunter22hunter"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("hunter22hunter"));
        assert_eq!(snapshot(install.home.path()), before);
    }
    fs::write(config, valid).unwrap();
    assert!(install.doctor().status.success());
}

#[test]
fn missing_settings_warn_without_migrating_and_setup_adds_them() {
    let install = Install::ready();
    let config = install.config();
    let old = fs::read_to_string(&config)
        .unwrap()
        .replace("review_max_pages = 10\n", "");
    fs::write(&config, &old).unwrap();
    let output = install.doctor();
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(
        text.contains("review_max_pages")
            && text.contains("missing")
            && text.contains("petit-poucet setup"),
        "{text}"
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), old);
    assert!(install.command().arg("setup").status().unwrap().success());
    assert!(install.doctor().status.success());
}

#[test]
fn missing_git_identity_and_agent_wiring_name_their_repairs() {
    let install = Install::ready();
    let mut missing_identity = install.command();
    for key in [
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ] {
        missing_identity.env_remove(key);
    }
    let output = missing_identity.arg("doctor").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(
        text.contains("git identity: error:")
            && text.contains("user.name")
            && text.contains("user.email"),
        "{text}"
    );
    fs::write(install.home.path().join(".claude.json"), "{}").unwrap();
    let output = install.doctor();
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(
        text.contains("Claude Code: error:") && text.contains("petit-poucet setup --agent claude"),
        "{text}"
    );
    assert!(
        install
            .command()
            .args(["setup", "--agent", "claude"])
            .status()
            .unwrap()
            .success()
    );
    assert!(install.doctor().status.success());
}

#[test]
fn a_different_binary_on_path_is_an_error() {
    let install = Install::ready();
    let other = install.home.path().join("other");
    fs::create_dir(&other).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_petit-poucet"),
        other.join(format!("petit-poucet{}", std::env::consts::EXE_SUFFIX)),
    )
    .unwrap();
    let path = std::env::join_paths(std::iter::once(other).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let output = install
        .command()
        .env("PATH", path)
        .arg("doctor")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("binary PATH: error:"));
    assert!(stdout(&output).contains("PATH=") || stdout(&output).contains("$env:Path"));
    assert!(install.doctor().status.success());
}

#[test]
fn a_newer_release_warns_without_creating_an_update_cache() {
    let install = Install::ready();
    let release = install.home.path().join("release.json");
    fs::write(&release, "{\"tag_name\":\"v99.0.0\"}").unwrap();
    let url = format!(
        "file:///{}",
        release
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    for name in [".curlrc", "_curlrc"] {
        fs::write(
            install.home.path().join(name),
            format!(
                "output = \"{}\"\n",
                install.home.path().join("must-not-be-written").display()
            ),
        )
        .unwrap();
    }
    let before = snapshot(install.home.path());
    let output = install
        .command()
        .env_remove("PETIT_POUCET_NO_UPDATE_CHECK")
        .env("PETIT_POUCET_LATEST_RELEASE", url)
        .arg("doctor")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(stdout(&output).contains("99.0.0") && stdout(&output).contains("petit-poucet update"));
    assert_eq!(snapshot(install.home.path()), before);
    let output = install
        .command()
        .env_remove("PETIT_POUCET_NO_UPDATE_CHECK")
        .env("PETIT_POUCET_LATEST_RELEASE", "http://127.0.0.1:1/release")
        .arg("doctor")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("release: info: unavailable"));
}

#[test]
fn conflicts_and_peer_usage_are_observed_without_creating_machine_state() {
    let install = Install::ready();
    let root = install.vault();
    fs::write(
        root.join(".sync-conflict-20260101-000000-ABCDEF7.last-cleanup"),
        "older index",
    )
    .unwrap();
    fs::create_dir_all(root.join(".petit-poucet")).unwrap();
    fs::write(root.join(".petit-poucet/machine-id"), "local\n").unwrap();
    fs::create_dir(root.join(".usage")).unwrap();
    fs::write(root.join(".usage/local.json"), "{}").unwrap();
    fs::write(root.join(".usage/peer.json"), "{}").unwrap();
    let before = snapshot(install.home.path());
    let output = install.doctor();
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(
        text.contains("sync conflicts: warning: 1") && text.contains("petit-poucet check"),
        "{text}"
    );
    assert!(text.contains("sync usage: ok: 1 other machine"), "{text}");
    assert_eq!(snapshot(install.home.path()), before);
}

#[test]
fn payload_preview_names_the_budget_and_what_is_left_out() {
    let install = Install::ready();
    for n in 0..120 {
        fs::write(install.vault().join(format!("Preferences/note-{n}.md")), format!("---\ntype: feedback\nscope: all repos\nsummary: a deliberately long synthetic preference number {n} about a distinct decision that an agent should remember\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n# Note {n}\n**Why:** test fixture\n")).unwrap();
    }
    let before = snapshot(install.home.path());
    let output = install.doctor();
    let text = stdout(&output);
    assert!(
        text.contains("Claude Code payload: warning:")
            && text.contains("8000 bytes")
            && text.contains("left out:"),
        "{text}"
    );
    assert!(!text.contains("left out: none"));
    assert_eq!(snapshot(install.home.path()), before);
}

#[test]
fn unknown_config_keys_and_malformed_agent_files_never_expose_their_values() {
    let install = Install::ready();
    let config = install.config();
    let text = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        format!("{text}\nauth_token = 'synthetic-private-value'\n"),
    )
    .unwrap();
    let output = install.doctor();
    assert_eq!(output.status.code(), Some(2));
    let text = stdout(&output);
    assert!(text.contains("unrecognized or obsolete keys"), "{text}");
    assert!(!text.contains("synthetic-private-value"));
    fs::write(
        install.home.path().join(".claude.json"),
        "{\"token\":\"synthetic-private-value\", broken}",
    )
    .unwrap();
    let output = install.doctor();
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("Claude Code: error:"));
    assert!(!text.contains("synthetic-private-value"));
}

#[test]
fn missing_vault_and_git_repository_each_name_a_repair() {
    let install = Install::ready();
    fs::remove_dir_all(install.vault().join(".git")).unwrap();
    let output = install.doctor();
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("vault git: error:") && stdout(&output).contains(" init`"));
    let moved = install.home.path().join("moved-vault");
    fs::rename(install.vault(), moved).unwrap();
    let output = install.doctor();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("vault: error:") && stdout(&output).contains("petit-poucet init")
    );
    let repaired = install
        .command()
        .arg("init")
        .arg(install.vault())
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(install.doctor().status.success());
}

#[test]
fn release_lookup_stops_when_a_server_does_not_answer() {
    use std::net::TcpListener;
    use std::time::{Duration, Instant};
    let install = Install::ready();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/release", listener.local_addr().unwrap());
    let (done, stop) = std::sync::mpsc::channel();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(8);
        while Instant::now() < until {
            match listener.accept() {
                Ok((_connection, _)) => {
                    let _ = stop.recv_timeout(Duration::from_secs(8));
                    return true;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if stop.try_recv().is_ok() {
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => panic!("{e}"),
            }
        }
        false
    });
    let started = Instant::now();
    let output = install
        .command()
        .env_remove("PETIT_POUCET_NO_UPDATE_CHECK")
        .env("PETIT_POUCET_LATEST_RELEASE", url)
        .arg("doctor")
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    let _ = done.send(());
    assert!(server.join().unwrap(), "doctor never attempted the lookup");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(elapsed < Duration::from_secs(6), "lookup took {elapsed:?}");
    assert!(!install.home.path().join("must-not-be-written").exists());
}
