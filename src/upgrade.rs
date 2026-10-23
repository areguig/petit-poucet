use std::env::consts::{ARCH, EXE_SUFFIX, OS};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use semver::Version;
use sha2::{Digest, Sha256};

use crate::update;

// Where the install scripts download releases from: GitHub, or a test's local copy.
const RELEASES_ENV: &str = "PETIT_POUCET_RELEASES";
const RELEASES: &str = "https://github.com/areguig/petit-poucet/releases";
// The install scripts' folder: their default, or the one the user gave them.
const INSTALL_DIR_ENV: &str = "PETIT_POUCET_INSTALL_DIR";

// Replaces the running binary, at `exe`, with the latest release once it is downloaded and its checksum matches.
pub fn upgrade(exe: &Path) -> Result<Option<String>, String> {
    check_installed_by_script(exe)?;
    let latest =
        update::latest().ok_or("cannot read the latest release: offline, or curl missing?")?;
    if latest <= Version::parse(update::CURRENT).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let url = format!("{}/download/v{latest}/{}", releases(), asset()?);
    let dir = exe.parent().ok_or("the binary has no folder")?;
    let download = tempfile::Builder::new()
        .prefix(".petit-poucet.download.")
        .tempfile_in(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    curl(&url, Some(download.path()))?;
    let expected = String::from_utf8_lossy(&curl(&format!("{url}.sha256"), None)?)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let actual = sha256(download.path())?;
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {url} (expected {expected}, got {actual}): nothing was replaced"
        ));
    }
    make_executable(download.path())?;
    self_replace::self_replace(download.path())
        .map_err(|e| format!("replacing {}: {e}", exe.display()))?;
    Ok(Some(latest.to_string()))
}

// Installed another way (Homebrew, cargo install, a build under target/): that way updates it.
fn check_installed_by_script(exe: &Path) -> Result<(), String> {
    let dir = match std::env::var_os(INSTALL_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => std::env::home_dir()
            .ok_or("cannot find the home folder")?
            .join(".local/bin"),
    };
    let installed = dunce::canonicalize(&dir).is_ok_and(|dir| exe.parent() == Some(dir.as_path()));
    match installed {
        true => Ok(()),
        false => Err(format!(
            "{} wasn't installed by the install script in {}: update it the way you installed it (set {INSTALL_DIR_ENV} if the script installed it elsewhere)",
            exe.display(),
            dir.display()
        )),
    }
}

fn releases() -> String {
    std::env::var(RELEASES_ENV).unwrap_or_else(|_| RELEASES.to_string())
}

// The install scripts' names for each platform's binary.
fn asset() -> Result<String, String> {
    let target = match (OS, ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        (os, arch) => return Err(format!("no release for {os} {arch}")),
    };
    Ok(format!("petit-poucet-{target}{EXE_SUFFIX}"))
}

// Saves to `output` when given, else returns the body.
fn curl(url: &str, output: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut command = Command::new("curl");
    command.args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--connect-timeout",
        "15",
        "--max-time",
        "300",
    ]);
    if let Some(path) = output {
        command.arg("--output").arg(path);
    }
    let result = command
        .arg(url)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    match result.status.success() {
        true => Ok(result.stdout),
        false => Err(format!(
            "download failed: {url}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )),
    }
}

fn sha256(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), String> {
    Ok(())
}
