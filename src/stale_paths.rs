use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::note::{Note, Place};

// Words joined by slashes in backticks; `is_file_or_folder` keeps the ones that name a file or folder of the checkout.
static PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([\w.-]+(?:/[\w.-]+)*/[\w.-]*)`").unwrap());

// Paths a project note names that are missing from that project's checkout on this machine; projects never seen here are skipped.
pub fn find<'a>(
    notes: &'a [Note],
    checkouts: &'a BTreeMap<String, PathBuf>,
) -> Vec<(&'a Note, String, &'a Path)> {
    notes
        .iter()
        .filter_map(|note| match note.place() {
            Place::Project(key) => Some((note, checkouts.get(key)?)),
            _ => None,
        })
        .flat_map(|(note, checkout)| {
            missing(&note.body, checkout).map(move |path| (note, path, checkout.as_path()))
        })
        .collect()
}

fn missing<'a>(body: &'a str, checkout: &'a Path) -> impl Iterator<Item = String> + 'a {
    PATH.captures_iter(body)
        .map(|c| c[1].to_string())
        .filter(|path| is_file_or_folder(path) && !checkout.join(path).exists())
}

// A file (`src/main.rs`) or a folder (`docs/`), not a branch (`feat/x`), a remote (`github.com/me/app`) or a relative climb (`../x`).
fn is_file_or_folder(path: &str) -> bool {
    let segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    let domain = segments[0].contains('.') && !segments[0].starts_with('.');
    let climbs = segments.iter().any(|s| *s == "." || *s == "..");
    let named = path.ends_with('/') || segments[segments.len() - 1].contains('.');
    named && !domain && !climbs
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn files_and_folders_named_in_backticks_but_not_branches_remotes_or_climbs() {
        for path in [
            "src/main.rs",
            "docs/",
            ".github/workflows/ci.yml",
            "a/b.c/d.txt",
        ] {
            assert!(is_file_or_folder(path), "{path}");
        }
        for path in [
            "feat/unused-notes",
            "github.com/me/app",
            "../x/y.rs",
            "src/./a.rs",
            "src/agents",
        ] {
            assert!(!is_file_or_folder(path), "{path}");
        }
    }

    #[test]
    fn only_paths_missing_from_a_known_checkout_of_the_note_s_project() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("scripts")).unwrap();
        fs::write(tmp.path().join("scripts/build.sh"), "").unwrap();
        let note = |path: &str, body: &str| {
            Note::parse(
                path.to_string(),
                &format!("---\ntype: project\n---\n{body}"),
            )
        };
        let notes = [
            note(
                "Projects/app/kept",
                "Run `scripts/build.sh`, not `scripts/old.sh` or `docs/`.",
            ),
            note("Projects/other/never-seen", "See `scripts/old.sh`."),
            note("Preferences/rule", "Never edit `scripts/old.sh`."),
        ];
        let checkouts = BTreeMap::from([("app".to_string(), tmp.path().to_path_buf())]);

        let found: Vec<(&str, String)> = find(&notes, &checkouts)
            .into_iter()
            .map(|(note, path, checkout)| {
                assert_eq!(checkout, tmp.path());
                (note.path.as_str(), path)
            })
            .collect();
        assert_eq!(
            found,
            [
                ("Projects/app/kept", "scripts/old.sh".to_string()),
                ("Projects/app/kept", "docs/".to_string()),
            ]
        );
    }
}
