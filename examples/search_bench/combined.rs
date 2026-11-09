use std::collections::BTreeMap;

use rust_stemmers::{Algorithm, Stemmer};

use crate::bench_text::note_text;
use crate::note::Note;

// Snapshot of today's query filtering; src/search.rs stays unchanged in this experiment.
const FILLERS: [&str; 20] = [
    "am", "an", "as", "at", "be", "by", "do", "he", "if", "in", "is", "it", "me", "my", "of", "on",
    "or", "so", "to", "we",
];

const K1: f64 = 1.2;
const B: f64 = 0.75;

struct Word {
    raw: String,
    english: String,
    french: String,
    count: usize,
}

impl Word {
    fn matches(&self, term: &Word) -> bool {
        let prefix = if term.raw.chars().count() < 3 {
            self.raw == term.raw
        } else {
            self.raw.starts_with(&term.raw)
        };
        prefix || self.english == term.english || self.french == term.french
    }
}

struct Document<'a> {
    path: &'a str,
    words: Vec<Word>,
    length: usize,
}

pub struct Combined<'a> {
    documents: Vec<Document<'a>>,
    average_length: f64,
    english: Stemmer,
    french: Stemmer,
}

fn words(text: &str, english: &Stemmer, french: &Stemmer) -> Vec<Word> {
    let mut counts = BTreeMap::new();
    let lower = text.to_lowercase();
    for word in lower.split(|c: char| !c.is_alphanumeric()) {
        if word.chars().count() >= 2 {
            *counts.entry(word).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .map(|(raw, count)| Word {
            raw: raw.to_string(),
            english: english.stem(raw).into_owned(),
            french: french.stem(raw).into_owned(),
            count,
        })
        .collect()
}

impl<'a> Combined<'a> {
    pub fn new(notes: &'a [Note]) -> Self {
        let english = Stemmer::create(Algorithm::English);
        let french = Stemmer::create(Algorithm::French);
        let documents: Vec<_> = notes
            .iter()
            .map(|note| {
                let words = words(&note_text(note), &english, &french);
                let length = words.iter().map(|w| w.count).sum();
                Document {
                    path: &note.path,
                    words,
                    length,
                }
            })
            .collect();
        let average_length = documents.iter().map(|d| d.length).sum::<usize>() as f64
            / documents.len().max(1) as f64;
        Self {
            documents,
            average_length,
            english,
            french,
        }
    }

    pub fn rank(&self, query: &str) -> Vec<&str> {
        let terms: Vec<_> = words(query, &self.english, &self.french)
            .into_iter()
            .filter(|w| !FILLERS.contains(&w.raw.as_str()))
            .collect();
        let frequencies: Vec<Vec<usize>> = self
            .documents
            .iter()
            .map(|d| {
                terms
                    .iter()
                    .map(|t| {
                        d.words
                            .iter()
                            .filter(|w| w.matches(t))
                            .map(|w| w.count)
                            .sum()
                    })
                    .collect()
            })
            .collect();
        let idfs: Vec<_> = (0..terms.len())
            .map(|i| {
                let df = frequencies.iter().filter(|f| f[i] > 0).count() as f64;
                (1.0 + (self.documents.len() as f64 - df + 0.5) / (df + 0.5)).ln()
            })
            .collect();
        let matched: Vec<_> = frequencies
            .iter()
            .map(|f| f.iter().filter(|&&n| n > 0).count())
            .collect();
        let best = matched.iter().copied().max().unwrap_or(0);
        let minimum = terms.len().div_ceil(2).min(best).max(1);
        let mut scored: Vec<_> = self
            .documents
            .iter()
            .zip(frequencies)
            .zip(matched)
            .filter(|(_, count)| *count >= minimum)
            .map(|((doc, frequencies), _)| {
                let length_ratio = doc.length as f64 / self.average_length.max(1.0);
                let score: f64 = frequencies
                    .iter()
                    .zip(&idfs)
                    .map(|(&tf, idf)| {
                        let tf = tf as f64;
                        idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * length_ratio))
                    })
                    .sum();
                (score, doc.path)
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        scored.into_iter().take(10).map(|(_, path)| path).collect()
    }
}
