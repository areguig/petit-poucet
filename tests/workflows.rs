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

fn step_using<'a>(job: &'a Value, action: &str) -> Option<(usize, &'a Value)> {
    let prefix = format!("{action}@");
    job["steps"]
        .as_array()?
        .iter()
        .enumerate()
        .find(|(_, step)| {
            step["uses"]
                .as_str()
                .is_some_and(|uses| uses.starts_with(&prefix))
        })
}

// Checks what the release flow does, not the action versions or step positions that carry it.
fn check_release_flow(release: &Value) -> Result<(), String> {
    let triggers = json!({
        "push": {"tags": ["v*"]},
        "pull_request": {"paths": [".github/workflows/release.yml", "install.sh", "install.ps1", "Cargo.toml", "Cargo.lock"]},
    });
    if release["on"] != triggers {
        return Err(format!("triggers: {}", release["on"]));
    }
    let build = &release["jobs"]["build"];
    if build.get("if").is_some() {
        return Err("pull requests must build".into());
    }
    let (_, upload) =
        step_using(build, "actions/upload-artifact").ok_or("build uploads no packages")?;
    if upload.get("if").is_some()
        || upload["with"] != json!({"name": "${{ matrix.target }}", "path": "petit-poucet-*"})
    {
        return Err(format!("upload: {upload}"));
    }

    let publish = &release["jobs"]["release"];
    if publish["needs"] != "build" || publish["if"] != "github.event_name == 'push'" {
        return Err(format!(
            "publish runs on: needs {}, if {}",
            publish["needs"], publish["if"]
        ));
    }
    if publish["permissions"] != json!({"contents": "write"}) {
        return Err(format!("publish permissions: {}", publish["permissions"]));
    }
    let (downloaded_at, download) =
        step_using(publish, "actions/download-artifact").ok_or("publish downloads no packages")?;
    if download["with"] != json!({"path": "dist", "merge-multiple": true}) {
        return Err(format!("download: {download}"));
    }
    let steps = publish["steps"].as_array().unwrap();
    let published_at = steps
        .iter()
        .position(|step| {
            step["run"].as_str().is_some_and(|run| {
                run.starts_with("gh release create ") && run.contains(" dist/* ")
            })
        })
        .ok_or("publish creates no release from dist")?;
    if published_at < downloaded_at {
        return Err("the release is created before the packages are downloaded".into());
    }
    Ok(())
}

// Pull requests build and upload the packages; only a tag push reaches the job that publishes them.
#[test]
fn pull_requests_package_and_only_tags_publish() {
    assert_eq!(check_release_flow(&workflow("release.yml")), Ok(()));
}

fn steps_of<'a>(release: &'a mut Value, job: &str) -> &'a mut Vec<Value> {
    release["jobs"][job]["steps"].as_array_mut().unwrap()
}

fn remove_step_using(release: &mut Value, job: &str, action: &str) {
    let at = step_using(&release["jobs"][job], action).unwrap().0;
    steps_of(release, job).remove(at);
}

#[test]
fn the_release_flow_check_ignores_versions_and_extra_steps_but_not_regressions() {
    let release = workflow("release.yml");
    let mut harmless = release.clone();
    for (job, action) in [
        ("build", "actions/upload-artifact"),
        ("release", "actions/download-artifact"),
    ] {
        let at = step_using(&harmless["jobs"][job], action).unwrap().0;
        let step = &mut steps_of(&mut harmless, job)[at];
        step["uses"] = json!(format!("{action}@v99"));
        steps_of(&mut harmless, job).insert(0, json!({"run": "echo extra"}));
        steps_of(&mut harmless, job).push(json!({"run": "echo extra"}));
    }
    assert_eq!(check_release_flow(&harmless), Ok(()));

    let mut no_upload = release.clone();
    remove_step_using(&mut no_upload, "build", "actions/upload-artifact");
    let mut no_download = release.clone();
    remove_step_using(&mut no_download, "release", "actions/download-artifact");
    let mut unguarded = release.clone();
    unguarded["jobs"]["release"]
        .as_object_mut()
        .unwrap()
        .remove("if");
    let mut published_first = release.clone();
    let at = step_using(
        &published_first["jobs"]["release"],
        "actions/download-artifact",
    )
    .unwrap()
    .0;
    let download = steps_of(&mut published_first, "release").remove(at);
    steps_of(&mut published_first, "release").push(download);
    let mut one_name_for_all_targets = release.clone();
    let at = step_using(
        &one_name_for_all_targets["jobs"]["build"],
        "actions/upload-artifact",
    )
    .unwrap()
    .0;
    steps_of(&mut one_name_for_all_targets, "build")[at]["with"]["name"] = json!("packages");
    for (name, regression) in [
        ("no upload", no_upload),
        ("no download", no_download),
        ("unguarded publish", unguarded),
        ("published before download", published_first),
        ("one upload name for every target", one_name_for_all_targets),
    ] {
        assert!(check_release_flow(&regression).is_err(), "{name}");
    }
}
