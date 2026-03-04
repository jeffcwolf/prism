use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn prism_cmd() -> Command {
    cargo_bin_cmd!("prism")
}

fn fixture_path() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{manifest}/tests/fixtures/deps_project")
}

#[test]
fn deps_runs_successfully_on_fixture_project() {
    prism_cmd()
        .args(["deps", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prism Dependency Health Report"));
}

#[test]
fn deps_shows_dependency_counts() {
    prism_cmd()
        .args(["deps", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dependencies:"))
        .stdout(predicate::str::contains("direct"));
}

#[test]
fn deps_shows_max_tree_depth() {
    prism_cmd()
        .args(["deps", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Max tree depth:"));
}

#[test]
fn deps_shows_direct_dependencies_section() {
    prism_cmd()
        .args(["deps", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Direct Dependencies:"));
}

#[test]
fn deps_lists_log_dependency() {
    prism_cmd()
        .args(["deps", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("log"));
}

#[test]
fn deps_shows_summary_line() {
    let output = prism_cmd()
        .args(["deps", &fixture_path()])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("healthy") || stdout.contains("WARNING"),
        "expected summary line in output, got:\n{stdout}"
    );
}

#[test]
fn deps_fails_on_nonexistent_path() {
    prism_cmd()
        .args(["deps", "/nonexistent/path/12345"])
        .assert()
        .failure();
}

#[test]
fn deps_fails_on_non_cargo_project() {
    prism_cmd()
        .args(["deps", "/tmp"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a cargo project"));
}
