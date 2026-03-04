use std::path::PathBuf;

use prism_stats::{StatsConfig, collect_stats};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/simple_crate")
}

#[test]
fn collect_stats_on_fixture_crate() {
    let config = StatsConfig::new(fixture_path()).with_skip_deps(true);
    let stats = collect_stats(&config).unwrap();

    assert_eq!(stats.name(), "simple-crate");

    let output = stats.to_string();
    assert!(
        output.contains("prism stats — simple-crate"),
        "should show crate name"
    );
    assert!(output.contains("lines"), "should show line count");
    assert!(output.contains("1 files"), "should show file count");
    assert!(
        !output.contains("workspace"),
        "single crate should not show workspace"
    );
    assert!(output.contains("2 unit"), "should find 2 unit tests");
    assert!(output.contains("1 doctests"), "should find 1 doctest");
    assert!(output.contains("unsafe"), "should mention unsafe");
}

#[test]
fn collect_stats_json_output() {
    let config = StatsConfig::new(fixture_path())
        .with_skip_deps(true)
        .with_json(true);
    let stats = collect_stats(&config).unwrap();

    let json = serde_json::to_value(&stats).unwrap();
    assert_eq!(json["name"], "simple-crate");
    assert_eq!(json["is_workspace"], false);
    assert_eq!(json["code"]["files"], 1);
    assert!(json["tests"]["unit"].as_u64().unwrap() >= 2);
    assert!(json["tests"]["doctests"].as_u64().unwrap() >= 1);
    assert!(json["safety"]["unsafe_blocks"].as_u64().unwrap() >= 1);
    assert!(json["deps"].is_null(), "deps should be null when skipped");
}

#[test]
fn collect_stats_not_a_rust_project() {
    let config = StatsConfig::new(PathBuf::from("/tmp/nonexistent-dir-xyz"));
    let result = collect_stats(&config);
    assert!(result.is_err());
}
