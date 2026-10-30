mod common;

use common::{petit_poucet, start};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn first_save_from_dot_prefixed_checkout_is_rejected_without_writing_project_or_note() {
    let home = TempDir::new().unwrap();
    let vault = home.path().join("vault");
    petit_poucet(home.path())
        .args(["init", vault.to_str().unwrap()])
        .assert()
        .success();
    let repo = home.path().join(".dotfiles");
    std::fs::create_dir(&repo).unwrap();

    let (mut child, mut client, _) = start(home.path());
    let (is_error, reply) = client.call(
        "memory_save",
        json!({
            "type": "project", "project_dir": repo, "title": "Stow layout",
            "summary": "dotfiles are stowed", "fact": "Run stow.",
            "source": "the user said so on 2026-09-24", "how_to_apply": "When linking.",
        }),
    );
    assert!(is_error && reply.contains(".dotfiles"), "{reply}");
    assert!(
        !vault.join("Projects/.dotfiles/_project.md").exists()
            && !vault.join("Projects/.dotfiles/stow-layout.md").exists(),
        "neither project identity nor note is written"
    );
    assert!(
        vault.join("Projects").read_dir().unwrap().next().is_none(),
        "no project folder is written"
    );
    drop(client);
    assert!(child.wait().unwrap().success());
}
