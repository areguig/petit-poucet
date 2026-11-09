use std::path::Path;

#[path = "bench/search/queries.rs"]
mod queries;

const CATEGORIES: [&str; 7] = [
    "exact",
    "keywords",
    "paraphrase",
    "cross-language",
    "short",
    "rare",
    "negative",
];

#[test]
fn every_expected_note_of_the_search_benchmark_exists() {
    for (file, minimum) in [("queries.toml", 80), ("queries-independent.toml", 60)] {
        let queries = queries::load(file);
        assert!(queries.len() >= minimum);
        for query in &queries {
            assert!(
                CATEGORIES.contains(&query.category.as_str()),
                "{:?}",
                query.text
            );
            assert!(
                !query.expect.is_empty()
                    || ["short", "negative"].contains(&query.category.as_str())
            );
            assert!(query.category != "negative" || query.expect.is_empty());
            for path in &query.expect {
                let file = Path::new(queries::VAULT).join(format!("{path}.md"));
                assert!(file.is_file(), "{:?} expects {path}", query.text);
            }
        }
    }
}
