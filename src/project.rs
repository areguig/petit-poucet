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

// Two machines that each learned a remote or a folder for the project keep both.
pub fn merge_identities(kept: &str, copy: &str) -> Result<String, String> {
    let (kept, copy) = (Identity::parse(kept)?, Identity::parse(copy)?);
    let mut remotes = kept.remotes;
    for remote in copy.remotes {
        let normal = normalise_remote(&remote);
        if !remotes.iter().any(|r| normalise_remote(r) == normal) {
            remotes.push(remote);
        }
    }
    let mut folders = kept.folders;
    for folder in copy.folders {
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    render_identity(&remotes, &folders)
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
// A non-default port names another server, so it keeps its scheme and port: `ssh://host:2222/Owner/repo`.
pub fn normalise_remote(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let (scheme, host, path) = match url.split_once("://") {
        Some((scheme, rest)) => {
            let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
            (Some(scheme), host, path)
        }
        None => match url.split_once(':') {
            Some((host, path)) if !host.contains('/') => (None, host, path),
            _ => return url.to_string(),
        },
    };
    let host = host.rsplit('@').next().unwrap_or(host).to_lowercase();
    let path = path.trim_start_matches('/');
    match (scheme, host.split_once(':')) {
        (Some(scheme), Some((name, port))) if Some(port) != default_port(scheme) => {
            format!("{scheme}://{name}:{port}/{path}")
        }
        (_, Some((name, _))) => format!("{name}/{path}"),
        (_, None) => format!("{host}/{path}"),
    }
}

fn default_port(scheme: &str) -> Option<&'static str> {
    match scheme {
        "ssh" => Some("22"),
        "https" => Some("443"),
        "http" => Some("80"),
        "git" => Some("9418"),
        _ => None,
    }
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
    // Two repos with remotes that differ are different projects, whatever their folders are called.
    let by_folder = || {
        known.iter().find(|(_, id)| {
            id.folders.contains(&folder) && (remotes.is_empty() || id.remotes.is_empty())
        })
    };
    by_remote.or_else(by_folder).map(|(key, _)| *key)
}

// `github.com/someone/api` gives `someone-api`: the key for a repo whose folder name another project already uses.
pub fn key_from_remote(remote: &str) -> Option<String> {
    let remote = normalise_remote(remote);
    let remote = remote.split_once("://").map_or(remote.as_str(), |(_, r)| r);
    let (_, path) = remote.split_once('/')?;
    let key = slug::slugify(path);
    (!key.is_empty()).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merging_identities_keeps_each_remote_and_folder_once() {
        let kept = render_identity(&["git@github.com:me/api.git".into()], &["api".into()]).unwrap();
        let copy = render_identity(
            &[
                "https://github.com/me/api".into(),
                "gitlab.com/me/api".into(),
            ],
            &["api".into(), "api-old".into()],
        )
        .unwrap();
        let merged = Identity::parse(&merge_identities(&kept, &copy).unwrap()).unwrap();
        assert_eq!(merged.remotes, ["github.com/me/api", "gitlab.com/me/api"]);
        assert_eq!(merged.folders, ["api", "api-old"]);
        assert!(merge_identities(&kept, "not an identity").is_err());
    }

    #[test]
    fn normalises_remotes() {
        for url in [
            "git@github.com:areguig/petit-poucet.git",
            "https://github.com/areguig/petit-poucet",
            "https://user@GitHub.com/areguig/petit-poucet.git/",
            "ssh://git@github.com:22/areguig/petit-poucet.git",
            "https://github.com:443/areguig/petit-poucet",
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
    fn keeps_a_non_default_port() {
        let on_2222 = normalise_remote("ssh://git@git.example.test:2222/team/app.git");
        let on_2223 = normalise_remote("ssh://git@git.example.test:2223/team/app.git");
        assert_eq!(on_2222, "ssh://git.example.test:2222/team/app");
        assert_ne!(on_2222, on_2223);
        assert_ne!(
            on_2222,
            normalise_remote("git@git.example.test:team/app.git")
        );
        assert_eq!(
            normalise_remote(&on_2222),
            on_2222,
            "stored remotes stay as they are"
        );
        assert_eq!(
            normalise_remote("https://git.example.test:8443/team/app"),
            "https://git.example.test:8443/team/app"
        );
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

    #[test]
    fn a_folder_name_never_matches_a_repo_with_another_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("api");
        std::fs::create_dir(&repo).unwrap();
        git::init(&repo).unwrap();
        std::process::Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "remote", "add", "origin"])
            .arg("git@gitlab.com:someone-else/api.git")
            .status()
            .unwrap();
        let project = |remotes: &[&str]| Project {
            key: "api".into(),
            identity: Ok(Identity {
                remotes: remotes.iter().map(|r| r.to_string()).collect(),
                folders: vec!["api".into()],
                ..Default::default()
            }),
        };
        assert_eq!(resolve(&[project(&["github.com/me/api"])], &repo), None);
        assert_eq!(
            resolve(&[project(&[])], &repo),
            Some("api"),
            "a project without a remote yet (from migrate) still matches by folder"
        );
    }

    #[test]
    fn a_key_from_a_remote_keeps_owner_and_repo() {
        assert_eq!(
            key_from_remote("git@gitlab.com:someone-else/api.git").as_deref(),
            Some("someone-else-api")
        );
        assert_eq!(
            key_from_remote("ssh://git@git.example.test:2222/team/app.git").as_deref(),
            Some("team-app")
        );
    }

    #[test]
    fn a_remote_never_matches_the_same_repo_on_another_ssh_port() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("app");
        std::fs::create_dir(&repo).unwrap();
        git::init(&repo).unwrap();
        std::process::Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "remote", "add", "origin"])
            .arg("ssh://git@git.example.test:2223/team/app.git")
            .status()
            .unwrap();
        let project = |key: &str, port: &str| Project {
            key: key.into(),
            identity: Ok(Identity {
                remotes: vec![format!("ssh://git@git.example.test:{port}/team/app.git")],
                folders: vec![format!("app-{port}")],
                ..Default::default()
            }),
        };
        assert_eq!(resolve(&[project("on-2222", "2222")], &repo), None);
        assert_eq!(
            resolve(
                &[project("on-2222", "2222"), project("on-2223", "2223")],
                &repo
            ),
            Some("on-2223")
        );
    }
}
