mod common;

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use common::{Client, FIXTURE, copy_dir, stdout};
use serde_json::{Value, json};
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
        .stdin(Stdio::null())
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

    let claude_context =
        |reply: &serde_json::Value| reply["hookSpecificOutput"]["additionalContext"].clone();
    let unaware = session_start(
        command(home.path(), &url).env("PETIT_POUCET_NO_UPDATE_CHECK", "1"),
        "claude",
    );
    session_start(&mut command(home.path(), &url), "claude");
    wait_for(&latest, "99.0.0");

    let claude = session_start(&mut command(home.path(), &url), "claude");
    let message = claude["systemMessage"].as_str().unwrap();
    assert!(
        message.ends_with(
            "\n🪨 petit-poucet · 99.0.0 is out: run `petit-poucet update`, or ask your agent to"
        ),
        "{message}"
    );
    // Claude Code adds a resumed session's context again unless it is unchanged: the memory would load twice.
    assert_eq!(
        claude_context(&claude),
        claude_context(&unaware),
        "the notice is for the user's line only"
    );
    let cursor = session_start(&mut command(home.path(), &url), "cursor");
    let context = cursor["additional_context"].as_str().unwrap();
    assert!(
        context.ends_with(
            "Tell the user once, in one line: petit-poucet: 99.0.0 is out: run `petit-poucet update`, or ask your agent to."
        ),
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
            "petit-poucet {current}: 99.0.0 is out: run `petit-poucet update`, or ask your agent to\n"
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

// The install scripts' name for this platform's binary, as the releases publish it.
fn asset() -> String {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        other => panic!("no release for {other:?}"),
    };
    format!("petit-poucet-{target}{}", std::env::consts::EXE_SUFFIX)
}

fn file_url(path: &Path) -> String {
    let path = path.display().to_string().replace('\\', "/");
    format!(
        "file://{}{path}",
        if path.starts_with('/') { "" } else { "/" }
    )
}

// A home with the binary where the install script puts it, dated long ago, and a release `tag` of it
// whose published checksum is `checksum`, or the right one.
struct Install {
    home: TempDir,
    exe: PathBuf,
    release: String,
    releases: String,
}

const LONG_AGO: Duration = Duration::from_secs(1_000_000_000);

fn install(tag: &str, checksum: Option<&str>) -> Install {
    use sha2::{Digest, Sha256};
    let home = TempDir::new().unwrap();
    let bin = home.path().join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("petit-poucet{}", std::env::consts::EXE_SUFFIX));
    fs::copy(env!("CARGO_BIN_EXE_petit-poucet"), &exe).unwrap();
    fs::File::options()
        .write(true)
        .open(&exe)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + LONG_AGO)
        .unwrap();
    let download = home.path().join(format!("releases/download/{tag}"));
    fs::create_dir_all(&download).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_petit-poucet"), download.join(asset())).unwrap();
    let right: String = Sha256::digest(fs::read(&exe).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    fs::write(
        download.join(format!("{}.sha256", asset())),
        format!("{}  {}\n", checksum.unwrap_or(&right), asset()),
    )
    .unwrap();
    let release = release(home.path(), tag);
    let releases = file_url(&home.path().join("releases"));
    Install {
        home,
        exe,
        release,
        releases,
    }
}

impl Install {
    fn command(&self) -> Command {
        let mut cmd = common::command_for(&self.exe, self.home.path());
        cmd.env("PETIT_POUCET_LATEST_RELEASE", &self.release)
            .env("PETIT_POUCET_RELEASES", &self.releases);
        cmd
    }

    fn replaced(&self) -> bool {
        fs::metadata(&self.exe).unwrap().modified().unwrap() > std::time::UNIX_EPOCH + LONG_AGO
    }

