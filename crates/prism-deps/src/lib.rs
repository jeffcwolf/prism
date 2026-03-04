//! Dependency health analysis for Rust projects.
//!
//! Analyzes a Rust project's dependency graph by invoking `cargo metadata`,
//! querying crates.io for staleness, running vulnerability checks, and
//! computing tree depth and bloat metrics. The public API is a single
//! entry point that returns a structured health report.

mod graph;
mod registry;
mod types;
mod vulnerability;

use std::path::Path;

pub use types::{
    DependencyGraph, DependencyHealth, DependencyKind, DependencySource, DepsError, DepsReport,
    DirectDependency, DuplicateDependency, HealthStatus, Vulnerability, VulnerabilitySeverity,
};

/// Analyze the dependency health of a Rust project at the given path.
///
/// Invokes `cargo metadata` on the project, queries crates.io for latest
/// versions, checks for known vulnerabilities, and computes dependency
/// tree metrics.
///
/// # Errors
///
/// Returns `DepsError` if the path does not contain a valid Cargo project,
/// if `cargo metadata` fails, or if network queries encounter errors.
pub fn analyze_dependencies(path: &Path) -> Result<DepsReport, DepsError> {
    let metadata = graph::load_metadata(path)?;
    let (direct_deps, dep_graph) = graph::build_dependency_info(&metadata)?;
    let duplicates = graph::find_duplicates(&metadata);

    let mut health_assessments = Vec::new();
    for dep in &direct_deps {
        let staleness = if dep.source().is_crates_io() {
            registry::check_staleness(dep.name(), dep.version())
        } else {
            None
        };

        let vulnerabilities = vulnerability::check_vulnerabilities(path, dep.name());

        let bloat_count = dep_graph.transitive_count_for(dep.name());

        health_assessments.push(DependencyHealth::new(
            dep.name().to_string(),
            staleness,
            vulnerabilities,
            bloat_count,
        ));
    }

    Ok(DepsReport::new(
        direct_deps,
        dep_graph,
        health_assessments,
        duplicates,
    ))
}
