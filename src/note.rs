use std::sync::LazyLock;

use jiff::civil::Date;
use regex::Regex;
use serde::Deserialize;

pub const REQUIRED_TAG: &str = "agent-memory";
pub const ALL_REPOS: &str = "all repos";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteType {
    User,
    Feedback,
    Project,
    Reference,
}

#[derive(Debug, Deserialize)]
pub struct Frontmatter {
    #[serde(rename = "type")]
    #[allow(
        dead_code,
        reason = "validated on parse; read by the MCP tools from M2"
    )]
    pub note_type: NoteType,
    pub scope: String,
    pub summary: Option<String>,
    pub created: Date,
    pub updated: Option<Date>,
    #[serde(default)]
    pub tags: Vec<String>,
}

// `path` is relative to the vault root, without `.md`, as in `[[links]]`.
#[derive(Debug)]
pub struct Note {
    pub path: String,
    pub frontmatter: Result<Frontmatter, String>,
    pub body: String,
}

impl Note {
    pub fn parse(path: String, text: &str) -> Note {
        let (frontmatter, body) = match split_frontmatter(text) {
            Some((yaml, body)) => (parse_yaml(yaml), body),
            None => (Err("no frontmatter".to_string()), text),
        };
        Note {
            path,
            frontmatter,
            body: body.to_string(),
        }
    }

    pub fn summary(&self) -> Option<&str> {
        let summary = self.frontmatter.as_ref().ok()?.summary.as_deref()?.trim();
        (!summary.is_empty()).then_some(summary)
    }
}

pub fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let end = rest
        .match_indices("\n---")
        .find(|(i, _)| matches!(rest[i + 4..].chars().next(), None | Some('\n' | '\r')))?
        .0;
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    Some((&rest[..=end], body))
}

pub fn parse_yaml<'a, T: Deserialize<'a>>(yaml: &'a str) -> Result<T, String> {
    // serde-saphyr renders a multi-line snippet; the first line holds the message.
    serde_saphyr::from_str(yaml).map_err(|e| {
        let text = e.to_string();
        let first = text.lines().next().unwrap_or_default();
        first.strip_prefix("error: ").unwrap_or(first).to_string()
    })
}

static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]").unwrap());

pub fn links(body: &str) -> impl Iterator<Item = &str> {
    LINK.captures_iter(body)
        .map(|c| c.get(1).unwrap().as_str().trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "---\ntype: feedback\nscope: all repos\nsummary: commit locally\ncreated: 2026-09-14\ntags: [agent-memory, git]\n---\n\n# Commit rules\n\nBody.\n";

    #[test]
    fn parses_a_note() {
        let note = Note::parse("Preferences/commit".into(), GOOD);
        let fm = note.frontmatter.as_ref().unwrap();
        assert_eq!(fm.note_type, NoteType::Feedback);
        assert_eq!(fm.scope, ALL_REPOS);
        assert_eq!(fm.created, jiff::civil::date(2026, 9, 14));
        assert_eq!(note.summary(), Some("commit locally"));
        assert!(note.body.starts_with("# Commit rules"));
    }

    #[test]
    fn reports_bad_frontmatter_on_one_line() {
        let note = Note::parse("x".into(), &GOOD.replace("feedback", "opinion"));
        let err = note.frontmatter.unwrap_err();
        assert!(err.contains("opinion") && !err.contains('\n'), "{err}");
        assert!(
            Note::parse("x".into(), "# no frontmatter")
                .frontmatter
                .is_err()
        );
    }

    #[test]
    fn finds_links() {
        let body = "See [[Preferences/a]], [[Projects/p/b|alias]] and [[Projects/p/c#Heading]].";
        assert_eq!(
            links(body).collect::<Vec<_>>(),
            ["Preferences/a", "Projects/p/b", "Projects/p/c"]
        );
    }
}
