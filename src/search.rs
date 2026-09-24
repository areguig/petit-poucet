use std::collections::BTreeSet;

use crate::note::Note;

const MAX_RESULTS: usize = 10;

fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_string)
        .collect()
}

// Substring matching, so "commit" also finds "commits"; the title weighs most, then the summary.
pub fn search<'a>(notes: impl Iterator<Item = &'a Note>, query: &str) -> Vec<&'a Note> {
    let terms = words(query);
    let mut scored: Vec<(usize, &Note)> = notes
        .filter_map(|note| {
            let title = note.title().to_lowercase();
            let summary = note.summary().unwrap_or_default().to_lowercase();
            let body = note.body.to_lowercase();
            let score: usize = terms
                .iter()
                .map(|t| {
                    3 * title.contains(t.as_str()) as usize
                        + 2 * summary.contains(t.as_str()) as usize
                        + body.contains(t.as_str()) as usize
                })
                .sum();
            (score > 0).then_some((score, note))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    scored
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, n)| n)
        .collect()
}

// Near-duplicate: at least two shared words, covering half of the shorter title + summary.
pub fn similar<'a>(
    notes: impl Iterator<Item = &'a Note>,
    title: &str,
    summary: &str,
) -> Vec<&'a Note> {
    let new = words(&format!("{title} {summary}"));
    notes
        .filter(|note| {
            let old = words(&format!(
                "{} {}",
                note.title(),
                note.summary().unwrap_or_default()
            ));
            let shared = new.intersection(&old).count();
            shared >= 2 && 2 * shared >= new.len().min(old.len())
        })
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
            "Commit locally",
            "commit each step locally, no push",
        );
        assert_eq!(
            found.iter().map(|n| n.path.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
        assert!(similar(notes.iter(), "Database", "keep local data").is_empty());
    }
}
