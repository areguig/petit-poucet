#![allow(
    dead_code,
    reason = "whole source modules are included for the few items used"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Parser;

use bm25::{Document, LanguageMode, SearchEngine, SearchEngineBuilder};
use walkdir::WalkDir;

#[path = "../src/note.rs"]
mod note;
#[path = "../tests/bench/search/queries.rs"]
mod queries;
#[path = "../src/search.rs"]
mod search;

// note.rs only needs the folder names, and the real vault module drags in half the binary.
mod vault {
    pub const PROJECTS: &str = "Projects";
    pub const TOPICS: &str = "Topics";
}

#[path = "search_bench/text.rs"]
mod bench_text;

#[path = "search_bench/combined.rs"]
mod combined;

#[path = "search_bench/ranker.rs"]
mod ranker;

use note::Note;
use queries::Query;
use ranker::Ranker;

const LIMIT: usize = 10;
const TIMING_RUNS: usize = 20;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = queries::VAULT)]
    vault: PathBuf,
    #[arg(long, value_name = "FILE")]
    queries: Option<PathBuf>,
}

struct Today<'a>(&'a [Note]);

impl Ranker for Today<'_> {
    fn name(&self) -> &'static str {
        "today"
    }

    fn rank(&self, query: &str) -> Vec<&str> {
        search::search(self.0.iter(), query)
            .into_iter()
            .map(|n| n.path.as_str())
            .collect()
    }
}

struct Bm25<'a> {
    notes: &'a [Note],
    engine: SearchEngine<usize>,
}

impl<'a> Bm25<'a> {
    fn new(notes: &'a [Note]) -> Self {
        let documents = notes.iter().enumerate().map(|(i, n)| {
            let text = bench_text::note_text(n);
            Document::new(i, text)
        });
        let engine = SearchEngineBuilder::with_documents(LanguageMode::Detect, documents).build();
        Bm25 { notes, engine }
    }
}

impl Ranker for Bm25<'_> {
    fn name(&self) -> &'static str {
        "bm25"
    }

    fn rank(&self, query: &str) -> Vec<&str> {
        let mut results = self.engine.search(query, self.notes.len());
        results.sort_by(|a, b| {
            b.score.total_cmp(&a.score).then_with(|| {
                self.notes[a.document.id]
                    .path
                    .cmp(&self.notes[b.document.id].path)
            })
        });
        results
            .into_iter()
            .take(LIMIT)
            .map(|r| self.notes[r.document.id].path.as_str())
            .collect()
    }
}

fn load_notes(root: &Path) -> Vec<Note> {
    let mut paths: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.expect("walk vault"))
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| is_note(root, p))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|file| {
            let relative = file.strip_prefix(root).expect("inside the vault");
            let path = relative
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let note = Note::parse(path, &fs::read_to_string(file).expect("read note"));
            assert!(note.frontmatter.is_ok(), "{}: bad frontmatter", note.path);
            note
        })
        .collect()
}

fn is_note(root: &Path, file: &Path) -> bool {
    if file.extension().is_none_or(|ext| ext != "md")
        || file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".sync-conflict-")
    {
        return false;
    }
    let relative = file.strip_prefix(root).expect("inside the vault");
    let parts: Vec<_> = relative.iter().map(|p| p.to_string_lossy()).collect();
    match parts.as_slice() {
        [area, _] if area == "Preferences" => true,
        [area, _, name] if area == "Projects" => name != "_project.md",
        [area, _, _] if area == "Topics" => true,
        _ => false,
    }
}

struct Outcome<'a> {
    query: &'a Query,
    results: Vec<&'a str>,
}

impl Outcome<'_> {
    // 1-based position of the best-ranked expected note.
    fn rank(&self) -> Option<usize> {
        self.results
            .iter()
            .position(|p| self.query.expect.iter().any(|e| e == p))
            .map(|i| i + 1)
    }
}

fn percent(count: usize, total: usize) -> String {
    format!("{:.0}%", 100.0 * count as f64 / total.max(1) as f64)
}

fn print_quality(rankers: &[&dyn Ranker], queries: &[Query]) {
    println!("## Quality\n");
    println!(
        "| method | category | queries | recall@3 | recall@10 | MRR | mean results | returned something |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for ranker in rankers {
        let outcomes = run(*ranker, queries);
        let mut groups: BTreeMap<&str, Vec<&Outcome>> = BTreeMap::new();
        for o in &outcomes {
            groups.entry(o.query.category.as_str()).or_default().push(o);
        }
        let positives: Vec<&Outcome> = outcomes
            .iter()
            .filter(|o| !o.query.expect.is_empty())
            .collect();
        for (category, group) in groups
            .iter()
            .map(|(c, g)| (*c, g.as_slice()))
            .chain([("all expecting a note", positives.as_slice())])
        {
            println!("{}", row(ranker.name(), category, group));
        }
    }
}

