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

    // Repeated, so one CI run on macOS (where a single try was flaky, #52) exercises it many times.
    #[test]
    fn one_holder_at_a_time_until_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        drop(vault(tmp.path()).unwrap());
        let other = File::open(tmp.path().join(state::DIR).join("lock")).unwrap();
        for round in 0..50 {
            let held = vault(tmp.path()).unwrap();
            assert!(
                other.try_lock().is_err(),
                "round {round}: held by the first handle"
            );
            drop(held);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut last = other.try_lock();
            while last.is_err() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
                last = other.try_lock();
            }
            assert!(last.is_ok(), "round {round}: released on drop: {last:?}");
            other.unlock().unwrap();
        }
    }
}
