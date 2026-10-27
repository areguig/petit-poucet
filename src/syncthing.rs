use std::io::ErrorKind;
use std::process::Command;

use serde_json::Value;

pub enum Unavailable {
    NotInstalled,
    NotRunning(String),
}

// The user's own Syncthing, reached through its CLI: it finds the running instance's address and API key itself.
pub struct Syncthing {
    pub my_id: String,
    pub config: Value,
}

fn cli(args: &[&str]) -> Result<String, Unavailable> {
    let output = Command::new("syncthing")
        .arg("cli")
        .args(args)
        .output()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => Unavailable::NotInstalled,
            _ => Unavailable::NotRunning(e.to_string()),
        })?;
    match output.status.success() {
        true => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        false => Err(Unavailable::NotRunning(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        )),
    }
}

fn json(args: &[&str]) -> Result<Value, Unavailable> {
    serde_json::from_str(&cli(args)?).map_err(|e| Unavailable::NotRunning(e.to_string()))
}

pub fn connect() -> Result<Syncthing, Unavailable> {
    let system = json(&["show", "system"])?;
    let my_id = system["myID"]
        .as_str()
        .ok_or_else(|| {
            Unavailable::NotRunning("no device ID in `syncthing cli show system`".into())
        })?
        .to_string();
    let config = json(&["config", "dump-json"])?;
    Ok(Syncthing { my_id, config })
}

// Changes made once Syncthing answered: a failure here is an error, not a missing install.
fn change(args: &[&str]) -> Result<(), String> {
    cli(args).map(drop).map_err(|e| match e {
        Unavailable::NotInstalled => "syncthing is no longer on the PATH".to_string(),
        Unavailable::NotRunning(e) => format!("syncthing cli {}: {e}", args.join(" ")),
    })
}

pub fn add_folder(id: &str, label: &str, path: &str, watch_delay_s: u32) -> Result<(), String> {
    change(&[
        "config",
        "folders",
        "add",
        "--id",
        id,
        "--label",
        label,
        "--path",
        path,
        "--type",
        "sendreceive",
        "--fswatcher-delays",
        &watch_delay_s.to_string(),
    ])
}

pub fn add_device(id: &str) -> Result<(), String> {
    change(&["config", "devices", "add", "--device-id", id])
}

pub fn share(folder: &str, device: &str) -> Result<(), String> {
    change(&[
        "config",
        "folders",
        folder,
        "devices",
        "add",
        "--device-id",
        device,
    ])
}

// Device IDs this machine is connected to right now.
pub fn connected() -> Vec<String> {
    let Ok(connections) = json(&["show", "connections"]) else {
        return Vec::new();
    };
    connections["connections"]
        .as_object()
        .map(|all| {
            all.iter()
                .filter(|(_, c)| c["connected"] == true)
                .map(|(id, _)| id.clone())
                .collect()
        })
        .unwrap_or_default()
}
