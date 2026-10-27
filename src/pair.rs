use std::path::Path;

use serde_json::Value;

use crate::config::Config;
use crate::init;
use crate::syncthing::{self, Syncthing, Unavailable};

// The same folder ID on every machine, so they share it with no folder offer to accept.
const FOLDER: &str = "petit-poucet";
const LABEL: &str = "petit-poucet memory";
// Saves replace a note by rename, which macOS reports as a removal, and Syncthing holds removals for six
// times this delay (a minute with its default of 10 s). A vault is small, so a short delay costs little.
const WATCH_DELAY_S: u32 = 1;

#[derive(Debug, PartialEq)]
enum Change {
    AddFolder,
    AddDevice(String),
    Share(String),
}

// Device IDs are pasted in any case, with or without dashes.
fn same_device(a: &str, b: &str) -> bool {
    let normal = |id: &str| -> String {
        id.chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .collect()
    };
    normal(a) == normal(b)
}

fn same_path(path: &Value, vault: &Path) -> bool {
    path.as_str()
        .and_then(|p| dunce::canonicalize(p).ok())
        .is_some_and(|p| p == vault)
}

fn has_device(list: &Value, device: &str) -> bool {
    list.as_array().is_some_and(|all| {
        all.iter().any(|d| {
            d["deviceID"]
                .as_str()
                .is_some_and(|id| same_device(id, device))
        })
    })
}

