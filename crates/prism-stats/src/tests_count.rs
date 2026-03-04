use std::path::Path;

use syn::visit::Visit;

use crate::types::{StatsError, TestStats};

pub(crate) fn collect(path: &Path) -> Result<TestStats, StatsError> {
    let mut unit = 0u64;
    let mut integration = 0u64;
    let mut doctests = 0u64;
    let mut total_rust_lines = 0u64;

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| !is_excluded(e))
    {
        let entry = entry.map_err(|e| StatsError::file_read(path, std::io::Error::other(e)))?;

        let entry_path = entry.path();
        if entry_path.extension().map(|e| e == "rs").unwrap_or(false) {
            let content = std::fs::read_to_string(entry_path)
                .map_err(|e| StatsError::file_read(entry_path, e))?;

            total_rust_lines += crate::code::count_logical_lines(&content);

            let file = match syn::parse_file(&content) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let is_integration = is_integration_test(entry_path, path);

            let mut visitor = TestVisitor {
                in_cfg_test: false,
                is_integration,
                unit_count: 0,
                integration_count: 0,
            };
            visitor.visit_file(&file);

            unit += visitor.unit_count;
            integration += visitor.integration_count;

            // Count doctests from doc comments
            let mut doc_visitor = DoctestVisitor {
                count: 0,
                in_code_fence: false,
            };
            doc_visitor.visit_file(&file);
            doctests += doc_visitor.count;
        }
    }

    let total_tests = unit + integration + doctests;
    let ratio_per_100_loc = if total_rust_lines > 0 {
        (total_tests as f64 / total_rust_lines as f64) * 100.0
    } else {
        0.0
    };

    // Round to one decimal
    let ratio_per_100_loc = (ratio_per_100_loc * 10.0).round() / 10.0;

    Ok(TestStats {
        unit,
        integration,
        doctests,
        ratio_per_100_loc,
    })
}

fn is_excluded(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_str().unwrap_or("");
    name == "target" || name.starts_with('.') || name == "node_modules"
}

fn is_integration_test(file_path: &Path, project_root: &Path) -> bool {
    let relative = file_path.strip_prefix(project_root).unwrap_or(file_path);

    relative.components().any(|c| c.as_os_str() == "tests")
}

struct TestVisitor {
    in_cfg_test: bool,
    is_integration: bool,
    unit_count: u64,
    integration_count: u64,
}

impl<'ast> Visit<'ast> for TestVisitor {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let was_in_cfg_test = self.in_cfg_test;
        if has_cfg_test_attr(&node.attrs) {
            self.in_cfg_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_cfg_test = was_in_cfg_test;
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_test_attr(&node.attrs) {
            if self.is_integration {
                self.integration_count += 1;
            } else {
                self.unit_count += 1;
            }
        }
        syn::visit::visit_item_fn(self, node);
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("test"))
}

fn has_cfg_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        // Parse the cfg attribute to check for "test"
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("test") {
                found = true;
            }
            Ok(())
        });
        found
    })
}

struct DoctestVisitor {
    count: u64,
    in_code_fence: bool,
}

impl<'ast> Visit<'ast> for DoctestVisitor {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if !attr.path().is_ident("doc") {
            return;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &nv.value
            && let syn::Lit::Str(lit) = &expr_lit.lit
        {
            let line = lit.value();
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if self.in_code_fence {
                    self.in_code_fence = false;
                    return;
                }
                let lang = trimmed.trim_start_matches('`').trim();
                self.in_code_fence = true;
                if !lang.contains("no_run")
                    && !lang.contains("ignore")
                    && !lang.contains("compile_fail")
                    && !lang.starts_with("text")
                    && !lang.starts_with("json")
                    && !lang.starts_with("toml")
                    && !lang.starts_with("yaml")
                    && !lang.starts_with("sh")
                    && !lang.starts_with("bash")
                    && (lang.is_empty() || lang.starts_with("rust") || lang == "should_panic")
                {
                    self.count += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn counts_unit_tests() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
pub fn add(a: i32, b: i32) -> i32 { a + b }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(1, 2), 3);
    }

    #[test]
    fn test_add_negative() {
        assert_eq!(add(-1, 1), 0);
    }
}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(stats.unit, 2, "should count 2 unit tests");
        assert_eq!(stats.integration, 0);
    }

    #[test]
    fn counts_integration_tests() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(
            root.join("tests/integration.rs"),
            r#"
#[test]
fn test_from_outside() {
    assert!(true);
}

#[test]
fn another_test() {
    assert!(true);
}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(stats.unit, 0);
        assert_eq!(stats.integration, 2, "should count 2 integration tests");
    }

    #[test]
    fn counts_doctests() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
/// Adds two numbers.
///
/// ```
/// assert_eq!(2, 1 + 1);
/// ```
pub fn add(a: i32, b: i32) -> i32 { a + b }

/// # Examples
///
/// ```rust
/// let x = 42;
/// ```
///
/// ```no_run
/// panic!();
/// ```
pub fn other() {}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.doctests, 2,
            "should count 2 doctests (skipping no_run)"
        );
    }

    #[test]
    fn ratio_calculation() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        // 10 logical lines, 2 tests = 20 per 100
        fs::write(
            root.join("src/lib.rs"),
            r#"pub fn a() {}
pub fn b() {}
pub fn c() {}
pub fn d() {}
pub fn e() {}
pub fn f() {}
pub fn g() {}
pub fn h() {}
#[cfg(test)]
mod tests {
    #[test]
    fn t1() {}
    #[test]
    fn t2() {}
}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(stats.unit, 2);
        assert!(stats.ratio_per_100_loc > 0.0, "ratio should be positive");
    }
}
