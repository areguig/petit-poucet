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
    let release = workflow("release.yml");
    assert_eq!(
        permissions_by_job(&release),
        BTreeMap::from([
            ("build".to_string(), json!({"contents": "read"})),
            ("release".to_string(), json!({"contents": "write"})),
        ])
    );
    assert_eq!(
        permissions_by_job(&workflow("agents.yml")),
        BTreeMap::from([("install".to_string(), json!({"contents": "read"}))])
    );
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
    let upload = build["steps"].as_array().unwrap().last().unwrap();
    assert_eq!(
        upload,
        &json!({"uses": "actions/upload-artifact@v7", "with": {"name": "${{ matrix.target }}", "path": "petit-poucet-*"}})
    );

    let publish = &release["jobs"]["release"];
    assert_eq!(publish["needs"], "build");
    assert_eq!(publish["if"], "github.event_name == 'push'");
    let steps = publish["steps"].as_array().unwrap();
    assert_eq!(
        steps[1],
        json!({"uses": "actions/download-artifact@v8", "with": {"path": "dist", "merge-multiple": true}})
    );
    assert!(
        steps[2]["run"]
            .as_str()
            .unwrap()
            .starts_with("gh release create "),
        "{}",
        steps[2]
    );
}
