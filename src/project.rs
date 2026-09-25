use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::git;

pub const IDENTITY_FILE: &str = "_project.md";
const IDENTITY_TYPE: &str = "project-identity";

#[derive(Debug, Default, Deserialize)]
pub struct Identity {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub remotes: Vec<String>,
    #[serde(default)]
    pub folders: Vec<String>,
}

impl Identity {
    pub fn parse(text: &str) -> Result<Identity, String> {
        let (yaml, _) = crate::note::split_frontmatter(text).ok_or("no frontmatter")?;
        let identity: Identity = crate::note::parse_yaml(yaml)?;
        if identity.kind != IDENTITY_TYPE {
            return Err(format!("type must be `{IDENTITY_TYPE}`"));
        }
        Ok(identity)
    }
}

#[derive(Serialize)]
struct NewIdentity<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    remotes: Vec<String>,
    folders: Vec<&'a str>,
}

pub fn render_identity(remotes: &[String], folders: &[String]) -> Result<String, String> {
    let identity = NewIdentity {
        kind: IDENTITY_TYPE,
        remotes: remotes.iter().map(|u| normalise_remote(u)).collect(),
        folders: folders.iter().map(String::as_str).collect(),
    };
    crate::note::render(&identity, "")
}

// A project created without a checkout at hand (e.g. by `migrate`) has no remote yet: learn it from one.
// Projects that already list remotes are left alone, so a same-named folder of another repo is never merged in.
pub fn missing_remotes(project: &Project, dir: &Path) -> Option<Vec<String>> {
    let identity = project.identity.as_ref().ok()?;
    if !identity.remotes.is_empty() {
        return None;
    }
    let remotes = git::remote_urls(dir);
    (!remotes.is_empty()).then_some(remotes)
}

pub struct Project {
    pub key: String,
    pub identity: Result<Identity, String>,
}

// `git@github.com:Owner/repo.git` and `https://github.com/Owner/repo` both become `github.com/Owner/repo`.
pub fn normalise_remote(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let (host, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/').unwrap_or((rest, "")),
        None => match url.split_once(':') {
            Some((host, path)) if !host.contains('/') => (host, path),
            _ => return url.to_string(),
        },
    };
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_lowercase();
    format!("{host}/{}", path.trim_start_matches('/'))
}

pub fn resolve<'a>(projects: &'a [Project], dir: &Path) -> Option<&'a str> {
    let known: Vec<(&str, &Identity)> = projects
        .iter()
        .filter_map(|p| Some((p.key.as_str(), p.identity.as_ref().ok()?)))
        .collect();
    let remotes: Vec<String> = git::remote_urls(dir)
        .iter()
        .map(|u| normalise_remote(u))
        .collect();
    let by_remote = known.iter().find(|(_, id)| {
        id.remotes
            .iter()
            .any(|r| remotes.contains(&normalise_remote(r)))
    });
    let top = git::toplevel(dir).unwrap_or_else(|| dir.to_string_lossy().into_owned());
    let folder = Path::new(&top).file_name()?.to_string_lossy().into_owned();
    by_remote
        .or_else(|| known.iter().find(|(_, id)| id.folders.contains(&folder)))
        .map(|(key, _)| *key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_remotes() {
        for url in [
            "git@github.com:areguig/petit-poucet.git",
            "https://github.com/areguig/petit-poucet",
            "https://user@GitHub.com/areguig/petit-poucet.git/",
            "ssh://git@github.com:22/areguig/petit-poucet.git",
        ] {
            assert_eq!(
                normalise_remote(url),
                "github.com/areguig/petit-poucet",
                "{url}"
            );
        }
        assert_eq!(normalise_remote("/srv/repos/x.git"), "/srv/repos/x");
    }

    #[test]
    fn parses_identity() {
        let id = Identity::parse(
            "---\ntype: project-identity\nremotes: [github.com/a/b]\nfolders: [b]\n---\n",
        )
        .unwrap();
        assert_eq!(id.remotes, ["github.com/a/b"]);
        assert!(Identity::parse("---\ntype: project\n---\n").is_err());
    }

    #[test]
    fn resolves_by_remote_then_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("checkout-with-other-name");
        std::fs::create_dir(&repo).unwrap();
        git::init(&repo).unwrap();
        let project = |key: &str, remote: &str, folder: &str| Project {
            key: key.into(),
            identity: Ok(Identity {
                remotes: vec![remote.into()],
                folders: vec![folder.into()],
                ..Default::default()
            }),
        };
        let projects = vec![
            project(
                "by-folder",
                "github.com/x/other",
                "checkout-with-other-name",
            ),
            project("by-remote", "github.com/a/b", "b"),
        ];
        assert_eq!(resolve(&projects, &repo), Some("by-folder"));

        std::process::Command::new("git")
            .args([
                "-C",
                repo.to_str().unwrap(),
                "remote",
                "add",
                "origin",
                "git@github.com:a/b.git",
            ])
            .status()
            .unwrap();
        assert_eq!(resolve(&projects, &repo), Some("by-remote"));
        assert_eq!(resolve(&projects, tmp.path()), None);
    }
}
