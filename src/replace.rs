use std::io;
use std::path::Path;

// Replaces the running binary, at `exe`, with `new`.
#[cfg(not(windows))]
pub fn replace(new: &Path, _exe: &Path) -> io::Result<()> {
    self_replace::self_replace(new)
}

#[cfg(windows)]
pub fn replace(new: &Path, exe: &Path) -> io::Result<()> {
    retry_while_unchanged(exe, std::thread::sleep, || self_replace::self_replace(new))
}

// Lets a transient sharing violation clear, yet fails an interactive command quickly when it lasts.
// No attempt starts after it; an attempt already running is not cut short.
#[cfg(windows)]
const SHARING_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

// Only self_replace's first step, moving `exe` aside, leaves it in place when it fails: a sharing violation with
// `exe` still the file opened before the first attempt (kept open so its ID can't be reused) changed nothing.
#[cfg(windows)]
fn retry_while_unchanged(
    exe: &Path,
    mut wait: impl FnMut(std::time::Duration),
    mut attempt: impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;

    let installed = open_identity(exe).ok();
    let installed_id = installed.as_ref().and_then(|file| file_id(file).ok());
    let deadline = Instant::now() + SHARING_WAIT;
    // The deadline is checked last: reading the identity can block, and must not let a late retry through.
    let retryable = |error: &io::Error| {
        error.raw_os_error() == Some(ERROR_SHARING_VIOLATION as i32)
            && installed_id.is_some_and(|installed| {
                open_identity(exe).and_then(|now| file_id(&now)).ok() == Some(installed)
            })
            && Instant::now() < deadline
    };
    let mut told = false;
    loop {
        let error = match attempt() {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        if !retryable(&error) {
            return Err(error);
        }
        if !told {
            eprintln!(
                "petit-poucet: the binary is in use, trying again for up to {} s",
                SHARING_WAIT.as_secs()
            );
            told = true;
        }
        wait(Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())));
        if !retryable(&error) {
            return Err(error);
        }
    }
}

// Reading attributes only, sharing everything: it never blocks self_replace from moving or deleting `exe`.
#[cfg(windows)]
fn open_identity(path: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    std::fs::File::options()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)
}

#[cfg(windows)]
type FileId = (u64, [u8; 16]);

// The volume serial and the full 128-bit ID: the 64-bit file index is not unique on ReFS.
#[cfg(windows)]
fn file_id(file: &std::fs::File) -> io::Result<FileId> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    let mut info = FILE_ID_INFO::default();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&raw mut info).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(id_of(&info))
}

#[cfg(windows)]
fn id_of(info: &windows_sys::Win32::Storage::FileSystem::FILE_ID_INFO) -> FileId {
    (info.VolumeSerialNumber, info.FileId.Identifier)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION};
    use windows_sys::Win32::Storage::FileSystem::FILE_ID_INFO;

    fn sharing_violation() -> io::Error {
        io::Error::from_raw_os_error(ERROR_SHARING_VIOLATION as i32)
    }

    fn installed() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("petit-poucet.exe");
        fs::write(&exe, "old").unwrap();
        (dir, exe)
    }

    fn id_at(path: &Path) -> FileId {
        file_id(&open_identity(path).unwrap()).unwrap()
    }

    fn no_wait(_: Duration) {}

    fn moved_aside(exe: &Path) {
        if exe.exists() {
            fs::rename(exe, exe.with_extension("relocated")).unwrap();
        }
    }

    fn replaced_by_the_same_bytes(exe: &Path) {
        let before = id_at(exe);
        let other = exe.with_extension("other");
        fs::write(&other, "old").unwrap();
        fs::rename(exe, exe.with_extension("relocated")).unwrap();
        fs::rename(&other, exe).unwrap();
        assert_ne!(id_at(exe), before);
    }

    #[test]
    fn a_sharing_violation_that_changed_nothing_is_tried_again() {
        let (_dir, exe) = installed();
        let mut attempts = 0;
        let result = retry_while_unchanged(&exe, no_wait, || {
            attempts += 1;
            match attempts {
                1 => Err(sharing_violation()),
                _ => Ok(()),
            }
        });
        assert!(result.is_ok());
        assert_eq!(attempts, 2);
    }

    #[test]
    fn the_identity_guard_does_not_stop_the_binary_being_moved_aside() {
        let (_dir, exe) = installed();
        let result = retry_while_unchanged(&exe, no_wait, || {
            fs::rename(&exe, exe.with_extension("relocated"))?;
            fs::write(&exe, "new")
        });
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(fs::read(&exe).unwrap(), b"new");
    }

    // Each attempt fails with a sharing violation after `in_attempt` ran; `in_wait` runs while waiting to retry.
    fn attempts_with(in_attempt: impl Fn(&Path), in_wait: impl Fn(&Path)) -> usize {
        let (_dir, exe) = installed();
        let mut attempts = 0;
        let result = retry_while_unchanged(
            &exe,
            |_| in_wait(&exe),
            || {
                attempts += 1;
                in_attempt(&exe);
                Err(sharing_violation())
            },
        );
        assert_eq!(
            result.unwrap_err().raw_os_error(),
            Some(ERROR_SHARING_VIOLATION as i32)
        );
        attempts
    }

    #[test]
    fn a_sharing_violation_after_the_binary_moved_aside_is_not_tried_again() {
        assert_eq!(attempts_with(moved_aside, |_| {}), 1);
    }

    #[test]
    fn a_sharing_violation_after_another_file_took_its_place_is_not_tried_again() {
        assert_eq!(attempts_with(replaced_by_the_same_bytes, |_| {}), 1);
    }

    #[test]
    fn a_binary_moved_aside_during_the_wait_is_not_tried_again() {
        assert_eq!(attempts_with(|_| {}, moved_aside), 1);
    }

    #[test]
    fn a_binary_replaced_during_the_wait_is_not_tried_again() {
        let replaced_once = std::cell::Cell::new(false);
        let replace_once = |exe: &Path| {
            if !replaced_once.replace(true) {
                replaced_by_the_same_bytes(exe);
            }
        };
        assert_eq!(attempts_with(|_| {}, replace_once), 1);
    }

    #[test]
    fn a_binary_whose_identity_cannot_be_read_is_tried_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut attempts = 0;
        let result = retry_while_unchanged(&dir.path().join("missing.exe"), no_wait, || {
            attempts += 1;
            Err(sharing_violation())
        });
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn another_error_is_not_tried_again() {
        let (_dir, exe) = installed();
        let mut attempts = 0;
        let result = retry_while_unchanged(&exe, no_wait, || {
            attempts += 1;
            Err(io::Error::from_raw_os_error(ERROR_ACCESS_DENIED as i32))
        });
        assert_eq!(
            result.unwrap_err().raw_os_error(),
            Some(ERROR_ACCESS_DENIED as i32)
        );
        assert_eq!(attempts, 1);
    }

    #[test]
    fn file_ids_differing_only_in_their_high_64_bits_are_different_files() {
        let low = FILE_ID_INFO::default();
        let mut high = low;
        high.FileId.Identifier[15] = 1;
        assert_ne!(id_of(&low), id_of(&high));
    }
}
