use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const VAULT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/bench/search/vault");

#[derive(Deserialize)]
pub struct Query {
    pub category: String,
    pub text: String,
    // Note paths, any one of which answers the query; empty when nothing should match.
    pub expect: Vec<String>,
}

#[derive(Deserialize)]
struct Queries {
    query: Vec<Query>,
}

pub fn load(file: impl AsRef<Path>) -> Vec<Query> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/bench/search")
        .join(file);
    let text = std::fs::read_to_string(path).expect("read query set");
    toml::from_str::<Queries>(&text)
        .expect("parse query set")
        .query
}
