use std::path::Path;

use syn::visit::Visit;

use crate::types::{StatsError, SurfaceStats};

pub(crate) fn collect(path: &Path) -> Result<SurfaceStats, StatsError> {
    let mut pub_items = 0u64;
    let mut total_items = 0u64;

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| !is_excluded(e))
    {
        let entry = entry.map_err(|e| StatsError::file_read(path, std::io::Error::other(e)))?;

        if entry.path().extension().map(|e| e == "rs").unwrap_or(false) {
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
    fn count_item(&mut self, vis: &syn::Visibility) {
        self.total_items += 1;
        if is_fully_public(vis) {
            self.pub_items += 1;
        }
    }
}

fn is_fully_public(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

impl<'ast> Visit<'ast> for SurfaceVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.count_item(&node.vis);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.count_item(&node.vis);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.count_item(&node.vis);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.count_item(&node.vis);
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.count_item(&node.vis);
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.count_item(&node.vis);
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.count_item(&node.vis);
        syn::visit::visit_item_static(self, node);
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
        assert_eq!(stats.pub_items, 1, "pub items in tests/ must not inflate pub_ratio");
        assert_eq!(stats.total_items, 2, "only src/ items counted");
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
