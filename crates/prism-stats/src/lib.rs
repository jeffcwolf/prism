//! Project statistics collection for Rust workspaces and crates.
//!
//! Provides a single entry point, [`collect_stats`], that analyses a Rust project
//! and returns a [`ProjectStats`] value containing code metrics, dependency counts,
//! test counts, documentation coverage, unsafe usage, public surface area, and
//! function complexity data.

mod code;
mod complexity;
mod deps;
mod docs;
mod safety;
mod surface;
mod tests_count;
mod types;

pub use types::{
    CodeStats, ComplexityStats, DepsStats, DocsStats, FunctionInfo, ProjectStats, SafetyStats,
    StatsConfig, StatsError, SurfaceStats, TestStats,
};

/// Collects all stats for a Rust project at the given path.
pub fn collect_stats(config: &StatsConfig) -> Result<ProjectStats, StatsError> {
    let path = config.path();

    if !path.join("Cargo.toml").exists() {
        return Err(StatsError::not_a_rust_project(path));
    }

    let code = code::collect(path)?;
    let docs = docs::collect(path)?;
    let safety = safety::collect(path)?;
    let surface = surface::collect(path)?;
    let complexity = complexity::collect(path)?;
    let tests = tests_count::collect(path)?;

    let deps = if config.skip_deps() {
        None
    } else {
        Some(deps::collect(path)?)
    };

    Ok(ProjectStats::new(
        code, deps, tests, docs, safety, surface, complexity,
    ))
}
