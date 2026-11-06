use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Value, json};

fn workflow(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".github/workflows")
        .join(name);
    let text = fs::read_to_string(&path).unwrap();
    serde_saphyr::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

// A job without its own permissions gets the workflow's, or the repository default if neither is set.
fn permissions_by_job(workflow: &Value) -> BTreeMap<String, Value> {
    workflow["jobs"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, job)| {
            let permissions = job.get("permissions").unwrap_or(&workflow["permissions"]);
            (name.clone(), permissions.clone())
        })
        .collect()
}

// Only the job that publishes a release may write to the repository (#173).
#[test]
fn only_the_release_job_can_write_to_the_repository() {
    let read = json!({"contents": "read"});
    assert_eq!(
        permissions_by_job(&workflow("release.yml")),
        BTreeMap::from([
            ("build".to_string(), read.clone()),
            ("release".to_string(), json!({"contents": "write"})),
        ])
    );
    for (name, job) in [
        ("agents.yml", "install"),
        ("ci.yml", "check"),
        ("syncthing.yml", "sync"),
    ] {
        assert_eq!(
            permissions_by_job(&workflow(name)),
            BTreeMap::from([(job.to_string(), read.clone())]),
            "{name}"
        );
    }
}

// Finds a step by what it does, so action versions and harmless extra steps don't matter.
fn step(job: &Value, does: impl Fn(&Value) -> bool) -> (usize, &Value) {
    job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .find(|(_, step)| does(step))
        .unwrap_or_else(|| panic!("no such step in {job}"))
}

fn uses(action: &str) -> impl Fn(&Value) -> bool {
    move |step| {
        step["uses"]
            .as_str()
            .is_some_and(|uses| uses.starts_with(&format!("{action}@")))
    }
}

// Pull requests build and upload the packages; only a tag push reaches the job that publishes them.
#[test]
fn pull_requests_package_and_only_tags_publish() {
    let release = workflow("release.yml");
    assert_eq!(
        release["on"],
        json!({
            "push": {"tags": ["v*"]},
            "pull_request": {"paths": [".github/workflows/release.yml", "install.sh", "install.ps1", "Cargo.toml", "Cargo.lock"]},
        })
    );
    let build = &release["jobs"]["build"];
    assert_eq!(build.get("if"), None);
    let (_, upload) = step(build, uses("actions/upload-artifact"));
    assert_eq!(upload.get("if"), None);
    assert_eq!(
        upload["with"],
        json!({"name": "${{ matrix.target }}", "path": "petit-poucet-*"})
    );

    let publish = &release["jobs"]["release"];
    assert_eq!(publish["needs"], "build");
    assert_eq!(publish["if"], "github.event_name == 'push'");
    assert_eq!(publish["permissions"], json!({"contents": "write"}));
    let (downloaded_at, download) = step(publish, uses("actions/download-artifact"));
    assert_eq!(
        download["with"],
        json!({"path": "dist", "merge-multiple": true})
    );
    let (published_at, _) = step(publish, |step| {
        step["run"]
            .as_str()
            .is_some_and(|run| run.starts_with("gh release create ") && run.contains(" dist/* "))
    });
    assert!(downloaded_at < published_at);
}

fn runs(script: &'static str) -> impl Fn(&Value) -> bool {
    move |step| {
        step["run"]
            .as_str()
            .is_some_and(|run| run.starts_with(script))
    }
}

// The #165 diagnostic's instrumented self-replace is built by Windows CI after the ordinary tests, never for a release.
#[test]
fn only_windows_ci_builds_the_instrumented_updater() {
    let ci = workflow("ci.yml");
    let check = &ci["jobs"]["check"];
    let (tested_at, _) = step(check, runs("cargo test "));
    let (diagnosed_at, diagnostic) = step(check, runs("bash tests/windows-update-phases/run.sh "));
    assert!(tested_at < diagnosed_at);
    assert_eq!(
        diagnostic["if"],
        "${{ !cancelled() && runner.os == 'Windows' }}"
    );
    let release = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml"),
    )
    .unwrap();
    assert!(!release.contains("windows-update-phases") && !release.contains("patch."));
}
