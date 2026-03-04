//! Core types for dependency health analysis.
//!
//! All public types use opaque fields with accessor methods, following
//! the information-hiding principle. Internal construction uses `pub(crate)`
//! constructors.

use std::collections::HashMap;
use std::fmt;

/// Errors that can occur during dependency analysis.
#[derive(Debug, thiserror::Error)]
pub enum DepsError {
    /// The provided path does not contain a valid Cargo project.
    #[error("not a cargo project: {path}")]
    NotACargoProject { path: String },

    /// `cargo metadata` invocation failed.
    #[error("cargo metadata failed: {message}")]
    MetadataError { message: String },

    /// An I/O error occurred while accessing the project.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

/// The source from which a dependency is resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    /// Published on crates.io.
    CratesIo,
    /// Sourced from a git repository.
    Git { url: String },
    /// A local path dependency.
    Path { path: String },
}

impl DependencySource {
    /// Returns true if this dependency comes from crates.io.
    pub fn is_crates_io(&self) -> bool {
        matches!(self, DependencySource::CratesIo)
    }
}

impl fmt::Display for DependencySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DependencySource::CratesIo => write!(f, "crates.io"),
            DependencySource::Git { url } => write!(f, "git: {url}"),
            DependencySource::Path { path } => write!(f, "path: {path}"),
        }
    }
}

/// The kind of dependency (normal, dev, or build).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Normal,
    Dev,
    Build,
}

impl fmt::Display for DependencyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DependencyKind::Normal => write!(f, "normal"),
            DependencyKind::Dev => write!(f, "dev"),
            DependencyKind::Build => write!(f, "build"),
        }
    }
}

/// A direct dependency of the analyzed project.
#[derive(Debug, Clone)]
pub struct DirectDependency {
    name: String,
    version: String,
    source: DependencySource,
    kind: DependencyKind,
    features: Vec<String>,
    uses_default_features: bool,
}

impl DirectDependency {
    pub(crate) fn new(
        name: String,
        version: String,
        source: DependencySource,
        kind: DependencyKind,
        features: Vec<String>,
        uses_default_features: bool,
    ) -> Self {
        Self {
            name,
            version,
            source,
            kind,
            features,
            uses_default_features,
        }
    }

    /// The crate name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The resolved version string.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Where this dependency is sourced from.
    pub fn source(&self) -> &DependencySource {
        &self.source
    }

    /// Whether this is a normal, dev, or build dependency.
    pub fn kind(&self) -> DependencyKind {
        self.kind
    }

    /// The features explicitly enabled for this dependency.
    pub fn features(&self) -> &[String] {
        &self.features
    }

    /// Whether default features are enabled.
    pub fn uses_default_features(&self) -> bool {
        self.uses_default_features
    }
}

/// Severity of a known vulnerability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VulnerabilitySeverity {
    /// Informational or low severity.
    Low,
    /// Medium severity.
    Medium,
    /// High severity.
    High,
    /// Critical severity.
    Critical,
}

impl fmt::Display for VulnerabilitySeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VulnerabilitySeverity::Low => write!(f, "low"),
            VulnerabilitySeverity::Medium => write!(f, "medium"),
            VulnerabilitySeverity::High => write!(f, "high"),
            VulnerabilitySeverity::Critical => write!(f, "critical"),
        }
    }
}

/// A known vulnerability affecting a dependency.
#[derive(Debug, Clone)]
pub struct Vulnerability {
    advisory_id: String,
    severity: VulnerabilitySeverity,
    title: String,
}

impl Vulnerability {
    pub(crate) fn new(advisory_id: String, severity: VulnerabilitySeverity, title: String) -> Self {
        Self {
            advisory_id,
            severity,
            title,
        }
    }

    /// The advisory identifier (e.g., RUSTSEC-2024-0001).
    pub fn advisory_id(&self) -> &str {
        &self.advisory_id
    }

    /// The severity level.
    pub fn severity(&self) -> VulnerabilitySeverity {
        self.severity
    }

    /// A human-readable title describing the vulnerability.
    pub fn title(&self) -> &str {
        &self.title
    }
}

