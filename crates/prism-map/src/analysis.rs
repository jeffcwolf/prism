//! Module tree walking and source file analysis.
//!
//! Parses Rust source files to build module trees, collect import edges,
//! and identify entry points. Uses `syn` for syntactic analysis of `mod`
//! and `use` declarations.

use std::fs;
use std::path::Path;

use crate::types::{EntryPoint, EntryPointKind, ImportEdge, MapError, ModuleNode};

/// Build the module tree for a crate rooted at the given file.
///
/// Recursively follows `mod` declarations to discover the full module
/// hierarchy. The `crate_name` is used to construct fully-qualified
/// module paths.
pub(crate) fn build_module_tree(
    _crate_name: &str,
    root_file: &Path,
    depth_limit: Option<usize>,
) -> Result<ModuleNode, MapError> {
    build_module_node("crate", root_file, depth_limit, 0)
}

fn build_module_node(
    module_path: &str,
    file_path: &Path,
    depth_limit: Option<usize>,
    current_depth: usize,
) -> Result<ModuleNode, MapError> {
    let source = fs::read_to_string(file_path).map_err(|e| MapError::FileRead {
        path: file_path.display().to_string(),
        source: e,
    })?;

    let syntax = syn::parse_file(&source).map_err(|e| MapError::ParseError {
        path: file_path.display().to_string(),
        message: e.to_string(),
    })?;

    let mut node = ModuleNode::new(
        module_path.to_string(),
        Some(file_path.to_path_buf()),
        false,
    );

    let at_depth_limit = depth_limit.is_some_and(|limit| current_depth >= limit);
    if at_depth_limit {
        return Ok(node);
    }

    let parent_dir = file_path
        .parent()
        .expect("source file should have a parent directory");

    for item in &syntax.items {
        if let syn::Item::Mod(item_mod) = item {
            let mod_name = item_mod.ident.to_string();
            let child_module_path = format!("{module_path}::{mod_name}");

            if item_mod.content.is_some() {
                // Inline module: mod foo { ... }
                let mut child = ModuleNode::new(child_module_path, None, true);
                // Parse inline module items for nested mods
                if let Some((_, items)) = &item_mod.content {
                    collect_inline_children(
                        &mut child,
                        items,
                        parent_dir,
                        depth_limit,
                        current_depth + 1,
                    )?;
                }
                node.add_child(child);
            } else {
                // File module: mod foo; — resolve to foo.rs or foo/mod.rs
                let child = resolve_file_module(
                    &child_module_path,
                    &mod_name,
                    parent_dir,
                    file_path,
                    depth_limit,
                    current_depth + 1,
                )?;
                node.add_child(child);
            }
        }
    }

    Ok(node)
}

fn resolve_file_module(
    module_path: &str,
    mod_name: &str,
    parent_dir: &Path,
    source_file: &Path,
    depth_limit: Option<usize>,
    current_depth: usize,
) -> Result<ModuleNode, MapError> {
    // Standard Rust module resolution:
    // For src/lib.rs or src/main.rs: mod foo -> src/foo.rs or src/foo/mod.rs
    // For src/bar.rs: mod baz -> src/bar/baz.rs or src/bar/baz/mod.rs
    // For src/bar/mod.rs: mod baz -> src/bar/baz.rs or src/bar/baz/mod.rs
    let file_stem = source_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let search_dir = if file_stem == "lib" || file_stem == "main" || file_stem == "mod" {
        parent_dir.to_path_buf()
    } else {
        parent_dir.join(file_stem)
    };

    let as_file = search_dir.join(format!("{mod_name}.rs"));
    let as_dir = search_dir.join(mod_name).join("mod.rs");

    if as_file.exists() {
        build_module_node(module_path, &as_file, depth_limit, current_depth)
    } else if as_dir.exists() {
        build_module_node(module_path, &as_dir, depth_limit, current_depth)
    } else {
        // Module file not found — record as a leaf with no file
        Ok(ModuleNode::new(module_path.to_string(), None, false))
    }
}

fn collect_inline_children(
    _parent: &mut ModuleNode,
    _items: &[syn::Item],
    _parent_dir: &Path,
    _depth_limit: Option<usize>,
    _current_depth: usize,
) -> Result<(), MapError> {
    // Will be expanded later for nested inline mods
    Ok(())
}

