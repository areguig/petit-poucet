// install.ps1 is for Windows: it runs on the Windows CI runner, in Windows PowerShell 5.1 and PowerShell 7.
#![cfg(windows)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const TARGET: &str = if cfg!(target_arch = "aarch64") {
    "aarch64-pc-windows-msvc"
} else {
    "x86_64-pc-windows-msvc"
};

fn shells() -> Vec<&'static str> {
    ["powershell", "pwsh"]
        .into_iter()
        .filter(|shell| Command::new(shell).arg("-Help").output().is_ok())
        .collect()
}

fn powershell(shell: &str, script: &str) -> String {
    let out = Command::new(shell)
        .args(["-NoProfile", "-Command", script])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// A release mirror laid out like GitHub's, holding this build as the release.
fn mirror(dir: &Path, release: &str, checksum: Option<&str>) {
    let asset = dir.join(release).join(format!("petit-poucet-{TARGET}.exe"));
    fs::create_dir_all(asset.parent().unwrap()).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_petit-poucet"), &asset).unwrap();
    let sha = checksum.map(str::to_string).unwrap_or_else(|| {
        powershell(
            "powershell",
            &format!(
                "-join ([System.Security.Cryptography.SHA256]::Create().ComputeHash([System.IO.File]::ReadAllBytes('{}')) | ForEach-Object {{ $_.ToString('x2') }})",
                asset.display()
            ),
        )
    });
    fs::write(
        asset.with_extension("exe.sha256"),
        format!("{sha}  petit-poucet-{TARGET}.exe\n"),
    )
    .unwrap();
}

fn install(shell: &str, home: &Path, envs: &[(&str, &str)]) -> Output {
    let mirror = format!(
        "file:///{}",
        home.join("mirror").display().to_string().replace('\\', "/")
    );
    let mut cmd = Command::new(shell);
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(Path::new(ROOT).join("install.ps1"))
        .env("PETIT_POUCET_RELEASES", mirror)
        .env("PETIT_POUCET_INSTALL_DIR", home.join("bin"))
        .env("PETIT_POUCET_NO_MODIFY_PATH", "1");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn installs_the_latest_release_then_a_chosen_version() {
    for shell in shells() {
        let home = TempDir::new().unwrap();
        mirror(&home.path().join("mirror"), "latest/download", None);
        let output = install(shell, home.path(), &[]);
        let log = stderr(&output);
        assert!(output.status.success(), "{shell}: {log}");
        let exe = home.path().join("bin/petit-poucet.exe");
        assert!(exe.is_file(), "{shell}: {log}");
        assert!(log.contains("installed petit-poucet "), "{shell}: {log}");
        assert!(log.contains("is not on your PATH"), "{shell}: {log}");
        assert!(
            log.contains("next: run `petit-poucet setup`"),
            "{shell}: {log}"
        );

        mirror(&home.path().join("mirror"), "download/v1.2.3", None);
        let pinned = install(shell, home.path(), &[("PETIT_POUCET_VERSION", "v1.2.3")]);
        assert!(
            stderr(&pinned).contains("/download/v1.2.3/"),
            "{shell}: {}",
            stderr(&pinned)
        );
        assert!(pinned.status.success(), "{shell}");
    }
}

#[test]
fn refuses_a_binary_with_a_wrong_checksum() {
    for shell in shells() {
        let home = TempDir::new().unwrap();
        mirror(&home.path().join("mirror"), "latest/download", Some("0000"));
        let output = install(shell, home.path(), &[]);
        assert!(!output.status.success(), "{shell}");
        assert!(
            stderr(&output).contains("checksum mismatch"),
            "{shell}: {}",
            stderr(&output)
        );
        let bin = home.path().join("bin");
        assert_eq!(
            fs::read_dir(&bin).unwrap().count(),
            0,
            "{shell}: nothing left in {}",
            bin.display()
        );
    }
}
