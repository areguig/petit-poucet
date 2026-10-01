// The launcher, the Copilot glue and install.sh are POSIX shell scripts.
#![cfg(unix)]

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

// A copy of the plugin folder under `dir`: run from this checkout, the launcher would use its own build.
fn copy_plugin(dir: &Path) -> std::path::PathBuf {
    fs::create_dir_all(dir.join("plugin/bin")).unwrap();
    for file in ["bin/petit-poucet", "release.env"] {
        fs::copy(
            Path::new(ROOT).join("plugin").join(file),
            dir.join("plugin").join(file),
        )
        .unwrap();
    }
    dir.join("plugin/bin/petit-poucet")
}

fn launcher(home: &Path, envs: &[(&str, &Path)], args: &[&str]) -> Output {
    let mut cmd = Command::new("sh");
    cmd.arg(copy_plugin(home))
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
    fake_binary_checksum(path);
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
    assert!(text(&output).starts_with("fake serve from "));
    assert!(text(&output).ends_with("/plugin/bin/petit-poucet\n"));
}

#[test]
fn launcher_runs_the_build_of_the_checkout_it_belongs_to() {
    let home = TempDir::new().unwrap();
    let build = home.path().join("target/release/petit-poucet");
    fs::create_dir_all(build.parent().unwrap()).unwrap();
    fake_binary(&build);
    fs::set_permissions(&build, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let output = launcher(home.path(), &[], &["serve"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text(&output).starts_with("fake serve from "));
    assert!(!home.path().join(".cache").exists(), "nothing downloaded");
}

#[test]
fn a_plugin_folder_symlinked_to_a_checkout_runs_its_build() {
    let home = TempDir::new().unwrap();
    let checkout = home.path().join("checkout");
    copy_plugin(&checkout);
    let build = checkout.join("target/release/petit-poucet");
    fs::create_dir_all(build.parent().unwrap()).unwrap();
    fake_binary(&build);
    fs::set_permissions(&build, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let installed = home.path().join("installed-plugins/petit-poucet");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(checkout.join("plugin"), &installed).unwrap();

    let output = Command::new("sh")
        .arg(installed.join("bin/petit-poucet"))
        .arg("serve")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        text(&output).starts_with("fake serve from "),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!home.path().join(".cache").exists(), "nothing downloaded");
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

#[test]
fn copilot_bootstrap_finds_the_installed_plugin_without_plugin_root() {
    let home = TempDir::new().unwrap();
    let installed = home.path().join(".copilot/installed-plugins/petit-poucet");
    copy_plugin(&installed);
    fs::rename(installed.join("plugin"), installed.join("petit-poucet")).unwrap();
    let bin = home.path().join("fake-build");
    fake_binary(&bin);
    fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let server = &json("plugin/copilot/mcp.json")["mcpServers"]["petit-poucet"];
    assert_eq!(server["command"], "sh");
    let args: Vec<String> = server["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect();
    for unexpanded in ["${PLUGIN_ROOT}", "${env:PLUGIN_ROOT}"] {
        let output = Command::new("sh")
            .args([&args[0], &args[1], &args[2], unexpanded])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", home.path())
            .env("PETIT_POUCET_BIN", &bin)
            .output()
            .unwrap();
        assert!(
            text(&output).starts_with("fake serve from "),
            "{unexpanded}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// VS Code runs plugin hooks without PLUGIN_ROOT; the Copilot CLI sets it.
#[test]
fn copilot_hooks_find_the_installed_plugin_with_or_without_plugin_root() {
    let home = TempDir::new().unwrap();
    let installed = home.path().join(".copilot/installed-plugins/petit-poucet");
    copy_plugin(&installed);
    let plugin = installed.join("petit-poucet");
    fs::rename(installed.join("plugin"), &plugin).unwrap();
    let bin = home.path().join("fake-build");
    fake_binary(&bin);
    fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let hooks = json("plugin/copilot/hooks.json");
    for (event, expected) in [
        ("sessionStart", "fake hook session-start"),
        ("agentStop", "fake hook stop"),
    ] {
        let command = hooks["hooks"][event][0]["bash"].as_str().unwrap();
        for plugin_root in [
            None,
            Some("${PLUGIN_ROOT}".into()),
            Some(plugin.display().to_string()),
        ] {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", command])
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap())
                .env("HOME", home.path())
                .env("PETIT_POUCET_BIN", &bin);
            if let Some(root) = &plugin_root {
                cmd.env("PLUGIN_ROOT", root);
            }
            let output = cmd.output().unwrap();
            assert!(
                text(&output).starts_with(expected),
                "{event} with PLUGIN_ROOT={plugin_root:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

// A leftover install from another marketplace sorts first; the newest install must win.
#[test]
fn copilot_commands_pick_the_newest_install() {
    let home = TempDir::new().unwrap();
    let plugins = home.path().join(".copilot/installed-plugins");
    for marketplace in ["a-old-marketplace", "petit-poucet"] {
        let installed = plugins.join(marketplace);
        copy_plugin(&installed);
        fs::rename(installed.join("plugin"), installed.join("petit-poucet")).unwrap();
    }
    let status = Command::new("touch")
        .args(["-t", "202001010000"])
        .arg(plugins.join("a-old-marketplace/petit-poucet"))
        .status()
        .unwrap();
    assert!(status.success());
    let bin = home.path().join("fake-build");
    fake_binary(&bin);
    fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let hooks = json("plugin/copilot/hooks.json");
    let server = &json("plugin/copilot/mcp.json")["mcpServers"]["petit-poucet"];
    let mut commands: Vec<Vec<String>> = ["sessionStart", "agentStop"]
        .iter()
        .map(|event| {
            let command = hooks["hooks"][event][0]["bash"].as_str().unwrap();
            vec!["-c".to_string(), command.to_string()]
        })
        .collect();
    let args = server["args"].as_array().unwrap();
    commands.push(vec![
        args[0].as_str().unwrap().into(),
        args[1].as_str().unwrap().into(),
        args[2].as_str().unwrap().into(),
        "${PLUGIN_ROOT}".into(),
    ]);
    for command in commands {
        let output = Command::new("sh")
            .args(&command)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", home.path())
            .env("PETIT_POUCET_BIN", &bin)
            .output()
            .unwrap();
        let expected = plugins.join("petit-poucet/petit-poucet/bin/petit-poucet");
        assert!(
            text(&output)
                .trim_end()
                .ends_with(&expected.display().to_string()),
            "{}: {}",
            command[1],
            text(&output)
        );
    }
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

#[test]
fn launcher_prefers_a_path_binary_of_the_pinned_version_only() {
    let home = TempDir::new().unwrap();
    let on_path = home.path().join("path-bin");
    let path = |dir: &Path| format!("{}:{}", dir.display(), std::env::var("PATH").unwrap());
    let reports = |version: &str| {
        format!(
            "[ \"$1\" = --version ] && {{ echo \"petit-poucet {version}\"; exit; }}\necho \"from path $* guard=$PETIT_POUCET_NO_PATH\""
        )
    };

    script(&on_path.join("petit-poucet"), &reports(VERSION));
    let matching = path(&on_path);
    let output = launcher(home.path(), &[("PATH", Path::new(&matching))], &["serve"]);
    assert_eq!(text(&output), "from path serve guard=1\n");

    // Another version: the launcher ignores it and fetches the pinned release.
    script(&on_path.join("petit-poucet"), &reports("0.0.1"));
    mirror_with(
        &home.path().join("mirror"),
        &format!("v{VERSION}"),
        "echo pinned $*",
    );
    let releases = format!("file://{}", home.path().join("mirror").display());
    let output = launcher(
        home.path(),
        &[
            ("PATH", Path::new(&matching)),
            ("PETIT_POUCET_RELEASES", Path::new(&releases)),
        ],
        &["serve"],
    );
    assert_eq!(text(&output), "pinned serve\n");
}

// A plugin's bin folder may itself be on PATH: the launcher must not chase itself.
#[test]
fn launcher_found_on_path_does_not_loop() {
    let home = TempDir::new().unwrap();
    mirror_with(
        &home.path().join("mirror"),
        &format!("v{VERSION}"),
        "echo pinned $*",
    );
    let releases = format!("file://{}", home.path().join("mirror").display());
    let launcher_path = copy_plugin(home.path());
    fs::set_permissions(
        &launcher_path,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let path = format!(
        "{}:{}",
        launcher_path.parent().unwrap().display(),
        std::env::var("PATH").unwrap()
    );
    let output = Command::new("sh")
        .arg(&launcher_path)
        .arg("serve")
        .env_clear()
        .env("PATH", path)
        .env("HOME", home.path())
        .env("PETIT_POUCET_RELEASES", releases)
        .output()
        .unwrap();
    assert_eq!(
        text(&output),
        "pinned serve\n",
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
