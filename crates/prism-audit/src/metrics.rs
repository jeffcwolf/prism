//! Metrics extraction: parses Rust source files to compute line counts,
//! function counts, and public/private item counts without a full AST.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::types::{AuditError, FileMetrics, ModuleAnalysis};

/// Extract metrics from a single Rust source file.
pub(crate) fn extract_file_metrics(path: &Path) -> Result<FileMetrics, AuditError> {
    let content = fs::read_to_string(path).map_err(|source| AuditError::Io {
        path: path.display().to_string(),
        source,
    })?;

    Ok(compute_metrics(&content))
}

/// Compute metrics from source text. Separated from I/O for testability.
fn compute_metrics(source: &str) -> FileMetrics {
    let line_count = source.lines().count();
    let mut function_count = 0;
    let mut public_item_count = 0;
    let mut total_item_count = 0;

    for line in source.lines() {
        let trimmed = line.trim();

        // Skip comments and attributes
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        let is_fn = is_function_declaration(trimmed);
        let is_item = is_item_declaration(trimmed);

        if is_fn {
            function_count += 1;
        }

        if is_item {
            total_item_count += 1;
            if is_public_item(trimmed) {
                public_item_count += 1;
            }
        }
    }

    FileMetrics::new(
        line_count,
        function_count,
        public_item_count,
        total_item_count,
    )
}

/// Check if a trimmed line declares a function (fn or pub fn, but not inside a comment).
fn is_function_declaration(trimmed: &str) -> bool {
    // Match: fn name, pub fn name, pub(crate) fn name, async fn, pub async fn, etc.
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    words.contains(&"fn") && !trimmed.starts_with("//") && !trimmed.contains("//!")
}

/// Check if a trimmed line declares any item (fn, struct, enum, trait, type, const, static, mod).
fn is_item_declaration(trimmed: &str) -> bool {
    let item_keywords = [
        "fn", "struct", "enum", "trait", "type", "const", "static", "mod",
    ];
    let words: Vec<&str> = trimmed.split_whitespace().collect();

    item_keywords.iter().any(|kw| words.contains(kw))
}

/// Check if a trimmed line starts with pub (possibly pub(crate), pub(super), etc.).
fn is_public_item(trimmed: &str) -> bool {
    trimmed.starts_with("pub ") || trimmed.starts_with("pub(")
}

/// Group file metrics by module (parent directory) and build ModuleAnalysis values.
pub(crate) fn build_module_analyses(
    file_metrics: Vec<(String, FileMetrics)>,
) -> Vec<ModuleAnalysis> {
    let mut modules: BTreeMap<String, Vec<FileMetrics>> = BTreeMap::new();

    for (path, metrics) in file_metrics {
        let module_name = module_name_from_path(&path);
        modules.entry(module_name).or_default().push(metrics);
    }

    modules
        .into_iter()
        .map(|(name, files)| ModuleAnalysis::new(name, files))
        .collect()
}

/// Derive a module name from a relative file path.
/// Files in a directory are grouped under that directory name.
/// Top-level files become their own module (stem name).
fn module_name_from_path(relative_path: &str) -> String {
    let path = Path::new(relative_path);
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.display().to_string(),
        _ => path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| relative_path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_lines() {
        let source = "line 1\nline 2\nline 3\n";
        let m = compute_metrics(source);
        assert_eq!(m.line_count(), 3);
    }

    #[test]
    fn counts_functions() {
        let source = "fn foo() {}\npub fn bar() {}\nfn baz() -> i32 { 42 }\n";
        let m = compute_metrics(source);
        assert_eq!(m.function_count(), 3);
    }

    #[test]
    fn counts_public_items() {
        let source = "\
pub fn public_fn() {}
fn private_fn() {}
pub struct MyStruct;
struct PrivateStruct;
pub(crate) fn crate_fn() {}
pub enum MyEnum {}
";
        let m = compute_metrics(source);
        assert_eq!(
            m.public_item_count(),
            4,
            "pub fn, pub struct, pub(crate) fn, pub enum"
        );
        assert_eq!(m.total_item_count(), 6);
    }

    #[test]
    fn ignores_comments_and_attributes() {
        let source = "\
// fn not_a_function() {}
/// fn also_not_a_function() {}
#[derive(Debug)]
pub struct Real;
fn actual_function() {}
";
        let m = compute_metrics(source);
        assert_eq!(m.function_count(), 1);
        assert_eq!(m.total_item_count(), 2, "struct + fn");
    }

    #[test]
    fn empty_source_produces_zero_metrics() {
        let m = compute_metrics("");
        assert_eq!(m.line_count(), 0);
        assert_eq!(m.function_count(), 0);
        assert_eq!(m.public_item_count(), 0);
        assert_eq!(m.total_item_count(), 0);
    }

    #[test]
    fn module_name_groups_by_directory() {
        assert_eq!(module_name_from_path("src/lib.rs"), "src");
        assert_eq!(module_name_from_path("src/utils/helpers.rs"), "src/utils");
        assert_eq!(module_name_from_path("main.rs"), "main");
    }

    #[test]
    fn build_module_analyses_groups_files() {
        let files = vec![
            ("src/lib.rs".to_string(), FileMetrics::new(100, 5, 2, 8)),
            ("src/utils.rs".to_string(), FileMetrics::new(50, 3, 1, 4)),
            (
                "tests/test_main.rs".to_string(),
                FileMetrics::new(30, 2, 0, 2),
            ),
        ];

        let modules = build_module_analyses(files);
        assert_eq!(modules.len(), 2, "src and tests");
        assert_eq!(modules[0].name(), "src");
        assert_eq!(modules[0].total_lines(), 150);
        assert_eq!(modules[1].name(), "tests");
    }
}