/// Staleness information for a dependency.
#[derive(Debug, Clone)]
pub struct StalenessInfo {
    current_version: String,
    latest_version: String,
    is_major_behind: bool,
}

impl StalenessInfo {
    pub(crate) fn new(
        current_version: String,
        latest_version: String,
        is_major_behind: bool,
    ) -> Self {
        Self {
            current_version,
            latest_version,
            is_major_behind,
        }
    }

    /// The currently used version.
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// The latest version available on crates.io.
    pub fn latest_version(&self) -> &str {
        &self.latest_version
    }

    /// Whether the current version is more than one major version behind.
    pub fn is_major_behind(&self) -> bool {
        self.is_major_behind
    }
}

/// Overall health status of a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthStatus {
    /// No issues detected.
    Healthy,
    /// The dependency is outdated.
    Stale,
    /// The dependency pulls in many transitive dependencies.
    Bloated,
    /// The dependency has known vulnerabilities.
    Vulnerable,
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Stale => write!(f, "stale"),
            HealthStatus::Bloated => write!(f, "bloated"),
            HealthStatus::Vulnerable => write!(f, "vulnerable"),
        }
    }
}

/// Health assessment for a single dependency.
#[derive(Debug, Clone)]
pub struct DependencyHealth {
    name: String,
    staleness: Option<StalenessInfo>,
    vulnerabilities: Vec<Vulnerability>,
    transitive_count: usize,
}

impl DependencyHealth {
    /// Threshold above which a dependency is considered bloated.
    const BLOAT_THRESHOLD: usize = 50;

    pub(crate) fn new(
        name: String,
        staleness: Option<StalenessInfo>,
        vulnerabilities: Vec<Vulnerability>,
        transitive_count: usize,
    ) -> Self {
        Self {
            name,
            staleness,
            vulnerabilities,
            transitive_count,
        }
    }

    /// The dependency name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The overall health status, determined by the worst condition.
    pub fn status(&self) -> HealthStatus {
        if !self.vulnerabilities.is_empty() {
            return HealthStatus::Vulnerable;
        }
        if self.transitive_count >= Self::BLOAT_THRESHOLD {
            return HealthStatus::Bloated;
        }
        if self.staleness.is_some() {
            return HealthStatus::Stale;
        }
        HealthStatus::Healthy
    }

    /// Staleness information, if the dependency is outdated.
    pub fn staleness(&self) -> Option<&StalenessInfo> {
        self.staleness.as_ref()
    }

    /// Known vulnerabilities affecting this dependency.
    pub fn vulnerabilities(&self) -> &[Vulnerability] {
        &self.vulnerabilities
    }

    /// Number of transitive dependencies pulled in.
    pub fn transitive_count(&self) -> usize {
        self.transitive_count
    }
}

/// A duplicate dependency: same crate name, different versions in the tree.
#[derive(Debug, Clone)]
pub struct DuplicateDependency {
    name: String,
    versions: Vec<String>,
}

impl DuplicateDependency {
    pub(crate) fn new(name: String, versions: Vec<String>) -> Self {
        Self { name, versions }
    }

    /// The crate name that appears multiple times.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The distinct versions present in the dependency tree.
    pub fn versions(&self) -> &[String] {
        &self.versions
    }
}

/// Metrics about the dependency graph structure.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    total_count: usize,
    direct_count: usize,
    max_depth: usize,
    transitive_counts: HashMap<String, usize>,
}

impl DependencyGraph {
    pub(crate) fn new(
        total_count: usize,
        direct_count: usize,
        max_depth: usize,
        transitive_counts: HashMap<String, usize>,
    ) -> Self {
        Self {
            total_count,
            direct_count,
            max_depth,
            transitive_counts,
        }
    }

    /// Total number of dependencies (direct + transitive).
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    /// Number of direct dependencies.
    pub fn direct_count(&self) -> usize {
        self.direct_count
    }

    /// Maximum depth of the dependency tree.
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Number of transitive dependencies pulled in by a specific direct dependency.
    pub fn transitive_count_for(&self, name: &str) -> usize {
        self.transitive_counts.get(name).copied().unwrap_or(0)
    }
}

