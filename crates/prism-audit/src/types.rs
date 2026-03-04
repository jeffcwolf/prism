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
}

/// Top-level audit result containing per-module analysis.
#[derive(Debug)]
pub struct AuditReport {
    modules: Vec<ModuleAnalysis>,
}

impl AuditReport {
    pub(crate) fn new(modules: Vec<ModuleAnalysis>) -> Self {
        Self { modules }
    }

    /// Per-module analysis results, sorted by name.
    pub fn modules(&self) -> &[ModuleAnalysis] {
        &self.modules
    }

    /// Total number of files scanned across all modules.
    pub fn total_files(&self) -> usize {
        self.modules.iter().map(|m| m.file_count).sum()
    }

    /// Total line count across all modules.
    pub fn total_lines(&self) -> usize {
        self.modules.iter().map(|m| m.total_lines).sum()
    }
}

/// Analysis of a single module (directory or top-level file grouping).
#[derive(Debug)]
pub struct ModuleAnalysis {
    name: String,
    public_item_count: usize,
    total_item_count: usize,
    total_lines: usize,
    total_function_count: usize,
    file_count: usize,
    files: Vec<FileMetrics>,
}

impl ModuleAnalysis {
    pub(crate) fn new(name: String, files: Vec<FileMetrics>) -> Self {
        let public_item_count: usize = files.iter().map(|f| f.public_item_count).sum();
        let total_item_count: usize = files.iter().map(|f| f.total_item_count).sum();
        let total_lines: usize = files.iter().map(|f| f.line_count).sum();
        let total_function_count: usize = files.iter().map(|f| f.function_count).sum();
        let file_count = files.len();

        Self {
            name,
            public_item_count,
            total_item_count,
            total_lines,
            total_function_count,
            file_count,
            files,
        }
    }

    /// Module name (relative directory path or filename).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Module depth ratio: interface_complexity / implementation_complexity.
    ///
    /// Interface complexity is the count of public items. Implementation
    /// complexity is total lines + function count. A ratio near 0.0 means
    /// deep (good); near 1.0 means shallow (bad). Returns 0.0 if the module
    /// has no implementation complexity.
    pub fn depth_ratio(&self) -> f64 {
        let implementation_complexity = (self.total_lines + self.total_function_count) as f64;
        if implementation_complexity == 0.0 {
            return 0.0;
        }
        self.public_item_count as f64 / implementation_complexity
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
}

/// Per-file metrics extracted from a single Rust source file.
#[derive(Debug, Clone)]
pub struct FileMetrics {
    line_count: usize,
    function_count: usize,
    public_item_count: usize,
    total_item_count: usize,
}

impl FileMetrics {
    pub(crate) fn new(
        line_count: usize,
        function_count: usize,
        public_item_count: usize,
        total_item_count: usize,
    ) -> Self {
        Self {
            line_count,
            function_count,
            public_item_count,
            total_item_count,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_metrics_stores_counts() {
        let m = FileMetrics::new(100, 5, 2, 8);
        assert_eq!(m.line_count(), 100);
        assert_eq!(m.function_count(), 5);
        assert_eq!(m.public_item_count(), 2);
        assert_eq!(m.total_item_count(), 8);
    }

    #[test]
    fn module_analysis_aggregates_file_metrics() {
        let files = vec![FileMetrics::new(50, 3, 1, 4), FileMetrics::new(80, 5, 2, 6)];
        let module = ModuleAnalysis::new("test_mod".to_string(), files);
        assert_eq!(module.total_lines(), 130);
        assert_eq!(module.public_item_count(), 3);
        assert_eq!(module.total_item_count(), 10);
    }

    #[test]
    fn depth_ratio_near_zero_for_deep_module() {
        // 2 public items, 1000 lines + 50 functions = 1050 impl complexity
        // ratio = 2/1050 ≈ 0.0019
        let files = vec![FileMetrics::new(1000, 50, 2, 30)];
        let module = ModuleAnalysis::new("deep".to_string(), files);
        assert!(
            module.depth_ratio() < 0.01,
            "expected deep module, got {}",
            module.depth_ratio()
        );
    }

    #[test]
    fn depth_ratio_high_for_shallow_module() {
        // 10 public items, 15 lines + 10 functions = 25 impl complexity
        // ratio = 10/25 = 0.4 — but let's make it even more shallow
        // 10 public items, 10 lines + 5 functions = 15 impl complexity
        // ratio = 10/15 ≈ 0.67
        let files = vec![FileMetrics::new(10, 5, 10, 10)];
        let module = ModuleAnalysis::new("shallow".to_string(), files);
        assert!(
            module.depth_ratio() > 0.5,
            "expected shallow module, got {}",
            module.depth_ratio()
        );
    }

    #[test]
    fn depth_ratio_zero_for_empty_module() {
        let files = vec![FileMetrics::new(0, 0, 0, 0)];
        let module = ModuleAnalysis::new("empty".to_string(), files);
        assert_eq!(module.depth_ratio(), 0.0);
    }

    #[test]
    fn audit_report_totals() {
        let modules = vec![
            ModuleAnalysis::new("a".to_string(), vec![FileMetrics::new(100, 5, 2, 8)]),
            ModuleAnalysis::new("b".to_string(), vec![FileMetrics::new(200, 10, 3, 12)]),
        ];
        let report = AuditReport::new(modules);
        assert_eq!(report.total_files(), 2);
        assert_eq!(report.total_lines(), 300);
        assert_eq!(report.modules().len(), 2);
    }
}
