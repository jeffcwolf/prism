//! Release-readiness assessment for Rust codebases.
//!
//! This crate orchestrates the other Prism analysis crates and evaluates their
//! results against configurable thresholds to produce a pass/fail/warn report.

mod checks;
mod config;
mod types;

pub use config::{CheckConfig, ConfigError};
pub use types::{CheckCategory, CheckReport, CheckResult, CheckStatus};

use std::collections::HashSet;
use std::path::Path;

/// Errors that can occur during release-readiness checking.
#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),
}

/// Run all release-readiness checks against the codebase at the configured path.
///
/// Each data source is tried independently. If a source fails, the failure is
/// captured as a failed check with an explanatory message rather than aborting.
pub fn run_checks(config: &CheckConfig) -> CheckReport {
    let thresholds = config.thresholds();
    let mut results = Vec::new();

    // --- Collect stats ---
    let stats_config =
        prism_stats::StatsConfig::new(config.path().to_path_buf()).with_skip_deps(config.no_deps());
    let stats = prism_stats::collect_stats(&stats_config);

    // --- Collect audit ---
    let audit = prism_audit::audit_codebase(config.path());

    // --- Collect deps (optional) ---
    let deps = if config.no_deps() {
        None
    } else {
        Some(prism_deps::analyze_dependencies(config.path()))
    };

    // --- Collect map ---
    let map_config = prism_map::MapConfig::new(config.path());
    let map = prism_map::map_codebase(&map_config);

    // === Quality checks ===
    match &stats {
        Ok(s) => {
            let json = serde_json::to_value(s).unwrap_or_default();
            let doc_coverage = json["docs"]["coverage_pct"].as_f64().unwrap_or(0.0);
            results.push(checks::check_doc_coverage(
                doc_coverage,
                thresholds.min_doc_coverage,
            ));
        }
        Err(e) => {
            results.push(types::CheckResult::new(
                types::CheckCategory::Quality,
                "doc_coverage".to_string(),
                types::CheckStatus::Fail,
                format!("Stats collection failed: {e}"),
            ));
        }
    }

    match &audit {
        Ok(report) => {
            let (max_cyclomatic, worst_cyclomatic_fn) =
                find_worst_complexity(report, |fc| fc.cyclomatic());
            let (max_cognitive, worst_cognitive_fn) =
                find_worst_complexity(report, |fc| fc.cognitive());
            let shallow_count = report
                .modules()
                .iter()
                .filter(|m| m.depth_ratio() > 0.5)
                .count();

            results.push(checks::check_cyclomatic_complexity(
                max_cyclomatic,
                worst_cyclomatic_fn.as_deref(),
                thresholds.max_cyclomatic,
            ));
            results.push(checks::check_cognitive_complexity(
                max_cognitive,
                worst_cognitive_fn.as_deref(),
                thresholds.max_cognitive,
            ));
            results.push(checks::check_shallow_modules(shallow_count));
        }
        Err(e) => {
            results.push(types::CheckResult::new(
                types::CheckCategory::Quality,
                "audit".to_string(),
                types::CheckStatus::Fail,
                format!("Audit failed: {e}"),
            ));
        }
    }

    // === Dependency checks ===
    match &deps {
        Some(Ok(report)) => {
            let vuln_count: usize = report
                .health()
                .iter()
                .flat_map(|h| h.vulnerabilities())
                .count();
            results.push(checks::check_vulnerabilities(vuln_count));

            let stale_deps: Vec<(String, String, String)> = report
                .health()
                .iter()
                .filter_map(|h| {
                    h.staleness().and_then(|s| {
                        if s.is_major_behind() {
                            Some((
                                h.name().to_string(),
                                s.current_version().to_string(),
                                s.latest_version().to_string(),
                            ))
                        } else {
                            None
                        }
                    })
                })
                .collect();
            results.push(checks::check_staleness(
                &stale_deps,
                thresholds.check_staleness,
            ));

            results.push(checks::check_duplicate_versions(
                report.duplicates().len(),
                thresholds.max_duplicate_versions,
            ));
        }
        Some(Err(e)) => {
            results.push(types::CheckResult::new(
                types::CheckCategory::Dependencies,
                "dependencies".to_string(),
                types::CheckStatus::Fail,
                format!("Dependency analysis failed: {e}"),
            ));
        }
        None => {
            results.push(types::CheckResult::new(
                types::CheckCategory::Dependencies,
                "dependencies".to_string(),
                types::CheckStatus::Skip,
                "Skipped (--no-deps)".to_string(),
            ));
        }
    }

    // === Testing checks ===
    match &stats {
        Ok(s) => {
            let json = serde_json::to_value(s).unwrap_or_default();
            let ratio = json["tests"]["ratio_per_100_loc"].as_f64().unwrap_or(0.0);
            let integration = json["tests"]["integration"].as_u64().unwrap_or(0);
            results.push(checks::check_test_ratio(ratio, thresholds.min_test_ratio));
            results.push(checks::check_integration_tests(
                integration,
                thresholds.require_integration_tests,
            ));
        }
        Err(_) => {
            // Already reported under quality
        }
    }

    // === Safety checks ===
    match &stats {
        Ok(s) => {
            let json = serde_json::to_value(s).unwrap_or_default();
            let unsafe_blocks = json["safety"]["unsafe_blocks"].as_u64().unwrap_or(0);
            results.push(checks::check_unsafe_blocks(
                unsafe_blocks,
                thresholds.max_unsafe_blocks,
            ));
        }
        Err(_) => {
            // Already reported under quality
        }
    }

    // === Structure checks ===
    match &map {
        Ok(codemap) => {
            results.push(checks::check_module_structure(true, None));

            let orphan_count = count_orphan_files(config.path(), codemap);
            results.push(checks::check_orphan_files(orphan_count));
        }
        Err(e) => {
            results.push(checks::check_module_structure(false, Some(&e.to_string())));
            results.push(checks::check_orphan_files(0));
        }
    }

    // === Coverage checks ===
    let coverage = if config.no_coverage() {
        None
    } else if thresholds.coverage_enabled {
        try_get_coverage(config.path())
    } else {
        None
    };
    results.push(checks::check_line_coverage(
        coverage,
        thresholds.min_line_coverage,
        config.no_coverage(),
    ));

    // Build project info
    let project_name = match &stats {
        Ok(s) => s.name().to_string(),
        Err(_) => config
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_string()),
    };
    let project_info = match &stats {
        Ok(s) => {
            let json = serde_json::to_value(s).unwrap_or_default();
            let lines = json["code"]["rust_lines"].as_u64().unwrap_or(0);
            let is_ws = json["is_workspace"].as_bool().unwrap_or(false);
            let crates = json["code"]["crates"].as_u64().unwrap_or(1);
            if is_ws {
                format!("workspace, {crates} crates, {lines} lines")
            } else {
                format!("{lines} lines")
            }
        }
        Err(_) => String::new(),
    };

    CheckReport::new(project_name, project_info, results)
}

