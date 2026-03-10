//! Core types for release-readiness checks.

use serde::Serialize;

/// The status of an individual check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    Warn,
    Skip,
}

/// The category grouping for a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckCategory {
    Quality,
    Dependencies,
    Testing,
    Safety,
    Structure,
    Coverage,
}

/// The result of a single check evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    category: CheckCategory,
    name: String,
    status: CheckStatus,
    message: String,
    value: Option<f64>,
    threshold: Option<f64>,
}

impl CheckResult {
    pub(crate) fn new(
        category: CheckCategory,
        name: String,
        status: CheckStatus,
        message: String,
    ) -> Self {
        Self {
            category,
            name,
            status,
            message,
            value: None,
            threshold: None,
        }
    }

    pub(crate) fn with_value_and_threshold(mut self, value: f64, threshold: f64) -> Self {
        self.value = Some(value);
        self.threshold = Some(threshold);
        self
    }

    pub fn category(&self) -> CheckCategory {
        self.category
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> CheckStatus {
        self.status
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn value(&self) -> Option<f64> {
        self.value
    }

    pub fn threshold(&self) -> Option<f64> {
        self.threshold
    }
}

/// The complete check report with all results and summary.
#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    project_name: String,
    project_info: String,
    checks: Vec<CheckResult>,
    total_pass: usize,
    total_fail: usize,
    total_warn: usize,
    total_skip: usize,
    overall_status: CheckStatus,
}

impl CheckReport {
    pub(crate) fn new(
        project_name: String,
        project_info: String,
        checks: Vec<CheckResult>,
    ) -> Self {
        let total_pass = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Pass)
            .count();
        let total_fail = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count();
        let total_warn = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Warn)
            .count();
        let total_skip = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Skip)
            .count();
        let overall_status = if total_fail > 0 {
            CheckStatus::Fail
        } else {
            CheckStatus::Pass
        };

        Self {
            project_name,
            project_info,
            checks,
            total_pass,
            total_fail,
            total_warn,
            total_skip,
            overall_status,
        }
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn project_info(&self) -> &str {
        &self.project_info
    }

    pub fn checks(&self) -> &[CheckResult] {
        &self.checks
    }

    pub fn total_pass(&self) -> usize {
        self.total_pass
    }

    pub fn total_fail(&self) -> usize {
        self.total_fail
    }

    pub fn total_warn(&self) -> usize {
        self.total_warn
    }

    pub fn total_skip(&self) -> usize {
        self.total_skip
    }

    pub fn overall_status(&self) -> CheckStatus {
        self.overall_status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_result_construction_and_accessors() {
        let result = CheckResult::new(
            CheckCategory::Quality,
            "doc_coverage".to_string(),
            CheckStatus::Pass,
            "Doc coverage: 85% (threshold: 80%)".to_string(),
        );
        assert_eq!(result.category(), CheckCategory::Quality);
        assert_eq!(result.name(), "doc_coverage");
        assert_eq!(result.status(), CheckStatus::Pass);
        assert_eq!(result.message(), "Doc coverage: 85% (threshold: 80%)");
        assert_eq!(result.value(), None);
        assert_eq!(result.threshold(), None);
    }

    #[test]
    fn check_result_with_value_and_threshold() {
        let result = CheckResult::new(
            CheckCategory::Testing,
            "test_ratio".to_string(),
            CheckStatus::Fail,
            "Test ratio: 0.5 per 100 LOC (threshold: 1.0)".to_string(),
        )
        .with_value_and_threshold(0.5, 1.0);

        assert_eq!(result.value(), Some(0.5));
        assert_eq!(result.threshold(), Some(1.0));
    }

    #[test]
    fn check_report_counts_statuses() {
        let checks = vec![
            CheckResult::new(
                CheckCategory::Quality,
                "a".to_string(),
                CheckStatus::Pass,
                "ok".to_string(),
            ),
            CheckResult::new(
                CheckCategory::Quality,
                "b".to_string(),
                CheckStatus::Fail,
                "bad".to_string(),
            ),
            CheckResult::new(
                CheckCategory::Safety,
                "c".to_string(),
                CheckStatus::Warn,
                "meh".to_string(),
            ),
            CheckResult::new(
                CheckCategory::Coverage,
                "d".to_string(),
                CheckStatus::Skip,
                "skipped".to_string(),
            ),
        ];
        let report = CheckReport::new("test".to_string(), "info".to_string(), checks);

        assert_eq!(report.total_pass(), 1);
        assert_eq!(report.total_fail(), 1);
        assert_eq!(report.total_warn(), 1);
        assert_eq!(report.total_skip(), 1);
        assert_eq!(report.overall_status(), CheckStatus::Fail);
    }

    #[test]
    fn check_report_passes_when_no_failures() {
        let checks = vec![
            CheckResult::new(
                CheckCategory::Quality,
                "a".to_string(),
                CheckStatus::Pass,
                "ok".to_string(),
            ),
            CheckResult::new(
                CheckCategory::Safety,
                "b".to_string(),
                CheckStatus::Warn,
                "meh".to_string(),
            ),
        ];
        let report = CheckReport::new("test".to_string(), "info".to_string(), checks);

        assert_eq!(report.overall_status(), CheckStatus::Pass);
    }

    #[test]
    fn check_report_json_serialization() {
        let checks = vec![
            CheckResult::new(
                CheckCategory::Quality,
                "doc_coverage".to_string(),
                CheckStatus::Pass,
                "85%".to_string(),
            )
            .with_value_and_threshold(85.0, 80.0),
        ];
        let report = CheckReport::new("my-app".to_string(), "workspace".to_string(), checks);

        let json: serde_json::Value = serde_json::to_value(&report).expect("should serialize");
        assert_eq!(json["project_name"], "my-app");
        assert_eq!(json["overall_status"], "pass");
        assert_eq!(json["total_pass"], 1);
        assert_eq!(json["checks"][0]["status"], "pass");
        assert_eq!(json["checks"][0]["category"], "quality");
        assert_eq!(json["checks"][0]["value"], 85.0);
        assert_eq!(json["checks"][0]["threshold"], 80.0);
    }

    #[test]
    fn check_status_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&CheckStatus::Pass).unwrap(),
            "\"pass\""
        );
        assert_eq!(
            serde_json::to_string(&CheckStatus::Fail).unwrap(),
            "\"fail\""
        );
        assert_eq!(
            serde_json::to_string(&CheckStatus::Warn).unwrap(),
            "\"warn\""
        );
        assert_eq!(
            serde_json::to_string(&CheckStatus::Skip).unwrap(),
            "\"skip\""
        );
    }
}
