use std::fs;
use std::path::Path;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const VERSION: &str = env!("CARGO_PKG_VERSION");

// The release workflow publishes this file as the release notes.
#[test]
fn the_crate_version_has_release_notes() {
    let notes = Path::new(ROOT).join(format!("docs/releases/v{VERSION}.md"));
    let text = fs::read_to_string(&notes).unwrap_or_default();
    assert!(!text.trim().is_empty(), "write {}", notes.display());
}

#[derive(serde::Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
}

// Agents silently ignore a skill or agent whose frontmatter is invalid YAML.
fn frontmatter_and_body(path: &str) -> (Frontmatter, String) {
    let text = fs::read_to_string(Path::new(ROOT).join(path)).unwrap();
    let (yaml, body) = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .unwrap_or_else(|| panic!("{path}: no frontmatter"));
    let frontmatter: Frontmatter =
        serde_saphyr::from_str(yaml).unwrap_or_else(|e| panic!("{path}: {e}"));
    (frontmatter, body.trim().to_string())
}

#[test]
fn skills_and_agents_have_valid_frontmatter() {
    for (path, name) in [
        ("skills/migrate-memory/SKILL.md", "migrate-memory"),
        ("skills/tidy-memory/SKILL.md", "tidy-memory"),
        ("skills/memory/SKILL.md", "memory"),
        ("agents/memory-cleanup.md", "memory-cleanup"),
    ] {
        let (frontmatter, body) = frontmatter_and_body(path);
        assert_eq!(frontmatter.name, name, "{path}");
        assert!(
            !frontmatter.description.is_empty() && !body.is_empty(),
            "{path}"
        );
    }
}
