use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

pub fn is_repo(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--git-dir"]).is_ok()
}

pub fn init(dir: &Path) -> Result<(), String> {
    git(dir, &["init", "--quiet"]).map(drop)
}

// Commits only `paths`: whatever else the user staged stays staged.
pub fn commit(dir: &Path, paths: &[&str], message: &str) -> Result<(), String> {
    let indexed = git(dir, &[&["ls-files", "-z", "--"][..], paths].concat())?;
    let known = on_disk_or_listed(dir, paths, &indexed);
    if known.is_empty() {
        return Ok(());
    }
    git(dir, &[&["add", "--all", "--"][..], &known[..]].concat())?;
    let diff = [
        "diff",
        "--cached",
        "--name-only",
        "--no-renames",
        "-z",
        "--",
    ];
    let staged = git(dir, &[&diff[..], &known[..]].concat())?;
    if staged.is_empty() {
        return Ok(());
    }
    let changed = on_disk_or_listed(dir, &known, &staged);
    git(
        dir,
        &[
            &["commit", "--quiet", "-m", message, "--"][..],
            &changed[..],
        ]
        .concat(),
    )
    .map(drop)
}

// git refuses a pathspec matching nothing, as a removed file it never committed.
fn on_disk_or_listed<'a>(dir: &Path, paths: &[&'a str], listed: &str) -> Vec<&'a str> {
    let listed: Vec<&str> = listed.split_terminator('\0').collect();
    paths
        .iter()
        .copied()
        .filter(|path| dir.join(path).exists() || listed.contains(path))
        .collect()
}

// Files with uncommitted changes. Takes no git lock, so it never collides with a commit running beside it.
pub fn changed(dir: &Path) -> Result<Vec<String>, String> {
    let out = git(
        dir,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
        ],
    )?;
    Ok(out
        .split('\0')
        .filter_map(|entry| entry.get(3..))
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn toplevel(dir: &Path) -> Option<String> {
    git(dir, &["rev-parse", "--show-toplevel"])
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn remote_urls(dir: &Path) -> Vec<String> {
    let Ok(out) = git(dir, &["remote", "-v"]) else {
        return Vec::new();
    };
    let mut urls: Vec<String> = out
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(str::to_string)
        .collect();
    urls.dedup();
    urls
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn a_removed_file_git_never_committed_adds_nothing_to_the_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        init(dir).unwrap();
        git(dir, &["config", "user.name", "test"]).unwrap();
        git(dir, &["config", "user.email", "test@example.com"]).unwrap();
        fs::write(dir.join("kept.md"), "kept").unwrap();
        fs::write(dir.join("staged.md"), "staged").unwrap();
        git(dir, &["add", "staged.md"]).unwrap();
        fs::remove_file(dir.join("staged.md")).unwrap();

        commit(dir, &["kept.md", "untracked.md", "staged.md"], "first").unwrap();
        commit(dir, &["untracked.md", "staged.md"], "second").unwrap();
        assert_eq!(git(dir, &["log", "--format=%s"]).unwrap(), "first\n");
        assert_eq!(git(dir, &["ls-files"]).unwrap(), "kept.md\n");
        assert_eq!(changed(dir).unwrap(), Vec::<String>::new());
    }
}
