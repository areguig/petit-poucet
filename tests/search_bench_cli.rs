use std::fs;
use std::process::Command;

#[test]
fn search_benchmark_reads_only_the_explicit_vault_and_query_file() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault copy");
    fs::create_dir_all(vault.join("Preferences")).unwrap();
    fs::create_dir_all(vault.join("Projects/demo")).unwrap();
    fs::create_dir_all(vault.join(".hidden")).unwrap();
    fs::write(
        vault.join("Preferences/needle.md"),
        "---\ntype: reference\nscope: all repos\nsummary: quasarneedle\ncreated: 2026-09-01\n---\n# Quasarneedle\nOnly this note answers the query.\n",
    ).unwrap();
    for path in [
        "Index.md",
        "Projects/demo/_project.md",
        ".hidden/ignored.md",
        "Preferences/needle.sync-conflict-20260101-000000-test.md",
    ] {
        fs::write(vault.join(path), "This is not a searchable note.").unwrap();
    }
    let queries = temp.path().join("private queries.toml");
    fs::write(&queries, "[[query]]\ncategory = 'rare'\ntext = 'quasarneedle'\nexpect = ['Preferences/needle']\n\n[[query]]\ncategory = 'negative'\ntext = 'absentword'\nexpect = []\n").unwrap();
    let output = Command::new(env!("CARGO"))
        .current_dir(temp.path())
        .args([
            "run",
            "--offline",
            "--quiet",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
            "--example",
            "search_bench",
            "--",
            "--vault",
        ])
        .arg("vault copy")
        .arg("--queries")
        .arg("private queries.toml")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("1 notes, 2 queries"), "{stdout}");
    assert!(!stdout.contains("# Query set: queries.toml"));
    for method in ["today", "bm25", "combined"] {
        assert!(
            stdout.contains(&format!(
                "| {method} | rare | 1 | 100% | 100% | 1.000 | 1.0 | 1 |"
            )),
            "{stdout}"
        );
        assert!(
            stdout.contains(&format!(
                "| {method} | negative | 1 | - | - | - | 0.0 | 0 |"
            )),
            "{stdout}"
        );
    }
    assert!(vault.join("Index.md").is_file());
}
