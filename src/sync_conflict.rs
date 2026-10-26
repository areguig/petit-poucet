use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

// Syncthing inserts this before the extension; a dot-file's whole name counts as its extension.
static MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.sync-conflict-\d{8}-\d{6}-[0-9A-Z]{7}").unwrap());

pub struct Conflict {
    pub copy: String,
    pub original: String,
}

impl Conflict {
    // Only a note's copy can hold a fact the agent would miss; other synced files are machine state.
    pub fn in_note(&self) -> bool {
        self.original.ends_with(".md")
    }
}

// A copy of a copy names the first original too.
pub fn original_of(path: &str) -> Option<String> {
    MARKER
        .is_match(path)
        .then(|| MARKER.replace_all(path, "").into_owned())
}

pub fn lines_only_in(copy: &str, original: &str) -> usize {
    let kept: HashSet<&str> = original.lines().collect();
    copy.lines().filter(|line| !kept.contains(line)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_original_of_every_conflict_shape() {
        let cases = [
            (
                "Projects/x/foo.sync-conflict-20261026-101112-ABCDEF7.md",
                "Projects/x/foo.md",
            ),
            (
                ".sync-conflict-20261026-101112-ABCDEF7.last-cleanup",
                ".last-cleanup",
            ),
            ("notes.sync-conflict-20261026-101112-ABCDEF7", "notes"),
            (
                "foo.sync-conflict-20261026-101112-ABCDEF7.sync-conflict-20261027-000000-ZZZZZZ9.md",
                "foo.md",
            ),
        ];
        for (copy, original) in cases {
            assert_eq!(original_of(copy).as_deref(), Some(original), "{copy}");
        }
    }

    #[test]
    fn leaves_other_names_alone() {
        for name in [
            "foo.md",
            "sync-conflict-notes.md",
            "foo.sync-conflict-2026.md",
        ] {
            assert_eq!(original_of(name), None, "{name}");
        }
    }

    #[test]
    fn counts_the_lines_only_the_copy_has() {
        assert_eq!(lines_only_in("a\nb\nc\n", "a\nc\n"), 1);
        assert_eq!(lines_only_in("a\n", "a\nb\n"), 0);
    }
}
