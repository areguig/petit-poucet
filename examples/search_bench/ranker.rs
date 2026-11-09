use crate::combined;

pub trait Ranker {
    fn name(&self) -> &'static str;
    fn rank(&self, query: &str) -> Vec<&str>;
}

impl Ranker for combined::Combined<'_> {
    fn name(&self) -> &'static str {
        "combined"
    }
    fn rank(&self, query: &str) -> Vec<&str> {
        combined::Combined::rank(self, query)
    }
}
