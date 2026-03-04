use std::path::Path;

use syn::visit::Visit;

use crate::types::{SafetyStats, StatsError};

pub(crate) fn collect(path: &Path) -> Result<SafetyStats, StatsError> {
    let mut locations = Vec::new();

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| !is_excluded(e))
    {
        let entry = entry.map_err(|e| StatsError::file_read(path, std::io::Error::other(e)))?;

        let entry_path = entry.path();
        if entry_path.extension().map(|e| e == "rs").unwrap_or(false) {
            let content = std::fs::read_to_string(entry_path)
                .map_err(|e| StatsError::file_read(entry_path, e))?;

            let file = match syn::parse_file(&content) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let relative = entry_path
                .strip_prefix(path)
                .unwrap_or(entry_path)
                .to_string_lossy()
                .to_string();

            let mut visitor = UnsafeVisitor {
                file_path: relative,
                locations: &mut locations,
            };
            visitor.visit_file(&file);
        }
    }

    locations.sort();
    let unsafe_blocks = locations.len() as u64;
    Ok(SafetyStats {
        unsafe_blocks,
        locations,
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
    if name == "fixtures" {
        return entry.path().components().any(|c| c.as_os_str() == "tests");
    }
    false
}

struct UnsafeVisitor<'a> {
    file_path: String,
    locations: &'a mut Vec<String>,
}

impl<'ast, 'a> Visit<'ast> for UnsafeVisitor<'a> {
    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        let span = node.unsafe_token.span;
        let line = span.start().line;
        self.locations.push(format!("{}:{}", self.file_path, line));
        syn::visit::visit_expr_unsafe(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if let Some(unsafety) = &node.sig.unsafety {
            let line = unsafety.span.start().line;
            self.locations.push(format!("{}:{}", self.file_path, line));
        }
        syn::visit::visit_item_fn(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detects_unsafe_blocks_and_fns() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"pub fn safe() {}

pub fn with_unsafe() {
    unsafe {
        std::ptr::null::<u8>().read();
    }
}

unsafe fn dangerous() {}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.unsafe_blocks, 2,
            "should find unsafe block and unsafe fn"
        );
        assert!(
            stats.locations.iter().any(|l| l.contains("src/lib.rs:4")),
            "should find unsafe block at line 4"
        );
        assert!(
            stats.locations.iter().any(|l| l.contains("src/lib.rs:9")),
            "should find unsafe fn at line 9"
        );
    }

    #[test]
    fn excludes_test_fixtures_from_unsafe_count() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn safe() {}\n").unwrap();

        // Unsafe code inside tests/fixtures should be excluded
        fs::create_dir_all(root.join("tests/fixtures/sample/src")).unwrap();
        fs::write(
            root.join("tests/fixtures/sample/src/lib.rs"),
            "unsafe fn dangerous() {}\npub fn with_unsafe() { unsafe { std::ptr::null::<u8>().read(); } }\n",
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.unsafe_blocks, 0,
            "should not count unsafe code from test fixtures"
        );
    }

    #[test]
    fn no_unsafe_returns_empty() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn safe() {}\n").unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(stats.unsafe_blocks, 0);
        assert!(stats.locations.is_empty());
    }
}
