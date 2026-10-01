use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::agent::Agent;
use crate::{antigravity, codex, edit, note};

// The plugin's skills and the subagent tidy-memory starts, embedded for the agents `setup` configures.
const SKILLS: [(&str, &str); 3] = [
    ("memory", include_str!("../plugin/skills/memory/SKILL.md")),
    (
        "migrate-memory",
        include_str!("../plugin/skills/migrate-memory/SKILL.md"),
    ),
    (
        "tidy-memory",
        include_str!("../plugin/skills/tidy-memory/SKILL.md"),
    ),
];
const CLEANUP: &str = include_str!("../plugin/agents/memory-cleanup.md");
const CLEANUP_NAME: &str = "memory-cleanup";

// Codex and Cursor share the Agent Skills folder; Antigravity reads only its own.
fn skills_dir(agent: Agent, home: &Path) -> Option<PathBuf> {
    match agent {
        Agent::Codex | Agent::Cursor => Some(home.join(".agents/skills")),
        Agent::Antigravity => Some(antigravity::dir(home).join("skills")),
        Agent::Claude | Agent::Copilot => None,
    }
}

#[derive(Deserialize)]
struct Frontmatter {
    description: String,
}

// Tools are left out: their names differ per agent, and the subagent inherits the main agent's.
fn subagent(agent: Agent, home: &Path) -> Option<(PathBuf, String)> {
    let (yaml, prompt) =
        note::split_frontmatter(CLEANUP).expect("memory-cleanup.md has frontmatter");
    let description = note::parse_yaml::<Frontmatter>(yaml)
        .expect("memory-cleanup.md has a description")
        .description;
    // A JSON string is a valid YAML scalar, quotes and colons included.
    let quoted = serde_json::to_string(&description).unwrap();
    let markdown = |extra: &str, heading: &str| {
        format!("---\nname: {CLEANUP_NAME}\ndescription: {quoted}\n{extra}---\n\n{heading}{prompt}")
    };
    Some(match agent {
        Agent::Codex => {
            let mut table = toml::Table::new();
            table.insert("name".into(), CLEANUP_NAME.into());
            table.insert("description".into(), description.into());
            table.insert("developer_instructions".into(), prompt.into());
            (
                codex::dir(home).join(format!("agents/{CLEANUP_NAME}.toml")),
                toml::to_string(&table).unwrap(),
            )
        }
        Agent::Cursor => (
            home.join(format!(".cursor/agents/{CLEANUP_NAME}.md")),
            markdown("", ""),
        ),
        // Antigravity's prompt starts at an H1.
        Agent::Antigravity => (
            antigravity::dir(home).join(format!("agents/{CLEANUP_NAME}/agent.md")),
            markdown("subagent: true\n", &format!("# {CLEANUP_NAME}\n\n")),
        ),
        Agent::Claude | Agent::Copilot => return None,
    })
}

// Every file petit-poucet writes for this agent, with its content.
fn files(agent: Agent, home: &Path) -> Vec<(PathBuf, String)> {
    let skills = skills_dir(agent, home).into_iter().flat_map(|dir| {
        SKILLS.map(|(name, text)| (dir.join(name).join("SKILL.md"), text.to_string()))
    });
    skills.chain(subagent(agent, home)).collect()
}

pub fn setup(agent: Agent, home: &Path) -> Result<String, String> {
    let mut changed = false;
    for (path, text) in files(agent, home) {
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        changed |= edit::replace(&path, &text)?;
    }
    Ok(match (changed, skills_dir(agent, home)) {
        (true, Some(dir)) => format!(
            "installed in {} (with the {CLEANUP_NAME} subagent)",
            dir.display()
        ),
        _ => "already installed".to_string(),
    })
}

pub fn check(agent: Agent, home: &Path) -> (String, bool) {
    let stale: Vec<String> = files(agent, home)
        .into_iter()
        .filter(|(path, text)| fs::read_to_string(path).ok().as_ref() != Some(text))
        .map(|(path, _)| path.display().to_string())
        .collect();
    match stale.is_empty() {
        true => ("installed".to_string(), true),
        false => (
            format!(
                "missing or outdated {}: run `petit-poucet setup --agent {}`",
                stale.join(", "),
                agent.flag()
            ),
            false,
        ),
    }
}