/// Collect use-statement imports from a source file.
pub(crate) fn collect_imports(
    source_module: &str,
    file_path: &Path,
    workspace_crate_names: &[String],
) -> Result<Vec<ImportEdge>, MapError> {
    let source = fs::read_to_string(file_path).map_err(|e| MapError::FileRead {
        path: file_path.display().to_string(),
        source: e,
    })?;
    let syntax = syn::parse_file(&source).map_err(|e| MapError::ParseError {
        path: file_path.display().to_string(),
        message: e.to_string(),
    })?;

    let mut edges = Vec::new();
    for item in &syntax.items {
        if let syn::Item::Use(item_use) = item {
            collect_use_tree(
                source_module,
                &item_use.tree,
                &mut String::new(),
                workspace_crate_names,
                &mut edges,
            );
        }
    }
    Ok(edges)
}

fn collect_use_tree(
    source_module: &str,
    tree: &syn::UseTree,
    prefix: &mut String,
    workspace_crate_names: &[String],
    edges: &mut Vec<ImportEdge>,
) {
    match tree {
        syn::UseTree::Path(use_path) => {
            let seg = use_path.ident.to_string();
            let was_empty = prefix.is_empty();
            if !prefix.is_empty() {
                prefix.push_str("::");
            }
            prefix.push_str(&seg);
            collect_use_tree(
                source_module,
                &use_path.tree,
                prefix,
                workspace_crate_names,
                edges,
            );
            // Restore prefix
            if was_empty {
                prefix.clear();
            } else {
                let len = prefix.len() - seg.len() - 2;
                prefix.truncate(len);
            }
        }
        syn::UseTree::Name(use_name) => {
            let item_name = use_name.ident.to_string();
            let full_path = if prefix.is_empty() {
                item_name.clone()
            } else {
                format!("{prefix}::{item_name}")
            };
            let (target_module, items) = split_import_path(&full_path);
            let is_internal = is_internal_import(&full_path, workspace_crate_names);
            edges.push(ImportEdge::new(
                source_module.to_string(),
                target_module,
                vec![items],
                is_internal,
            ));
        }
        syn::UseTree::Group(use_group) => {
            for subtree in &use_group.items {
                collect_use_tree(source_module, subtree, prefix, workspace_crate_names, edges);
            }
        }
        syn::UseTree::Glob(_) => {
            let target = if prefix.is_empty() {
                "*".to_string()
            } else {
                prefix.clone()
            };
            let is_internal = is_internal_import(&target, workspace_crate_names);
            edges.push(ImportEdge::new(
                source_module.to_string(),
                target,
                vec!["*".to_string()],
                is_internal,
            ));
        }
        syn::UseTree::Rename(use_rename) => {
            let item_name = use_rename.ident.to_string();
            let full_path = if prefix.is_empty() {
                item_name.clone()
            } else {
                format!("{prefix}::{item_name}")
            };
            let (target_module, items) = split_import_path(&full_path);
            let is_internal = is_internal_import(&full_path, workspace_crate_names);
            edges.push(ImportEdge::new(
                source_module.to_string(),
                target_module,
                vec![items],
                is_internal,
            ));
        }
    }
}

fn split_import_path(path: &str) -> (String, String) {
    if let Some(pos) = path.rfind("::") {
        (path[..pos].to_string(), path[pos + 2..].to_string())
    } else {
        (path.to_string(), path.to_string())
    }
}

fn is_internal_import(path: &str, workspace_crate_names: &[String]) -> bool {
    if path.starts_with("crate::") || path.starts_with("self::") || path.starts_with("super::") {
        return true;
    }
    if path == "crate" || path == "self" || path == "super" {
        return true;
    }
    let root_segment = path.split("::").next().unwrap_or(path);
    workspace_crate_names.iter().any(|name| {
        // Cargo crate names use hyphens but Rust paths use underscores
        root_segment == name || root_segment == name.replace('-', "_")
    })
}

