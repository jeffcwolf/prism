//! Individual check functions that evaluate data against thresholds.
//!
//! Each function takes extracted values and a threshold, returning a CheckResult.
//! These are plain functions, not a trait hierarchy — the checks are a flat list
//! of evaluations that an aggregator calls sequentially.

use crate::types::{CheckCategory, CheckResult, CheckStatus};

/// Check that doc coverage meets the minimum threshold.
pub(crate) fn check_doc_coverage(coverage_pct: f64, threshold: f64) -> CheckResult {
    let status = if coverage_pct >= threshold {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    CheckResult::new(
        CheckCategory::Quality,
        "doc_coverage".to_string(),
        status,
        format!("Doc coverage: {coverage_pct:.0}% (threshold: {threshold:.0}%)"),
    )
    .with_value_and_threshold(coverage_pct, threshold)
}

/// Check that no function exceeds the cyclomatic complexity threshold.
pub(crate) fn check_cyclomatic_complexity(
    max_cyclomatic: u32,
    worst_fn_name: Option<&str>,
    threshold: u32,
) -> CheckResult {
    let status = if max_cyclomatic <= threshold {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    let message = match (status, worst_fn_name) {
        (CheckStatus::Pass, _) => {
            format!("Max cyclomatic complexity: {max_cyclomatic} (threshold: {threshold})")
        }
        (_, Some(name)) => {
            format!("{name} has cyclomatic {max_cyclomatic} (threshold: {threshold})")
        }
        (_, None) => {
            format!("Max cyclomatic complexity: {max_cyclomatic} (threshold: {threshold})")
        }
    };
    CheckResult::new(
        CheckCategory::Quality,
        "cyclomatic_complexity".to_string(),
        status,
        message,
    )
    .with_value_and_threshold(f64::from(max_cyclomatic), f64::from(threshold))
}

/// Check that no function exceeds the cognitive complexity threshold.
pub(crate) fn check_cognitive_complexity(
    max_cognitive: u32,
    worst_fn_name: Option<&str>,
    threshold: u32,
) -> CheckResult {
    let status = if max_cognitive <= threshold {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    let message = match (status, worst_fn_name) {
        (CheckStatus::Pass, _) => {
            format!("Max cognitive complexity: {max_cognitive} (threshold: {threshold})")
        }
        (_, Some(name)) => {
            format!("{name} has cognitive {max_cognitive} (threshold: {threshold})")
        }
        (_, None) => {
            format!("Max cognitive complexity: {max_cognitive} (threshold: {threshold})")
        }
    };
    CheckResult::new(
        CheckCategory::Quality,
        "cognitive_complexity".to_string(),
        status,
        message,
    )
    .with_value_and_threshold(f64::from(max_cognitive), f64::from(threshold))
}

/// Check that no modules are flagged as shallow (depth ratio > 0.5).
pub(crate) fn check_shallow_modules(shallow_count: usize) -> CheckResult {
    let status = if shallow_count == 0 {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    let message = if shallow_count == 0 {
        "No shallow modules".to_string()
    } else {
        format!("{shallow_count} module(s) flagged as shallow (depth ratio > 0.5)")
    };
    CheckResult::new(
        CheckCategory::Quality,
        "shallow_modules".to_string(),
        status,
        message,
    )
}

/// Check for zero known vulnerabilities. Always fails on count > 0.
pub(crate) fn check_vulnerabilities(vuln_count: usize) -> CheckResult {
    let status = if vuln_count == 0 {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    let message = if vuln_count == 0 {
        "No vulnerabilities".to_string()
    } else {
        format!("{vuln_count} known vulnerability(ies)")
    };
    CheckResult::new(
        CheckCategory::Dependencies,
        "vulnerabilities".to_string(),
        status,
        message,
    )
}

/// Check for dependencies more than one major version behind.
pub(crate) fn check_staleness(
    stale_deps: &[(String, String, String)],
    enabled: bool,
) -> CheckResult {
    if !enabled {
        return CheckResult::new(
            CheckCategory::Dependencies,
            "staleness".to_string(),
            CheckStatus::Skip,
            "Staleness check disabled".to_string(),
        );
    }
    let status = if stale_deps.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Warn
    };
    let message = if stale_deps.is_empty() {
        "No dependencies are major-version behind".to_string()
    } else {
        let names: Vec<&str> = stale_deps
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect();
        format!(
            "{} dependency(ies) 1+ major version behind: {}",
            stale_deps.len(),
            names.join(", ")
        )
    };
    CheckResult::new(
        CheckCategory::Dependencies,
        "staleness".to_string(),
        status,
        message,
    )
}

/// Check for excessive duplicate dependency versions.
pub(crate) fn check_duplicate_versions(duplicate_count: usize, threshold: usize) -> CheckResult {
    let status = if duplicate_count <= threshold {
        CheckStatus::Pass
    } else {
        CheckStatus::Warn
    };
    let message = if duplicate_count == 0 {
        "No excessive duplicates".to_string()
    } else {
        format!("{duplicate_count} duplicate dependency version(s) (threshold: {threshold})")
    };
    CheckResult::new(
        CheckCategory::Dependencies,
        "duplicate_versions".to_string(),
        status,
        message,
    )
}

/// Check test ratio against threshold.
pub(crate) fn check_test_ratio(ratio_per_100_loc: f64, threshold: f64) -> CheckResult {
    let status = if ratio_per_100_loc >= threshold {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    CheckResult::new(
        CheckCategory::Testing,
        "test_ratio".to_string(),
        status,
        format!("Test ratio: {ratio_per_100_loc:.1} per 100 LOC (threshold: {threshold:.1})"),
    )
    .with_value_and_threshold(ratio_per_100_loc, threshold)
}

/// Check that at least 1 integration test exists.
pub(crate) fn check_integration_tests(integration_count: u64, required: bool) -> CheckResult {
    if !required {
        return CheckResult::new(
            CheckCategory::Testing,
            "integration_tests".to_string(),
            CheckStatus::Skip,
            "Integration test check disabled".to_string(),
        );
    }
    let status = if integration_count > 0 {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    let message = if integration_count > 0 {
        format!("Integration tests present ({integration_count} found)")
    } else {
        "No integration tests found".to_string()
    };
    CheckResult::new(
        CheckCategory::Testing,
        "integration_tests".to_string(),
        status,
        message,
    )
}

/// Check unsafe block count. When threshold is 0, any unsafe triggers a warning (not fail).
pub(crate) fn check_unsafe_blocks(count: u64, threshold: u64) -> CheckResult {
    let status = if count == 0 {
        CheckStatus::Pass
    } else if threshold == 0 {
        CheckStatus::Warn
    } else if count <= threshold {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    let message = if count == 0 {
        "No unsafe blocks".to_string()
    } else {
        format!("{count} unsafe block(s) (threshold: {threshold})")
    };
    CheckResult::new(
        CheckCategory::Safety,
        "unsafe_blocks".to_string(),
        status,
        message,
    )
}

/// Check that the module map parses successfully.
pub(crate) fn check_module_structure(parsed_ok: bool, error_msg: Option<&str>) -> CheckResult {
    let status = if parsed_ok {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    let message = if parsed_ok {
        "Module structure is coherent".to_string()
    } else {
        format!(
            "Module structure could not be parsed: {}",
            error_msg.unwrap_or("unknown error")
        )
    };
    CheckResult::new(
        CheckCategory::Structure,
        "module_structure".to_string(),
        status,
        message,
    )
}

/// Check for orphan .rs files not reachable from any crate root.
pub(crate) fn check_orphan_files(orphan_count: usize) -> CheckResult {
    let status = if orphan_count == 0 {
        CheckStatus::Pass
    } else {
        CheckStatus::Warn
    };
    let message = if orphan_count == 0 {
        "No orphan files".to_string()
    } else {
        format!("{orphan_count} orphan .rs file(s) not reachable from any crate root")
    };
    CheckResult::new(
        CheckCategory::Structure,
        "orphan_files".to_string(),
        status,
        message,
    )
}

/// Check line coverage against threshold.
pub(crate) fn check_line_coverage(
    coverage_pct: Option<f64>,
    threshold: f64,
    no_coverage: bool,
) -> CheckResult {
    if no_coverage {
        return CheckResult::new(
            CheckCategory::Coverage,
            "line_coverage".to_string(),
            CheckStatus::Skip,
            "Skipped (--no-coverage)".to_string(),
        );
    }
    match coverage_pct {
        None => CheckResult::new(
            CheckCategory::Coverage,
            "line_coverage".to_string(),
            CheckStatus::Skip,
            "Skipped (cargo-tarpaulin not installed)".to_string(),
        ),
        Some(pct) => {
            let status = if pct >= threshold {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            };
            CheckResult::new(
                CheckCategory::Coverage,
                "line_coverage".to_string(),
                status,
                format!("Line coverage: {pct:.0}% (threshold: {threshold:.0}%)"),
            )
            .with_value_and_threshold(pct, threshold)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_coverage_pass_at_threshold() {
        let result = check_doc_coverage(80.0, 80.0);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn doc_coverage_pass_above_threshold() {
        let result = check_doc_coverage(85.0, 80.0);
        assert_eq!(result.status(), CheckStatus::Pass);
        assert_eq!(result.value(), Some(85.0));
        assert_eq!(result.threshold(), Some(80.0));
    }

    #[test]
    fn doc_coverage_fail_below_threshold() {
        let result = check_doc_coverage(75.0, 80.0);
        assert_eq!(result.status(), CheckStatus::Fail);
    }

    #[test]
    fn cyclomatic_pass_at_threshold() {
        let result = check_cyclomatic_complexity(20, None, 20);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn cyclomatic_fail_above_threshold() {
        let result = check_cyclomatic_complexity(25, Some("bad_fn"), 20);
        assert_eq!(result.status(), CheckStatus::Fail);
        assert!(result.message().contains("bad_fn"));
    }

    #[test]
    fn cognitive_pass_below_threshold() {
        let result = check_cognitive_complexity(15, None, 30);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn cognitive_fail_above_threshold() {
        let result = check_cognitive_complexity(35, Some("parse_config"), 30);
        assert_eq!(result.status(), CheckStatus::Fail);
        assert!(result.message().contains("parse_config"));
    }

    #[test]
    fn shallow_modules_pass_when_none() {
        let result = check_shallow_modules(0);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn shallow_modules_fail_when_present() {
        let result = check_shallow_modules(2);
        assert_eq!(result.status(), CheckStatus::Fail);
        assert!(result.message().contains("2"));
    }

    #[test]
    fn vulnerabilities_always_fail_on_nonzero() {
        let result = check_vulnerabilities(1);
        assert_eq!(result.status(), CheckStatus::Fail);
    }

    #[test]
    fn vulnerabilities_pass_on_zero() {
        let result = check_vulnerabilities(0);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn staleness_warn_when_deps_behind() {
        let stale = vec![(
            "serde_yaml".to_string(),
            "0.9".to_string(),
            "1.0".to_string(),
        )];
        let result = check_staleness(&stale, true);
        assert_eq!(result.status(), CheckStatus::Warn);
        assert!(result.message().contains("serde_yaml"));
    }

    #[test]
    fn staleness_pass_when_all_current() {
        let result = check_staleness(&[], true);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn staleness_skip_when_disabled() {
        let result = check_staleness(&[], false);
        assert_eq!(result.status(), CheckStatus::Skip);
    }

    #[test]
    fn duplicate_versions_pass_within_threshold() {
        let result = check_duplicate_versions(2, 3);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn duplicate_versions_warn_above_threshold() {
        let result = check_duplicate_versions(5, 3);
        assert_eq!(result.status(), CheckStatus::Warn);
    }

    #[test]
    fn test_ratio_pass_above_threshold() {
        let result = check_test_ratio(2.3, 1.0);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn test_ratio_fail_below_threshold() {
        let result = check_test_ratio(0.5, 1.0);
        assert_eq!(result.status(), CheckStatus::Fail);
    }

    #[test]
    fn integration_tests_pass_when_present() {
        let result = check_integration_tests(5, true);
        assert_eq!(result.status(), CheckStatus::Pass);
        assert!(result.message().contains("5"));
    }

    #[test]
    fn integration_tests_fail_when_none() {
        let result = check_integration_tests(0, true);
        assert_eq!(result.status(), CheckStatus::Fail);
    }

    #[test]
    fn integration_tests_skip_when_not_required() {
        let result = check_integration_tests(0, false);
        assert_eq!(result.status(), CheckStatus::Skip);
    }

    #[test]
    fn unsafe_blocks_pass_when_zero() {
        let result = check_unsafe_blocks(0, 0);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn unsafe_blocks_warn_when_threshold_zero_and_count_nonzero() {
        let result = check_unsafe_blocks(3, 0);
        assert_eq!(result.status(), CheckStatus::Warn);
    }

    #[test]
    fn unsafe_blocks_pass_within_explicit_threshold() {
        let result = check_unsafe_blocks(3, 5);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn unsafe_blocks_fail_above_explicit_threshold() {
        let result = check_unsafe_blocks(6, 5);
        assert_eq!(result.status(), CheckStatus::Fail);
    }

    #[test]
    fn module_structure_pass_on_success() {
        let result = check_module_structure(true, None);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn module_structure_fail_on_error() {
        let result = check_module_structure(false, Some("missing lib.rs"));
        assert_eq!(result.status(), CheckStatus::Fail);
        assert!(result.message().contains("missing lib.rs"));
    }

    #[test]
    fn orphan_files_pass_when_none() {
        let result = check_orphan_files(0);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn orphan_files_warn_when_present() {
        let result = check_orphan_files(3);
        assert_eq!(result.status(), CheckStatus::Warn);
    }

    #[test]
    fn line_coverage_skip_when_no_coverage_flag() {
        let result = check_line_coverage(Some(80.0), 60.0, true);
        assert_eq!(result.status(), CheckStatus::Skip);
    }

    #[test]
    fn line_coverage_skip_when_tool_unavailable() {
        let result = check_line_coverage(None, 60.0, false);
        assert_eq!(result.status(), CheckStatus::Skip);
        assert!(result.message().contains("cargo-tarpaulin"));
    }

    #[test]
    fn line_coverage_pass_above_threshold() {
        let result = check_line_coverage(Some(75.0), 60.0, false);
        assert_eq!(result.status(), CheckStatus::Pass);
    }

    #[test]
    fn line_coverage_fail_below_threshold() {
        let result = check_line_coverage(Some(45.0), 60.0, false);
        assert_eq!(result.status(), CheckStatus::Fail);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn doc_coverage_deterministic(value in 0.0f64..=100.0, threshold in 0.0f64..=100.0) {
            let result = check_doc_coverage(value, threshold);
            let expected = if value >= threshold {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            };
            prop_assert_eq!(result.status(), expected);
        }

        #[test]
        fn test_ratio_deterministic(value in 0.0f64..=50.0, threshold in 0.0f64..=50.0) {
            let result = check_test_ratio(value, threshold);
            let expected = if value >= threshold {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            };
            prop_assert_eq!(result.status(), expected);
        }

        #[test]
        fn vulnerabilities_always_fail_nonzero(count in 1usize..1000) {
            let result = check_vulnerabilities(count);
            prop_assert_eq!(result.status(), CheckStatus::Fail);
        }

        #[test]
        fn unsafe_with_zero_threshold_warns_on_any(count in 1u64..1000) {
            let result = check_unsafe_blocks(count, 0);
            prop_assert_eq!(result.status(), CheckStatus::Warn);
        }
    }
}