// What Syncthing's config lacks for the vault to be shared, and shared with `device` when one is given.
fn plan(
    config: &Value,
    my_id: &str,
    vault: &Path,
    device: Option<&str>,
) -> Result<Vec<Change>, String> {
    let folders = config["folders"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if let Some(other) = folders
        .iter()
        .find(|f| f["id"] != FOLDER && same_path(&f["path"], vault))
    {
        return Err(format!(
            "Syncthing already shares the vault as folder `{}`: remove that folder in Syncthing (keep the files), then run `petit-poucet sync` again",
            other["id"].as_str().unwrap_or_default()
        ));
    }
    let ours = folders.iter().find(|f| f["id"] == FOLDER);
    if let Some(folder) = ours
        && !same_path(&folder["path"], vault)
    {
        return Err(format!(
            "Syncthing's folder `{FOLDER}` is at {}, not at the vault {}",
            folder["path"].as_str().unwrap_or_default(),
            vault.display()
        ));
    }
    let mut changes = Vec::new();
    if ours.is_none() {
        changes.push(Change::AddFolder);
    }
    let Some(device) = device else {
        return Ok(changes);
    };
    if same_device(device, my_id) {
        return Err("that is this machine's device ID: run this on the other machine".to_string());
    }
    if !has_device(&config["devices"], device) {
        changes.push(Change::AddDevice(device.to_string()));
    }
    if !ours.is_some_and(|folder| has_device(&folder["devices"], device)) {
        changes.push(Change::Share(device.to_string()));
    }
    Ok(changes)
}

// Syncthing's short form: the first 7 characters, upper case, as its own UI shows them.
fn short(id: &str) -> String {
    id.chars().take(7).collect::<String>().to_ascii_uppercase()
}

// The machines the vault is shared with, other than this one.
fn peers(syncthing: &Syncthing) -> Vec<String> {
    let folders = syncthing.config["folders"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    folders
        .iter()
        .filter(|f| f["id"] == FOLDER)
        .flat_map(|f| f["devices"].as_array().into_iter().flatten())
        .filter_map(|d| d["deviceID"].as_str())
        .filter(|id| !same_device(id, &syncthing.my_id))
        .map(str::to_string)
        .collect()
}

pub fn run(config: &Config, device: Option<&str>) -> Result<(Vec<String>, bool), String> {
    // Before sharing anything, so git history and the Index never leave this machine.
    let warning = init::update_vault_files(config)?;
    let mut lines: Vec<String> = warning
        .iter()
        .flat_map(|w| w.lines())
        .map(str::to_string)
        .collect();
    let syncthing = match syncthing::connect() {
        Ok(syncthing) => syncthing,
        Err(Unavailable::NotInstalled) => {
            lines.extend(not_installed(&Host::current()));
            return Ok((lines, false));
        }
        Err(Unavailable::NotRunning(e)) => {
            lines.extend(not_running(&Host::current(), &e));
            return Ok((lines, false));
        }
    };
    let vault = config.vault.to_string_lossy();
    for change in plan(&syncthing.config, &syncthing.my_id, &config.vault, device)? {
        match change {
            Change::AddFolder => {
                syncthing::add_folder(FOLDER, LABEL, &vault, WATCH_DELAY_S)?;
                lines.push(format!("vault shared in Syncthing: {vault}"));
            }
            Change::AddDevice(id) => syncthing::add_device(&id)?,
            Change::Share(id) => {
                syncthing::share(FOLDER, &id)?;
                lines.push(format!(
                    "paired with {}: run the command below on it too, if not done yet",
                    short(&id)
                ));
            }
        }
    }
    let syncthing = syncthing::connect().map_err(|_| "Syncthing stopped answering".to_string())?;
    let connected = syncthing::connected();
    let peers: Vec<String> = peers(&syncthing)
        .iter()
        .map(|id| match connected.iter().any(|c| same_device(c, id)) {
            true => format!("{} (connected)", short(id)),
            false => format!("{} (not connected yet)", short(id)),
        })
        .collect();
    lines.push(match peers.is_empty() {
        true => "not shared with another machine yet".to_string(),
        false => format!("syncing with: {}", peers.join(", ")),
    });
    lines.push(format!(
        "on another machine, run: petit-poucet sync {}",
        syncthing.my_id
    ));
    Ok((lines, true))
}

// What the hints depend on, so each OS's hint is tested on any runner.
struct Host {
    os: &'static str,
    on_path: Box<dyn Fn(&str) -> bool>,
}

impl Host {
    fn current() -> Host {
        Host {
            os: std::env::consts::OS,
            on_path: Box::new(|program| {
                std::env::var_os("PATH").is_some_and(|path| {
                    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
                })
            }),
        }
    }
}

const AUTOSTART: &str = "https://docs.syncthing.net/users/autostart.html";

fn install(host: &Host) -> String {
    let managers = [
        ("apt", "sudo apt install syncthing"),
        ("dnf", "sudo dnf install syncthing"),
        ("pacman", "sudo pacman -S syncthing"),
        ("zypper", "sudo zypper install syncthing"),
        ("apk", "sudo apk add syncthing"),
    ];
    match host.os {
        "macos" => "brew install syncthing".to_string(),
        "windows" => "winget install Syncthing.Syncthing".to_string(),
        _ => managers
            .iter()
            .find(|(program, _)| (host.on_path)(program))
            .map_or(
                "install syncthing with your package manager: https://syncthing.net/downloads/"
                    .to_string(),
                |(_, command)| command.to_string(),
            ),
    }
}

fn start(host: &Host) -> Vec<String> {
    match host.os {
        "macos" => vec!["brew services start syncthing".to_string()],
        "windows" => vec![format!(
            "start it at login: {AUTOSTART}#autostart-windows-taskschd"
        )],
        _ if (host.on_path)("systemctl") => vec![
            "systemctl --user enable --now syncthing.service".to_string(),
            "on a server, keep it running after you log out: sudo loginctl enable-linger $USER"
                .to_string(),
        ],
        _ => vec![format!("start it at login: {AUTOSTART}")],
    }
}

fn not_installed(host: &Host) -> Vec<String> {
    let mut lines = vec![
        "Syncthing is not installed. Install it, start it at login, then run `petit-poucet sync` again:".to_string(),
        format!("  {}", install(host)),
    ];
    lines.extend(start(host).into_iter().map(|line| format!("  {line}")));
    lines
}

fn not_running(host: &Host, error: &str) -> Vec<String> {
    let mut lines = vec![format!(
        "Syncthing is installed but doesn't answer ({error}). Start it at login, then run `petit-poucet sync` again:"
    )];
    lines.extend(start(host).into_iter().map(|line| format!("  {line}")));
    lines
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const ME: &str = "AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA";
    const OTHER: &str = "BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB";

    fn vault() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let vault = dunce::canonicalize(tmp.path()).unwrap();
        (tmp, vault)
    }

    #[test]
    fn a_new_machine_shares_the_vault_then_pairs() {
        let (_tmp, vault) = vault();
        let empty = json!({"folders": [], "devices": [{"deviceID": ME}]});
        assert_eq!(plan(&empty, ME, &vault, None).unwrap(), [Change::AddFolder]);
        assert_eq!(
            plan(
                &empty,
                ME,
                &vault,
                Some("bbbbbbb bbbbbbb-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB")
            )
            .unwrap(),
            [
                Change::AddFolder,
                Change::AddDevice(
                    "bbbbbbb bbbbbbb-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB".into()
                ),
                Change::Share(
                    "bbbbbbb bbbbbbb-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB-BBBBBBB".into()
                ),
            ]
        );
    }

    #[test]
    fn running_it_again_changes_nothing() {
        let (_tmp, vault) = vault();
        let paired = json!({
            "folders": [{"id": FOLDER, "path": vault, "devices": [{"deviceID": ME}, {"deviceID": OTHER}]}],
            "devices": [{"deviceID": ME}, {"deviceID": OTHER}],
        });
        assert_eq!(plan(&paired, ME, &vault, None).unwrap(), []);
        assert_eq!(
            plan(&paired, ME, &vault, Some(&OTHER.to_lowercase())).unwrap(),
            []
        );
    }

    #[test]
    fn a_known_device_only_gets_the_folder() {
        let (_tmp, vault) = vault();
        let config = json!({
            "folders": [{"id": FOLDER, "path": vault, "devices": [{"deviceID": ME}]}],
            "devices": [{"deviceID": ME}, {"deviceID": OTHER}],
        });
        assert_eq!(
            plan(&config, ME, &vault, Some(OTHER)).unwrap(),
            [Change::Share(OTHER.into())]
        );
    }

    #[test]
    fn refuses_what_would_never_sync() {
        let (_tmp, vault) = vault();
        let elsewhere = tempfile::tempdir().unwrap();
        let by_hand = json!({"folders": [{"id": "abcd-1234", "path": vault}], "devices": []});
        let err = plan(&by_hand, ME, &vault, None).unwrap_err();
        assert!(
            err.starts_with("Syncthing already shares the vault as folder `abcd-1234`"),
            "{err}"
        );
        let moved = json!({"folders": [{"id": FOLDER, "path": elsewhere.path()}], "devices": []});
        let err = plan(&moved, ME, &vault, None).unwrap_err();
        assert!(
            err.starts_with("Syncthing's folder `petit-poucet` is at"),
            "{err}"
        );
        let err = plan(&json!({}), ME, &vault, Some(&ME.replace('-', ""))).unwrap_err();
        assert!(err.starts_with("that is this machine's device ID"), "{err}");
    }

    fn host(os: &'static str, programs: &'static [&'static str]) -> Host {
        Host {
            os,
            on_path: Box::new(move |program| programs.contains(&program)),
        }
    }

    #[test]
    fn hints_name_each_os_s_package_manager_and_autostart() {
        assert_eq!(
            not_installed(&host("macos", &[])),
            [
                "Syncthing is not installed. Install it, start it at login, then run `petit-poucet sync` again:",
                "  brew install syncthing",
                "  brew services start syncthing",
            ]
        );
        assert_eq!(
            not_installed(&host("linux", &["dnf", "systemctl"]))[1..],
            [
                "  sudo dnf install syncthing",
                "  systemctl --user enable --now syncthing.service",
                "  on a server, keep it running after you log out: sudo loginctl enable-linger $USER",
            ]
        );
        assert_eq!(
            not_installed(&host("linux", &["apk"]))[1..],
            [
                "  sudo apk add syncthing",
                "  start it at login: https://docs.syncthing.net/users/autostart.html"
            ]
        );
        assert_eq!(
            not_installed(&host("windows", &[]))[1..],
            [
                "  winget install Syncthing.Syncthing",
                "  start it at login: https://docs.syncthing.net/users/autostart.html#autostart-windows-taskschd",
            ]
        );
        assert_eq!(
            not_running(&host("macos", &[]), "connection refused"),
            [
                "Syncthing is installed but doesn't answer (connection refused). Start it at login, then run `petit-poucet sync` again:",
                "  brew services start syncthing",
            ]
        );
    }
}
