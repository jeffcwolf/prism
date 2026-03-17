//! Dependency graph construction from `cargo metadata`.
//!
//! Invokes `cargo metadata` on a project, extracts the direct dependencies
//! and their metadata, computes the dependency tree structure including
//! maximum depth and per-dependency transitive counts.
//!
//! Both single-crate projects and virtual workspaces (no root package) are
//! supported. For virtual workspaces, dependencies are aggregated across all
//! workspace members; internal crate-to-crate edges are excluded.

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
///
/// Handles both single-crate projects (with a root package) and virtual
/// workspaces (no root package). For virtual workspaces, dependencies are
/// aggregated across all workspace members; internal workspace crate edges
/// are excluded from the results.
pub(crate) fn build_dependency_info(
    metadata: &cargo_metadata::Metadata,
) -> Result<(Vec<DirectDependency>, DependencyGraph), DepsError> {
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or_else(|| DepsError::MetadataError {
            message: "no dependency resolution found in metadata".to_string(),
        })?;

    // Build shared lookups used by both the single-root and virtual paths.
    let package_map: HashMap<&cargo_metadata::PackageId, &cargo_metadata::Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();

    let node_map: HashMap<&cargo_metadata::PackageId, &cargo_metadata::Node> =
        resolve.nodes.iter().map(|n| (&n.id, n)).collect();

    match resolve.root.as_ref() {
        // ── Single-crate project: use the root package directly ───────────────
        Some(root_id) => {
            let root_node = resolve
                .nodes
                .iter()
                .find(|n| &n.id == root_id)
                .ok_or_else(|| DepsError::MetadataError {
                    message: "root package not found in resolve graph".to_string(),
                })?;

            let root_package = package_map
                .get(root_id)
                .ok_or_else(|| DepsError::MetadataError {
                    message: "root package not found in package list".to_string(),
                })?;

            let direct_deps =
                extract_direct_dependencies(root_package, root_node, &package_map);

            let total_count = resolve.nodes.len().saturating_sub(1); // exclude root
            let direct_count = direct_deps.len();
            let max_depth = compute_max_depth(root_id, &node_map);
            let transitive_counts = compute_transitive_counts(root_node, &node_map);

            let graph =
                DependencyGraph::new(total_count, direct_count, max_depth, transitive_counts);

            Ok((direct_deps, graph))
        }

        // ── Virtual workspace: aggregate across all workspace members ─────────
        None => {
            let workspace_ids: HashSet<&cargo_metadata::PackageId> =
                metadata.workspace_members.iter().collect();

            build_dependency_info_for_virtual_workspace(
                metadata,
                &workspace_ids,
                &package_map,
                &node_map,
                resolve,
            )
        }
    }
}

/// Build dependency info for a virtual workspace by aggregating across all
/// workspace members. Internal crate-to-crate edges are excluded.
fn build_dependency_info_for_virtual_workspace<'a>(
    metadata: &'a cargo_metadata::Metadata,
    workspace_ids: &HashSet<&'a cargo_metadata::PackageId>,
    package_map: &HashMap<&'a cargo_metadata::PackageId, &'a cargo_metadata::Package>,
    node_map: &HashMap<&'a cargo_metadata::PackageId, &'a cargo_metadata::Node>,
    resolve: &'a cargo_metadata::Resolve,
) -> Result<(Vec<DirectDependency>, DependencyGraph), DepsError> {
    // Collect external direct deps from all workspace members, deduplicating
    // by name. The first occurrence of a given dep name wins.
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut direct_deps: Vec<DirectDependency> = Vec::new();
    let mut all_transitive_counts: HashMap<String, usize> = HashMap::new();

    for member_id in &metadata.workspace_members {
        let member_package = match package_map.get(member_id) {
            Some(p) => p,
            None => continue,
        };
        let member_node = match node_map.get(member_id) {
            Some(n) => n,
            None => continue,
        };

        let member_deps =
            extract_direct_dependencies(member_package, member_node, package_map);
        let member_transitive = compute_transitive_counts(member_node, node_map);

        for dep in member_deps {
            // Skip internal workspace crate dependencies.
            let is_internal = package_map.values().any(|p| {
                workspace_ids.contains(&p.id) && p.name == dep.name()
            });
            if is_internal {
                continue;
            }

            // Deduplicate: first workspace member that declares a dep wins.
            if seen_names.insert(dep.name().to_string()) {
                direct_deps.push(dep);
            }
        }

        // Accumulate transitive counts, keeping the maximum across members.
        for (name, count) in member_transitive {
            let is_internal = package_map.values().any(|p| {
                workspace_ids.contains(&p.id) && p.name == name
            });
            if !is_internal {
                let entry = all_transitive_counts.entry(name).or_insert(0);
                *entry = (*entry).max(count);
            }
        }
    }

    // Total count: all resolved packages that are not workspace members.
    let total_count = resolve
        .nodes
        .iter()
        .filter(|n| !workspace_ids.contains(&n.id))
        .count();

    let direct_count = direct_deps.len();

    // Max depth: the greatest depth reachable from any workspace member.
    let max_depth = metadata
        .workspace_members
        .iter()
        .map(|id| compute_max_depth(id, node_map))
        .max()
        .unwrap_or(0);

    let graph = DependencyGraph::new(
        total_count,
        direct_count,
        max_depth,
        all_transitive_counts,
    );

    Ok((direct_deps, graph))
}

