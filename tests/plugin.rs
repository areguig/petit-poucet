use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const TARGETS: [&str; 4] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
];

fn json(path: &str) -> Value {
    let text = fs::read_to_string(Path::new(ROOT).join(path)).unwrap();
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn every_manifest_is_valid_and_pins_the_crate_version() {
    for path in [
        "plugin/.mcp.json",
        "plugin/hooks/hooks.json",
        "plugin/copilot/mcp.json",
        "plugin/copilot/hooks.json",
    ] {
        json(path);
    }
    assert_eq!(
        json("plugin/.claude-plugin/plugin.json")["version"],
        VERSION
    );
    assert_eq!(json("plugin/plugin.json")["version"], VERSION);
    let copilot = json(".github/plugin/marketplace.json");
    assert_eq!(copilot["metadata"]["version"], VERSION);
    assert_eq!(copilot["plugins"][0]["version"], VERSION);
    assert_eq!(
        json(".claude-plugin/marketplace.json")["plugins"][0]["source"],
        "./plugin"
    );
    let release = fs::read_to_string(Path::new(ROOT).join("plugin/release.env")).unwrap();
    assert!(
        release.contains(&format!("PETIT_POUCET_VERSION={VERSION}\n")),
        "{release}"
    );
}

fn launcher(home: &Path, envs: &[(&str, &Path)], args: &[&str]) -> Output {
    let mut cmd = Command::new("sh");
    cmd.arg(Path::new(ROOT).join("plugin/bin/petit-poucet"))
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", home);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

// A stand-in release: a script that prints its arguments and where the launcher lives.
fn fake_binary(path: &Path) {
    fs::write(
        path,
        "#!/bin/sh\necho \"fake $* from $PETIT_POUCET_LAUNCHER\"\n",
    )
    .unwrap();
    let sha = Command::new("sh")
        .arg("-c")
        .arg("sha256sum \"$0\" 2>/dev/null || shasum -a 256 \"$0\"")
        .arg(path)
        .output()
        .unwrap();
    fs::write(path.with_extension("sha256"), sha.stdout).unwrap();
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn launcher_runs_a_local_binary_override() {
    let home = TempDir::new().unwrap();
    let bin = home.path().join("dev-build");
    fake_binary(&bin);
    fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let output = launcher(home.path(), &[("PETIT_POUCET_BIN", &bin)], &["serve"]);
    assert!(output.status.success());
    let expected = format!("fake serve from {ROOT}/plugin/bin/petit-poucet\n");
    assert_eq!(text(&output), expected);
}

#[test]
fn launcher_downloads_verifies_and_caches_the_pinned_release() {
    let home = TempDir::new().unwrap();
    let mirror = home.path().join("mirror");
    let release = mirror.join(format!("v{VERSION}"));
    fs::create_dir_all(&release).unwrap();
    for target in TARGETS {
        fake_binary(&release.join(format!("petit-poucet-{target}")));
    }
    let releases = format!("file://{}", mirror.display());
    let env = [("PETIT_POUCET_RELEASES", Path::new(&releases))];

    let first = launcher(home.path(), &env, &["hook", "stop"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(text(&first).starts_with("fake hook stop from "));
    let cached = home
        .path()
        .join(format!(".cache/petit-poucet/petit-poucet-{VERSION}"));
    assert!(cached.is_file());

    fs::remove_dir_all(&mirror).unwrap();
    let offline = launcher(home.path(), &env, &["check"]);
    assert!(
        text(&offline).starts_with("fake check"),
        "served from the cache"
    );
}

#[test]
fn launcher_refuses_a_binary_with_a_wrong_checksum() {
    let home = TempDir::new().unwrap();
    let release = home.path().join(format!("mirror/v{VERSION}"));
    fs::create_dir_all(&release).unwrap();
    for target in TARGETS {
        let path = release.join(format!("petit-poucet-{target}"));
        fake_binary(&path);
        fs::write(&path, "#!/bin/sh\necho tampered\n").unwrap();
    }
    let releases = format!("file://{}", home.path().join("mirror").display());
    let output = launcher(
        home.path(),
        &[("PETIT_POUCET_RELEASES", Path::new(&releases))],
        &["serve"],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("checksum mismatch"));
    let cache = home.path().join(".cache/petit-poucet");
    assert_eq!(fs::read_dir(cache).unwrap().count(), 0, "nothing cached");
}
