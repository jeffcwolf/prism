//! Dependency graph construction from `cargo metadata`.
//!
//! Invokes `cargo metadata` on a project, extracts the direct dependencies
//! and their metadata, computes the dependency tree structure including
//! maximum depth and per-dependency transitive counts.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use cargo_metadata::{DependencyKind as CargoDependencyKind, MetadataCommand};

use crate::types::{
    DependencyGraph, DependencyKind, DependencySource, DepsError, DirectDependency,
    DuplicateDependency,
};

/// Load cargo metadata for the project at the given path.
pub(crate) fn load_metadata(path: &Path) -> Result<cargo_metadata::Metadata, DepsError> {
    let manifest_path = path.join("Cargo.toml");
    if !manifest_path.exists() {
        return Err(DepsError::NotACargoProject {
            path: path.display().to_string(),
        });
    }

    MetadataCommand::new()
        .manifest_path(&manifest_path)
        .exec()
        .map_err(|e| DepsError::MetadataError {
            message: e.to_string(),
        })
}

/// Extract direct dependencies and compute dependency graph metrics.
pub(crate) fn build_dependency_info(
    metadata: &cargo_metadata::Metadata,
) -> Result<(Vec<DirectDependency>, DependencyGraph), DepsError> {
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or_else(|| DepsError::MetadataError {
            message: "no dependency resolution found in metadata".to_string(),
        })?;

    // Find the root package(s)
    let root_id = resolve
        .root
        .as_ref()
        .ok_or_else(|| DepsError::MetadataError {
            message: "no root package found; is this a virtual workspace?".to_string(),
        })?;

    let root_node = resolve
        .nodes
        .iter()
        .find(|n| &n.id == root_id)
        .ok_or_else(|| DepsError::MetadataError {
            message: "root package not found in resolve graph".to_string(),
        })?;

    // Build a lookup from package ID to package
    let package_map: HashMap<&cargo_metadata::PackageId, &cargo_metadata::Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();

    // Build a lookup from package ID to resolve node
    let node_map: HashMap<&cargo_metadata::PackageId, &cargo_metadata::Node> =
        resolve.nodes.iter().map(|n| (&n.id, n)).collect();

    // Extract direct dependencies from the root node
    let root_package = package_map
        .get(root_id)
        .ok_or_else(|| DepsError::MetadataError {
            message: "root package not found in package list".to_string(),
        })?;

    let direct_deps = extract_direct_dependencies(root_package, root_node, &package_map);

    // Compute graph metrics
    let total_count = resolve.nodes.len().saturating_sub(1); // exclude root
    let direct_count = direct_deps.len();

    // Compute max depth via BFS from root
    let max_depth = compute_max_depth(root_id, &node_map);

    // Compute transitive dependency count for each direct dependency
    let transitive_counts = compute_transitive_counts(root_node, &node_map);

    let graph = DependencyGraph::new(total_count, direct_count, max_depth, transitive_counts);

    Ok((direct_deps, graph))
}

/// Extract direct dependencies from the root package manifest and resolve data.
fn extract_direct_dependencies(
    root_package: &cargo_metadata::Package,
    root_node: &cargo_metadata::Node,
    package_map: &HashMap<&cargo_metadata::PackageId, &cargo_metadata::Package>,
) -> Vec<DirectDependency> {
    // Build a set of dep package IDs from the resolve node for quick lookup
    let resolved_dep_ids: HashMap<&str, &cargo_metadata::PackageId> = root_node
        .deps
        .iter()
        .map(|d| {
            let pkg = package_map.get(&d.pkg);
            let name = pkg.map(|p| p.name.as_str()).unwrap_or("");
            (name, &d.pkg)
        })
        .collect();

    let mut direct_deps = Vec::new();

    for dep in &root_package.dependencies {
        let name = &dep.name;

        // Determine the resolved version from the resolve graph
        let resolved_pkg = resolved_dep_ids
            .get(name.as_str())
            .and_then(|id| package_map.get(id));

        let version = resolved_pkg
            .map(|p| p.version.to_string())
            .unwrap_or_else(|| dep.req.to_string());

        let source = match &dep.path {
            Some(path) => DependencySource::Path {
                path: path.to_string(),
            },
            None => {
                if let Some(pkg) = resolved_pkg {
                    match &pkg.source {
                        Some(src) if src.repr.starts_with("git+") => DependencySource::Git {
                            url: src.repr.clone(),
                        },
                        _ => DependencySource::CratesIo,
                    }
                } else {
                    DependencySource::CratesIo
                }
            }
        };

        let kind = match dep.kind {
            CargoDependencyKind::Normal => DependencyKind::Normal,
            CargoDependencyKind::Development => DependencyKind::Dev,
            CargoDependencyKind::Build => DependencyKind::Build,
            _ => DependencyKind::Normal,
        };

        let features = dep.features.clone();
        let uses_default_features = dep.uses_default_features;

        direct_deps.push(DirectDependency::new(
            name.clone(),
            version,
            source,
            kind,
            features,
            uses_default_features,
        ));
    }

    direct_deps
}

