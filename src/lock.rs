use std::fs::{File, OpenOptions};
use std::path::Path;

use crate::state;

// Every agent runs its own server: vault changes take this lock, released when the file is dropped.
pub fn vault(root: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state::dir(root)?.join("lock"))
        .map_err(|e| format!("cannot open the vault lock: {e}"))?;
    file.lock()
        .map_err(|e| format!("cannot lock the vault: {e}"))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_holder_at_a_time_until_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let held = vault(tmp.path()).unwrap();
        let other = File::open(tmp.path().join(state::DIR).join("lock")).unwrap();
        assert!(other.try_lock().is_err(), "held by the first handle");
        drop(held);
        assert!(other.try_lock().is_ok(), "released on drop");
    }
}
