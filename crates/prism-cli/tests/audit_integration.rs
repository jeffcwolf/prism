use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn prism_cmd() -> Command {
    cargo_bin_cmd!("prism")
}

fn fixture_path() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{manifest}/tests/fixtures/sample_project")
}

#[test]
fn audit_runs_successfully_on_fixture_project() {
    prism_cmd()
        .args(["audit", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prism Audit Report"));
}

#[test]
fn audit_shows_files_scanned() {
    prism_cmd()
        .args(["audit", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Files scanned:"));
}

#[test]
fn audit_shows_depth_ratios() {
    prism_cmd()
        .args(["audit", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("depth="));
}

#[test]
fn audit_flags_shallow_modules() {
    prism_cmd()
        .args(["audit", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("[SHALLOW]"));
}

#[test]
fn audit_shows_summary_line() {
    let output = prism_cmd()
        .args(["audit", &fixture_path()])
        .assert()
        .success();

    // Should have either a warning about shallow modules or an all-clear message
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("shallow") || stdout.contains("acceptable"),
        "expected summary line in output, got:\n{stdout}"
    );
}

#[test]
fn audit_fails_on_nonexistent_path() {
    prism_cmd()
        .args(["audit", "/nonexistent/path/12345"])
        .assert()
        .failure();
}

#[test]
fn no_subcommand_shows_help() {
    prism_cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn deps_subcommand_not_yet_implemented() {
    prism_cmd()
        .args(["deps", "."])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not yet implemented"));
}
