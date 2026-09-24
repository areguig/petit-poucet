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

#[derive(serde::Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
}

// Copilot silently ignores a skill or agent whose frontmatter is invalid YAML.
fn frontmatter_and_body(path: &str) -> (Frontmatter, String) {
    let text = fs::read_to_string(Path::new(ROOT).join(path)).unwrap();
    let (yaml, body) = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .unwrap_or_else(|| panic!("{path}: no frontmatter"));
    let frontmatter: Frontmatter =
        serde_saphyr::from_str(yaml).unwrap_or_else(|e| panic!("{path}: {e}"));
    (frontmatter, body.trim().to_string())
}

#[test]
fn skills_and_agents_have_valid_frontmatter() {
    for (path, name) in [
        ("plugin/skills/migrate-memory/SKILL.md", "migrate-memory"),
        ("plugin/skills/tidy-memory/SKILL.md", "tidy-memory"),
        ("plugin/agents/memory-cleanup.md", "memory-cleanup"),
        (
            "plugin/copilot/agents/memory-cleanup.agent.md",
            "memory-cleanup",
        ),
    ] {
        let (frontmatter, body) = frontmatter_and_body(path);
        assert_eq!(frontmatter.name, name, "{path}");
        assert!(
            !frontmatter.description.is_empty() && !body.is_empty(),
            "{path}"
        );
    }
}

#[test]
fn both_agents_get_the_same_cleanup_instructions() {
    let (_, claude) = frontmatter_and_body("plugin/agents/memory-cleanup.md");
    let (_, copilot) = frontmatter_and_body("plugin/copilot/agents/memory-cleanup.agent.md");
    assert_eq!(claude, copilot);
}

#[test]
fn copilot_manifest_points_to_existing_files() {
    let manifest = json("plugin/plugin.json");
    for key in ["mcpServers", "hooks", "agents", "skills"] {
        let path = manifest[key].as_str().unwrap();
        assert!(
            Path::new(ROOT).join("plugin").join(path).exists(),
            "{key}: {path}"
        );
    }
}
