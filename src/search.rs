use std::collections::BTreeSet;

use crate::note::Note;

const MAX_RESULTS: usize = 10;

// Two-letter words that say nothing about a note: a query term only when it's a name like CI, DB or Go.
const FILLERS: [&str; 20] = [
    "am", "an", "as", "at", "be", "by", "do", "he", "if", "in", "is", "it", "me", "my", "of", "on",
    "or", "so", "to", "we",
];

pub fn words(text: &str, min_chars: usize) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= min_chars)
        .map(str::to_string)
        .collect()
}

// A term matches the start of a word, so "commit" finds "commits" but "omp" not "compile"; two-letter terms
// match whole words only, or "db" would find every "dbt".
fn contains(text: &str, term: &str) -> bool {
    match term.chars().count() {
        ..3 => words(text, 2).contains(term),
        _ => text
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word.starts_with(term)),
    }
}

// Notes matching the most terms come first, and fewer than half of them is noise unless no note matches more;
// then rare terms beat common ones, and the title weighs most, then the summary.
pub fn search<'a>(notes: impl Iterator<Item = &'a Note>, query: &str) -> Vec<&'a Note> {
    let terms: Vec<String> = words(query, 2)
        .into_iter()
        .filter(|t| !FILLERS.contains(&t.as_str()))
        .collect();
    let weighed: Vec<(Vec<usize>, &Note)> = notes
        .map(|note| {
            let title = note.title().to_lowercase();
            let summary = note.summary().unwrap_or_default().to_lowercase();
            let body = note.body.to_lowercase();
            let weights = terms
                .iter()
                .map(|t| {
                    3 * contains(&title, t) as usize
                        + 2 * contains(&summary, t) as usize
                        + contains(&body, t) as usize
                })
                .collect();
            (weights, note)
        })
        .collect();
    let total = weighed.len() as f64;
    let rarity: Vec<f64> = (0..terms.len())
        .map(|i| {
            let matching = weighed.iter().filter(|(w, _)| w[i] > 0).count() as f64;
            (1.0 + total / matching.max(1.0)).ln()
        })
        .collect();
    let mut scored: Vec<(usize, f64, &Note)> = weighed
        .into_iter()
        .map(|(weights, note)| {
            let matched = weights.iter().filter(|&&w| w > 0).count();
            let score = weights
                .iter()
                .zip(&rarity)
                .map(|(&w, r)| w as f64 * r)
                .sum();
            (matched, score, note)
        })
        .collect();
    let best = scored.iter().map(|s| s.0).max().unwrap_or(0);
    let min_matched = terms.len().div_ceil(2).min(best).max(1);
    scored.retain(|s| s.0 >= min_matched);
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.total_cmp(&a.1))
            .then_with(|| a.2.path.cmp(&b.2.path))
    });
    scored
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, _, n)| n)
        .collect()
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
    fn ranks_title_over_summary_over_body() {
        let notes = [
            note("a", "Other", "other", "mentions commits"),
            note("b", "Commit rules", "local only", "x"),
            note("c", "Other", "commit locally", "x"),
            note("d", "Unrelated", "nothing", "x"),
        ];
        let found: Vec<&str> = search(notes.iter(), "commit")
            .iter()
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(found, ["b", "c", "a"]);
    }

    #[test]
    fn two_letter_terms_match_whole_words_and_fillers_are_ignored() {
        let notes = [
            note("a", "CI runners", "self-hosted", "x"),
            note("b", "Feedback", "reply to feedback", "x"),
            note("c", "Go version", "pin Go 1.24", "uses the DB"),
        ];
        let found = |query: &str| -> Vec<String> {
            search(notes.iter(), query)
                .iter()
                .map(|n| n.path.clone())
                .collect()
        };
        assert_eq!(found("ci"), ["a"]);
        assert_eq!(found("DB"), ["c"], "not the db in feedback");
        assert_eq!(found("how to upgrade go"), ["c"], "`to` matches nothing");
        assert!(found("to").is_empty());
    }

    #[test]
    fn terms_match_at_the_start_of_a_word() {
        let notes = [
            note("a", "Commits", "x", "x"),
            note("b", "Compile flags", "x", "x"),
        ];
        let found = |query: &str| -> Vec<String> {
            search(notes.iter(), query)
                .iter()
                .map(|n| n.path.clone())
                .collect()
        };
        assert_eq!(found("commit"), ["a"]);
        assert!(found("omp").is_empty(), "not the omp in compile");
    }

    #[test]
    fn notes_matching_under_half_the_terms_are_dropped_and_rare_terms_rank_first() {
        let notes = [
            note("config", "Session config", "x", "x"),
            note("jsonb", "Jsonb filters", "x", "x"),
            note("both", "Jsonb config", "x", "x"),
            note("other-1", "Session timeout", "x", "x"),
            note("other-2", "Session cookies", "x", "x"),
        ];
        let found: Vec<&str> = search(notes.iter(), "jsonb config hibernate refactor")
            .iter()
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(found, ["both"], "the only note matching 2 of 4 terms");
        let found: Vec<&str> = search(notes.iter(), "jsonb session")
            .iter()
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(
            found,
            ["both", "jsonb", "config", "other-1", "other-2"],
            "jsonb is in 2 notes, session in 3"
        );
    }
}
