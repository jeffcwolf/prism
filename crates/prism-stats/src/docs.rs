use std::collections::HashSet;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

use crate::types::{DocsStats, StatsError};

pub(crate) fn collect(path: &Path) -> Result<DocsStats, StatsError> {
    let mut total_pub_items = 0u64;
    let mut documented_pub_items = 0u64;
    let mut doctests = 0u64;

    // --- Phase 1a: identify files that are only reachable through feature-gated
    //     `mod` declarations.  walkdir discovers every `.rs` file on disk, but
    //     files behind `#[cfg(feature = "…")] mod foo;` are not part of the
    //     default public API.  rustdoc never sees them unless the feature is
    //     explicitly enabled, so Prism must skip them too.
    let feature_gated_files = collect_feature_gated_module_files(path)?;

    // --- Phase 1b: identify binary-only crate directories.  rustdoc does not
    //     document binary crates (those with src/main.rs but no src/lib.rs).
    //     `-D missing_docs` never fires on their items, so Prism must skip them.
    let binary_crate_dirs = collect_binary_only_crate_dirs(path)?;

    // --- Phase 2: walk and count, skipping gated files and binary crates.
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| !is_excluded(e))
    {
        let entry = entry.map_err(|e| StatsError::file_read(path, std::io::Error::other(e)))?;

        if entry.path().extension().map(|e| e == "rs").unwrap_or(false) {
            // Skip files whose only inclusion path is behind cfg(feature).
            if feature_gated_files.contains(entry.path()) {
                continue;
            }
            // Skip files inside binary-only crates.
            if binary_crate_dirs.iter().any(|d| entry.path().starts_with(d)) {
                continue;
            }

            let content = std::fs::read_to_string(entry.path())
                .map_err(|e| StatsError::file_read(entry.path(), e))?;

            let file = match syn::parse_file(&content) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let mut visitor = DocsVisitor::default();
            visitor.visit_file(&file);
            total_pub_items += visitor.total_pub;
            documented_pub_items += visitor.documented_pub;
            doctests += visitor.doctests;
        }
    }

    let coverage_pct = if total_pub_items > 0 {
        (documented_pub_items as f64 / total_pub_items as f64) * 100.0
    } else {
        0.0
    };

    Ok(DocsStats {
        documented_pub_items,
        total_pub_items,
        coverage_pct,
        doctests,
    })
}

/// Walk the project tree and collect absolute paths of `.rs` files that are
/// only included via a feature-gated external module declaration.
///
/// For each file that contains `#[cfg(feature = "…")] mod foo;` (an external
/// module — no body braces), we resolve `foo.rs` and `foo/mod.rs` relative to
/// the file's parent directory and add them to the exclusion set.
fn collect_feature_gated_module_files(root: &Path) -> Result<HashSet<PathBuf>, StatsError> {
    let mut gated: HashSet<PathBuf> = HashSet::new();

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_excluded(e))
    {
        let entry = entry.map_err(|e| StatsError::file_read(root, std::io::Error::other(e)))?;

        if entry.path().extension().map(|e| e == "rs").unwrap_or(false) {
            let content = std::fs::read_to_string(entry.path())
                .map_err(|e| StatsError::file_read(entry.path(), e))?;

            let file = match syn::parse_file(&content) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let parent_dir = entry.path().parent().unwrap_or(entry.path());

            for item in &file.items {
                if let syn::Item::Mod(item_mod) = item {
                    // Only external modules (declared with `;`, no inline body).
                    if item_mod.content.is_some() {
                        continue;
                    }
                    if has_cfg_feature_attr(&item_mod.attrs) {
                        let mod_name = item_mod.ident.to_string();
                        // Rust's module resolution: `foo.rs` or `foo/mod.rs`.
                        let candidate_file = parent_dir.join(format!("{mod_name}.rs"));
                        let candidate_dir = parent_dir.join(&mod_name).join("mod.rs");

                        if candidate_file.is_file() {
                            gated.insert(candidate_file);
                        }
                        if candidate_dir.is_file() {
                            gated.insert(candidate_dir);
                        }
                    }
                }
            }
        }
    }

    Ok(gated)
}

/// Collect directories of binary-only crates (have `src/main.rs` but no
/// `src/lib.rs`).  rustdoc does not document binary crates, so `-D
/// missing_docs` never fires on their public items.  Prism must skip them
/// to match rustdoc's view.
fn collect_binary_only_crate_dirs(root: &Path) -> Result<Vec<PathBuf>, StatsError> {
    let mut dirs = Vec::new();

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_excluded(e))
    {
        let entry = entry.map_err(|e| StatsError::file_read(root, std::io::Error::other(e)))?;

        if entry.file_name() == "Cargo.toml" && entry.file_type().is_file() {
            let crate_dir = entry.path().parent().unwrap_or(entry.path());
            let has_main = crate_dir.join("src/main.rs").is_file();
            let has_lib = crate_dir.join("src/lib.rs").is_file();
            if has_main && !has_lib {
                dirs.push(crate_dir.to_path_buf());
            }
        }
    }

    Ok(dirs)
}