/// Find the worst complexity across all functions in the audit report.
fn find_worst_complexity(
    report: &prism_audit::AuditReport,
    metric: fn(&prism_audit::FunctionComplexity) -> u32,
) -> (u32, Option<String>) {
    let mut max_val = 0u32;
    let mut worst_name: Option<String> = None;

    for module in report.modules() {
        for fc in module.function_complexities() {
            let val = metric(fc);
            if val > max_val {
                max_val = val;
                worst_name = Some(fc.name().to_string());
            }
        }
    }

    (max_val, worst_name)
}

/// Count .rs files under the path that are not reachable from any crate root.
fn count_orphan_files(base_path: &Path, map: &prism_map::CodebaseMap) -> usize {
    let mut mapped_files: HashSet<std::path::PathBuf> = HashSet::new();
    for tree in map.module_trees() {
        collect_file_paths(tree.root(), &mut mapped_files);
    }

    // Walk the filesystem for .rs files
    let mut rs_files = Vec::new();
    if let Ok(entries) = walkdir(base_path) {
        for entry in entries {
            if entry.extension().is_some_and(|e| e == "rs") {
                rs_files.push(entry);
            }
        }
    }

    rs_files
        .iter()
        .filter(|f| !mapped_files.contains(f.as_path()))
        .count()
}

fn collect_file_paths(node: &prism_map::ModuleNode, set: &mut HashSet<std::path::PathBuf>) {
    if let Some(path) = node.file_path() {
        set.insert(path.to_path_buf());
    }
    for child in node.children() {
        collect_file_paths(child, set);
    }
}

/// Simple recursive directory walk for .rs files, skipping target/ and hidden dirs.
fn walkdir(path: &Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    if !path.is_dir() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip target, hidden dirs, and test fixtures
        if name_str.starts_with('.') || name_str == "target" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            files.extend(walkdir(&path)?);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    Ok(files)
}

/// Try to run cargo-tarpaulin or cargo-llvm-cov and extract coverage percentage.
fn try_get_coverage(path: &Path) -> Option<f64> {
    if let Some(pct) = try_tarpaulin(path) {
        return Some(pct);
    }
    try_llvm_cov(path)
}

fn try_tarpaulin(path: &Path) -> Option<f64> {
    let output = std::process::Command::new("cargo")
        .args(["tarpaulin", "--out", "json", "--output-dir", "/dev/null"])
        .current_dir(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("coverage").and_then(|v| v.as_f64())
}

fn try_llvm_cov(path: &Path) -> Option<f64> {
    let output = std::process::Command::new("cargo")
        .args(["llvm-cov", "--json"])
        .current_dir(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.pointer("/data/0/totals/lines/percent")
        .and_then(|v| v.as_f64())
}
