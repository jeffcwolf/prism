//! Prism audit engine: analyzes Rust codebases for module depth, complexity, and API surface.
//!
//! The primary entry point is [`audit_codebase`], which recursively discovers Rust source files,
//! computes per-file metrics, groups them into modules, and produces an [`AuditReport`].

mod discovery;
mod metrics;
mod types;

pub use types::{AuditError, AuditReport, FileMetrics, ModuleAnalysis};

use std::path::Path;

/// Audit a Rust codebase at the given path, returning a structured report.
///
/// Recursively discovers `.rs` files (skipping `target/` and hidden directories),
/// computes file-level metrics, groups files into modules, and calculates
/// module depth ratios.
pub fn audit_codebase(path: &Path) -> Result<AuditReport, AuditError> {
    let files = discovery::discover_rust_files(path)?;
    let file_metrics: Vec<(String, FileMetrics)> = files
        .into_iter()
        .map(|file_path| {
            let m = metrics::extract_file_metrics(&file_path)?;
            let relative = file_path
                .strip_prefix(path)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .into_owned();
            Ok((relative, m))
        })
        .collect::<Result<Vec<_>, AuditError>>()?;

    let modules = metrics::build_module_analyses(file_metrics);
    Ok(AuditReport::new(modules))
}
