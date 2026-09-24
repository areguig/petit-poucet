use std::collections::HashMap;
use std::fs;
use std::sync::LazyLock;

use regex::Regex;

use crate::config::Config;
use crate::init::ensure_repo;
use crate::note;
use crate::project::{self, IDENTITY_FILE};
use crate::vault::{INDEX_FILE, PROJECTS, Vault, write_atomic};
use crate::{git, index};

static INDEX_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^- \[\[([^\]]+)\]\] — (.+)$").unwrap());

// Upgrades a hand-maintained vault: summaries come from the old Index, short links become full paths.
pub fn migrate(config: &Config) -> Result<String, String> {
    let root = &config.vault;
    let old_index = fs::read_to_string(root.join(INDEX_FILE)).unwrap_or_default();
    let summaries: HashMap<&str, &str> = INDEX_LINE
        .captures_iter(&old_index)
        .map(|c| {
            (
                c.get(1).unwrap().as_str(),
                c.get(2).unwrap().as_str().trim(),
            )
        })
        .filter(|(_, summary)| *summary != index::NO_SUMMARY)
        .collect();
    let vault = Vault::load(root)?;
    let mut by_name: HashMap<&str, Vec<&str>> = HashMap::new();
    for note in &vault.notes {
        let name = note.path.rsplit('/').next().unwrap();
        by_name.entry(name).or_default().push(&note.path);
    }

    let mut report = Vec::new();
    let (mut summaries_added, mut links_fixed, mut projects_added) = (0, 0, 0);
    for note in &vault.notes {
        let Ok(frontmatter) = &note.frontmatter else {
            report.push(format!("{}.md: skipped, unreadable frontmatter", note.path));
            continue;
        };
        let mut frontmatter = frontmatter.clone();
        if note.summary().is_none() {
            match summaries.get(note.path.as_str()) {
                Some(summary) => {
                    frontmatter.summary = Some(summary.to_string());
                    summaries_added += 1;
                }
                None => report.push(format!(
                    "{}.md: no Index line to take a summary from",
                    note.path
                )),
            }
        }
        let body = note::map_links(&note.body, |target| full_path(&vault, &by_name, target));
        links_fixed += note::links(&note.body)
            .zip(note::links(&body))
            .filter(|(old, new)| old != new)
            .count();
        if frontmatter.summary != note.frontmatter.as_ref().unwrap().summary || body != note.body {
            write_atomic(
                &root.join(format!("{}.md", note.path)),
                &note::render(&frontmatter, &body)?,
            )?;
        }
    }

    for project in vault.projects.iter().filter(|p| p.identity.is_err()) {
        let file = root.join(PROJECTS).join(&project.key).join(IDENTITY_FILE);
        if file.exists() {
            report.push(format!(
                "{PROJECTS}/{}/{IDENTITY_FILE}: invalid, left as is",
                project.key
            ));
            continue;
        }
        write_atomic(&file, &project::render_identity(&[], &project.key)?)?;
        projects_added += 1;
    }

    ensure_repo(root)?;
    let vault = Vault::load(root)?;
    write_atomic(&root.join(INDEX_FILE), &index::generate(&vault))?;
    if config.git_autocommit {
        git::commit(root, &["."], "migrate: vault")?;
    }
    report.insert(
        0,
        format!(
            "migrated {}: {summaries_added} summaries added, {links_fixed} links rewritten, {projects_added} projects identified",
            root.display()
        ),
    );
    Ok(report.join("\n"))
}

// `[[name]]` becomes `[[Folder/name]]` when exactly one note has that name.
fn full_path(vault: &Vault, by_name: &HashMap<&str, Vec<&str>>, target: &str) -> Option<String> {
    if vault.has_note(target) || vault.root.join(target).exists() {
        return None;
    }
    match by_name.get(target)?.as_slice() {
        [path] => Some(path.to_string()),
        _ => None,
    }
}