fn is_excluded(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_str().unwrap_or("");
    if name == "target" || name.starts_with('.') || name == "node_modules" {
        return true;
    }
    // Exclude integration test directories. Public items in tests/ are test
    // helpers, not part of the documented public API surface. rustdoc ignores
    // them under -D missing_docs; Prism must match.
    if name == "tests" && entry.file_type().is_dir() {
        return true;
    }
    if name == "fixtures" {
        return entry.path().components().any(|c| c.as_os_str() == "tests");
    }
    false
}

#[derive(Default)]
struct DocsVisitor {
    total_pub: u64,
    documented_pub: u64,
    doctests: u64,
}

impl DocsVisitor {
    fn check_item_attrs(&mut self, vis: &syn::Visibility, attrs: &[syn::Attribute]) {
        if !is_fully_public(vis) {
            return;
        }
        // Items behind #[cfg(feature = "…")] are not part of the default public
        // API.  rustdoc only documents them when the feature is explicitly
        // enabled.  Skip them so Prism's count matches rustdoc's.
        if has_cfg_feature_attr(attrs) {
            return;
        }
        self.total_pub += 1;
        let has_doc = attrs.iter().any(|a| a.path().is_ident("doc"));
        if has_doc {
            self.documented_pub += 1;
            self.doctests += count_doctests_in_attrs(attrs);
        }
    }
}

fn is_fully_public(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn has_cfg_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
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

/// Detect `#[cfg(feature = "…")]` on an item.
///
/// Uses `parse_nested_meta` to walk the cfg predicate.  Matches plain
/// `cfg(feature = "…")` as well as compound predicates that mention `feature`
/// anywhere (e.g. `cfg(all(feature = "x", unix))`).  This conservative approach
/// ensures we never count an item that requires a non-default feature to compile.
fn has_cfg_feature_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("feature") {
                found = true;
                // Consume the `= "value"` part so the parser doesn't choke.
                let _ = meta.value().and_then(|v| v.parse::<syn::LitStr>());
            }
            Ok(())
        });
        found
    })
}

fn count_doctests_in_attrs(attrs: &[syn::Attribute]) -> u64 {
    let mut count = 0u64;
    let mut in_code_fence = false;

    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &nv.value
            && let syn::Lit::Str(lit) = &expr_lit.lit
        {
            let line = lit.value();
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if in_code_fence {
                    // Closing fence
                    in_code_fence = false;
                    continue;
                }
                let lang = trimmed.trim_start_matches('`').trim();
                in_code_fence = true;
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
                    count += 1;
                }
            }
        }
    }
    count
}

impl<'ast> Visit<'ast> for DocsVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.check_item_attrs(&node.vis, &node.attrs);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.check_item_attrs(&node.vis, &node.attrs);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.check_item_attrs(&node.vis, &node.attrs);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.check_item_attrs(&node.vis, &node.attrs);
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.check_item_attrs(&node.vis, &node.attrs);
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.check_item_attrs(&node.vis, &node.attrs);
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.check_item_attrs(&node.vis, &node.attrs);
        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Skip #[cfg(test)] modules — pub items inside them are not part of
        // the documented public API surface. rustdoc ignores them under
        // -D missing_docs; Prism must match.
        if has_cfg_test_attr(&node.attrs) {
            return;
        }
        // Skip #[cfg(feature = "…")] modules — they are not compiled (and
        // therefore not documented by rustdoc) unless the feature is
        // explicitly enabled.  This handles inline modules; external module
        // *files* are excluded at the walkdir level by Phase 1.
        if has_cfg_feature_attr(&node.attrs) {
            return;
        }
        self.check_item_attrs(&node.vis, &node.attrs);
        syn::visit::visit_item_mod(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn counts_documented_and_undocumented_pub_items() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
/// Documented function.
pub fn documented() {}

pub fn undocumented() {}

/// Documented struct.
pub struct Foo;

pub(crate) fn internal() {}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(stats.total_pub_items, 3, "should count 3 pub items");
        assert_eq!(
            stats.documented_pub_items, 2,
            "should count 2 documented pub items"
        );
        assert!(
            (stats.coverage_pct - 66.6).abs() < 1.0,
            "coverage should be ~66.7%"
        );
    }

    #[test]
    fn does_not_count_pub_items_in_tests_directory() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        // Documented pub item in src/ — must count
        fs::write(
            root.join("src/lib.rs"),
            "/// A documented function.\npub fn documented() {}\n",
        )
        .unwrap();
        // Undocumented pub item in tests/ — must NOT count against coverage
        fs::write(
            root.join("tests/integration.rs"),
            "pub fn undocumented_test_helper() {}\n",
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.total_pub_items, 1,
            "pub items in tests/ must not be counted"
        );
        assert_eq!(stats.documented_pub_items, 1);
        assert!((stats.coverage_pct - 100.0).abs() < 1.0);
    }

    #[test]
    fn does_not_count_pub_items_in_cfg_test_modules() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
