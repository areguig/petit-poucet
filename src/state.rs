use std::fs;
use std::path::{Path, PathBuf};

use crate::vault::write_atomic;

// petit-poucet's own files, next to the notes but hidden: the vault loader and Obsidian skip dot-folders, git ignores it.
pub const DIR: &str = ".petit-poucet";

pub fn dir(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join(DIR);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Ignores itself, so vaults created before this folder existed keep a clean git status.
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        write_atomic(&ignore, "*\n")?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_folder_that_git_ignores() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = dir(tmp.path()).unwrap();
        assert_eq!(dir, tmp.path().join(DIR));
        assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), "*\n");
        assert_eq!(super::dir(tmp.path()).unwrap(), dir, "idempotent");
    }
}
