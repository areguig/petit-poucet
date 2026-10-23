use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::edit;

const SERVER: &str = "petit-poucet";
const EXTENSION: &str = include_str!("../plugins/omp.js");
const EXTENSION_EXE: &str = "__PETIT_POUCET__";

pub fn dir(home: &Path) -> PathBuf {
    home.join(".omp/agent")
}

fn extension_file(dir: &Path) -> PathBuf {
    dir.join(format!("extensions/{SERVER}.js"))
}

// A JSON string is a valid JavaScript string, quotes and backslashes of Windows paths included.
fn extension(exe: &Path) -> String {
    let quoted = serde_json::to_string(&exe.display().to_string()).unwrap();
    EXTENSION.replace(EXTENSION_EXE, &quoted)
}

pub fn setup(dir: &Path, exe: &Path) -> Result<String, String> {
    let mcp_file = dir.join("mcp.json");
    let mut mcp = edit::read_json(&mcp_file)?;
    if !mcp["mcpServers"].is_object() {
        mcp["mcpServers"] = json!({});
    }
    mcp["mcpServers"][SERVER] = json!({"command": exe.display().to_string(), "args": ["serve"]});
    let changed =
        edit::write_json(&mcp_file, &mcp)? | edit::replace(&extension_file(dir), &extension(exe))?;
    Ok(match changed {
        true => {
            "set up (MCP server in mcp.json, extension in extensions/petit-poucet.js)".to_string()
        }
        false => "already set up".to_string(),
    })
}

pub fn check(dir: &Path) -> Result<(String, bool), String> {
    let mcp = edit::read_json(&dir.join("mcp.json"))?;
    let mut missing = Vec::new();
    match mcp["mcpServers"][SERVER]["command"].as_str() {
        None => missing.push("the MCP server".to_string()),
        Some(path) if !Path::new(path).is_file() => {
            missing.push(format!("{path} (the MCP server's binary)"))
        }
        Some(_) => {}
    }
    if !extension_file(dir).is_file() {
        missing.push("the extension".to_string());
    }
    Ok(match missing.is_empty() {
        true => ("set up".to_string(), true),
        false => (
            format!(
                "missing {}: run `petit-poucet setup --agent omp`",
                missing.join(", ")
            ),
            false,
        ),
    })
}

pub fn uninstall(dir: &Path) -> Result<String, String> {
    let mcp_file = dir.join("mcp.json");
    let mut mcp = edit::read_json(&mcp_file)?;
    let had_server = mcp
        .pointer_mut("/mcpServers")
        .and_then(Value::as_object_mut)
        .is_some_and(|servers| servers.remove(SERVER).is_some());
    let mut changed = false;
    if had_server {
        edit::drop_if_empty(&mut mcp, "mcpServers");
        changed |= edit::write_json(&mcp_file, &mcp)?;
    }
    let extension = extension_file(dir);
    if extension.is_file() {
        fs::remove_file(&extension).map_err(|e| format!("{}: {e}", extension.display()))?;
        changed = true;
    }
    Ok(match changed {
        true => "removed (MCP server and extension)".to_string(),
        false => "nothing to remove".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(dir.join("mcp.json")).unwrap()).unwrap()
    }

    #[test]
    fn setup_adds_its_entries_and_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(
            dir.join("mcp.json"),
            r#"{"disabledServers": ["x"], "mcpServers": {"other": {"url": "http://localhost:1"}}}"#,
        )
        .unwrap();
        let exe = Path::new(r"C:\Users\a b\petit-poucet.exe");

        assert!(setup(dir, exe).unwrap().starts_with("set up"));
        let mcp = read(dir);
        assert_eq!(mcp["disabledServers"], json!(["x"]));
        assert_eq!(mcp["mcpServers"]["other"]["url"], "http://localhost:1");
        assert_eq!(
            mcp["mcpServers"]["petit-poucet"],
            json!({"command": r"C:\Users\a b\petit-poucet.exe", "args": ["serve"]})
        );
        assert!(edit::backup_path(&dir.join("mcp.json")).is_file());
        let extension = fs::read_to_string(extension_file(dir)).unwrap();
        assert!(extension.contains(r#"const EXE = "C:\\Users\\a b\\petit-poucet.exe";"#));
        assert!(!extension.contains(EXTENSION_EXE));

        assert_eq!(setup(dir, exe).unwrap(), "already set up");
    }

    #[test]
    fn check_names_what_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert_eq!(
            check(dir).unwrap(),
            (
                "missing the MCP server, the extension: run `petit-poucet setup --agent omp`"
                    .to_string(),
                false
            )
        );
        let exe = dir.join("petit-poucet");
        fs::write(&exe, "").unwrap();
        setup(dir, &exe).unwrap();
        assert_eq!(check(dir).unwrap(), ("set up".to_string(), true));
        fs::remove_file(extension_file(dir)).unwrap();
        assert!(!check(dir).unwrap().1);
    }

    #[test]
    fn uninstall_removes_only_its_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("mcp.json"), r#"{"disabledServers": ["x"]}"#).unwrap();
        setup(dir, Path::new("/opt/pp/petit-poucet")).unwrap();

        assert_eq!(
            uninstall(dir).unwrap(),
            "removed (MCP server and extension)"
        );
        assert_eq!(read(dir), json!({"disabledServers": ["x"]}));
        assert!(!extension_file(dir).exists());
        assert_eq!(uninstall(dir).unwrap(), "nothing to remove");
    }
}
