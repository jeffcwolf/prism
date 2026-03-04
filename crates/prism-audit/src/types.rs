//! Core audit result types.
//!
//! These types represent the structured output of a codebase audit. They hide
//! internal computation details behind accessor methods, following the deep-module
//! principle: callers get a simple read interface without exposure to how metrics
//! are gathered or aggregated.

use thiserror::Error;

/// Errors that can occur during a codebase audit.
#[derive(Debug, Error)]
pub enum AuditError {
    /// The target path does not exist or is not a directory.
    #[error("invalid codebase path: {path}")]
    InvalidPath { path: String },

    /// An I/O error occurred while reading files.
    #[error("failed to read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    /// A Rust source file could not be parsed.
    #[error("parse error: {message}")]
    ParseError { message: String },
}

/// Severity level for audit findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A single audit finding with severity and descriptive message.
#[derive(Debug, Clone)]
pub struct Finding {
    severity: Severity,
    message: String,
}

impl Finding {
    pub(crate) fn new(severity: Severity, message: String) -> Self {
        Self { severity, message }
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Complexity metrics for a single function.
#[derive(Debug, Clone)]
pub struct FunctionComplexity {
    name: String,
    is_public: bool,
    cyclomatic: u32,
    nesting_depth: u32,
    cognitive: u32,
}

impl FunctionComplexity {
    pub(crate) fn new(
        name: String,
        is_public: bool,
        cyclomatic: u32,
        nesting_depth: u32,
        cognitive: u32,
    ) -> Self {
        Self {
            name,
            is_public,
            cyclomatic,
            nesting_depth,
            cognitive,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_public(&self) -> bool {
        self.is_public
    }

    /// Cyclomatic complexity: number of linearly independent paths.
    pub fn cyclomatic(&self) -> u32 {
        self.cyclomatic
    }

    /// Maximum nesting depth within the function body.
    pub fn nesting_depth(&self) -> u32 {
        self.nesting_depth
    }

    /// Cognitive complexity: decision points weighted by nesting level.
    pub fn cognitive(&self) -> u32 {
        self.cognitive
    }
}

const CYCLOMATIC_WARNING: u32 = 10;
const NESTING_WARNING: u32 = 4;
const COGNITIVE_WARNING: u32 = 15;

/// Top-level audit result containing per-module analysis and findings.
#[derive(Debug)]
pub struct AuditReport {
    modules: Vec<ModuleAnalysis>,
    findings: Vec<Finding>,
}

impl AuditReport {
    pub(crate) fn new(modules: Vec<ModuleAnalysis>) -> Self {
        let findings = Self::generate_findings(&modules);
        Self { modules, findings }
    }

    /// Per-module analysis results, sorted by name.
    pub fn modules(&self) -> &[ModuleAnalysis] {
        &self.modules
    }

    /// Audit findings across all modules, ordered by severity.
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Total number of files scanned across all modules.
    pub fn total_files(&self) -> usize {
        self.modules.iter().map(|m| m.file_count).sum()
    }

    /// Total line count across all modules.
    pub fn total_lines(&self) -> usize {
        self.modules.iter().map(|m| m.total_lines).sum()
    }

    fn generate_findings(modules: &[ModuleAnalysis]) -> Vec<Finding> {
        let mut findings = Vec::new();

        for module in modules {
            if module.depth_ratio() > 0.5 {
                findings.push(Finding::new(
                    Severity::Warning,
                    format!(
                        "module '{}' is shallow (depth ratio {:.2})",
                        module.name(),
                        module.depth_ratio()
                    ),
                ));
            }

            for file in &module.files {
                for func in &file.function_complexities {
                    if func.cyclomatic >= CYCLOMATIC_WARNING {
                        let severity = if func.cyclomatic >= 20 {
                            Severity::Error
                        } else {
                            Severity::Warning
                        };
                        findings.push(Finding::new(
                            severity,
                            format!(
                                "function '{}' has high cyclomatic complexity ({})",
                                func.name, func.cyclomatic
                            ),
                        ));
                    }

                    if func.nesting_depth > NESTING_WARNING {
                        let severity = if func.nesting_depth > 6 {
                            Severity::Error
                        } else {
                            Severity::Warning
                        };
                        findings.push(Finding::new(
                            severity,
                            format!(
                                "function '{}' has deep nesting (depth {})",
                                func.name, func.nesting_depth
                            ),
                        ));
                    }

                    if func.cognitive >= COGNITIVE_WARNING {
                        let severity = if func.cognitive >= 30 {
                            Severity::Error
                        } else {
                            Severity::Warning
                        };
                        findings.push(Finding::new(
                            severity,
                            format!(
                                "function '{}' has high cognitive complexity ({})",
                                func.name, func.cognitive
                            ),
                        ));
                    }
                }
            }
        }

        findings.sort_by(|a, b| b.severity.cmp(&a.severity));
        findings
    }
}

/// Analysis of a single module (directory or top-level file grouping).
#[derive(Debug)]
pub struct ModuleAnalysis {
    name: String,
    public_item_count: usize,
    total_item_count: usize,
    total_lines: usize,
    file_count: usize,
    files: Vec<FileMetrics>,
}

impl ModuleAnalysis {
    pub(crate) fn new(name: String, files: Vec<FileMetrics>) -> Self {
        let public_item_count: usize = files.iter().map(|f| f.public_item_count).sum();
        let total_item_count: usize = files.iter().map(|f| f.total_item_count).sum();
        let total_lines: usize = files.iter().map(|f| f.line_count).sum();
        let file_count = files.len();

        Self {
            name,
            public_item_count,
            total_item_count,
            total_lines,
            file_count,
            files,
        }
    }

    /// Module name (relative directory path or filename).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Module depth ratio using AST-aware complexity weighting.
    ///
    /// Interface complexity weights each public item by its cyclomatic complexity:
    /// a `pub fn` with complexity 15 contributes more than one with complexity 2.
    /// Implementation complexity is the total complexity of all items plus the
    /// count of private items. Returns 0.0 if implementation complexity is zero.
    pub fn depth_ratio(&self) -> f64 {
        let mut interface_complexity: f64 = 0.0;
        let mut implementation_complexity: f64 = 0.0;

        for file in &self.files {
            if file.function_complexities.is_empty() {
                // Fallback for files without AST data: use simple counts
                interface_complexity += file.public_item_count as f64;
                implementation_complexity += (file.line_count + file.function_count) as f64;
            } else {
                // Weight public items by their cyclomatic complexity
                for func in &file.function_complexities {
                    let weight = func.cyclomatic.max(1) as f64;
                    if func.is_public {
                        interface_complexity += weight;
                    }
                    implementation_complexity += weight;
                }
                // Private non-function items contribute to implementation
                let private_items = file.total_item_count.saturating_sub(file.public_item_count);
                implementation_complexity += private_items as f64;
            }
        }

        if implementation_complexity == 0.0 {
            return 0.0;
        }
        interface_complexity / implementation_complexity
    }

    /// Number of public items across all files in this module.
    pub fn public_item_count(&self) -> usize {
        self.public_item_count
    }

    /// Total number of items (public + private) across all files.
    pub fn total_item_count(&self) -> usize {
        self.total_item_count
    }

    /// Total line count across all files in this module.
    pub fn total_lines(&self) -> usize {
        self.total_lines
    }

    /// Individual file metrics within this module.
    pub fn files(&self) -> &[FileMetrics] {
        &self.files
    }

    /// All function complexity data across all files in this module.
    pub fn function_complexities(&self) -> Vec<&FunctionComplexity> {
        self.files
            .iter()
            .flat_map(|f| &f.function_complexities)
            .collect()
    }
}

/// Per-file metrics extracted from a single Rust source file.
#[derive(Debug, Clone)]
pub struct FileMetrics {
    line_count: usize,
    function_count: usize,
    public_item_count: usize,
    total_item_count: usize,
    function_complexities: Vec<FunctionComplexity>,
}

impl FileMetrics {
    pub(crate) fn new(
        line_count: usize,
        function_count: usize,
        public_item_count: usize,
        total_item_count: usize,
        function_complexities: Vec<FunctionComplexity>,
    ) -> Self {
        Self {
            line_count,
            function_count,
            public_item_count,
            total_item_count,
            function_complexities,
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_count
    }

    pub fn function_count(&self) -> usize {
        self.function_count
    }

    pub fn public_item_count(&self) -> usize {
        self.public_item_count
    }

    pub fn total_item_count(&self) -> usize {
        self.total_item_count
    }

    /// Per-function complexity breakdowns for this file.
    pub fn function_complexities(&self) -> &[FunctionComplexity] {
        &self.function_complexities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_metrics_stores_counts() {
        let m = FileMetrics::new(100, 5, 2, 8, vec![]);
        assert_eq!(m.line_count(), 100);
        assert_eq!(m.function_count(), 5);
        assert_eq!(m.public_item_count(), 2);
        assert_eq!(m.total_item_count(), 8);
    }

    #[test]
    fn file_metrics_with_complexities() {
        let funcs = vec![FunctionComplexity::new("foo".into(), true, 5, 2, 8)];
        let m = FileMetrics::new(100, 1, 1, 1, funcs);
        assert_eq!(m.function_complexities().len(), 1);
        assert_eq!(m.function_complexities()[0].name(), "foo");
        assert_eq!(m.function_complexities()[0].cyclomatic(), 5);
        assert_eq!(m.function_complexities()[0].nesting_depth(), 2);
        assert_eq!(m.function_complexities()[0].cognitive(), 8);
    }

    #[test]
    fn module_analysis_aggregates_file_metrics() {
        let files = vec![
            FileMetrics::new(50, 3, 1, 4, vec![]),
            FileMetrics::new(80, 5, 2, 6, vec![]),
        ];
        let module = ModuleAnalysis::new("test_mod".to_string(), files);
        assert_eq!(module.total_lines(), 130);
        assert_eq!(module.public_item_count(), 3);
        assert_eq!(module.total_item_count(), 10);
    }

    #[test]
    fn depth_ratio_near_zero_for_deep_module_with_complexity_data() {
        // One public function with low complexity, many private items
        let funcs = vec![
            FunctionComplexity::new("public_api".into(), true, 2, 1, 3),
            FunctionComplexity::new("internal_a".into(), false, 8, 3, 12),
            FunctionComplexity::new("internal_b".into(), false, 6, 2, 9),
        ];
        let files = vec![FileMetrics::new(500, 3, 1, 10, funcs)];
        let module = ModuleAnalysis::new("deep".to_string(), files);
        // interface = 2 (public fn complexity)
        // implementation = 2 + 8 + 6 (all fn complexities) + 9 (private items: 10 - 1) = 25
        // ratio = 2/25 = 0.08
        assert!(
            module.depth_ratio() < 0.15,
            "expected deep module, got {}",
            module.depth_ratio()
        );
    }

    #[test]
    fn depth_ratio_high_for_shallow_module_with_complexity_data() {
        // Many public functions each with low complexity
        let funcs: Vec<FunctionComplexity> = (0..10)
            .map(|i| FunctionComplexity::new(format!("pub_fn_{i}"), true, 1, 0, 0))
            .collect();
        let files = vec![FileMetrics::new(12, 10, 10, 10, funcs)];
        let module = ModuleAnalysis::new("shallow".to_string(), files);
        // interface = 10 * 1 = 10, implementation = 10 * 1 + 0 private = 10
        // ratio = 10/10 = 1.0
        assert!(
            module.depth_ratio() > 0.5,
            "expected shallow module, got {}",
            module.depth_ratio()
        );
    }

    #[test]
    fn depth_ratio_zero_for_empty_module() {
        let files = vec![FileMetrics::new(0, 0, 0, 0, vec![])];
        let module = ModuleAnalysis::new("empty".to_string(), files);
        assert_eq!(module.depth_ratio(), 0.0);
    }

    #[test]
    fn depth_ratio_fallback_without_ast_data() {
        // Files without function_complexities use the old heuristic
        let files = vec![FileMetrics::new(1000, 50, 2, 30, vec![])];
        let module = ModuleAnalysis::new("fallback".to_string(), files);
        // interface = 2, implementation = 1000 + 50 = 1050
        // ratio = 2/1050 ≈ 0.0019
        assert!(
            module.depth_ratio() < 0.01,
            "expected deep module in fallback, got {}",
            module.depth_ratio()
        );
    }

    #[test]
    fn audit_report_totals() {
        let modules = vec![
            ModuleAnalysis::new(
                "a".to_string(),
                vec![FileMetrics::new(100, 5, 2, 8, vec![])],
            ),
            ModuleAnalysis::new(
                "b".to_string(),
                vec![FileMetrics::new(200, 10, 3, 12, vec![])],
            ),
        ];
        let report = AuditReport::new(modules);
        assert_eq!(report.total_files(), 2);
        assert_eq!(report.total_lines(), 300);
        assert_eq!(report.modules().len(), 2);
    }

    #[test]
    fn findings_generated_for_high_complexity() {
        let funcs = vec![FunctionComplexity::new(
            "complex_fn".into(),
            false,
            15,
            5,
            20,
        )];
        let files = vec![FileMetrics::new(100, 1, 0, 1, funcs)];
        let modules = vec![ModuleAnalysis::new("m".to_string(), files)];
        let report = AuditReport::new(modules);

        let findings = report.findings();
        assert!(
            findings.len() >= 3,
            "should flag cyclomatic, nesting, and cognitive: got {:?}",
            findings
        );

        let messages: Vec<&str> = findings.iter().map(|f| f.message()).collect();
        assert!(messages.iter().any(|m| m.contains("cyclomatic")));
        assert!(messages.iter().any(|m| m.contains("nesting")));
        assert!(messages.iter().any(|m| m.contains("cognitive")));
    }

    #[test]
    fn findings_sorted_by_severity_descending() {
        let funcs = vec![
            FunctionComplexity::new("error_fn".into(), false, 25, 8, 35),
            FunctionComplexity::new("warn_fn".into(), false, 12, 5, 18),
        ];
        let files = vec![FileMetrics::new(200, 2, 0, 2, funcs)];
        let modules = vec![ModuleAnalysis::new("m".to_string(), files)];
        let report = AuditReport::new(modules);

        let severities: Vec<Severity> = report.findings().iter().map(|f| f.severity()).collect();
        for window in severities.windows(2) {
            assert!(
                window[0] >= window[1],
                "findings should be sorted by severity descending"
            );
        }
    }

    #[test]
    fn no_findings_for_simple_code() {
        let funcs = vec![FunctionComplexity::new("simple".into(), true, 2, 1, 1)];
        let files = vec![FileMetrics::new(50, 1, 1, 5, funcs)];
        let modules = vec![ModuleAnalysis::new("m".to_string(), files)];
        let report = AuditReport::new(modules);

        assert!(
            report.findings().is_empty(),
            "simple code should have no findings: {:?}",
            report.findings()
        );
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
    }
}