/// A documented function.
pub fn documented() {}

#[cfg(test)]
mod tests {
    // These pub helpers must not count against doc coverage.
    pub fn test_helper() {}
    pub fn another_helper() {}
}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.total_pub_items, 1,
            "cfg(test) pub items must not be counted"
        );
        assert_eq!(stats.documented_pub_items, 1);
        assert!((stats.coverage_pct - 100.0).abs() < 1.0);
    }

    #[test]
    fn does_not_count_pub_items_in_cfg_feature_inline_modules() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
/// A documented function.
pub fn documented() {}

/// Another documented function.
pub fn also_documented() {}

#[cfg(feature = "test-utils")]
pub mod mocks {
    pub struct MockClient;
    pub fn mock_helper() {}
}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.total_pub_items, 2,
            "cfg(feature) inline module pub items must not be counted"
        );
        assert_eq!(stats.documented_pub_items, 2);
        assert!((stats.coverage_pct - 100.0).abs() < 1.0);
    }

    #[test]
    fn does_not_count_pub_items_in_cfg_feature_external_module_file() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        // lib.rs: one documented pub fn + a feature-gated external module
        fs::write(
            root.join("src/lib.rs"),
            r#"
/// A documented function.
pub fn documented() {}

#[cfg(feature = "test-utils")]
pub mod mock;
"#,
        )
        .unwrap();
        // mock.rs: undocumented pub items that should NOT count
        fs::write(
            root.join("src/mock.rs"),
            r#"
pub struct MockClient;
pub struct AnotherMock;
pub fn mock_helper() {}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.total_pub_items, 1,
            "pub items in feature-gated external module file must not be counted"
        );
        assert_eq!(stats.documented_pub_items, 1);
        assert!((stats.coverage_pct - 100.0).abs() < 1.0);
    }

    #[test]
    fn does_not_count_cfg_feature_items_at_top_level() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
/// Documented function.
pub fn documented() {}

#[cfg(feature = "test-utils")]
pub use mock::{MockA, MockB};

#[cfg(feature = "unstable")]
pub fn experimental_api() {}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        // Only the non-gated `documented` should count.
        // `pub use` is not visited by DocsVisitor (no visit_item_use),
        // and `experimental_api` is behind cfg(feature).
        assert_eq!(
            stats.total_pub_items, 1,
            "cfg(feature) top-level items must not be counted"
        );
    }

    #[test]
    fn does_not_count_pub_items_in_binary_only_crates() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // A library crate with one documented pub item
        fs::create_dir_all(root.join("crates/mylib/src")).unwrap();
        fs::write(
            root.join("crates/mylib/Cargo.toml"),
            "[package]\nname=\"mylib\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/mylib/src/lib.rs"),
            "/// Documented.\npub fn lib_fn() {}\n",
        )
        .unwrap();

        // A binary-only crate (main.rs, no lib.rs) with undocumented pub items
        fs::create_dir_all(root.join("crates/mycli/src")).unwrap();
        fs::write(
            root.join("crates/mycli/Cargo.toml"),
            "[package]\nname=\"mycli\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/mycli/src/main.rs"),
            "pub fn cli_helper() {}\nfn main() {}\n",
        )
        .unwrap();

        // Workspace Cargo.toml
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.total_pub_items, 1,
            "binary-only crate pub items must not be counted"
        );
        assert_eq!(stats.documented_pub_items, 1);
        assert!((stats.coverage_pct - 100.0).abs() < 1.0);
    }

    #[test]
    fn counts_doctests_in_doc_comments() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
/// A function with a doctest.
///
/// ```
/// assert_eq!(1 + 1, 2);
/// ```
pub fn with_doctest() {}

/// A function with a no_run doctest (should not count).
///
/// ```no_run
/// panic!();
/// ```
pub fn with_no_run() {}

/// A function with an ignore doctest (should not count).
///
/// ```ignore
/// panic!();
/// ```
pub fn with_ignore() {}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(stats.doctests, 1, "should count exactly 1 runnable doctest");
        assert_eq!(stats.total_pub_items, 3);
        assert_eq!(stats.documented_pub_items, 3);
    }
}