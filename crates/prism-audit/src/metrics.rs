//! Metrics extraction: parses Rust source files using `syn` for accurate
//! complexity analysis, item counting, and per-function breakdowns.

use std::collections::BTreeMap;
use std::path::Path;

use crate::complexity;
use crate::source_file::SourceFile;
use crate::types::{AuditError, FileMetrics, FunctionComplexity, ModuleAnalysis};

/// Extract metrics from a single Rust source file using syn-based parsing.
pub(crate) fn extract_file_metrics(path: &Path) -> Result<FileMetrics, AuditError> {
    let source_file = SourceFile::from_path(path)?;
    Ok(compute_metrics_from_ast(&source_file))
}

/// Compute metrics from a parsed source file.
fn compute_metrics_from_ast(source_file: &SourceFile) -> FileMetrics {
    let counts = source_file.item_counts();
    let functions = source_file.functions();

    let function_complexities: Vec<FunctionComplexity> = functions
        .iter()
        .map(|func| {
            let cyclomatic = complexity::cyclomatic_complexity(&func.body);
            let depth = complexity::nesting_depth(&func.body);
            let cognitive = complexity::cognitive_complexity(&func.body);
            FunctionComplexity::new(
                func.name.clone(),
                func.is_public,
                cyclomatic,
                depth,
                cognitive,
            )
        })
        .collect();

    FileMetrics::new(
        source_file.line_count(),
        counts.functions,
        counts.public,
        counts.total,
        function_complexities,
    )
}

/// Compute metrics from source text (convenience for testing).
#[cfg(test)]
fn compute_metrics(source: &str) -> FileMetrics {
    match SourceFile::parse(source) {
        Ok(sf) => compute_metrics_from_ast(&sf),
        Err(_) => {
            // Fallback for unparseable source: line counting only
            let line_count = source.lines().count();
            FileMetrics::new(line_count, 0, 0, 0, vec![])
        }
    }
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
        let source = "fn foo() {}\nfn bar() {}\nfn baz() {}\n";
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
            (
                "src/lib.rs".to_string(),
                FileMetrics::new(100, 5, 2, 8, vec![]),
            ),
            (
                "src/utils.rs".to_string(),
                FileMetrics::new(50, 3, 1, 4, vec![]),
            ),
            (
                "tests/test_main.rs".to_string(),
                FileMetrics::new(30, 2, 0, 2, vec![]),
            ),
        ];

        let modules = build_module_analyses(files);
        assert_eq!(modules.len(), 2, "src and tests");
        assert_eq!(modules[0].name(), "src");
        assert_eq!(modules[0].total_lines(), 150);
        assert_eq!(modules[1].name(), "tests");
    }

    #[test]
    fn function_complexities_extracted() {
        let source = "\
fn simple() {}
fn branchy(x: i32) -> i32 {
    if x > 0 {
        if x > 10 {
            match x {
                1 => 1,
                2 => 2,
                _ => 3,
            }
        } else {
            x
        }
    } else {
        0
    }
}
";
        let m = compute_metrics(source);
        assert_eq!(m.function_complexities().len(), 2);

        let simple = &m.function_complexities()[0];
        assert_eq!(simple.name(), "simple");
        assert_eq!(simple.cyclomatic(), 1);
        assert_eq!(simple.nesting_depth(), 0);
        assert_eq!(simple.cognitive(), 0);

        let branchy = &m.function_complexities()[1];
        assert_eq!(branchy.name(), "branchy");
        assert!(
            branchy.cyclomatic() > 1,
            "branchy should have complexity > 1, got {}",
            branchy.cyclomatic()
        );
        assert!(
            branchy.nesting_depth() >= 3,
            "branchy should have nesting >= 3, got {}",
            branchy.nesting_depth()
        );
    }

    #[test]
    fn impl_methods_counted() {
        let source = "\
struct Foo;
impl Foo {
    pub fn public_method(&self) -> i32 { 42 }
    fn private_method(&self) {}
}
";
        let m = compute_metrics(source);
        assert_eq!(m.function_count(), 2);
        assert_eq!(m.function_complexities().len(), 2);
        assert!(m.function_complexities()[0].is_public());
        assert!(!m.function_complexities()[1].is_public());
    }
}
