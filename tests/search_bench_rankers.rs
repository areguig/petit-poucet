#![allow(dead_code, reason = "source modules supply the benchmark's note model")]

#[path = "../examples/search_bench/text.rs"]
mod bench_text;
#[path = "../examples/search_bench/combined.rs"]
mod combined;
#[path = "../src/note.rs"]
mod note;
#[path = "../examples/search_bench/ranker.rs"]
mod ranker;

mod vault {
    pub const PROJECTS: &str = "Projects";
    pub const TOPICS: &str = "Topics";
}

fn note(path: &str, body: &str) -> note::Note {
    note::Note::parse(
        path.into(),
        &format!(
            "---\ntype: reference\nscope: sample\nsummary: sample\ncreated: 2026-09-01\n---\n# Sample\n{body}"
        ),
    )
}

#[test]
fn technical_terms_match_word_prefixes_and_two_letter_terms_match_whole_words() {
    let notes = [
        note("lint", "gochecknoglobals"),
        note("build", "compile dbt"),
        note("database", "DB"),
    ];
    let ranker = combined::Combined::new(&notes);
    assert_eq!(ranker.rank("gochecknoglobals"), ["lint"]);
    assert_eq!(ranker.rank("gochecknoglobal"), ["lint"]);
    assert_eq!(ranker.rank("gocheckno"), ["lint"]);
    assert!(ranker.rank("omp").is_empty());
    assert_eq!(ranker.rank("DB"), ["database"]);
}

#[test]
fn requires_half_the_terms_but_falls_back_to_the_best_match_count() {
    let notes = [
        note("two", "jsonb config"),
        note("one", "jsonb"),
        note("none", "unused"),
    ];
    let ranker = combined::Combined::new(&notes);
    assert_eq!(ranker.rank("jsonb config hibernate refactor"), ["two"]);
    assert_eq!(
        ranker.rank("jsonb hibernate refactor missing"),
        ["one", "two"]
    );
    assert!(ranker.rank("absent").is_empty());
    assert!(ranker.rank("do it").is_empty());
    assert!(ranker.rank("").is_empty());
    assert!(combined::Combined::new(&[]).rank("anything").is_empty());
}

#[test]
fn english_and_french_inflections_match() {
    let notes = [note("english", "connected"), note("french", "chevaux")];
    let ranker = combined::Combined::new(&notes);
    assert_eq!(ranker.rank("connections"), ["english"]);
    assert_eq!(ranker.rank("cheval"), ["french"]);
}

#[test]
fn bm25_favours_rare_terms_and_normalizes_length_with_stable_ties() {
    let notes = [
        note("common-a", "config"),
        note("common-b", "config"),
        note("rare", "jsonb"),
        note("a-long", "config unrelated words take up extra space here"),
    ];
    let ranker = combined::Combined::new(&notes);
    assert_eq!(
        ranker.rank("config jsonb"),
        ["rare", "common-a", "common-b", "a-long"]
    );
}

#[test]
fn combined_dispatches_through_a_ranker_trait_object() {
    let notes = [note("lint", "gochecknoglobals"), note("other", "unrelated")];
    let combined = combined::Combined::new(&notes);
    let ranker: &dyn ranker::Ranker = &combined;
    assert_eq!(ranker.rank("gochecknoglobals"), ["lint"]);
    assert!(ranker.rank("absentword").is_empty());
}
