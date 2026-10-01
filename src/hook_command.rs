use std::path::Path;

// The `petit-poucet hook` events agents run.
pub const EVENTS: [&str; 2] = ["session-start", "stop"];

// What follows the binary: petit-poucet knows its own entries by it, wherever the binary lives.
pub fn args(event: &str, agent: &str) -> [String; 4] {
    ["hook", event, "--agent", agent].map(String::from)
}

fn args_line(event: &str, agent: &str) -> String {
    format!(" {}", args(event, agent).join(" "))
}

pub fn is_ours(command: &str, agent: &str) -> bool {
    EVENTS
        .iter()
        .any(|event| command.ends_with(&args_line(event, agent)))
}

// For agents that run a hook as one command line: `sh -c` on Unix, `cmd /c` on Windows.
pub fn shell(exe: &Path, event: &str, agent: &str) -> String {
    match cfg!(windows) {
        true => format!("{}{}", windows_path(exe), args_line(event, agent)),
        false => posix(exe, event, agent),
    }
}

// A POSIX shell command line (sh, bash), quoted by `shlex` where needed.
pub fn posix(exe: &Path, event: &str, agent: &str) -> String {
    let path = exe.display().to_string();
    let args = args(event, agent);
    let words = std::iter::once(path.as_str()).chain(args.iter().map(String::as_str));
    shlex::try_join(words).expect("a path has no NUL byte")
}

// PowerShell's rule: inside single quotes, a quote is doubled.
pub fn powershell(exe: &Path, event: &str, agent: &str) -> String {
    let path = exe.display().to_string().replace('\'', "''");
    format!("& '{path}'{}", args_line(event, agent))
}

// Agents escape the command they hand to `cmd /c`, which turns any quote into `\"`: so the path goes
// unquoted, as its 8.3 short form when it has spaces.
fn windows_path(exe: &Path) -> String {
    let path = exe.display().to_string();
    match path.contains(' ') {
        true => short_path(exe).unwrap_or(path),
        false => path,
    }
}

#[cfg(windows)]
fn short_path(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut short = vec![0u16; 32768];
    // SAFETY: `wide` is NUL-terminated and `short` holds as many u16 as the length passed.
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), short.as_mut_ptr(), short.len() as u32) };
    let short = short.get(..len as usize).filter(|s| !s.is_empty())?;
    String::from_utf16(short).ok()
}

#[cfg(not(windows))]
fn short_path(_: &Path) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_recognised_by_their_arguments_only() {
        let ours = shell(Path::new("/any/name"), "stop", "codex");
        assert!(is_ours(&ours, "codex"));
        assert!(is_ours(
            &posix(Path::new("/x"), "session-start", "codex"),
            "codex"
        ));
        assert!(is_ours(
            &powershell(Path::new("/x"), "stop", "codex"),
            "codex"
        ));
        assert!(!is_ours(&ours, "cursor"), "another agent's entry");
        assert!(!is_ours("notify --agent codex", "codex"));
        assert_eq!(args("stop", "codex"), ["hook", "stop", "--agent", "codex"]);
    }

    #[test]
    fn posix_quotes_the_path_only_when_needed() {
        assert_eq!(
            posix(Path::new("/opt/pp"), "stop", "codex"),
            "/opt/pp hook stop --agent codex"
        );
        assert!(posix(Path::new("/opt/my tools/pp"), "stop", "codex").starts_with('\''));
    }

    #[cfg(not(windows))]
    #[test]
    fn the_shell_command_is_a_posix_command_line_off_windows() {
        let exe = Path::new("/opt/my tools/pp");
        assert_eq!(shell(exe, "stop", "codex"), posix(exe, "stop", "codex"));
    }

    #[cfg(windows)]
    #[test]
    fn the_shell_command_has_no_quotes_or_spaces_on_windows() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("petit poucet").join("petit-poucet.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        let ours = shell(&exe, "stop", "codex");
        let path = ours.strip_suffix(" hook stop --agent codex").unwrap();
        assert!(!path.contains(' ') && !path.contains('"'), "{ours}");
        assert!(Path::new(path).is_file(), "{path} names the same file");
    }

    #[test]
    fn powershell_doubles_a_quote_in_the_path() {
        assert_eq!(
            powershell(Path::new("/opt/it's/pp"), "stop", "copilot"),
            "& '/opt/it''s/pp' hook stop --agent copilot"
        );
    }

    // The command `posix` writes, run by a real shell.
    #[cfg(unix)]
    #[test]
    fn a_shell_runs_the_posix_command_whatever_the_path_holds() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("it's $HOME \"here\"");
        std::fs::write(&exe, "#!/bin/sh\necho \"$@\"\n").unwrap();
        std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(posix(&exe, "stop", "copilot"))
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "hook stop --agent copilot\n"
        );
    }
}
