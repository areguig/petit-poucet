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

pub fn commit(dir: &Path, paths: &[&str], message: &str) -> Result<(), String> {
    let mut add = vec!["add", "--all", "--"];
    add.extend(paths);
    git(dir, &add)?;
    let mut diff = vec!["diff", "--cached", "--quiet", "--"];
    diff.extend(paths);
    if git(dir, &diff).is_ok() {
        return Ok(());
    }
    let mut commit = vec!["commit", "--quiet", "-m", message, "--"];
    commit.extend(paths);
    git(dir, &commit).map(drop)
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
