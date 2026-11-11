use std::sync::Mutex;

use crate::cleanup;
use crate::config::Config;
use crate::review::{self, Review};

// The review an agent session is reading: page 1 starts a new one, the other pages come from it.
#[derive(Default)]
pub struct ReviewSession(Mutex<Option<Review>>);

impl ReviewSession {
    // Reading the last page records the cleanup, at the time its review was built.
    pub fn page(
        &self,
        config: &Config,
        page: usize,
        full: bool,
        agent: &str,
    ) -> Result<String, String> {
        let mut held = self.0.lock().unwrap();
        let review = match held.take() {
            Some(review) if page != 1 && review.full == full => review,
            _ => review::review(config, full)?,
        };
        let text = review.page(page).map(str::to_string);
        if review.ends_at(page) {
            cleanup::record(config, review.started, agent)?;
        }
        *held = Some(review);
        text
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    // 80 preferences: several pages.
    fn vault() -> (tempfile::TempDir, Config) {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("Preferences")).unwrap();
        for i in 0..80 {
            fs::write(
                tmp.path().join(format!("Preferences/rule-{i}.md")),
                "---\ntype: feedback\nscope: all repos\nsummary: s\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n**Why:** x\n",
            )
            .unwrap();
        }
        let config = Config {
            git_autocommit: false,
            ..Config::new(tmp.path().to_path_buf())
        };
        (tmp, config)
    }

    #[test]
    fn a_cleanup_counts_only_once_its_last_page_is_read() {
        let (tmp, config) = vault();
        let session = ReviewSession::default();
        let first = session.page(&config, 1, false, "test").unwrap();
        assert!(
            first.contains("more: call memory_review with page=2"),
            "{first}"
        );
        assert_eq!(cleanup::last(tmp.path()), None);
        let mut page = 2;
        while session
            .page(&config, page, false, "test")
            .unwrap()
            .contains("more:")
        {
            page += 1;
        }
        assert!(cleanup::last(tmp.path()).is_some());
    }

    #[test]
    fn pages_come_from_the_review_page_1_built_until_page_1_or_another_extent_is_asked() {
        let (tmp, config) = vault();
        let session = ReviewSession::default();
        session.page(&config, 1, false, "test").unwrap();
        let second = session.page(&config, 2, false, "test").unwrap();
        assert!(second.contains("\nfirst cleanup: all 80 notes"), "{second}");

        cleanup::record(&config, std::time::SystemTime::now(), "test").unwrap();
        fs::remove_file(tmp.path().join("Preferences/rule-0.md")).unwrap();
        assert_eq!(session.page(&config, 2, false, "test").unwrap(), second);
        assert_eq!(
            session.page(&config, 99, false, "test").unwrap_err(),
            "no page 99: the review has 2 pages"
        );
        assert_eq!(session.page(&config, 2, false, "test").unwrap(), second);

        let full = session.page(&config, 2, true, "test").unwrap();
        assert!(full.contains("\nall 79 notes, 0 changed"), "{full}");
        let restarted = session.page(&config, 1, false, "test").unwrap();
        assert!(
            restarted.contains("\nall 79 notes, 0 changed"),
            "{restarted}"
        );
    }

    #[test]
    fn a_page_asked_before_page_1_builds_the_review() {
        let (_tmp, config) = vault();
        let page = ReviewSession::default()
            .page(&config, 2, false, "test")
            .unwrap();
        assert!(page.contains("\npage 2 of "), "{page}");
    }
}
