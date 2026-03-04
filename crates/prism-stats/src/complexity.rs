use std::path::Path;

use syn::visit::Visit;

use crate::types::{ComplexityStats, FunctionInfo, StatsError};

pub(crate) fn collect(path: &Path) -> Result<ComplexityStats, StatsError> {
    let mut functions: Vec<FunctionInfo> = Vec::new();

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

            let lines: Vec<&str> = content.lines().collect();
            let mut visitor = ComplexityVisitor {
                file_path: &relative,
                source_lines: &lines,
                functions: &mut functions,
            };
            visitor.visit_file(&file);
        }
    }

    // Sort by lines descending
    functions.sort_by(|a, b| b.lines.cmp(&a.lines));

    let fns_over_50_lines = functions.iter().filter(|f| f.lines > 50).count() as u64;

    let (max_fn_lines, max_fn_name, max_fn_location) = functions
        .first()
        .map(|f| (f.lines, f.name.clone(), f.location.clone()))
        .unwrap_or((0, String::new(), String::new()));

    let top_functions: Vec<FunctionInfo> = functions.into_iter().take(10).collect();

    Ok(ComplexityStats {
        max_fn_lines,
        max_fn_name,
        max_fn_location,
        fns_over_50_lines,
        top_functions,
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

struct ComplexityVisitor<'a> {
    file_path: &'a str,
    source_lines: &'a [&'a str],
    functions: &'a mut Vec<FunctionInfo>,
}

impl<'ast, 'a> Visit<'ast> for ComplexityVisitor<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        self.record_function(&name, node.block.brace_token.span.open());
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        self.record_function(&name, node.block.brace_token.span.open());
        syn::visit::visit_impl_item_fn(self, node);
    }
}

impl<'a> ComplexityVisitor<'a> {
    fn record_function(&mut self, name: &str, open_brace: proc_macro2::Span) {
        let start_line = open_brace.start().line;
        let end_line = open_brace.end().line;

        // For proc_macro2 spans, the brace token span covers the whole block
        // We need to count logical lines between the braces
        // However, with the default span behavior, we may only get the open brace line
        // Use a heuristic: find the matching close brace by looking at the source
        let logical_lines = count_fn_logical_lines(self.source_lines, start_line, end_line);

        let display_location = format!("{}:{}", self.file_path, name);

        self.functions.push(FunctionInfo {
            name: name.to_string(),
            location: display_location,
            lines: logical_lines,
        });
    }
}

/// Count logical lines in a function body.
/// `start_line` is 1-indexed (the opening brace line).
fn count_fn_logical_lines(source_lines: &[&str], start_line: usize, end_line: usize) -> u64 {
    // If span gives us a proper range, use it
    if end_line > start_line {
        let mut count = 0u64;
        let mut in_block_comment = false;
        for line_idx in start_line..end_line.saturating_sub(1) {
            if line_idx >= source_lines.len() {
                break;
            }
            let trimmed = source_lines[line_idx].trim();

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
        return count;
    }

    // Fallback: scan forward from start_line to find the function body extent
    // by tracking brace depth
    if start_line == 0 || start_line > source_lines.len() {
        return 0;
    }

    let mut depth = 0i32;
    let mut found_open = false;
    let mut body_start = start_line; // 1-indexed
    let mut body_end = start_line;

    for (idx, line) in source_lines.iter().enumerate().skip(start_line - 1) {
        for ch in line.chars() {
            if ch == '{' {
                if !found_open {
                    found_open = true;
                    body_start = idx + 2; // line after the opening brace (1-indexed)
                }
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if found_open && depth == 0 {
                    body_end = idx; // 0-indexed, the closing brace line
                    break;
                }
            }
        }
        if found_open && depth == 0 {
            break;
        }
    }

    if !found_open || body_start > body_end {
        return 0;
    }

    let mut count = 0u64;
    let mut in_block_comment = false;
    for idx in (body_start - 1)..body_end {
        if idx >= source_lines.len() {
            break;
        }
        let trimmed = source_lines[idx].trim();

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
    fn counts_function_lines() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"pub fn short() {
    let x = 1;
    let y = 2;
}

pub fn longer() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
    let f = 6;
    let g = 7;
}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert!(stats.max_fn_lines > 0, "should find functions");
        assert_eq!(
            stats.max_fn_name, "longer",
            "longest function should be 'longer'"
        );
        assert!(
            stats.max_fn_lines > 2,
            "longer should have more lines than short"
        );
    }

    #[test]
    fn identifies_fns_over_50_lines() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();

        let mut big_fn = String::from("pub fn big() {\n");
        for i in 0..60 {
            big_fn.push_str(&format!("    let x{i} = {i};\n"));
        }
        big_fn.push_str("}\n\npub fn small() {\n    let x = 1;\n}\n");

        fs::write(root.join("src/lib.rs"), &big_fn).unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(
            stats.fns_over_50_lines, 1,
            "should find 1 function over 50 lines"
        );
        assert_eq!(stats.max_fn_name, "big");
    }

    #[test]
    fn top_functions_sorted_descending() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"fn small() {
    let x = 1;
}

fn medium() {
    let a = 1;
    let b = 2;
    let c = 3;
}

fn large() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
}
"#,
        )
        .unwrap();

        let stats = collect(root).unwrap();
        assert_eq!(stats.top_functions[0].name, "large");
        assert_eq!(stats.top_functions[1].name, "medium");
        assert_eq!(stats.top_functions[2].name, "small");
    }
}
