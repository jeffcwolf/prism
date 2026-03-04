use assert_cmd::Command;
use predicates::prelude::*;

fn fixture_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!(
        "{}/../../crates/prism-stats/tests/fixtures/simple_crate",
        manifest_dir
    )
}

#[test]
fn stats_human_output() {
    Command::cargo_bin("prism")
        .unwrap()
        .args(["stats", "--no-deps", "--path", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("prism stats"))
        .stdout(predicate::str::contains("Code"))
        .stdout(predicate::str::contains("Tests"))
        .stdout(predicate::str::contains("Docs"))
        .stdout(predicate::str::contains("Safety"))
        .stdout(predicate::str::contains("Surface"))
        .stdout(predicate::str::contains("Complexity"));
}

#[test]
fn stats_json_output() {
    let output = Command::cargo_bin("prism")
        .unwrap()
        .args(["stats", "--no-deps", "--json", "--path", &fixture_path()])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert_eq!(json["name"], "simple-crate");
    assert!(json["code"]["files"].as_u64().unwrap() > 0);
}

#[test]
fn stats_nonexistent_path() {
    Command::cargo_bin("prism")
        .unwrap()
        .args(["stats", "--path", "/tmp/nonexistent-path-xyz"])
        .assert()
        .failure();
}