/// Compute the maximum depth of the dependency tree using BFS.
fn compute_max_depth<'a>(
    root_id: &'a cargo_metadata::PackageId,
    node_map: &HashMap<&'a cargo_metadata::PackageId, &'a cargo_metadata::Node>,
) -> usize {
    let mut visited = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((root_id, 0usize));
    visited.insert(root_id);
    let mut max_depth = 0;

    while let Some((pkg_id, depth)) = queue.pop_front() {
        max_depth = max_depth.max(depth);
        if let Some(node) = node_map.get(pkg_id) {
            for dep in &node.deps {
                if visited.insert(&dep.pkg) {
                    queue.push_back((&dep.pkg, depth + 1));
                }
            }
        }
    }

    max_depth
}

/// Compute the number of transitive dependencies each direct dependency brings in.
fn compute_transitive_counts<'a>(
    root_node: &'a cargo_metadata::Node,
    node_map: &HashMap<&'a cargo_metadata::PackageId, &'a cargo_metadata::Node>,
) -> HashMap<String, usize> {
    let mut counts = HashMap::new();

    for dep in &root_node.deps {
        let name = dep.name.as_str();
        let mut visited = HashSet::new();
        visited.insert(&dep.pkg);
        count_transitive(&dep.pkg, node_map, &mut visited);
        // Subtract 1 because we don't count the direct dep itself
        counts.insert(name.to_string(), visited.len().saturating_sub(1));
    }

    counts
}

/// Recursively count all transitive dependencies reachable from a package.
fn count_transitive<'a>(
    pkg_id: &'a cargo_metadata::PackageId,
    node_map: &HashMap<&'a cargo_metadata::PackageId, &'a cargo_metadata::Node>,
    visited: &mut HashSet<&'a cargo_metadata::PackageId>,
) {
    if let Some(node) = node_map.get(pkg_id) {
        for dep in &node.deps {
            if visited.insert(&dep.pkg) {
                count_transitive(&dep.pkg, node_map, visited);
            }
        }
    }
}

/// Find crates that appear multiple times in the resolve graph with different versions.
pub(crate) fn find_duplicates(metadata: &cargo_metadata::Metadata) -> Vec<DuplicateDependency> {
    let mut crate_versions: HashMap<&str, Vec<String>> = HashMap::new();

    for package in &metadata.packages {
        crate_versions
            .entry(&package.name)
            .or_default()
            .push(package.version.to_string());
    }

    let mut duplicates: Vec<DuplicateDependency> = crate_versions
        .into_iter()
        .filter(|(_, versions)| versions.len() > 1)
        .map(|(name, mut versions)| {
            versions.sort();
            versions.dedup();
            DuplicateDependency::new(name.to_string(), versions)
        })
        .filter(|d| d.versions().len() > 1)
        .collect();

    duplicates.sort_by(|a, b| a.name().cmp(b.name()));
    duplicates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_metadata_fails_for_nonexistent_path() {
        let result = load_metadata(Path::new("/nonexistent/path/12345"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            DepsError::NotACargoProject { path } => {
                assert!(path.contains("nonexistent"), "error should include path");
            }
            other => panic!("expected NotACargoProject, got: {other}"),
        }
    }

    #[test]
    fn load_metadata_succeeds_for_prism_workspace_member() {
        // Use prism-deps itself as a test subject
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let metadata = load_metadata(path);
        assert!(
            metadata.is_ok(),
            "should parse metadata for prism-deps: {:?}",
            metadata.err()
        );
    }

    #[test]
    fn build_dependency_info_returns_deps_and_graph() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let metadata = load_metadata(path).expect("metadata should load");
        let result = build_dependency_info(&metadata);
        assert!(
            result.is_ok(),
            "should build dependency info: {:?}",
            result.err()
        );
        let (deps, graph) = result.unwrap();
        // prism-deps has several dependencies
        assert!(
            !deps.is_empty(),
            "prism-deps should have direct dependencies"
        );
        assert!(
            graph.direct_count() > 0,
            "should have at least one direct dependency"
        );
        assert!(
            graph.total_count() >= graph.direct_count(),
            "total count should be >= direct count"
        );
    }

    #[test]
    fn find_duplicates_on_prism_deps() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let metadata = load_metadata(path).expect("metadata should load");
        let duplicates = find_duplicates(&metadata);
        // Just verify the function runs without panicking and returns valid data
        for dup in &duplicates {
            assert!(
                dup.versions().len() >= 2,
                "duplicate {} should have at least 2 versions",
                dup.name()
            );
        }
    }
}
