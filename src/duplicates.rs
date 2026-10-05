use std::collections::{BTreeSet, HashMap};

use petgraph::unionfind::UnionFind;

use crate::fillers;
use crate::note::Note;
use crate::search::words;

// A note's title and summary words, the ones near-duplicates are judged on; fillers like "the" or "les" never make two notes alike.
fn note_words(note: &Note) -> BTreeSet<String> {
    let summary = note.summary().unwrap_or_default();
    let fillers = fillers::of(&format!("{summary}\n{}", note.body));
    words(&format!("{} {summary}", note.title()), 3)
        .into_iter()
        .filter(|word| !fillers.contains(&word.as_str()))
        .collect()
}

// Near-duplicate: at least two shared words, covering half of the shorter title + summary.
fn alike(shared: usize, a: usize, b: usize) -> bool {
    shared >= 2 && 2 * shared >= a.min(b)
}

pub fn similar<'a>(notes: impl Iterator<Item = &'a Note>, note: &Note) -> Vec<&'a Note> {
    let new = note_words(note);
    notes
        .filter(|other| {
            let old = note_words(other);
            alike(new.intersection(&old).count(), new.len(), old.len())
        })
        .collect()
}

// Notes linked by near-duplicate pairs, grouped in vault order; only notes sharing a word are compared.
pub fn groups(notes: &[Note]) -> Vec<Vec<&Note>> {
    let keys: Vec<BTreeSet<String>> = notes.iter().map(note_words).collect();
    let mut holders: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, key) in keys.iter().enumerate() {
        for word in key {
            holders.entry(word).or_default().push(i);
        }
    }
    let mut groups = UnionFind::new(notes.len());
    for (i, key) in keys.iter().enumerate() {
        let mut shared: HashMap<usize, usize> = HashMap::new();
        for &j in key.iter().flat_map(|w| &holders[w.as_str()]) {
            if j > i {
                *shared.entry(j).or_default() += 1;
            }
        }
        for (j, count) in shared {
            if alike(count, key.len(), keys[j].len()) {
                groups.union(i, j);
            }
        }
    }
    let mut by_root: HashMap<usize, Vec<&Note>> = HashMap::new();
    for (i, note) in notes.iter().enumerate() {
        by_root.entry(groups.find_mut(i)).or_default().push(note);
    }
    let mut members: Vec<Vec<&Note>> = by_root.into_values().filter(|m| m.len() > 1).collect();
    members.sort_by(|a, b| a[0].path.cmp(&b[0].path));
    members
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, title: &str, summary: &str, body: &str) -> Note {
        Note::parse(
            path.into(),
            &format!(
                "---\ntype: user\nscope: x\nsummary: {summary}\ncreated: 2026-09-24\n---\n# {title}\n\n{body}\n"
            ),
        )
    }

    #[test]
    fn finds_near_duplicates_only() {
        let notes = [
            note(
                "a",
                "Commit rules",
                "commit locally per step, never push",
                "x",
            ),
            note("b", "Secrets", "never print secret values", "x"),
        ];
        let found = similar(
            notes.iter(),
            &note(
                "new",
                "Commit locally",
                "commit each step locally, no push",
                "x",
            ),
        );
        assert_eq!(
            found.iter().map(|n| n.path.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
        assert!(
            similar(
                notes.iter(),
                &note("new", "Database", "keep local data", "x")
            )
            .is_empty()
        );
    }

    #[test]
    fn near_duplicates_are_grouped_once_and_unrelated_notes_left_out() {
        let notes = [
            note(
                "a",
                "Commit rules",
                "commit locally per step, never push",
                "x",
            ),
            note("b", "Secrets", "never print secret values", "x"),
            note(
                "c",
                "Commit locally",
                "commit each step locally, no push",
                "x",
            ),
            note("d", "Local commits", "commit each step locally", "x"),
            note(
                "e",
                "Database host",
                "the staging database runs on port 5433",
                "x",
            ),
            note(
                "f",
                "Staging database",
                "staging database port is 5433",
                "x",
            ),
        ];
        let groups: Vec<Vec<&str>> = groups(&notes)
            .iter()
            .map(|g| g.iter().map(|n| n.path.as_str()).collect())
            .collect();
        assert_eq!(groups, [vec!["a", "c", "d"], vec!["e", "f"]]);
    }

    #[test]
    fn fillers_of_the_note_language_never_make_notes_alike() {
        let english = [
            note(
                "a",
                "Stale path test",
                "test note for the stale path check",
                "Run the release script before tagging.",
            ),
            note(
                "b",
                "Confluence sandbox",
                "use the sandbox page for every live test and check",
                "The sandbox page is reset every night.",
            ),
        ];
        assert!(groups(&english).is_empty());
        let french = [
            note(
                "c",
                "Déploiement sur le serveur de recette",
                "le déploiement passe par une pipeline dans la CI pour les branches",
                "Le serveur de recette est mis à jour à chaque fusion sur la branche principale.",
            ),
            note(
                "d",
                "Sauvegardes de la base",
                "les sauvegardes passent par une tâche dans cron pour la base",
                "La base de production est sauvegardée chaque nuit sur un disque externe.",
            ),
            note(
                "e",
                "Déploiement en recette",
                "chaque branche est déployée en recette par la pipeline de CI",
                "Une fusion sur une branche lance le déploiement vers le serveur de recette.",
            ),
        ];
        let groups: Vec<Vec<&str>> = groups(&french)
            .iter()
            .map(|g| g.iter().map(|n| n.path.as_str()).collect())
            .collect();
        assert_eq!(groups, [vec!["c", "e"]]);
    }
}
