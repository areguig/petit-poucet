use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Mutex;

// What this session last read of each note, so a write never replaces text the agent hasn't seen.
#[derive(Default)]
pub struct ReadLog(Mutex<HashMap<String, u64>>);

enum Seen {
    Current,
    Unread(String),
    Changed(String),
}

fn fingerprint(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn key(path: &str) -> &str {
    path.strip_suffix(".md").unwrap_or(path)
}

impl ReadLog {
    pub fn record(&self, path: &str, text: &str) {
        self.0
            .lock()
            .unwrap()
            .insert(key(path).to_string(), fingerprint(text));
    }

    pub fn forget(&self, path: &str) {
        self.0.lock().unwrap().remove(key(path));
    }

    // A note missing on disk counts as current: the write itself reports it.
    fn seen(&self, root: &Path, path: &str) -> Seen {
        let Ok(text) = fs::read_to_string(root.join(format!("{path}.md"))) else {
            return Seen::Current;
        };
        match self.0.lock().unwrap().get(path) {
            None => Seen::Unread(text),
            Some(&seen) if seen != fingerprint(&text) => Seen::Changed(text),
            Some(_) => Seen::Current,
        }
    }

    pub fn check(&self, root: &Path, path: &str) -> Result<(), String> {
        let path = key(path);
        match self.seen(root, path) {
            Seen::Current => Ok(()),
            Seen::Unread(_) => Err(format!("read {path} first (memory_read), then change it")),
            Seen::Changed(_) => Err(format!(
                "{path} changed since you read it (edited by the user or another agent): read it again, then redo your change"
            )),
        }
    }

    // A delete needs no edit of its own: the refusal shows the text and counts as reading it, so a retry goes through.
    pub fn check_delete(&self, root: &Path, path: &str) -> Result<(), String> {
        let path = key(path);
        let (why, text) = match self.seen(root, path) {
            Seen::Current => return Ok(()),
            Seen::Unread(text) => ("not read yet", text),
            Seen::Changed(text) => ("changed since you read it", text),
        };
        self.record(path, &text);
        Err(format!(
            "{path} {why}: its current text is below, now counted as read. Call memory_delete again if it should still go.\n\n{text}"
        ))
    }

    // After this session's own tool rewrote links in a note it had read, the new text counts as seen.
    pub fn refresh(&self, root: &Path, path: &str) {
        if self.0.lock().unwrap().contains_key(key(path)) {
            self.record_file(root, path);
        }
    }

    pub fn record_file(&self, root: &Path, path: &str) {
        match fs::read_to_string(root.join(format!("{}.md", key(path)))) {
            Ok(text) => self.record(path, &text),
            Err(_) => self.forget(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_a_write_only_on_what_was_read() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("note.md");
        fs::write(&file, "v1").unwrap();
        let log = ReadLog::default();

        let err = log.check(tmp.path(), "note").unwrap_err();
        assert!(err.starts_with("read note first"), "{err}");

        log.record("note.md", "v1");
        assert!(log.check(tmp.path(), "note").is_ok());

        fs::write(&file, "v2 from Obsidian").unwrap();
        let err = log.check(tmp.path(), "note.md").unwrap_err();
        assert!(err.contains("changed since you read it"), "{err}");

        log.record_file(tmp.path(), "note");
        assert!(log.check(tmp.path(), "note").is_ok());

        fs::write(&file, "v3 from this session's own link rewrite").unwrap();
        log.refresh(tmp.path(), "note");
        assert!(
            log.check(tmp.path(), "note").is_ok(),
            "known notes are refreshed"
        );
        fs::write(tmp.path().join("unread.md"), "x").unwrap();
        log.refresh(tmp.path(), "unread.md");
        assert!(
            log.check(tmp.path(), "unread").is_err(),
            "unread notes stay unread"
        );

        log.forget("note");
        assert!(log.check(tmp.path(), "note").is_err());
        assert!(log.check(tmp.path(), "missing").is_ok());
    }

    #[test]
    fn a_refused_delete_shows_the_text_and_counts_as_read() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("note.md");
        fs::write(&file, "v1").unwrap();
        let log = ReadLog::default();

        let err = log.check_delete(tmp.path(), "note").unwrap_err();
        assert!(err.starts_with("note not read yet: "), "{err}");
        assert!(err.ends_with("\n\nv1"), "{err}");
        assert!(log.check_delete(tmp.path(), "note.md").is_ok());

        fs::write(&file, "v2 from Obsidian").unwrap();
        let err = log.check_delete(tmp.path(), "note").unwrap_err();
        assert!(err.starts_with("note changed since you read it: "), "{err}");
        assert!(err.ends_with("v2 from Obsidian"), "{err}");
        assert!(log.check_delete(tmp.path(), "note").is_ok());
        assert!(log.check_delete(tmp.path(), "missing").is_ok());
    }
}