/// Extract direct dependencies from a package manifest and its resolve node.
fn extract_direct_dependencies(
    root_package: &cargo_metadata::Package,
    root_node: &cargo_metadata::Node,
    package_map: &HashMap<&cargo_metadata::PackageId, &cargo_metadata::Package>,
) -> Vec<DirectDependency> {
    // Build a map from dep name → resolved package ID for quick lookup.
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

        direct_deps.push(DirectDependency::new(
            name.clone(),
            version,
            source,
            kind,
            dep.features.clone(),
            dep.uses_default_features,
        ));
    }

    direct_deps
}

/// Compute the maximum depth of the dependency tree using BFS from a root node.
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
        // Use prism-deps itself as a test subject (a non-virtual crate).
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
        assert!(!deps.is_empty(), "prism-deps should have direct dependencies");
        assert!(graph.direct_count() > 0, "should have at least one direct dependency");
        assert!(
            graph.total_count() >= graph.direct_count(),
            "total count should be >= direct count"
        );
    }

    #[test]
    fn build_dependency_info_succeeds_for_virtual_workspace() {
        // Use the prism workspace root (a virtual workspace) as the test subject.
        // CARGO_MANIFEST_DIR is prism-deps; the workspace root is two levels up.
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()  // crates/
            .and_then(|p| p.parent())  // workspace root
            .expect("workspace root should exist");

        let metadata = load_metadata(workspace_root);
        assert!(
            metadata.is_ok(),
            "should load metadata for virtual workspace root: {:?}",
            metadata.err()
        );

        let result = build_dependency_info(&metadata.unwrap());
        assert!(
            result.is_ok(),
            "virtual workspace dependency analysis should not fail: {:?}",
            result.err()
        );

        let (deps, graph) = result.unwrap();
        // The prism workspace has external dependencies
        assert!(!deps.is_empty(), "workspace should have external dependencies");
        assert!(graph.direct_count() > 0, "should report at least one direct dependency");
        assert!(graph.total_count() > 0, "should report transitive dependencies");

        // Internal workspace crates should not appear in the dep list
        let internal_names = ["prism-audit", "prism-deps", "prism-stats", "prism-map",
                               "prism-check", "prism-cli"];
        for internal in &internal_names {
            assert!(
                !deps.iter().any(|d| d.name() == *internal),
                "internal crate {internal} should not appear in external deps"
            );
        }
    }

    #[test]
    fn find_duplicates_on_prism_deps() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let metadata = load_metadata(path).expect("metadata should load");
        let duplicates = find_duplicates(&metadata);
        for dup in &duplicates {
            assert!(
                dup.versions().len() >= 2,
                "duplicate {} should have at least 2 versions",
                dup.name()
            );
        }
    }
}