use std::path::Path;
use std::time::Duration;

use super::report::{Item, Status, quote};
use crate::update;

pub fn on_path(exe: &Path, found: Option<&Path>) -> Item {
    if found.is_some_and(|found| same_file::is_same_file(exe, found).unwrap_or(false)) {
        return Item::new("binary PATH", Status::Ok, "this binary", None);
    }
    let parent = exe.parent().expect("current executable has a parent");
    let fix = if cfg!(windows) {
        format!("$env:Path = {} + ';' + $env:Path", quote(parent))
    } else {
        format!("export PATH={}:\"$PATH\"", quote(parent))
    };
    Item::new(
        "binary PATH",
        Status::Error,
        "petit-poucet is missing or points to another binary",
        Some(fix),
    )
}

pub fn release() -> (Item, Option<String>) {
    if std::env::var_os(update::OFF).is_some() {
        return (
            Item::new("release", Status::Info, "lookup disabled", None),
            None,
        );
    }
    release_status(update::latest_with_timeout(Duration::from_secs(2)))
}

fn release_status(latest: Option<semver::Version>) -> (Item, Option<String>) {
    let Some(latest) = latest else {
        return (
            Item::new(
                "release",
                Status::Info,
                "unavailable (offline or lookup failed)",
                None,
            ),
            None,
        );
    };
    let current = semver::Version::parse(update::CURRENT).expect("package version is semver");
    if latest > current {
        return (
            Item::new(
                "release",
                Status::Warning,
                format!("{latest} is available"),
                Some("petit-poucet update".into()),
            ),
            Some(update::notice(&latest.to_string())),
        );
    }
    (
        Item::new(
            "release",
            Status::Ok,
            format!("no release newer than {}", update::CURRENT),
            None,
        ),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_reports_the_repair_and_accepts_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("petit-poucet");
        std::fs::write(&exe, "binary").unwrap();
        let missing = on_path(&exe, None);
        assert_eq!(missing.status, Status::Error);
        assert!(missing.fix.unwrap().contains(if cfg!(windows) {
            "$env:Path"
        } else {
            "export PATH="
        }));
        assert_eq!(on_path(&exe, Some(&exe)).status, Status::Ok);
        let other = dir.path().join("other");
        std::fs::write(&other, "other binary").unwrap();
        assert_eq!(on_path(&exe, Some(&other)).status, Status::Error);
    }

    #[test]
    fn releases_warn_only_when_newer_and_name_update() {
        let (newer, notice) = release_status(Some(semver::Version::new(99, 0, 0)));
        assert_eq!(newer.status, Status::Warning);
        assert_eq!(newer.fix.as_deref(), Some("petit-poucet update"));
        assert!(notice.is_some());
        assert_eq!(
            release_status(Some(semver::Version::parse(update::CURRENT).unwrap()))
                .0
                .status,
            Status::Ok
        );
        assert_eq!(release_status(None).0.status, Status::Info);
    }
}
