// Two real Syncthing instances share one vault (#81), paired by `petit-poucet sync` (#134). Run by the Syncthing CI job:
// SYNCTHING=<path to the binary> cargo test --test syncthing -- --ignored
mod common;

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::{Client, git_log, petit_poucet, review_pages, session_start, start};
use serde_json::json;
use tempfile::TempDir;

const API_KEY: &str = "petit-poucet-test";

fn syncthing_binary() -> PathBuf {
    PathBuf::from(std::env::var_os("SYNCTHING").expect("SYNCTHING names the syncthing binary"))
}

fn syncthing() -> Command {
    Command::new(syncthing_binary())
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

// One machine: a home for petit-poucet, a vault, and a Syncthing that stops with it.
struct Machine {
    home: TempDir,
    vault: PathBuf,
    syncthing_home: PathBuf,
    port: u16,
    gui_port: u16,
    process: Option<Child>,
}

impl Machine {
    fn new() -> Machine {
        let home = TempDir::new().unwrap();
        let vault = home.path().join("vault");
        petit_poucet(home.path())
            .args(["init", vault.to_str().unwrap()])
            .assert()
            .success();
        let vault = dunce::canonicalize(vault).unwrap();
        let syncthing_home = home.path().join("syncthing");
        let generated = syncthing()
            .arg("generate")
            .arg(format!("--home={}", syncthing_home.display()))
            .output()
            .unwrap();
        assert!(generated.status.success(), "{generated:?}");
        let (port, gui_port) = (free_port(), free_port());
        // Loopback only: no discovery, relays or NAT; `petit-poucet sync` adds the folder and the devices.
        let config = format!(
            r#"<configuration version="52">
  <gui enabled="true"><address>127.0.0.1:{gui_port}</address><apikey>{API_KEY}</apikey></gui>
  <options>
    <listenAddress>tcp://127.0.0.1:{port}</listenAddress>
    <globalAnnounceEnabled>false</globalAnnounceEnabled>
    <localAnnounceEnabled>false</localAnnounceEnabled>
    <relaysEnabled>false</relaysEnabled>
    <natEnabled>false</natEnabled>
    <urAccepted>-1</urAccepted>
    <crashReportingEnabled>false</crashReportingEnabled>
    <autoUpgradeIntervalH>0</autoUpgradeIntervalH>
  </options>
</configuration>
"#
        );
        fs::write(syncthing_home.join("config.xml"), config).unwrap();
        Machine {
            home,
            vault,
            syncthing_home,
            port,
            gui_port,
            process: None,
        }
    }

    // `petit-poucet sync` as the user runs it: Syncthing on the PATH, found through its own home.
    fn sync(&self, device: Option<&str>) -> (bool, String) {
        let path = std::env::join_paths(
            std::iter::once(syncthing_binary().parent().unwrap().to_path_buf())
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let output = petit_poucet(self.home.path())
            .env("PATH", path)
            .env("STHOMEDIR", &self.syncthing_home)
            .arg("sync")
            .args(device)
            .output()
            .unwrap();
        (output.status.success(), common::stdout(&output))
    }

    fn answers(&self) -> bool {
        syncthing()
            .args(["cli", "show", "system"])
            .env("STHOMEDIR", &self.syncthing_home)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn device_id(&self) -> String {
        let (ok, out) = self.sync(None);
        assert!(ok, "{out}");
        let line = out.lines().last().unwrap();
        line.strip_prefix("on another machine, run: petit-poucet sync ")
            .unwrap_or_else(|| panic!("{out}"))
            .to_string()
    }

    // What discovery does for real machines. Only one side dials: two machines dialing each other from their
    // listening ports at the same moment merge into one TCP connection that both take as theirs, and the TLS
    // handshake fails until their retries drift apart.
    fn dial(&self, device: &str, other: &Machine) {
        let added = syncthing()
            .args(["cli", "config", "devices", device, "addresses", "add"])
            .arg(format!("tcp://127.0.0.1:{}", other.port))
            .env("STHOMEDIR", &self.syncthing_home)
            .output()
            .unwrap();
        assert!(added.status.success(), "{added:?}");
    }

    fn run(&mut self) {
        let log = fs::File::create(self.home.path().join("syncthing.log")).unwrap();
        let child = syncthing()
            .args(["serve", "--no-browser", "--no-restart", "--no-upgrade"])
            .arg(format!("--home={}", self.syncthing_home.display()))
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        self.process = Some(child);
    }

    // Through its API: killing the process can leave the instance it started holding the folder.
    fn stop(&mut self) {
        let Some(mut child) = self.process.take() else {
            return;
        };
        let stopped = syncthing()
            .args(["cli", "operations", "shutdown"])
            .arg(format!("--gui-address=127.0.0.1:{}", self.gui_port))
            .arg(format!("--gui-apikey={API_KEY}"))
            .status()
            .is_ok_and(|s| s.success());
        if !stopped {
            child.kill().unwrap();
        }
        child.wait().unwrap();
    }

    fn mcp(&self) -> (Child, Client) {
        let (child, client, _) = start(self.home.path());
        (child, client)
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.vault.join(path)).unwrap_or_default()
    }

    fn files(&self, dir: &str) -> Vec<String> {
        fs::read_dir(self.vault.join(dir))
            .map(|entries| {
                entries
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn wait_for(what: &str, machines: &[&Machine], done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !done() {
        if Instant::now() > deadline {
            let logs: Vec<String> = machines
                .iter()
                .map(|m| {
                    fs::read_to_string(m.home.path().join("syncthing.log")).unwrap_or_default()
                })
                .collect();
            panic!("timed out waiting for {what}\n{}", logs.join("\n---\n"));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn save(machine: &Machine, title: &str, summary: &str) {
    let (mut child, mut client) = machine.mcp();
    let (is_error, reply) = client.call(
        "memory_save",
        json!({"type": "user", "scope": "all repos", "title": title, "summary": summary,
               "fact": "A fact.", "source": "test on 2026-10-26", "how_to_apply": "Test."}),
    );
    assert!(!is_error, "{reply}");
    drop(client);
    assert!(child.wait().unwrap().success());
}

fn edit_with_time(machine: &Machine, path: &str, text: &str, modified: std::time::SystemTime) {
    let file = machine.vault.join(path);
    fs::write(&file, text).unwrap();
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

#[test]
#[ignore = "needs a Syncthing binary: run by the Syncthing CI job"]
fn two_machines_share_one_vault_through_syncthing() {
    let mut laptop = Machine::new();
    let mut server = Machine::new();
    let (ok, out) = laptop.sync(None);
    assert!(
        !ok && out.starts_with("Syncthing is installed but doesn't answer"),
        "{out}"
    );
    laptop.run();
    server.run();
    for machine in [&laptop, &server] {
        wait_for("Syncthing to answer", &[&laptop, &server], || {
            machine.answers()
        });
    }

    // Pairing: each machine names the other once; running it again changes nothing.
    let laptop_id = laptop.device_id();
    let server_id = server.device_id();
    let (ok, out) = server.sync(Some(&laptop_id));
    assert!(
        ok && out.contains(&format!("paired with {}", &laptop_id[..7])),
        "{out}"
    );
    let (ok, out) = laptop.sync(Some(&server_id.to_lowercase()));
    assert!(
        ok && out.contains(&format!("paired with {}", &server_id[..7])),
        "{out}"
    );
    let (ok, again) = laptop.sync(Some(&server_id));
    assert!(
        ok && !again.contains("paired with") && !again.contains("vault shared"),
        "{again}"
    );
    assert!(laptop.vault.join(".stignore").is_file());
    laptop.dial(&server_id, &server);

    // A line the server adds to its own .stignore keeps a work project off it (#146), and `sync` shows it.
    let stignore = server.vault.join(".stignore");
    let ignored = format!("{}/Projects/work\n", fs::read_to_string(&stignore).unwrap());
    fs::write(&stignore, ignored).unwrap();
    let (ok, out) = server.sync(None);
    assert!(
        ok && out.contains("not synced with this machine (its .stignore): /Projects/work\n"),
        "{out}"
    );
    fs::create_dir_all(laptop.vault.join("Projects/work")).unwrap();
    fs::write(laptop.vault.join("Projects/work/deploy.md"), "work only").unwrap();

    // A note saved on the laptop reaches the server; git, the Index and petit-poucet's state stay on each machine.
    save(&laptop, "Tabs", "indent with tabs");
    wait_for("the note on the server", &[&laptop, &server], || {
        server
            .read("Preferences/tabs.md")
            .contains("indent with tabs")
    });
    assert!(
        !server.read("Index.md").contains("tabs"),
        "the Index never syncs"
    );
    assert!(!git_log(&server.vault).contains("tabs"), "git never syncs");
    assert!(
        !server.vault.join("Projects/work").exists(),
        "the server's own .stignore keeps it off"
    );
    let (reply, errors) = session_start(server.home.path(), "claude");
    assert!(
        reply.contains("[[Preferences/tabs]] — indent with tabs"),
        "{reply}"
    );
    assert_eq!(errors, "");
    assert!(git_log(&server.vault).starts_with("sync: notes changed outside petit-poucet\n"));
    petit_poucet(server.home.path())
        .arg("check")
        .assert()
        .success();

    // A cleanup on the laptop counts on the server, and reads on both add up.
    let (mut child, mut client) = laptop.mcp();
    client.call("memory_read", json!({"path": "Preferences/tabs"}));
    review_pages(&mut client, false);
    drop(client);
    assert!(child.wait().unwrap().success());
    let cleanup = laptop.read(".last-cleanup");
    // The laptop's usage file exists since its save: wait for the version that holds the read.
    let usage = laptop.files(".usage").pop().unwrap();
    let laptop_usage = laptop.read(&format!(".usage/{usage}"));
    wait_for(
        "the cleanup time and usage on the server",
        &[&laptop, &server],
        || {
            server.read(".last-cleanup") == cleanup
                && server.read(&format!(".usage/{usage}")) == laptop_usage
        },
    );
    save(&server, "Spaces", "spaces in YAML");
    let (mut child, mut client) = server.mcp();
    client.call("memory_read", json!({"path": "Preferences/tabs"}));
    let (pages, _) = review_pages(&mut client, false);
    drop(client);
    assert!(child.wait().unwrap().success());
    assert!(
        pages[0].contains("all 2 notes, 1 changed since the last cleanup ("),
        "{}",
        pages[0]
    );
    assert!(
        pages[0].contains(" | 2r "),
        "1 read on each machine: {}",
        pages[0]
    );
    assert_eq!(
        server.files(".usage").len(),
        2,
        "one usage file per machine"
    );

    // The same note changed on both sides while they can't sync: the copy reaches the server and check names it.
    wait_for(
        "the server's note on the laptop",
        &[&laptop, &server],
        || laptop.vault.join("Preferences/spaces.md").is_file(),
    );
    laptop.stop();
    server.stop();
    let now = std::time::SystemTime::now();
    edit_with_time(
        &laptop,
        "Preferences/tabs.md",
        &laptop
            .read("Preferences/tabs.md")
            .replace("A fact.", "Laptop fact."),
        now - Duration::from_secs(60),
    );
    edit_with_time(
        &server,
        "Preferences/tabs.md",
        &server
            .read("Preferences/tabs.md")
            .replace("A fact.", "Server fact."),
        now,
    );
    laptop.run();
    server.run();
    wait_for("a conflict copy on the server", &[&laptop, &server], || {
        server
            .files("Preferences")
            .iter()
            .any(|f| f.starts_with("tabs.sync-conflict-"))
    });
    let check = petit_poucet(server.home.path())
        .arg("check")
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&check.stdout);
    assert!(!check.status.success());
    assert!(
        out.contains(
            "error: older sync conflict copy of Preferences/tabs.md (lines only in this copy: 1)"
        ),
        "{out}"
    );
}
