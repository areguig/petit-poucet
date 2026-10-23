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
        message.ends_with("\n🪨 petit-poucet · 99.0.0 is out: run `petit-poucet update`"),
        "{message}"
    );
    let cursor = session_start(&mut command(home.path(), &url), "cursor");
    let context = cursor["additional_context"].as_str().unwrap();
    assert!(
        context.ends_with(
            "Tell the user once, in one line: petit-poucet: 99.0.0 is out: run `petit-poucet update`."
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
            "petit-poucet {current}: 99.0.0 is out: run `petit-poucet update`\n"
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

    // The download never stays next to the binary.
    fn leftovers(&self) -> Vec<String> {
        fs::read_dir(self.exe.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".petit-poucet.download."))
            .collect()
    }
}

#[test]
fn update_installs_the_latest_release_while_a_server_runs_then_sets_up_the_agents() {
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
    // An agent's MCP server, started from the installed binary, keeps running across the update.
    let mut server = install
        .command()
        .arg("serve")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let output = install.command().arg("update").output().unwrap();
    let out = stdout(&output);
    assert!(
        output.status.success(),
        "{out}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        out.starts_with("petit-poucet: updated to 99.0.0\n"),
        "{out}"
    );
    assert!(
        out.contains("\nClaude Code: set up"),
        "the new binary ran setup: {out}"
    );
    assert!(install.replaced());
    assert_eq!(install.leftovers(), Vec::<String>::new());
    assert!(
        server.try_wait().unwrap().is_none(),
        "the server still runs"
    );
    server.kill().unwrap();
    server.wait().unwrap();
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
