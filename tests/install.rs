// install.sh is a POSIX shell script.
#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const TARGETS: [&str; 4] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
];

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

// A release mirror laid out like GitHub's: latest/download/<asset> and download/v<version>/<asset>.
fn mirror_with(dir: &Path, release: &str, body: &str) {
    let release = dir.join(release);
    for target in TARGETS {
        let asset = release.join(format!("petit-poucet-{target}"));
        script(&asset, body);
        fake_binary_checksum(&asset);
    }
}

fn fake_binary_checksum(path: &Path) {
    let sha = Command::new("sh")
        .arg("-c")
        .arg("sha256sum \"$0\" 2>/dev/null || shasum -a 256 \"$0\"")
        .arg(path)
        .output()
        .unwrap();
    fs::write(path.with_extension("sha256"), sha.stdout).unwrap();
}

fn install(home: &Path, envs: &[(&str, String)]) -> Output {
    let mut cmd = Command::new("sh");
    cmd.arg(Path::new(ROOT).join("install.sh"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", home)
        .env(
            "PETIT_POUCET_RELEASES",
            format!("file://{}", home.join("mirror").display()),
        );
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

#[test]
fn install_script_installs_the_latest_release_then_a_chosen_version() {
    let home = TempDir::new().unwrap();
    mirror_with(
        &home.path().join("mirror"),
        "latest/download",
        "echo latest $*",
    );
    mirror_with(
        &home.path().join("mirror"),
        "download/v1.2.3",
        "echo v1.2.3 $*",
    );
    let installed = home.path().join(".local/bin/petit-poucet");

    let latest = install(home.path(), &[]);
    let log = String::from_utf8_lossy(&latest.stderr);
    assert!(latest.status.success(), "{log}");
    assert!(log.contains("installed latest --version in "), "{log}");
    assert!(log.contains(".local/bin is not on your PATH"), "{log}");
    assert!(log.contains("next: run `petit-poucet setup`"), "{log}");
    assert!(text(&latest).is_empty(), "messages go to stderr");
    let run = Command::new(&installed).output().unwrap();
    assert_eq!(text(&run), "latest\n");

    let pinned = install(home.path(), &[("PETIT_POUCET_VERSION", "v1.2.3".into())]);
    assert!(pinned.status.success());
    assert_eq!(
        text(&Command::new(&installed).output().unwrap()),
        "v1.2.3\n"
    );
    let leftovers: Vec<_> = fs::read_dir(installed.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, ["petit-poucet"], "no temp file left");
}

#[test]
fn install_script_honours_the_install_dir_and_knows_when_it_is_on_path() {
    let home = TempDir::new().unwrap();
    mirror_with(
        &home.path().join("mirror"),
        "latest/download",
        "echo latest $*",
    );
    let dir = home.path().join("tools");
    let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap());
    let output = install(
        home.path(),
        &[
            ("PETIT_POUCET_INSTALL_DIR", dir.display().to_string()),
            ("PATH", path),
        ],
    );
    let log = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{log}");
    assert!(dir.join("petit-poucet").is_file());
    assert!(!log.contains("not on your PATH"), "{log}");
}

#[test]
fn install_script_refuses_a_binary_with_a_wrong_checksum() {
    let home = TempDir::new().unwrap();
    mirror_with(
        &home.path().join("mirror"),
        "latest/download",
        "echo latest",
    );
    for target in TARGETS {
        let asset = home
            .path()
            .join(format!("mirror/latest/download/petit-poucet-{target}"));
        fs::write(asset, "#!/bin/sh\necho tampered\n").unwrap();
    }
    let output = install(home.path(), &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("checksum mismatch"));
    let dir = home.path().join(".local/bin");
    assert_eq!(fs::read_dir(dir).unwrap().count(), 0, "nothing installed");
}