/// Identify entry points (fn main, pub items in lib roots) from a source file.
pub(crate) fn collect_entry_points(
    crate_name: &str,
    file_path: &Path,
    is_lib: bool,
) -> Result<Vec<EntryPoint>, MapError> {
    let source = fs::read_to_string(file_path).map_err(|e| MapError::FileRead {
        path: file_path.display().to_string(),
        source: e,
    })?;
    let syntax = syn::parse_file(&source).map_err(|e| MapError::ParseError {
        path: file_path.display().to_string(),
        message: e.to_string(),
    })?;

    let mut entry_points = Vec::new();
    for item in &syntax.items {
        match item {
            syn::Item::Fn(item_fn) => {
                let name = item_fn.sig.ident.to_string();
                if name == "main" && !is_lib {
                    entry_points.push(EntryPoint::new(
                        crate_name.to_string(),
                        name,
                        EntryPointKind::MainFn,
                        file_path.to_path_buf(),
                    ));
                } else if is_lib && matches!(item_fn.vis, syn::Visibility::Public(_)) {
                    entry_points.push(EntryPoint::new(
                        crate_name.to_string(),
                        name,
                        EntryPointKind::LibPubItem,
                        file_path.to_path_buf(),
                    ));
                }
            }
            syn::Item::Struct(s) if is_lib && matches!(s.vis, syn::Visibility::Public(_)) => {
                entry_points.push(EntryPoint::new(
                    crate_name.to_string(),
                    s.ident.to_string(),
                    EntryPointKind::LibPubItem,
                    file_path.to_path_buf(),
                ));
            }
            syn::Item::Enum(e) if is_lib && matches!(e.vis, syn::Visibility::Public(_)) => {
                entry_points.push(EntryPoint::new(
                    crate_name.to_string(),
                    e.ident.to_string(),
                    EntryPointKind::LibPubItem,
                    file_path.to_path_buf(),
                ));
            }
            syn::Item::Trait(t) if is_lib && matches!(t.vis, syn::Visibility::Public(_)) => {
                entry_points.push(EntryPoint::new(
                    crate_name.to_string(),
                    t.ident.to_string(),
                    EntryPointKind::LibPubItem,
                    file_path.to_path_buf(),
                ));
            }
            _ => {}
        }
    }
    Ok(entry_points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_crate(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("failed to create temp dir");
        for (path, content) in files {
            let full_path = dir.path().join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).expect("failed to create dirs");
            }
            fs::write(&full_path, content).expect("failed to write file");
        }
        dir
    }

    #[test]
    fn single_file_crate_has_root_node_only() {
        let dir = create_temp_crate(&[("src/main.rs", "fn main() {}")]);
        let root = build_module_tree("my-crate", &dir.path().join("src/main.rs"), None).unwrap();

        assert_eq!(root.module_path(), "crate");
        assert!(!root.is_inline());
        assert!(
            root.children().is_empty(),
            "single file crate should have no children"
        );
    }

    #[test]
    fn crate_with_one_file_module() {
        let dir = create_temp_crate(&[
            ("src/lib.rs", "mod foo;"),
            ("src/foo.rs", "pub fn bar() {}"),
        ]);
        let root = build_module_tree("my-crate", &dir.path().join("src/lib.rs"), None).unwrap();

        assert_eq!(root.module_path(), "crate");
        assert_eq!(root.children().len(), 1, "should have one child module");

        let child = &root.children()[0];
        assert_eq!(child.module_path(), "crate::foo");
        assert!(!child.is_inline());
        assert!(child.file_path().is_some());
    }

    #[test]
    fn nested_modules_via_directory() {
        let dir = create_temp_crate(&[
            ("src/lib.rs", "mod a;"),
            ("src/a/mod.rs", "mod b;"),
            ("src/a/b.rs", "pub fn deep() {}"),
        ]);
        let root = build_module_tree("my-crate", &dir.path().join("src/lib.rs"), None).unwrap();

        assert_eq!(root.children().len(), 1);
        let a = &root.children()[0];
        assert_eq!(a.module_path(), "crate::a");

        assert_eq!(a.children().len(), 1);
        let b = &a.children()[0];
        assert_eq!(b.module_path(), "crate::a::b");
        assert!(b.children().is_empty());
    }

    #[test]
    fn inline_module_detected() {
        let dir = create_temp_crate(&[("src/lib.rs", "mod tests { fn test_something() {} }")]);
        let root = build_module_tree("my-crate", &dir.path().join("src/lib.rs"), None).unwrap();

        assert_eq!(root.children().len(), 1);
        let tests_mod = &root.children()[0];
        assert_eq!(tests_mod.module_path(), "crate::tests");
        assert!(tests_mod.is_inline());
        assert_eq!(tests_mod.file_path(), None);
    }

    #[test]
    fn internal_import_classified_correctly() {
        let dir = create_temp_crate(&[(
            "src/lib.rs",
            "use crate::types::Config;\nuse serde::Serialize;\nmod types;",
        )]);
        let imports = collect_imports("crate", &dir.path().join("src/lib.rs"), &[]).unwrap();

        let internal: Vec<_> = imports.iter().filter(|i| i.is_internal()).collect();
        let external: Vec<_> = imports.iter().filter(|i| !i.is_internal()).collect();

        assert_eq!(internal.len(), 1, "crate::types::Config should be internal");
        assert_eq!(internal[0].target_module(), "crate::types");
        assert_eq!(internal[0].items(), &["Config"]);

        assert_eq!(external.len(), 1, "serde::Serialize should be external");
        assert_eq!(external[0].target_module(), "serde");
    }

    #[test]
    fn self_and_super_imports_are_internal() {
        let dir = create_temp_crate(&[("src/foo.rs", "use self::bar::Baz;\nuse super::Config;")]);
        let imports = collect_imports("crate::foo", &dir.path().join("src/foo.rs"), &[]).unwrap();

        assert!(
            imports.iter().all(|i| i.is_internal()),
            "self:: and super:: imports should be internal"
        );
    }

    #[test]
    fn workspace_crate_import_is_internal() {
        let dir = create_temp_crate(&[("src/lib.rs", "use beta::utils::helper;")]);
        let imports = collect_imports(
            "crate",
            &dir.path().join("src/lib.rs"),
            &["beta".to_string()],
        )
        .unwrap();

        assert_eq!(imports.len(), 1);
        assert!(
            imports[0].is_internal(),
            "workspace crate import should be internal"
        );
        assert_eq!(imports[0].target_module(), "beta::utils");
    }

    #[test]
    fn detect_main_fn_entry_point() {
        let dir = create_temp_crate(&[("src/main.rs", "fn main() { println!(\"hello\"); }")]);
        let eps = collect_entry_points("my-crate", &dir.path().join("src/main.rs"), false).unwrap();

        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].name(), "main");
        assert_eq!(eps[0].kind(), &EntryPointKind::MainFn);
    }

    #[test]
    fn detect_lib_pub_items() {
        let dir = create_temp_crate(&[(
            "src/lib.rs",
            "pub fn entry_point() {}\npub struct Config;\nfn private() {}",
        )]);
        let eps = collect_entry_points("my-crate", &dir.path().join("src/lib.rs"), true).unwrap();

        assert_eq!(
            eps.len(),
            2,
            "should find pub fn and pub struct but not private fn"
        );
        let names: Vec<&str> = eps.iter().map(|e| e.name()).collect();
        assert!(names.contains(&"entry_point"));
        assert!(names.contains(&"Config"));
        assert!(eps.iter().all(|e| e.kind() == &EntryPointKind::LibPubItem));
    }

    #[test]
    fn depth_limit_truncates_tree() {
        let dir = create_temp_crate(&[
            ("src/lib.rs", "mod a;"),
            ("src/a/mod.rs", "mod b;"),
            ("src/a/b.rs", "pub fn deep() {}"),
        ]);
        let root = build_module_tree("my-crate", &dir.path().join("src/lib.rs"), Some(1)).unwrap();

        // Depth 0 is root, depth 1 is 'a' — 'b' at depth 2 should be excluded
        assert_eq!(root.children().len(), 1);
        let a = &root.children()[0];
        assert_eq!(a.module_path(), "crate::a");
        assert!(
            a.children().is_empty(),
            "depth limit should prevent going deeper than 1"
        );
    }

    #[test]
    fn depth_limit_zero_shows_only_root() {
        let dir = create_temp_crate(&[("src/lib.rs", "mod a;"), ("src/a.rs", "pub fn foo() {}")]);
        let root = build_module_tree("my-crate", &dir.path().join("src/lib.rs"), Some(0)).unwrap();

        assert!(
            root.children().is_empty(),
            "depth 0 should show only root with no children"
        );
    }
}
