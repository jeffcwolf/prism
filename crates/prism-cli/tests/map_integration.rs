use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn prism_cmd() -> Command {
    cargo_bin_cmd!("prism")
}

fn fixture_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{manifest_dir}/tests/fixtures/map_project")
}

#[test]
fn map_human_output_contains_module_names() {
    prism_cmd()
        .args(["map", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("alpha"))
        .stdout(predicate::str::contains("beta"))
        .stdout(predicate::str::contains("Module Tree"));
}

#[test]
fn map_json_output_is_valid() {
    let output = prism_cmd()
        .args(["map", "--json", &fixture_path()])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert!(json["module_trees"].is_array());
    assert!(json["imports"].is_array());
    assert!(json["entry_points"].is_array());
    assert!(
        json["crate_graph"].is_object(),
        "workspace should have crate graph"
    );
}

#[test]
fn map_mermaid_output_contains_graph_keyword() {
    prism_cmd()
        .args(["map", "--mermaid", &fixture_path()])
        .assert()
        .success()
        .stdout(predicate::str::contains("graph"));
}

#[test]
fn map_nonexistent_path_fails() {
    prism_cmd()
        .args(["map", "/tmp/nonexistent-path-xyz-prism"])
        .assert()
        .failure();
}

#[test]
fn map_json_and_mermaid_are_mutually_exclusive() {
    prism_cmd()
        .args(["map", "--json", "--mermaid", &fixture_path()])
        .assert()
        .failure();
}

#[test]
fn map_with_depth_limit() {
    prism_cmd()
        .args(["map", "--depth", "1", &fixture_path()])
        .assert()
        .success();
}