/// The complete dependency health report.
#[derive(Debug)]
pub struct DepsReport {
    dependencies: Vec<DirectDependency>,
    graph: DependencyGraph,
    health: Vec<DependencyHealth>,
    duplicates: Vec<DuplicateDependency>,
}

impl DepsReport {
    pub(crate) fn new(
        dependencies: Vec<DirectDependency>,
        graph: DependencyGraph,
        health: Vec<DependencyHealth>,
        duplicates: Vec<DuplicateDependency>,
    ) -> Self {
        Self {
            dependencies,
            graph,
            health,
            duplicates,
        }
    }

    /// The direct dependencies of the project.
    pub fn dependencies(&self) -> &[DirectDependency] {
        &self.dependencies
    }

    /// Dependency graph metrics.
    pub fn graph(&self) -> &DependencyGraph {
        &self.graph
    }

    /// Health assessments for each direct dependency.
    pub fn health(&self) -> &[DependencyHealth] {
        &self.health
    }

    /// Dependencies that appear multiple times with different versions.
    pub fn duplicates(&self) -> &[DuplicateDependency] {
        &self.duplicates
    }

    /// Whether all dependencies are healthy.
    pub fn is_healthy(&self) -> bool {
        self.health
            .iter()
            .all(|h| h.status() == HealthStatus::Healthy)
            && self.duplicates.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_source_display() {
        assert_eq!(DependencySource::CratesIo.to_string(), "crates.io");
        assert_eq!(
            DependencySource::Git {
                url: "https://github.com/foo/bar".to_string()
            }
            .to_string(),
            "git: https://github.com/foo/bar"
        );
        assert_eq!(
            DependencySource::Path {
                path: "../local".to_string()
            }
            .to_string(),
            "path: ../local"
        );
    }

    #[test]
    fn dependency_source_is_crates_io() {
        assert!(DependencySource::CratesIo.is_crates_io());
        assert!(
            !DependencySource::Git {
                url: "x".to_string()
            }
            .is_crates_io()
        );
        assert!(
            !DependencySource::Path {
                path: "x".to_string()
            }
            .is_crates_io()
        );
    }

    #[test]
    fn direct_dependency_accessors() {
        let dep = DirectDependency::new(
            "serde".to_string(),
            "1.0.200".to_string(),
            DependencySource::CratesIo,
            DependencyKind::Normal,
            vec!["derive".to_string()],
            true,
        );

        assert_eq!(dep.name(), "serde");
        assert_eq!(dep.version(), "1.0.200");
        assert!(dep.source().is_crates_io());
        assert_eq!(dep.kind(), DependencyKind::Normal);
        assert_eq!(dep.features(), &["derive"]);
        assert!(dep.uses_default_features());
    }

    #[test]
    fn health_status_healthy_when_no_issues() {
        let health = DependencyHealth::new("serde".to_string(), None, vec![], 10);
        assert_eq!(health.status(), HealthStatus::Healthy);
    }

    #[test]
    fn health_status_vulnerable_takes_priority() {
        let vuln = Vulnerability::new(
            "RUSTSEC-2024-0001".to_string(),
            VulnerabilitySeverity::High,
            "test vuln".to_string(),
        );
        let health = DependencyHealth::new(
            "bad-crate".to_string(),
            Some(StalenessInfo::new(
                "0.1.0".to_string(),
                "2.0.0".to_string(),
                true,
            )),
            vec![vuln],
            100,
        );
        assert_eq!(
            health.status(),
            HealthStatus::Vulnerable,
            "vulnerable should take priority over stale and bloated"
        );
    }

    #[test]
    fn health_status_bloated_over_stale() {
        let health = DependencyHealth::new(
            "heavy-crate".to_string(),
            Some(StalenessInfo::new(
                "0.1.0".to_string(),
                "0.2.0".to_string(),
                false,
            )),
            vec![],
            60,
        );
        assert_eq!(
            health.status(),
            HealthStatus::Bloated,
            "bloated should take priority over stale"
        );
    }

    #[test]
    fn health_status_stale_when_only_outdated() {
        let health = DependencyHealth::new(
            "old-crate".to_string(),
            Some(StalenessInfo::new(
                "0.1.0".to_string(),
                "0.2.0".to_string(),
                false,
            )),
            vec![],
            5,
        );
        assert_eq!(health.status(), HealthStatus::Stale);
    }

    #[test]
    fn dependency_graph_transitive_count_for_unknown_returns_zero() {
        let graph = DependencyGraph::new(10, 3, 4, HashMap::new());
        assert_eq!(graph.transitive_count_for("nonexistent"), 0);
    }

    #[test]
    fn deps_report_is_healthy_when_all_good() {
        let graph = DependencyGraph::new(1, 1, 1, HashMap::new());
        let health = vec![DependencyHealth::new("a".to_string(), None, vec![], 0)];
        let report = DepsReport::new(vec![], graph, health, vec![]);
        assert!(report.is_healthy());
    }

    #[test]
    fn deps_report_not_healthy_with_duplicates() {
        let graph = DependencyGraph::new(2, 1, 1, HashMap::new());
        let health = vec![DependencyHealth::new("a".to_string(), None, vec![], 0)];
        let duplicates = vec![DuplicateDependency::new(
            "syn".to_string(),
            vec!["1.0.0".to_string(), "2.0.0".to_string()],
        )];
        let report = DepsReport::new(vec![], graph, health, duplicates);
        assert!(
            !report.is_healthy(),
            "duplicates should make report unhealthy"
        );
    }

    #[test]
    fn vulnerability_accessors() {
        let v = Vulnerability::new(
            "RUSTSEC-2024-0042".to_string(),
            VulnerabilitySeverity::Critical,
            "Remote code execution".to_string(),
        );
        assert_eq!(v.advisory_id(), "RUSTSEC-2024-0042");
        assert_eq!(v.severity(), VulnerabilitySeverity::Critical);
        assert_eq!(v.title(), "Remote code execution");
    }

    #[test]
    fn duplicate_dependency_accessors() {
        let dup = DuplicateDependency::new(
            "syn".to_string(),
            vec!["1.0.109".to_string(), "2.0.48".to_string()],
        );
        assert_eq!(dup.name(), "syn");
        assert_eq!(dup.versions().len(), 2);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn empty_health_list_is_always_healthy(
            total in 0usize..100,
            direct in 0usize..100,
            depth in 0usize..20,
        ) {
            let graph = DependencyGraph::new(total, direct, depth, HashMap::new());
            let report = DepsReport::new(vec![], graph, vec![], vec![]);
            prop_assert!(report.is_healthy());
        }

        #[test]
        fn adding_dependency_increases_count(
            base_count in 0usize..100,
            extra in 1usize..50,
        ) {
            let graph1 = DependencyGraph::new(base_count, base_count, 1, HashMap::new());
            let graph2 = DependencyGraph::new(base_count + extra, base_count + extra, 1, HashMap::new());
            prop_assert!(graph2.total_count() > graph1.total_count());
            prop_assert!(graph2.direct_count() > graph1.direct_count());
        }

        #[test]
        fn vulnerability_always_makes_unhealthy(
            transitive in 0usize..100,
        ) {
            let vuln = Vulnerability::new(
                "RUSTSEC-TEST".to_string(),
                VulnerabilitySeverity::High,
                "test".to_string(),
            );
            let health = DependencyHealth::new(
                "crate".to_string(),
                None,
                vec![vuln],
                transitive,
            );
            prop_assert_eq!(health.status(), HealthStatus::Vulnerable);
        }

        #[test]
        fn bloat_threshold_is_50(count in 50usize..1000) {
            let health = DependencyHealth::new(
                "crate".to_string(),
                None,
                vec![],
                count,
            );
            prop_assert_eq!(health.status(), HealthStatus::Bloated);
        }

        #[test]
        fn below_bloat_threshold_without_issues_is_healthy(count in 0usize..50) {
            let health = DependencyHealth::new(
                "crate".to_string(),
                None,
                vec![],
                count,
            );
            prop_assert_eq!(health.status(), HealthStatus::Healthy);
        }
    }
}
