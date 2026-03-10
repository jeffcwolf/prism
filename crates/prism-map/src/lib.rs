//! Structural mapping of Rust codebases.
//!
//! Analyzes a Rust project to produce a structural map including module trees,
//! cross-module imports, crate dependency graphs, and entry points. The public
//! API is a single entry point that returns a structured map suitable for
//! human-readable, JSON, or Mermaid output.

mod analysis;
mod types;

use std::path::PathBuf;

use cargo_metadata::MetadataCommand;

pub use types::{
    CodebaseMap, CrateGraph, CrateGraphEdge, CrateGraphNode, DependencyEdgeKind, EntryPoint,
    EntryPointKind, ImportEdge, MapConfig, MapError, ModuleNode, ModuleTree,
};

/// Map the structure of a Rust codebase at the given path.
///
/// Analyzes module trees, cross-module imports, crate graphs (for workspaces),
/// and entry points to produce a complete structural map.
///
/// # Errors
///
/// Returns `MapError` if the path does not exist, is not a Rust project,
/// or if parsing fails.
pub fn map_codebase(config: &MapConfig) -> Result<CodebaseMap, MapError> {
    let path = config.path();
    if !path.exists() {
        return Err(MapError::InvalidPath {
            path: path.display().to_string(),
        });
    }

    let cargo_toml = path.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(MapError::NotACargoProject {
            path: path.display().to_string(),
        });
    }

    let metadata = MetadataCommand::new()
        .manifest_path(&cargo_toml)
        .exec()
        .map_err(|e| MapError::MetadataError {
            message: e.to_string(),
        })?;

    let workspace_members: Vec<_> = metadata
        .workspace_packages()
        .into_iter()
        .map(|p| (p.name.clone(), p.manifest_path.clone()))
        .collect();

    let workspace_crate_names: Vec<String> =
        workspace_members.iter().map(|(n, _)| n.clone()).collect();

    let is_workspace = workspace_members.len() > 1;

    let crate_graph = if is_workspace {
        Some(build_crate_graph(&metadata, &workspace_crate_names))
    } else {
        None
    };

    let mut module_trees = Vec::new();
    let mut all_imports = Vec::new();
    let mut all_entry_points = Vec::new();

    for (crate_name, manifest_path) in &workspace_members {
        let crate_dir = manifest_path
            .parent()
            .expect("manifest should have parent")
            .as_std_path();

        let lib_rs = crate_dir.join("src/lib.rs");
        let main_rs = crate_dir.join("src/main.rs");

        let root_files: Vec<(PathBuf, bool)> = [(lib_rs, true), (main_rs, false)]
            .into_iter()
            .filter(|(p, _)| p.exists())
            .collect();

        for (root_file, is_lib) in &root_files {
            let tree_root =
                analysis::build_module_tree(crate_name, root_file, config.depth_limit())?;

            // Collect imports and entry points by walking the tree
            collect_from_tree(
                crate_name,
                &tree_root,
                &workspace_crate_names,
                *is_lib,
                &mut all_imports,
                &mut all_entry_points,
            )?;

            module_trees.push(ModuleTree::new(crate_name.clone(), tree_root));
        }
    }

    Ok(CodebaseMap::new(
        crate_graph,
        module_trees,
        all_imports,
        all_entry_points,
    ))
}

fn collect_from_tree(
    crate_name: &str,
    node: &ModuleNode,
    workspace_crate_names: &[String],
    is_lib_root: bool,
    imports: &mut Vec<ImportEdge>,
    entry_points: &mut Vec<EntryPoint>,
) -> Result<(), MapError> {
    if let Some(file_path) = node.file_path() {
        let mut file_imports =
            analysis::collect_imports(node.module_path(), file_path, workspace_crate_names)?;
        imports.append(&mut file_imports);

        // Only collect entry points from the crate root file
        let is_root = node.module_path() == "crate";
        if is_root {
            let mut eps = analysis::collect_entry_points(crate_name, file_path, is_lib_root)?;
            entry_points.append(&mut eps);
        }
    }

    for child in node.children() {
        collect_from_tree(
            crate_name,
            child,
            workspace_crate_names,
            false,
            imports,
            entry_points,
        )?;
    }

    Ok(())
}

fn build_crate_graph(
    metadata: &cargo_metadata::Metadata,
    workspace_crate_names: &[String],
) -> CrateGraph {
    let workspace_packages: Vec<_> = metadata.workspace_packages();

    let nodes: Vec<CrateGraphNode> = workspace_packages
        .iter()
        .map(|p| {
            let crate_dir = p
                .manifest_path
                .parent()
                .expect("manifest should have parent")
                .as_std_path()
                .to_path_buf();
            CrateGraphNode::new(p.name.clone(), crate_dir)
        })
        .collect();

    let mut edges = Vec::new();
    for pkg in &workspace_packages {
        for dep in &pkg.dependencies {
            if workspace_crate_names.contains(&dep.name) {
                let kind = match dep.kind {
                    cargo_metadata::DependencyKind::Normal => DependencyEdgeKind::Normal,
                    cargo_metadata::DependencyKind::Development => DependencyEdgeKind::Dev,
                    cargo_metadata::DependencyKind::Build => DependencyEdgeKind::Build,
                    _ => DependencyEdgeKind::Normal,
                };
                edges.push(CrateGraphEdge::new(
                    pkg.name.clone(),
                    dep.name.clone(),
                    kind,
                ));
            }
        }
    }

    CrateGraph::new(nodes, edges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_single_crate_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        let cargo_toml = r#"
[package]
name = "test-crate"
version = "0.1.0"
edition = "2021"
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn hello() {}\nmod utils;\n",
        )
        .unwrap();
        fs::write(dir.path().join("src/utils.rs"), "pub fn help() {}\n").unwrap();
        dir
    }

    #[test]
    fn map_nonexistent_path_returns_error() {
        let config = MapConfig::new("/nonexistent/path/xyz");
        let result = map_codebase(&config);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), MapError::InvalidPath { .. }),
            "should return InvalidPath error"
        );
    }

    #[test]
    fn map_single_crate_produces_module_tree() {
        let dir = create_single_crate_project();
        let config = MapConfig::new(dir.path());
        let map = map_codebase(&config).unwrap();

        assert!(
            map.crate_graph().is_none(),
            "single crate should have no crate graph"
        );
        assert_eq!(map.module_trees().len(), 1);

        let tree = &map.module_trees()[0];
        assert_eq!(tree.crate_name(), "test-crate");
        assert_eq!(tree.root().module_path(), "crate");
        assert_eq!(tree.root().children().len(), 1);
        assert_eq!(tree.root().children()[0].module_path(), "crate::utils");
    }

    #[test]
    fn map_single_crate_finds_entry_points() {
        let dir = create_single_crate_project();
        let config = MapConfig::new(dir.path());
        let map = map_codebase(&config).unwrap();

        let pub_items: Vec<_> = map
            .entry_points()
            .iter()
            .filter(|e| e.kind() == &EntryPointKind::LibPubItem)
            .collect();
        assert!(
            pub_items.iter().any(|e| e.name() == "hello"),
            "should find pub fn hello as entry point"
        );
    }
}
