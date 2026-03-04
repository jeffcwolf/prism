use std::path::Path;

use crate::types::{CodeStats, StatsError};

pub(crate) fn collect(path: &Path) -> Result<CodeStats, StatsError> {
    let root_cargo = path.join("Cargo.toml");
    let cargo_content =
        std::fs::read_to_string(&root_cargo).map_err(|e| StatsError::file_read(&root_cargo, e))?;

    let is_workspace = cargo_content.contains("[workspace]");

    let name = extract_project_name(&cargo_content, path);

    let mut files: u64 = 0;
    let mut rust_lines: u64 = 0;
    let mut crates: u64 = 0;

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| !is_excluded_dir(e))
    {
        let entry = entry.map_err(|e| StatsError::file_read(path, std::io::Error::other(e)))?;

        let entry_path = entry.path();

        if entry_path
            .file_name()
            .map(|f| f == "Cargo.toml")
            .unwrap_or(false)
            && entry_path != root_cargo
        {
            let content = std::fs::read_to_string(entry_path)
                .map_err(|e| StatsError::file_read(entry_path, e))?;
            if content.contains("[package]") {
                crates += 1;
            }
        }

        if entry_path.extension().map(|e| e == "rs").unwrap_or(false) {
            files += 1;
            let content = std::fs::read_to_string(entry_path)
                .map_err(|e| StatsError::file_read(entry_path, e))?;
            rust_lines += count_logical_lines(&content);
        }
    }

    // The root Cargo.toml with [package] counts as a crate too
    if cargo_content.contains("[package]") {
        crates += 1;
    }

    // If no sub-crates found and it's not a workspace, it's a single crate
    if crates == 0 {
        crates = 1;
    }

    Ok(CodeStats {
        name,
        is_workspace,
        rust_lines,
        files,
        crates,
    })
}

fn extract_project_name(cargo_content: &str, path: &Path) -> String {
    // Try to extract name from [package] or [workspace] section
    for line in cargo_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name")
            && let Some(value) = trimmed.split('=').nth(1)
        {
            let name = value.trim().trim_matches('"');
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    // Fall back to directory name
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn is_excluded_dir(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_str().unwrap_or("");
    name == "target" || name.starts_with('.') || name == "node_modules"
}

/// Counts logical lines: non-blank, non-comment lines.
pub(crate) fn count_logical_lines(source: &str) -> u64 {
    let mut count = 0u64;
    let mut in_block_comment = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        count += 1;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn count_logical_lines_skips_blanks_and_comments() {
        let source = r#"
// A comment
use std::io;

fn main() {
    /* block comment */
    println!("hello");
}
"#;
        assert_eq!(count_logical_lines(source), 4);
    }

    #[test]
    fn count_logical_lines_handles_multiline_block_comments() {
        let source = r#"
fn foo() {
    /*
     * multi
     * line
     */
    let x = 1;
}
"#;
        assert_eq!(count_logical_lines(source), 3);
    }

    #[test]
    fn collect_single_crate() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"test-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(stats.name, "test-crate");
        assert!(!stats.is_workspace);
        assert_eq!(stats.files, 1);
        assert_eq!(stats.rust_lines, 3);
        assert_eq!(stats.crates, 1);
    }

    #[test]
    fn collect_workspace() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n",
        )
        .unwrap();

        fs::create_dir_all(root.join("crates/a/src")).unwrap();
        fs::write(
            root.join("crates/a/Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(root.join("crates/a/src/lib.rs"), "pub fn a() {}\n").unwrap();

        fs::create_dir_all(root.join("crates/b/src")).unwrap();
        fs::write(
            root.join("crates/b/Cargo.toml"),
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/b/src/lib.rs"),
            "pub fn b() {}\npub fn b2() {}\n",
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert!(stats.is_workspace);
        assert_eq!(stats.files, 2);
        assert_eq!(stats.crates, 2);
        assert_eq!(stats.rust_lines, 3);
    }

    #[test]
    fn extract_name_from_workspace_dir() {
        let content = "[workspace]\nmembers = [\"a\"]\n";
        let name = extract_project_name(content, Path::new("/home/user/my-project"));
        assert_eq!(name, "my-project");
    }
}
