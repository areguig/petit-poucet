mod common;

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use common::{FIXTURE, copy_dir, stdout};
use serde_json::Value;
use tempfile::TempDir;

// A home with a vault, its update check turned on and pointed at `release`.
fn home_with_vault(release: &str) -> (TempDir, PathBuf) {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    copy_dir(Path::new(FIXTURE), &vault);
    let latest = vault.join(".petit-poucet/latest-release");
    let mut cmd = command(home.path(), release);
    assert!(
        cmd.args(["init", vault.to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
    (home, latest)
}

fn command(home: &Path, release: &str) -> Command {
    let mut cmd = common::command(home);
    cmd.env_remove("PETIT_POUCET_NO_UPDATE_CHECK")
        .env("PETIT_POUCET_LATEST_RELEASE", release);
    cmd
}

// The latest release, as GitHub's API describes it, in a local file.
fn release(dir: &Path, tag: &str) -> String {
    let file = dir.join("latest.json");
    fs::write(
        &file,
        format!(r#"{{"tag_name": "{tag}", "name": "petit-poucet {tag}"}}"#),
    )
    .unwrap();
    let path = file.display().to_string().replace('\\', "/");
    format!(
        "file://{}{path}",
        if path.starts_with('/') { "" } else { "/" }
    )
}

fn session_start(cmd: &mut Command, agent: &str) -> Value {
    let output = cmd
        .args(["hook", "session-start", "--agent", agent])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    serde_json::from_str(&stdout(&output)).unwrap()
}

fn wait_for(file: &Path, content: &str) {
    let start = Instant::now();
    while fs::read_to_string(file).unwrap_or_default() != content {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "{} never held {content}",
            file.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn session_start_tells_of_a_newer_release_found_in_the_background() {
    let tmp = TempDir::new().unwrap();
    let url = release(tmp.path(), "v99.0.0");
    let (home, latest) = home_with_vault(&url);

    session_start(&mut command(home.path(), &url), "claude");
    wait_for(&latest, "99.0.0");

    let claude = session_start(&mut command(home.path(), &url), "claude");
    let message = claude["systemMessage"].as_str().unwrap();
    assert!(
        message.ends_with("\n🪨 petit-poucet · 99.0.0 is out: run the installer again"),
        "{message}"
    );
    let cursor = session_start(&mut command(home.path(), &url), "cursor");
    let context = cursor["additional_context"].as_str().unwrap();
    assert!(
        context.ends_with("petit-poucet 99.0.0 is out: tell the user once, in one line, to run its installer again."),
        "{context}"
    );
}

#[test]
fn an_older_or_unreachable_release_shows_nothing() {
    let tmp = TempDir::new().unwrap();
    let url = release(tmp.path(), "v0.0.1");
    let (home, latest) = home_with_vault(&url);
    session_start(&mut command(home.path(), &url), "claude");
    wait_for(&latest, "0.0.1");
    let claude = session_start(&mut command(home.path(), &url), "claude");
    assert!(!claude["systemMessage"].as_str().unwrap().contains("is out"));

    let missing = format!("{url}.missing");
    let output = command(home.path(), &missing)
        .arg("check")
        .output()
        .unwrap();
    assert!(
        !stdout(&output).contains("latest release"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn the_variable_turns_the_check_off() {
    let tmp = TempDir::new().unwrap();
    let url = release(tmp.path(), "v99.0.0");
    let (home, latest) = home_with_vault(&url);
    fs::create_dir_all(latest.parent().unwrap()).unwrap();
    fs::write(&latest, "99.0.0").unwrap();

    let mut off = command(home.path(), &url);
    off.env("PETIT_POUCET_NO_UPDATE_CHECK", "1");
    let claude = session_start(&mut off, "claude");
    assert!(!claude["systemMessage"].as_str().unwrap().contains("is out"));
    let mut off = command(home.path(), &url);
    off.env("PETIT_POUCET_NO_UPDATE_CHECK", "1");
    let output = off.arg("check").output().unwrap();
    assert!(!stdout(&output).contains("99.0.0"), "{}", stdout(&output));
}

#[test]
fn check_reports_the_latest_release() {
    let tmp = TempDir::new().unwrap();
    let url = release(tmp.path(), "v99.0.0");
    let (home, latest) = home_with_vault(&url);
    let output = command(home.path(), &url).arg("check").output().unwrap();
    let current = env!("CARGO_PKG_VERSION");
    assert!(
        stdout(&output).ends_with(&format!(
            "petit-poucet {current}: 99.0.0 is out: run the installer again\n"
        )),
        "{}",
        stdout(&output)
    );
    assert_eq!(fs::read_to_string(&latest).unwrap(), "99.0.0", "recorded");

    let url = release(tmp.path(), &format!("v{current}"));
    let output = command(home.path(), &url).arg("check").output().unwrap();
    assert!(
        stdout(&output).ends_with(&format!("petit-poucet {current} is the latest release\n")),
        "{}",
        stdout(&output)
    );
}

// The agent reads the hook's reply until its output closes: the background check must not hold it open.
#[test]
fn session_start_never_waits_for_the_check() {
    let silent = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/latest", silent.local_addr().unwrap());
    let (home, _) = home_with_vault(&url);
    let start = Instant::now();
    session_start(&mut command(home.path(), &url), "claude");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "took {:?}",
        start.elapsed()
    );
}
