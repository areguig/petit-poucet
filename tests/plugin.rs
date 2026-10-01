use std::fs;
use std::path::Path;

use serde_json::Value;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn json(path: &str) -> Value {
    let text = fs::read_to_string(Path::new(ROOT).join(path)).unwrap();
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn every_manifest_is_valid_and_pins_the_crate_version() {
    for path in [
        "plugin/.mcp.json",
        "plugin/hooks/hooks.json",
        "plugin/copilot/mcp.json",
        "plugin/copilot/hooks.json",
    ] {
        json(path);
    }
    assert_eq!(
        json("plugin/.claude-plugin/plugin.json")["version"],
        VERSION
    );
    assert_eq!(json("plugin/plugin.json")["version"], VERSION);
    let copilot = json(".github/plugin/marketplace.json");
    assert_eq!(copilot["metadata"]["version"], VERSION);
    assert_eq!(copilot["plugins"][0]["version"], VERSION);
    assert_eq!(
        json(".claude-plugin/marketplace.json")["plugins"][0]["source"],
        "./plugin"
    );
    let release = fs::read_to_string(Path::new(ROOT).join("plugin/release.env")).unwrap();
    assert!(
        release.contains(&format!("PETIT_POUCET_VERSION={VERSION}\n")),
        "{release}"
    );
}

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

// Copilot silently ignores a skill or agent whose frontmatter is invalid YAML.
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
        ("plugin/skills/migrate-memory/SKILL.md", "migrate-memory"),
        ("plugin/skills/tidy-memory/SKILL.md", "tidy-memory"),
        ("plugin/skills/memory/SKILL.md", "memory"),
        ("plugin/agents/memory-cleanup.md", "memory-cleanup"),
        (
            "plugin/copilot/agents/memory-cleanup.agent.md",
            "memory-cleanup",
        ),
    ] {
        let (frontmatter, body) = frontmatter_and_body(path);
        assert_eq!(frontmatter.name, name, "{path}");
        assert!(
            !frontmatter.description.is_empty() && !body.is_empty(),
            "{path}"
        );
    }
}

#[test]
fn both_agents_get_the_same_cleanup_instructions() {
    let (_, claude) = frontmatter_and_body("plugin/agents/memory-cleanup.md");
    let (_, copilot) = frontmatter_and_body("plugin/copilot/agents/memory-cleanup.agent.md");
    assert_eq!(claude, copilot);
}

#[test]
fn copilot_manifest_points_to_existing_files() {
    let manifest = json("plugin/plugin.json");
    for key in ["mcpServers", "hooks", "agents", "skills"] {
        let path = manifest[key].as_str().unwrap();
        assert!(
            Path::new(ROOT).join("plugin").join(path).exists(),
            "{key}: {path}"
        );
    }
    // One folder serves both agents, like each marketplace says.
    assert_eq!(
        json(".github/plugin/marketplace.json")["plugins"][0]["source"],
        "./plugin"
    );
    assert_eq!(
        json(".claude-plugin/marketplace.json")["plugins"][0]["source"],
        "./plugin"
    );
}