    fn files(&self) -> Vec<String> {
        fs::read_dir(self.exe.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    // The download never stays next to the binary.
    fn leftovers(&self) -> Vec<String> {
        self.files()
            .into_iter()
            .filter(|name| name.starts_with(".petit-poucet.download."))
            .collect()
    }
}

// Kills the server even when an assertion fails first.
struct Server(std::process::Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Install {
    // An agent's MCP server, started from the installed binary.
    fn serve(&self, stdout: Stdio) -> Server {
        Server(
            self.command()
                .arg("serve")
                .stdin(Stdio::piped())
                .stdout(stdout)
                .spawn()
                .unwrap(),
        )
    }

    fn update_and_set_up(&self) {
        let output = self.command().arg("update").output().unwrap();
        let out = stdout(&output);
        // Which of self_replace's renames failed shows in what it left next to the binary.
        assert!(
            output.status.success(),
            "{out}{}\nnext to the binary: {:?}",
            String::from_utf8_lossy(&output.stderr),
            self.files()
        );
        assert!(
            out.starts_with("petit-poucet: updated to 99.0.0\n"),
            "{out}"
        );
        assert!(
            out.contains("\nClaude Code: set up"),
            "the new binary ran setup: {out}"
        );
        assert!(self.replaced());
        assert_eq!(self.leftovers(), Vec::<String>::new());
    }
}

// A release to update to, a vault and Claude Code for the new binary's setup.
fn install_with_agent_and_vault() -> Install {
    let install = install("v99.0.0", None);
    fs::create_dir(install.home.path().join(".claude")).unwrap();
    let vault = install.home.path().join("vault");
    assert!(
        install
            .command()
            .args(["init", vault.to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
    install
}

// The server's answer, or a failure once it has kept silent too long.
fn request_within(mut client: Client, method: &'static str, params: Value) -> (Client, Value) {
    let (answer, answered) = mpsc::channel();
    std::thread::spawn(move || {
        let result = client.request(method, params);
        let _ = answer.send((client, result));
    });
    answered
        .recv_timeout(Duration::from_secs(20))
        .unwrap_or_else(|e| panic!("no answer to {method}: {e}"))
}

#[test]
fn update_installs_the_latest_release_while_a_server_runs_then_sets_up_the_agents() {
    let install = install_with_agent_and_vault();
    // The server may still be starting when the update replaces its binary.
    let mut server = install.serve(Stdio::null());
    install.update_and_set_up();
    assert!(
        server.0.try_wait().unwrap().is_none(),
        "the server still runs"
    );
}

// Diagnostic for #165: the same update once the server has answered its handshake, to tell
// a failure from a server still starting apart from one with a server already running.
#[test]
fn update_installs_the_latest_release_while_an_initialized_server_runs() {
    let install = install_with_agent_and_vault();
    let mut server = install.serve(Stdio::piped());
    let (mut client, init) = request_within(
        common::client(&mut server.0),
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test-agent", "version": "1"}}),
    );
    assert!(
        init["instructions"]
            .as_str()
            .is_some_and(|i| i.contains("call memory_index")),
        "{init}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    install.update_and_set_up();
    let (_, tools) = request_within(client, "tools/list", json!({}));
    assert!(
        tools["tools"].as_array().is_some_and(|t| !t.is_empty()),
        "the same server still answers: {tools}"
    );
}

#[test]
fn a_checksum_mismatch_replaces_nothing() {
    let install = install("v99.0.0", Some(&"0".repeat(64)));
    let output = install.command().arg("update").output().unwrap();
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(err.contains("checksum mismatch for "), "{err}");
    assert!(err.contains("nothing was replaced"), "{err}");
    assert!(!install.replaced());
    assert_eq!(install.leftovers(), Vec::<String>::new());
}

#[test]
fn the_latest_release_already_running_is_left_alone() {
    let current = env!("CARGO_PKG_VERSION");
    let install = install(&format!("v{current}"), None);
    let output = install.command().arg("update").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        format!("petit-poucet {current} is already up to date\n")
    );
    assert!(!install.replaced());
}

#[test]
fn a_binary_the_install_script_did_not_place_is_never_replaced() {
    let install = install("v99.0.0", None);
    let mut cmd = common::command(install.home.path());
    let output = cmd
        .env("PETIT_POUCET_LATEST_RELEASE", &install.release)
        .env("PETIT_POUCET_RELEASES", &install.releases)
        .arg("update")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        err.contains("wasn't installed by the install script in ")
            && err.contains("update it the way you installed it"),
        "{err}"
    );
}
