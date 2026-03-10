use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn check_runs_on_sample_project_with_no_deps_no_coverage() {
    Command::cargo_bin("prism")
        .unwrap()
        .args(["check", "--no-deps", "--no-coverage"])
        .arg(fixture_path("sample_project"))
        .assert()
        .stdout(predicate::str::contains("Prism Release Readiness Check"))
        .stdout(predicate::str::contains("Quality"))
        .stdout(predicate::str::contains("Structure"));
}

#[test]
fn check_json_output_is_valid() {
    let output = Command::cargo_bin("prism")
        .unwrap()
        .args(["check", "--json", "--no-deps", "--no-coverage"])
        .arg(fixture_path("sample_project"))
        .output()
        .unwrap();

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert!(json["project_name"].is_string());
    assert!(json["checks"].is_array());
    assert!(json["overall_status"].is_string());
    assert!(json["total_pass"].is_number());
    assert!(json["total_fail"].is_number());
    assert!(json["total_warn"].is_number());
    assert!(json["total_skip"].is_number());
}

#[test]
fn check_nonexistent_path_fails_gracefully() {
    Command::cargo_bin("prism")
        .unwrap()
        .args(["check", "--no-deps", "--no-coverage", "/nonexistent/path"])
        .assert()
        .stdout(predicate::str::contains("Prism Release Readiness Check"));
    // Should not panic — failures are captured in the report
}

#[test]
fn check_strict_mode_changes_thresholds() {
    let normal = Command::cargo_bin("prism")
        .unwrap()
        .args(["check", "--json", "--no-deps", "--no-coverage"])
        .arg(fixture_path("sample_project"))
        .output()
        .unwrap();

    let strict = Command::cargo_bin("prism")
        .unwrap()
        .args(["check", "--json", "--no-deps", "--no-coverage", "--strict"])
        .arg(fixture_path("sample_project"))
        .output()
        .unwrap();

    let normal_json: serde_json::Value = serde_json::from_slice(&normal.stdout).unwrap();
    let strict_json: serde_json::Value = serde_json::from_slice(&strict.stdout).unwrap();

    // Strict should have same or more failures
    let normal_fail = normal_json["total_fail"].as_u64().unwrap_or(0);
    let strict_fail = strict_json["total_fail"].as_u64().unwrap_or(0);
    assert!(
        strict_fail >= normal_fail,
        "strict mode should not have fewer failures than normal mode"
    );
}

#[test]
fn check_exit_code_zero_when_all_pass_or_only_warns() {
    let output = Command::cargo_bin("prism")
        .unwrap()
        .args(["check", "--json", "--no-deps", "--no-coverage"])
        .arg(fixture_path("sample_project"))
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let fail_count = json["total_fail"].as_u64().unwrap_or(0);

    if fail_count == 0 {
        assert!(output.status.success(), "exit 0 when no failures");
    } else {
        assert!(!output.status.success(), "exit 1 when failures exist");
    }
}
