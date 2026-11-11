use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::git;
use crate::note::{Note, Place};

// Words joined by slashes in backticks; `is_file_or_folder` keeps the ones that name a file or folder of the checkout.
static PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([\w.-]+(?:/[\w.-]+)*/[\w.-]*)`").unwrap());

// Paths a project note names that are missing from that project's checkout on this machine; projects never seen here are skipped.
// A path found deeper in a checkout, or in another project's, is not missing: notes shorten paths and name other repos.
pub fn find<'a>(
    notes: &'a [Note],
    checkouts: &'a BTreeMap<String, PathBuf>,
) -> Vec<(&'a Note, String, &'a Path)> {
    let missing: Vec<(&Note, String, &Path)> = notes
        .iter()
        .filter_map(|note| match note.place() {
            Place::Project(key) => Some((note, checkouts.get(key)?)),
            _ => None,
        })
        .flat_map(|(note, checkout)| {
            named(&note.body)
                .filter(|path| !checkout.join(path).exists())
                .map(move |path| (note, path, checkout.as_path()))
        })
        .collect();
    if missing.is_empty() {
        return missing;
    }
    let files: Vec<String> = checkouts.values().flat_map(|c| git::files(c)).collect();
    missing
        .into_iter()
        .filter(|(_, path, _)| {
            !checkouts.values().any(|c| c.join(path).exists())
                && !files.iter().any(|file| is_under(file, path))
        })
        .collect()
}

fn named(body: &str) -> impl Iterator<Item = String> + '_ {
    PATH.captures_iter(body)
        .map(|c| c[1].to_string())
        .filter(|path| is_file_or_folder(path))
}

// `file` is `path` under some folder (`src/a/b.rs` for `a/b.rs`), or lies in the folder `path` under one (`src/docs/x.md` for `docs/`).
fn is_under(file: &str, path: &str) -> bool {
    let below = format!("/{path}");
    match path.ends_with('/') {
        true => file.contains(&below),
        false => file.ends_with(&below),
    }
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

    #[test]
    fn a_path_is_under_a_file_that_ends_with_it_or_lies_in_its_folder() {
        for (file, path) in [
            (
                "src/main/resources/prompts/system.txt",
                "prompts/system.txt",
            ),
            ("src/docs/guide.md", "docs/"),
        ] {
            assert!(is_under(file, path), "{file} for {path}");
        }
        for (file, path) in [
            ("src/myprompts/system.txt", "prompts/system.txt"),
            ("prompts/system.txt.bak", "prompts/system.txt"),
            ("src/mydocs/guide.md", "docs/"),
            ("src/docs", "docs/"),
        ] {
            assert!(!is_under(file, path), "{file} for {path}");
        }
    }

    #[test]
    fn a_path_deeper_in_a_checkout_or_in_another_project_s_is_not_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let (api, web) = (tmp.path().join("api"), tmp.path().join("web"));
        for (repo, file) in [
            (&api, "src/main/resources/prompts/system.txt"),
            (&api, "target/built/out.bin"),
            (&web, "src/components/App.tsx"),
            (&web, "public/index.html"),
        ] {
            let file = repo.join(file);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, "").unwrap();
        }
        git::init(&api).unwrap();
        git::init(&web).unwrap();
        fs::write(api.join(".gitignore"), "target/\n").unwrap();
        let note = Note::parse(
            "Projects/api/prompt".to_string(),
            "---\ntype: project\n---\nSee `prompts/system.txt` in `main/resources/`, `components/App.tsx`, \
             `public/index.html`, `built/out.bin` and `prompts/old.txt`.",
        );
        let checkouts = BTreeMap::from([("api".to_string(), api), ("web".to_string(), web)]);

        let found: Vec<String> = find(std::slice::from_ref(&note), &checkouts)
            .into_iter()
            .map(|(_, path, _)| path)
            .collect();
        assert_eq!(
            found,
            ["built/out.bin", "prompts/old.txt"],
            "ignored files are not searched"
        );
    }
}
