use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use semver::Version;

use crate::config::Config;
use crate::state;
use crate::vault::write_atomic;

// Turns the check off (CI, offline machines).
pub const OFF: &str = "PETIT_POUCET_NO_UPDATE_CHECK";
// Where the latest release is described: GitHub's API, or a test's local copy.
const RELEASE_ENV: &str = "PETIT_POUCET_LATEST_RELEASE";
const RELEASE: &str = "https://api.github.com/repos/areguig/petit-poucet/releases/latest";
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
// Holds the latest release's version as last fetched; its date is the last check's.
const FILE: &str = "latest-release";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

fn file() -> Option<PathBuf> {
    if std::env::var_os(OFF).is_some() {
        return None;
    }
    Some(state::dir(&Config::load().ok()?.vault).ok()?.join(FILE))
}

// The newer release the last check found.
pub fn newer() -> Option<String> {
    newer_than(&fs::read_to_string(file()?).ok()?, CURRENT)
}

fn newer_than(latest: &str, current: &str) -> Option<String> {
    let latest = Version::parse(latest.trim()).ok()?;
    (latest > Version::parse(current).ok()?).then(|| latest.to_string())
}

// Once a day, checks in a background process: the session never waits for it.
pub fn start() {
    let Some(file) = file() else { return };
    let last = fs::metadata(&file).and_then(|m| m.modified()).ok();
    if !due(last, SystemTime::now()) {
        return;
    }
    // Dated first, so sessions starting meanwhile don't check too.
    let dated = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&file)
        .and_then(|f| f.set_modified(SystemTime::now()));
    if let (Ok(()), Ok(exe)) = (dated, std::env::current_exe()) {
        spawn_detached(Command::new(exe).arg("update-check"));
    }
}

fn due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.and_then(|t| now.duration_since(t).ok())
        .is_none_or(|age| age >= EVERY)
}

// The agent reads the hook's reply until its stdout closes: a child holding it would make the session wait.
fn spawn_detached(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        keep_own_handles();
    }
    let _ = command.spawn();
}

// Windows hands every inheritable handle to a child, the hook's stdout included, whatever its own stdio.
#[cfg(windows)]
fn keep_own_handles() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

    let handles = [
        std::io::stdin().as_raw_handle(),
        std::io::stdout().as_raw_handle(),
        std::io::stderr().as_raw_handle(),
    ];
    for handle in handles {
        // SAFETY: a standard handle stays valid for the process' life; a missing one only makes the call fail.
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
    }
}

// Fetches the latest release's version and records it; None offline, on failure or without curl.
pub fn fetch() -> Option<String> {
    let file = file()?;
    let url = std::env::var(RELEASE_ENV).unwrap_or_else(|_| RELEASE.to_string());
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "10", &url])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let release: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let tag = release["tag_name"].as_str()?;
    let version = Version::parse(tag.trim_start_matches('v'))
        .ok()?
        .to_string();
    write_atomic(&file, &version).ok()?;
    Some(version)
}

pub fn notice(version: &str) -> String {
    format!("{version} is out: run the installer again")
}

// For `petit-poucet check`.
pub fn report() -> Option<String> {
    let latest = fetch()?;
    Some(match newer_than(&latest, CURRENT) {
        Some(version) => format!("petit-poucet {CURRENT}: {}", notice(&version)),
        None => format!("petit-poucet {CURRENT} is the latest release"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_later_version_is_newer() {
        assert_eq!(newer_than("0.3.1\n", "0.3.0"), Some("0.3.1".into()));
        assert_eq!(newer_than("0.10.0", "0.9.0"), Some("0.10.0".into()));
        assert_eq!(newer_than("0.3.0", "0.3.0"), None);
        assert_eq!(newer_than("0.2.2", "0.3.0"), None);
        assert_eq!(newer_than("", "0.3.0"), None, "not checked yet");
        assert_eq!(newer_than("garbage", "0.3.0"), None);
    }

    #[test]
    fn checks_at_most_once_a_day() {
        let now = SystemTime::now();
        let hours = |h: u64| now - Duration::from_secs(h * 60 * 60);
        assert!(due(None, now), "never checked");
        assert!(!due(Some(hours(1)), now));
        assert!(!due(Some(hours(23)), now));
        assert!(due(Some(hours(24)), now));
        assert!(due(Some(now + EVERY), now), "a date in the future");
    }
}
