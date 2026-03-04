//! File discovery: recursively finds Rust source files while skipping
//! build artifacts and hidden directories.

use std::fs;
use std::path::{Path, PathBuf};

use crate::types::AuditError;

/// Recursively discover all `.rs` files under `root`, skipping `target/` and hidden directories.
pub(crate) fn discover_rust_files(root: &Path) -> Result<Vec<PathBuf>, AuditError> {
    if !root.is_dir() {
        return Err(AuditError::InvalidPath {
            path: root.display().to_string(),
        });
    }

    let mut files = Vec::new();
    collect_rust_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), AuditError> {
    let entries = fs::read_dir(dir).map_err(|source| AuditError::Io {
        path: dir.display().to_string(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| AuditError::Io {
            path: dir.display().to_string(),
            source,
        })?;
        let path = entry.path();

        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (name.starts_with('.') || name == "target")
        {
            continue;
        }

        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_temp_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // src/lib.rs
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn hello() {}").unwrap();

        // src/utils.rs
        fs::write(root.join("src/utils.rs"), "fn helper() {}").unwrap();

        // target/debug/build.rs — should be skipped
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::write(root.join("target/debug/build.rs"), "").unwrap();

        // .hidden/secret.rs — should be skipped
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join(".hidden/secret.rs"), "").unwrap();

        // not_rust.txt — should be skipped
        fs::write(root.join("not_rust.txt"), "hello").unwrap();

        dir
    }

    #[test]
    fn discovers_rs_files_skipping_target_and_hidden() {
        let dir = make_temp_project();
        let files = discover_rust_files(dir.path()).unwrap();

        let names: Vec<&str> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();

        assert!(names.contains(&"lib.rs"), "should find lib.rs");
        assert!(names.contains(&"utils.rs"), "should find utils.rs");
        assert_eq!(names.len(), 2, "should find exactly 2 .rs files");
    }

    #[test]
    fn returns_error_for_nonexistent_path() {
        let result = discover_rust_files(Path::new("/nonexistent/path/abc123"));
        assert!(result.is_err());
    }

    #[test]
    fn returns_error_for_file_not_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "hello").unwrap();

        let result = discover_rust_files(&file_path);
        assert!(result.is_err());
    }
}
