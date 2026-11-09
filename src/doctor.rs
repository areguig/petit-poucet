mod binary;
mod configuration;
mod report;

pub use report::exit_code;
use report::{Item, Status, quote};

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use crate::agent::{Agent, Dirs};
use crate::config::Config;
use crate::vault::Vault;
use crate::{check, cleanup, context, git, hook, project, setup, state, update, usage};

pub fn run() -> Vec<Item> {
    let mut items = vec![Item::new(
        "binary",
        Status::Ok,
        format!("petit-poucet {}", update::CURRENT),
        None,
    )];
    match std::env::current_exe() {
        Ok(exe) => items.push(binary::on_path(
            &exe,
            which::which("petit-poucet").ok().as_deref(),
        )),
        Err(_) => items.push(Item::new(
            "binary PATH",
            Status::Error,
            "cannot locate this binary",
            Some("petit-poucet setup".into()),
        )),
    }
    let (release, notice) = binary::release();
    items.push(release);
    let (config, config_items) = configuration::inspect();
    items.extend(config_items);
    let vault = match &config {
        Some(config) => inspect_vault(config, &mut items),
        None => {
            items.push(Item::new(
                "vault",
                Status::Info,
                "not checked until the config is valid",
                None,
            ));
            None
        }
    };
    let Some(home) = std::env::home_dir() else {
        return items;
    };
    let dirs = Dirs::from_env(home);
    for agent in Agent::ALL {
        if !agent.installed(&dirs) {
            items.push(Item::new(agent.name(), Status::Info, "not installed", None));
            continue;
        }
        let (_, wired) = setup::report(agent, setup::Mode::Check, &dirs, true);
        items.push(Item::new(
            agent.name(),
            if wired { Status::Ok } else { Status::Error },
            if wired {
                "wired (configuration and skills)"
            } else {
                "configuration or skills missing, invalid or outdated"
            },
            (!wired).then(|| format!("petit-poucet setup --agent {}", agent.flag())),
        ));
        if let (Some(config), Some(vault)) = (&config, &vault) {
            items.push(payload(agent, vault, config, notice.as_deref()));
        }
    }
    items
}

fn inspect_vault(config: &Config, items: &mut Vec<Item>) -> Option<Vault> {
    let vault = match Vault::load(&config.vault) {
        Ok(vault) => vault,
        Err(_) => {
            items.push(Item::new(
                "vault",
                Status::Error,
                format!("not found or unreadable at {}", config.vault.display()),
                Some(format!("petit-poucet init {}", quote(&config.vault))),
            ));
            return None;
        }
    };
    items.push(Item::new(
        "vault",
        Status::Ok,
        format!("{} ({} notes)", config.vault.display(), vault.notes.len()),
        None,
    ));
    let repo = git::is_repo(&vault.root);
    items.push(Item::new(
        "vault git",
        if repo { Status::Ok } else { Status::Error },
        if repo {
            "repository found"
        } else {
            "not a git repository"
        },
        (!repo).then(|| format!("git -C {} init", quote(&vault.root))),
    ));
    if repo {
        items.push(identity(&vault.root, git::has_identity(&vault.root)));
    }
    let issues = check::check(&vault, &config.limits);
    let errors = issues
        .iter()
        .filter(|i| i.level == check::Level::Error)
        .count();
    let warnings = issues.len() - errors;
    let status = if errors > 0 {
        Status::Error
    } else if warnings > 0 {
        Status::Warning
    } else {
        Status::Ok
    };
    items.push(Item::new(
        "vault check",
        status,
        format!("{errors} errors, {warnings} warnings"),
        (errors + warnings > 0).then(|| "petit-poucet check".into()),
    ));
    let conflicts = vault.conflicts.len();
    let conflict_error = issues.iter().any(|i| {
        i.level == check::Level::Error && vault.conflicts.iter().any(|c| c.copy == i.file)
    });
    items.push(Item::new(
        "sync conflicts",
        if conflict_error {
            Status::Error
        } else if conflicts > 0 {
            Status::Warning
        } else {
            Status::Ok
        },
        conflicts.to_string(),
        (conflicts > 0).then(|| "petit-poucet check".into()),
    ));
    let files = usage::machine_files(&vault.root);
    let own = fs::read_to_string(vault.root.join(state::DIR).join("machine-id")).ok();
    items.push(match own {
        Some(id) => {
            let peers = files
                .iter()
                .filter(|f| f.file_stem().is_none_or(|s| s != id.trim()))
                .count();
            Item::new(
                "sync usage",
                Status::Ok,
                format!("{peers} other machine(s) have usage files present"),
                None,
            )
        }
        None => Item::new(
            "sync usage",
            Status::Info,
            format!(
                "{} usage files present; local machine identity not recorded",
                files.len()
            ),
            None,
        ),
    });
    Some(vault)
}

fn identity(root: &Path, valid: bool) -> Item {
    let quoted = quote(root);
    Item::new("git identity", if valid { Status::Ok } else { Status::Error }, if valid { "author and committer configured" } else { "author or committer name/email missing" },
        (!valid).then(|| format!("git -C {quoted} config user.name 'Your Name'; git -C {quoted} config user.email 'you@example.com'")))
}

fn payload(agent: Agent, vault: &Vault, config: &Config, release: Option<&str>) -> Item {
    let cwd = std::env::current_dir().ok();
    let project = cwd
        .as_deref()
        .and_then(|cwd| project::resolve(&vault.projects, cwd));
    let notices = hook::conflicts_notice(vault)
        .into_iter()
        .chain(cleanup::due(vault, &config.limits, SystemTime::now()))
        .chain(release.map(str::to_string))
        .collect();
    let (notices, _) = hook::route(agent, notices);
    let max = config.limits.session_start_max_bytes;
    let view = context::session_view(vault, project, &notices, max);
    let partial = view.left_out.is_some();
    Item::new(
        format!("{} payload", agent.name()),
        if partial { Status::Warning } else { Status::Ok },
        format!(
            "{}/{} bytes; left out: {}",
            view.text.len(),
            max,
            view.left_out.as_deref().unwrap_or("none")
        ),
        partial.then(|| "petit-poucet check".into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_identity_names_local_git_configuration_as_the_fix() {
        let root = Path::new("vault with spaces");
        let missing = identity(root, false);
        assert_eq!(missing.status, Status::Error);
        let fix = missing.fix.unwrap();
        assert!(fix.contains("config user.name") && fix.contains("config user.email"));
        assert_eq!(identity(root, true).status, Status::Ok);
    }
}
