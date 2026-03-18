use std::collections::HashSet;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

use crate::types::{StatsError, SurfaceStats};

pub(crate) fn collect(path: &Path) -> Result<SurfaceStats, StatsError> {
    let mut pub_items = 0u64;
    let mut total_items = 0u64;

    // Phase 1: identify files only reachable through feature-gated module
    // declarations, so we can skip them in Phase 2.
    let feature_gated_files = collect_feature_gated_module_files(path)?;

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| !is_excluded(e))
    {
        let entry = entry.map_err(|e| StatsError::file_read(path, std::io::Error::other(e)))?;

        if entry.path().extension().map(|e| e == "rs").unwrap_or(false) {
            if feature_gated_files.contains(entry.path()) {
                continue;
            }

            let content = std::fs::read_to_string(entry.path())
                .map_err(|e| StatsError::file_read(entry.path(), e))?;

            let file = match syn::parse_file(&content) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let mut visitor = SurfaceVisitor::default();
            visitor.visit_file(&file);
            pub_items += visitor.pub_items;
            total_items += visitor.total_items;
        }
    }

    let pub_ratio = if total_items > 0 {
        pub_items as f64 / total_items as f64
    } else {
        0.0
    };

    Ok(SurfaceStats {
        pub_items,
        total_items,
        pub_ratio,
    })
}

/// Walk the project tree and collect absolute paths of `.rs` files that are
/// only included via a feature-gated external module declaration.
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
                    if item_mod.content.is_some() {
                        continue;
                    }
                    if has_cfg_feature_attr(&item_mod.attrs) {
                        let mod_name = item_mod.ident.to_string();
                        let candidate_file = parent_dir.join(format!("{mod_name}.rs"));
                        let candidate_dir = parent_dir.join(&mod_name).join("mod.rs");

                        if candidate_file.is_file() {
                            gated.insert(candidate_file.canonicalize().unwrap_or(candidate_file));
                        }
                        if candidate_dir.is_file() {
                            gated.insert(candidate_dir.canonicalize().unwrap_or(candidate_dir));
                        }
                    }
                }
            }
        }
    }

    Ok(gated)
}

fn is_excluded(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_str().unwrap_or("");
    if name == "target" || name.starts_with('.') || name == "node_modules" {
        return true;
    }
    // Exclude integration test directories. Public items in tests/ inflate the
    // pub surface ratio; they are not part of the exported API.
    if name == "tests" && entry.file_type().is_dir() {
        return true;
    }
    if name == "fixtures" {
        return entry.path().components().any(|c| c.as_os_str() == "tests");
    }
    false
}

#[derive(Default)]
struct SurfaceVisitor {
    pub_items: u64,
    total_items: u64,
}

impl SurfaceVisitor {
    fn count_item(&mut self, vis: &syn::Visibility, attrs: &[syn::Attribute]) {
        // Items behind cfg(feature) are not part of the default API surface.
        if has_cfg_feature_attr(attrs) {
            return;
        }
        self.total_items += 1;
        if is_fully_public(vis) {
            self.pub_items += 1;
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
fn has_cfg_feature_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("feature") {
                found = true;
                let _ = meta.value().and_then(|v| v.parse::<syn::LitStr>());
            }
            Ok(())
        });
        found
    })
}

impl<'ast> Visit<'ast> for SurfaceVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.count_item(&node.vis, &node.attrs);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.count_item(&node.vis, &node.attrs);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.count_item(&node.vis, &node.attrs);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.count_item(&node.vis, &node.attrs);
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.count_item(&node.vis, &node.attrs);
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.count_item(&node.vis, &node.attrs);
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.count_item(&node.vis, &node.attrs);
        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Skip #[cfg(test)] modules — items inside them are not part of the
        // exported API surface.
        if has_cfg_test_attr(&node.attrs) {
            return;
        }
        // Skip #[cfg(feature = "…")] modules — not compiled by default.
        if has_cfg_feature_attr(&node.attrs) {
            return;
        }
        self.count_item(&node.vis, &node.attrs);
        syn::visit::visit_item_mod(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
        fs::write(
            root.join("src/lib.rs"),
            "pub fn public_fn() {}\nfn private_fn() {}\n",
        )
        .unwrap();
        // This pub item must not be counted in the pub surface ratio
        fs::write(
            root.join("tests/integration.rs"),
            "pub fn test_helper() {}\n",
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.pub_items, 1,
            "pub items in tests/ must not inflate pub_ratio"
        );
        assert_eq!(stats.total_items, 2, "only src/ items counted");
    }

    #[test]
    fn does_not_count_items_in_cfg_test_modules() {
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
pub fn public_fn() {}
fn private_fn() {}

#[cfg(test)]
mod tests {
    // These must not be counted in pub_items or total_items.
    pub fn test_helper() {}
    fn internal_helper() {}
}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.pub_items, 1,
            "cfg(test) pub items must not inflate pub_items"
        );
        assert_eq!(
            stats.total_items, 2,
            "cfg(test) items must not inflate total_items"
        );
    }

    #[test]
    fn does_not_count_items_in_cfg_feature_modules() {
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
pub fn public_fn() {}
fn private_fn() {}

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
            stats.pub_items, 1,
            "cfg(feature) pub items must not inflate pub_items"
        );
        assert_eq!(
            stats.total_items, 2,
            "cfg(feature) items must not inflate total_items"
        );
    }

    #[test]
    fn does_not_count_items_in_cfg_feature_external_module_file() {
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
pub fn public_fn() {}
fn private_fn() {}

#[cfg(feature = "test-utils")]
pub mod mock;
"#,
        )
        .unwrap();
        fs::write(
            root.join("src/mock.rs"),
            r#"
pub struct MockClient;
pub fn mock_helper() {}
fn internal() {}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.pub_items, 1,
            "pub items in feature-gated external module file must not be counted"
        );
        // total_items: pub fn + fn from lib.rs only (2), mock.rs items excluded
        assert_eq!(
            stats.total_items, 2,
            "items in feature-gated external module file must not be counted"
        );
    }

    #[test]
    fn counts_pub_and_total_items() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
pub fn public_fn() {}
fn private_fn() {}
pub struct PubStruct;
struct PrivateStruct;
pub(crate) fn crate_fn() {}
pub enum PubEnum { A }
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        // pub fn, pub struct, pub enum = 3 pub items
        // total: pub fn, fn, pub struct, struct, pub(crate) fn, pub enum = 6
        assert_eq!(stats.pub_items, 3, "should count 3 fully pub items");
        assert_eq!(stats.total_items, 6, "should count 6 total items");
        assert!(
            (stats.pub_ratio - 0.5).abs() < 0.01,
            "pub ratio should be 0.5"
        );
    }
}