pub fn uninstall(agent: Agent, home: &Path) -> Result<String, String> {
    let mut removed = false;
    if let Some((path, _)) = subagent(agent, home)
        && path.is_file()
    {
        fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        removed = true;
        if agent == Agent::Antigravity {
            fs::remove_dir(path.parent().unwrap()).ok();
        }
    }
    let Some(dir) = skills_dir(agent, home) else {
        return Ok("nothing to remove".to_string());
    };
    // Shared skills stay while another agent that reads them still has its subagent.
    let users: Vec<&str> = Agent::ALL
        .into_iter()
        .filter(|&other| other != agent && skills_dir(other, home).as_ref() == Some(&dir))
        .filter(|&other| subagent(other, home).is_some_and(|(path, _)| path.is_file()))
        .map(Agent::name)
        .collect();
    if !users.is_empty() {
        return Ok(format!(
            "{CLEANUP_NAME} subagent removed, skills kept for {}",
            users.join(", ")
        ));
    }
    for (name, _) in SKILLS {
        let skill = dir.join(name);
        if skill.is_dir() {
            fs::remove_dir_all(&skill).map_err(|e| format!("{}: {e}", skill.display()))?;
            removed = true;
        }
    }
    Ok(match removed {
        true => "removed".to_string(),
        false => "nothing to remove".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_the_skills_and_the_subagent_in_each_agents_format() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        for agent in [Agent::Codex, Agent::Cursor, Agent::Antigravity] {
            assert!(!check(agent, home).1);
            assert!(setup(agent, home).unwrap().starts_with("installed in "));
            assert_eq!(setup(agent, home).unwrap(), "already installed");
            assert_eq!(check(agent, home), ("installed".to_string(), true));
        }
        let skill = fs::read_to_string(home.join(".agents/skills/tidy-memory/SKILL.md")).unwrap();
        assert_eq!(skill, SKILLS[2].1);
        assert!(home.join(".gemini/config/skills/memory/SKILL.md").is_file());

        let codex: toml::Table = toml::from_str(
            &fs::read_to_string(home.join(".codex/agents/memory-cleanup.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(codex["name"].as_str(), Some("memory-cleanup"));
        assert!(
            codex["description"]
                .as_str()
                .unwrap()
                .starts_with("Reviews the whole")
        );
        assert!(
            codex["developer_instructions"]
                .as_str()
                .unwrap()
                .starts_with("You review a petit-poucet")
        );
        for (path, extra) in [
            (".cursor/agents/memory-cleanup.md", ""),
            (
                ".gemini/config/agents/memory-cleanup/agent.md",
                "subagent: true\n",
            ),
        ] {
            let text = fs::read_to_string(home.join(path)).unwrap();
            let (yaml, body) = note::split_frontmatter(&text).unwrap();
            assert!(
                yaml.starts_with("name: memory-cleanup\ndescription: \"Reviews the whole"),
                "{path}"
            );
            assert!(yaml.ends_with(&format!("\n{extra}")), "{path}");
            assert!(body.contains("You review a petit-poucet"), "{path}");
        }
    }

    #[test]
    fn check_reports_a_changed_skill() {
        let home = tempfile::tempdir().unwrap();
        setup(Agent::Codex, home.path()).unwrap();
        let skill = skills_dir(Agent::Codex, home.path())
            .unwrap()
            .join("memory")
            .join("SKILL.md");
        fs::write(&skill, "old").unwrap();
        let (line, ok) = check(Agent::Codex, home.path());
        assert!(
            !ok && line.contains(&skill.display().to_string()) && line.ends_with("--agent codex`"),
            "{line}"
        );
    }

    #[test]
    fn shared_skills_stay_until_the_last_agent_using_them_is_uninstalled() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        setup(Agent::Codex, home).unwrap();
        setup(Agent::Cursor, home).unwrap();
        fs::write(home.join(".agents/skills/mine.md"), "the user's").unwrap();

        assert_eq!(
            uninstall(Agent::Codex, home).unwrap(),
            "memory-cleanup subagent removed, skills kept for Cursor"
        );
        assert!(!home.join(".codex/agents/memory-cleanup.toml").exists());
        assert!(home.join(".agents/skills/memory/SKILL.md").is_file());

        assert_eq!(uninstall(Agent::Cursor, home).unwrap(), "removed");
        assert!(!home.join(".agents/skills/memory").exists());
        assert!(
            home.join(".agents/skills/mine.md").is_file(),
            "only its own skills go"
        );
        assert_eq!(uninstall(Agent::Cursor, home).unwrap(), "nothing to remove");

        setup(Agent::Antigravity, home).unwrap();
        assert_eq!(uninstall(Agent::Antigravity, home).unwrap(), "removed");
        assert!(!home.join(".gemini/config/agents/memory-cleanup").exists());
    }
}
