use std::collections::HashMap;

use crate::vault::Vault;

const MIN_WORDS: usize = 4;

pub fn slug_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

// Ends of the first `k` words of a slug, for each k.
fn word_ends(slug: &str) -> impl Iterator<Item = usize> {
    slug.match_indices('-')
        .map(|(i, _)| i)
        .chain(std::iter::once(slug.len()))
}

// For each note, the name the session view shows: the slug's first words, extended until no other
// note's slug equals or starts with them; else the whole slug, or the full path when another note has that slug.
pub fn all(vault: &Vault) -> HashMap<&str, String> {
    let mut with_prefix: HashMap<&str, usize> = HashMap::new();
    let mut with_slug: HashMap<&str, usize> = HashMap::new();
    for note in &vault.notes {
        let slug = slug_of(&note.path);
        *with_slug.entry(slug).or_default() += 1;
        for end in word_ends(slug) {
            *with_prefix.entry(&slug[..end]).or_default() += 1;
        }
    }
    vault
        .notes
        .iter()
        .map(|note| {
            let slug = slug_of(&note.path);
            let shortest_unique = word_ends(slug)
                .skip(MIN_WORDS - 1)
                .map(|end| &slug[..end])
                .find(|prefix| with_prefix[prefix] == 1);
            let name = match (shortest_unique, with_slug[slug]) {
                (Some(prefix), _) => prefix,
                (None, 1) => slug,
                (None, _) => &note.path,
            };
            (note.path.as_str(), name.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::change;

    fn vault_of(paths: &[&str]) -> (tempfile::TempDir, Vault) {
        let tmp = tempfile::tempdir().unwrap();
        for path in paths {
            let file = tmp.path().join(format!("{path}.md"));
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, "x").unwrap();
        }
        let vault = Vault::load(tmp.path()).unwrap();
        (tmp, vault)
    }

    fn name_of(vault: &Vault, path: &str) -> String {
        all(vault)[path].clone()
    }

    #[test]
    fn a_long_slug_is_cut_to_its_first_four_words() {
        let (_tmp, vault) =
            vault_of(&["Preferences/always-run-the-full-test-suite-before-committing"]);
        assert_eq!(
            name_of(
                &vault,
                "Preferences/always-run-the-full-test-suite-before-committing"
            ),
            "always-run-the-full"
        );
    }

    #[test]
    fn slugs_sharing_their_first_words_are_extended_until_they_differ() {
        let (_tmp, vault) = vault_of(&[
            "Preferences/use-the-release-build-for-benchmarks",
            "Preferences/use-the-release-build-for-installs",
            "Projects/app/use-the-release-profile",
        ]);
        assert_eq!(
            name_of(&vault, "Preferences/use-the-release-build-for-benchmarks"),
            "use-the-release-build-for-benchmarks"
        );
        assert_eq!(
            name_of(&vault, "Preferences/use-the-release-build-for-installs"),
            "use-the-release-build-for-installs"
        );
        assert_eq!(
            name_of(&vault, "Projects/app/use-the-release-profile"),
            "use-the-release-profile"
        );
    }

    #[test]
    fn a_slug_that_prefixes_another_is_shown_whole() {
        let (_tmp, vault) = vault_of(&[
            "Preferences/one-two-three-four-five",
            "Preferences/one-two-three-four-five-six-seven",
        ]);
        assert_eq!(
            name_of(&vault, "Preferences/one-two-three-four-five"),
            "one-two-three-four-five"
        );
        assert_eq!(
            name_of(&vault, "Preferences/one-two-three-four-five-six-seven"),
            "one-two-three-four-five-six"
        );
    }

    #[test]
    fn short_slugs_are_shown_whole() {
        let (_tmp, vault) = vault_of(&["Preferences/tabs", "Preferences/no-secrets-here"]);
        assert_eq!(name_of(&vault, "Preferences/tabs"), "tabs");
        assert_eq!(
            name_of(&vault, "Preferences/no-secrets-here"),
            "no-secrets-here"
        );
    }

    #[test]
    fn the_same_slug_in_two_folders_is_shown_by_full_path() {
        let (_tmp, vault) = vault_of(&["Preferences/style", "Projects/app/style"]);
        assert_eq!(name_of(&vault, "Preferences/style"), "Preferences/style");
        assert_eq!(name_of(&vault, "Projects/app/style"), "Projects/app/style");
    }

    #[test]
    fn every_name_is_unique_and_resolves_to_its_own_note() {
        let (_tmp, vault) = vault_of(&[
            "Preferences/a-b-c-d-e-f",
            "Preferences/a-b-c-d-e-g",
            "Preferences/a-b-c-d",
            "Preferences/a-b-c-d-e",
            "Preferences/a-b-c-x-y",
            "Projects/app/a-b-c-d-e-f-g-h",
            "Projects/app/solo",
            "Topics/lab/a-b-c-d-e-f-z",
            "Topics/lab/style",
            "Projects/app/style",
        ]);
        let names = all(&vault);
        assert_eq!(names.len(), vault.notes.len());
        let mut seen = std::collections::HashSet::new();
        for note in &vault.notes {
            let name = &names[note.path.as_str()];
            assert!(seen.insert(name.clone()), "{name} names two notes");
            assert_eq!(change::find_note(&vault, name).unwrap().path, note.path);
        }
    }
}
