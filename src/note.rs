use std::sync::LazyLock;

use jiff::civil::Date;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::vault::{PROJECTS, TOPICS};

pub const REQUIRED_TAG: &str = "agent-memory";
pub const ALL_REPOS: &str = "all repos";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NoteType {
    User,
    Feedback,
    Project,
    Reference,
}

impl NoteType {
    pub fn name(self) -> &'static str {
        match self {
            NoteType::User => "user",
            NoteType::Feedback => "feedback",
            NoteType::Project => "project",
            NoteType::Reference => "reference",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Frontmatter {
    #[serde(rename = "type")]
    pub note_type: NoteType,
    pub scope: String,
    pub summary: Option<String>,
    pub created: Date,
    #[serde(skip_serializing_if = "Option::is_none")]
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

    pub fn place(&self) -> Place<'_> {
        Place::of(&self.path)
    }

    pub fn title(&self) -> &str {
        self.body
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .map(str::trim)
            .unwrap_or_else(|| self.path.rsplit('/').next().unwrap_or_default())
    }

    pub fn summary(&self) -> Option<&str> {
        let summary = self.frontmatter.as_ref().ok()?.summary.as_deref()?.trim();
        (!summary.is_empty()).then_some(summary)
    }
}

// Where a note lives; its folder decides its scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place<'a> {
    Preferences,
    Project(&'a str),
    Topic(&'a str),
}

impl<'a> Place<'a> {
    pub fn of(path: &'a str) -> Place<'a> {
        match path.split('/').collect::<Vec<_>>()[..] {
            [PROJECTS, key, _] => Place::Project(key),
            [TOPICS, key, _] => Place::Topic(key),
            _ => Place::Preferences,
        }
    }

    pub fn scope(self) -> &'a str {
        match self {
            Place::Preferences => ALL_REPOS,
            Place::Project(key) | Place::Topic(key) => key,
        }
    }
}

pub fn render(frontmatter: &impl Serialize, body: &str) -> Result<String, String> {
    // One line per value: folded `>-` blocks are hard to edit by hand in Obsidian.
    let mut options = serde_saphyr::SerializerOptions::default();
    options.prefer_block_scalars = false;
    let yaml =
        serde_saphyr::to_string_with_options(frontmatter, options).map_err(|e| e.to_string())?;
    Ok(format!("---\n{yaml}---\n\n{body}"))
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
    LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)([#|][^\]]*)?\]\]").unwrap());

pub fn links(body: &str) -> impl Iterator<Item = &str> {
    LINK.captures_iter(body)
        .map(|c| c.get(1).unwrap().as_str().trim())
}

// Replaces each link target for which `new_target` returns Some, keeping `#heading` and `|alias`.
pub fn map_links(body: &str, new_target: impl Fn(&str) -> Option<String>) -> String {
    LINK.replace_all(body, |c: &regex::Captures| match new_target(c[1].trim()) {
        Some(target) => format!("[[{target}{}]]", c.get(2).map_or("", |m| m.as_str())),
        None => c[0].to_string(),
    })
    .into_owned()
}

// Turns links to `target` into plain text: their alias, or `text` when they have none.
pub fn unlink(body: &str, target: &str, text: &str) -> String {
    LINK.replace_all(body, |c: &regex::Captures| {
        if c[1].trim() != target {
            return c[0].to_string();
        }
        match c.get(2).and_then(|m| m.as_str().split_once('|')) {
            Some((_, alias)) => alias.to_string(),
            None => text.to_string(),
        }
    })
    .into_owned()
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
    fn render_round_trips_and_quotes_what_yaml_needs() {
        let note = Note::parse("Preferences/commit".into(), GOOD);
        let mut fm = note.frontmatter.unwrap();
        fm.summary = Some("plugin + MCP server: 3 tools".into());
        let text = render(&fm, "# Title\n\nBody\n").unwrap();
        let again = Note::parse("Preferences/commit".into(), &text);
        let parsed = again.frontmatter.as_ref().unwrap();
        assert_eq!(parsed.summary, fm.summary);
        assert_eq!(parsed.created, fm.created);
        assert_eq!(parsed.tags, fm.tags);
        assert_eq!(again.title(), "Title");
        assert!(!text.contains("updated"), "{text}");

        fm.summary = Some("a long summary ".repeat(10).trim().to_string());
        let text = render(&fm, "").unwrap();
        assert!(
            text.contains(&format!("summary: {}\n", fm.summary.as_ref().unwrap())),
            "{text}"
        );
    }

    #[test]
    fn a_notes_folder_decides_its_place_and_scope() {
        assert_eq!(Place::of("Preferences/a"), Place::Preferences);
        assert_eq!(Place::of("Projects/app/a"), Place::Project("app"));
        assert_eq!(Place::of("Topics/homelab/a").scope(), "homelab");
        assert_eq!(Place::Preferences.scope(), ALL_REPOS);
    }

    #[test]
    fn title_falls_back_to_the_file_name() {
        assert_eq!(
            Note::parse("Preferences/commit".into(), "no heading").title(),
            "commit"
        );
    }

    #[test]
    fn maps_links_keeping_heading_and_alias() {
        let body = "[[a]], [[b|alias]], [[a#Heading]] and [[c]]";
        let mapped = map_links(body, |t| (t != "c").then(|| format!("Preferences/{t}")));
        assert_eq!(
            mapped,
            "[[Preferences/a]], [[Preferences/b|alias]], [[Preferences/a#Heading]] and [[c]]"
        );
    }

    #[test]
    fn unlinks_only_the_target_keeping_aliases() {
        let body = "[[a]], [[a|the rule]], [[a#H]] and [[b]]";
        assert_eq!(
            unlink(body, "a", "Commit rules"),
            "Commit rules, the rule, Commit rules and [[b]]"
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