fn row(method: &str, category: &str, group: &[&Outcome]) -> String {
    let n = group.len();
    let positives = group.iter().filter(|o| !o.query.expect.is_empty()).count();
    let results: usize = group.iter().map(|o| o.results.len()).sum();
    let something = group.iter().filter(|o| !o.results.is_empty()).count();
    let (r3, r10, mrr) = if positives == 0 {
        ("-".to_string(), "-".to_string(), "-".to_string())
    } else {
        let within = |k: usize| {
            group
                .iter()
                .filter(|o| o.rank().is_some_and(|r| r <= k))
                .count()
        };
        let mrr: f64 = group
            .iter()
            .filter_map(|o| o.rank())
            .map(|r| 1.0 / r as f64)
            .sum();
        (
            percent(within(3), positives),
            percent(within(10), positives),
            format!("{:.3}", mrr / positives as f64),
        )
    };
    format!(
        "| {method} | {category} | {n} | {r3} | {r10} | {mrr} | {:.1} | {something} |",
        results as f64 / n as f64
    )
}

fn run<'a>(ranker: &'a dyn Ranker, queries: &'a [Query]) -> Vec<Outcome<'a>> {
    queries
        .iter()
        .map(|query| Outcome {
            query,
            results: ranker.rank(&query.text),
        })
        .collect()
}

fn print_misses(rankers: &[&dyn Ranker], queries: &[Query]) {
    println!("\n## Misses\n");
    println!(
        "Queries whose expected note is not in the top 3 (rank shown when it is in the top 10), and negatives that returned something.\n"
    );
    for ranker in rankers {
        println!("### {}\n", ranker.name());
        let outcomes = run(*ranker, queries);
        let mut by_category: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for o in &outcomes {
            let top: Vec<&str> = o.results.iter().take(3).copied().collect();
            let line = if o.query.expect.is_empty() {
                (!o.results.is_empty()).then(|| {
                    format!(
                        "- {:?} returned {}: {}",
                        o.query.text,
                        o.results.len(),
                        top.join(", ")
                    )
                })
            } else if o.rank().is_none_or(|r| r > 3) {
                let rank = o
                    .rank()
                    .map_or("missed".to_string(), |r| format!("rank {r}"));
                Some(format!(
                    "- {:?} -> expected {} ({rank}); returned {}",
                    o.query.text,
                    o.query.expect.join(" or "),
                    if top.is_empty() {
                        "nothing".to_string()
                    } else {
                        top.join(", ")
                    }
                ))
            } else {
                None
            };
            by_category
                .entry(o.query.category.as_str())
                .or_default()
                .extend(line);
        }
        for (category, lines) in by_category.iter().filter(|(_, l)| !l.is_empty()) {
            println!("**{category}**\n");
            for line in lines {
                println!("{line}");
            }
            println!();
        }
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    sorted[((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1]
}

// Timings vary between runs and machines; the other sections do not.
fn print_timings(rankers: &[(&dyn Ranker, Duration, Duration)], queries: &[Query], notes: usize) {
    println!("## Timings (vary by machine and run)\n");
    println!(
        "{notes} notes, {} queries, median of {TIMING_RUNS} runs per query.\n",
        queries.len()
    );
    println!("| method | index build | first query | query p50 | query p95 |");
    println!("|---|---|---|---|---|");
    for (ranker, build, first) in rankers {
        let mut medians: Vec<Duration> = queries
            .iter()
            .map(|q| {
                let mut runs: Vec<Duration> = (0..TIMING_RUNS)
                    .map(|_| {
                        let start = Instant::now();
                        std::hint::black_box(ranker.rank(&q.text));
                        start.elapsed()
                    })
                    .collect();
                runs.sort();
                runs[TIMING_RUNS / 2]
            })
            .collect();
        medians.sort();
        println!(
            "| {} | {build:?} | {first:?} | {:?} | {:?} |",
            ranker.name(),
            percentile(&medians, 0.5),
            percentile(&medians, 0.95)
        );
    }
}

fn main() {
    let args = Args::parse();
    let start = Instant::now();
    let notes = load_notes(&args.vault);
    let load_time = start.elapsed();
    let start = Instant::now();
    let today = Today(&notes);
    let today_build = start.elapsed();
    let start = Instant::now();
    let bm25 = Bm25::new(&notes);
    let bm25_build = start.elapsed();
    let start = Instant::now();
    let combined = combined::Combined::new(&notes);
    let combined_build = start.elapsed();
    let rankers: [&dyn Ranker; 3] = [&today, &bm25, &combined];

    let first_queries: Vec<_> = rankers
        .iter()
        .map(|ranker| {
            let start = Instant::now();
            std::hint::black_box(ranker.rank("cursor pagination"));
            start.elapsed()
        })
        .collect();
    println!("# Search benchmark\n");
    println!("Vault read and parse: {load_time:?}. First query: `cursor pagination`.\n");
    let files = match args.queries {
        Some(file) => vec![fs::canonicalize(file).expect("query file exists")],
        None => vec![
            PathBuf::from("queries.toml"),
            PathBuf::from("queries-independent.toml"),
        ],
    };
    for file in files {
        let queries = queries::load(&file);
        println!("# Query set: {}\n", file.display());
        print_quality(&rankers, &queries);
        print_misses(&rankers, &queries);
        print_timings(
            &[
                (&today, today_build, first_queries[0]),
                (&bm25, bm25_build, first_queries[1]),
                (&combined, combined_build, first_queries[2]),
            ],
            &queries,
            notes.len(),
        );
    }
}
