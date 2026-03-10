//! Core types for codebase structural mapping.
//!
//! All public types use opaque fields with accessor methods, following
//! the information-hiding principle. Internal construction uses `pub(crate)`
//! constructors.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Errors that can occur during codebase mapping.
#[derive(Debug, thiserror::Error)]
pub enum MapError {
    /// The provided path does not exist.
    #[error("path does not exist: {path}")]
    InvalidPath { path: String },

    /// The provided path does not contain a Cargo project.
    #[error("not a cargo project: {path}")]
    NotACargoProject { path: String },

    /// `cargo metadata` invocation failed.
    #[error("cargo metadata failed: {message}")]
    MetadataError { message: String },

    /// Failed to read a source file.
    #[error("failed to read {path}: {source}")]
    FileRead {
        path: String,
        source: std::io::Error,
    },

    /// Failed to parse a source file.
    #[error("parse error in {path}: {message}")]
    ParseError { path: String, message: String },
}

/// Configuration for the map analysis.
#[derive(Debug, Clone)]
pub struct MapConfig {
    path: PathBuf,
    depth_limit: Option<usize>,
}

impl MapConfig {
    /// Create a new configuration for the given codebase path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            depth_limit: None,
        }
    }

    /// Set the maximum module tree depth to report.
    pub fn with_depth_limit(mut self, depth: usize) -> Self {
        self.depth_limit = Some(depth);
        self
    }

    /// The codebase path to analyze.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The maximum module tree depth, if set.
    pub fn depth_limit(&self) -> Option<usize> {
        self.depth_limit
    }
}

/// The kind of dependency between workspace crates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyEdgeKind {
    Normal,
    Dev,
    Build,
}

impl fmt::Display for DependencyEdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DependencyEdgeKind::Normal => write!(f, "normal"),
            DependencyEdgeKind::Dev => write!(f, "dev"),
            DependencyEdgeKind::Build => write!(f, "build"),
        }
    }
}

/// A workspace crate node in the crate graph.
#[derive(Debug, Clone, Serialize)]
pub struct CrateGraphNode {
    name: String,
    path: PathBuf,
}

impl CrateGraphNode {
    pub(crate) fn new(name: String, path: PathBuf) -> Self {
        Self { name, path }
    }

    /// The crate name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The path to the crate root directory.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A dependency edge between workspace crates.
#[derive(Debug, Clone, Serialize)]
pub struct CrateGraphEdge {
    from: String,
    to: String,
    kind: DependencyEdgeKind,
}

impl CrateGraphEdge {
    pub(crate) fn new(from: String, to: String, kind: DependencyEdgeKind) -> Self {
        Self { from, to, kind }
    }

    /// The depending crate name.
    pub fn from(&self) -> &str {
        &self.from
    }

    /// The depended-upon crate name.
    pub fn to(&self) -> &str {
        &self.to
    }

    /// The kind of dependency.
    pub fn kind(&self) -> DependencyEdgeKind {
        self.kind
    }
}

/// The crate dependency graph for a workspace.
#[derive(Debug, Clone, Serialize)]
pub struct CrateGraph {
    nodes: Vec<CrateGraphNode>,
    edges: Vec<CrateGraphEdge>,
}

impl CrateGraph {
    pub(crate) fn new(nodes: Vec<CrateGraphNode>, edges: Vec<CrateGraphEdge>) -> Self {
        Self { nodes, edges }
    }

    /// The workspace crates.
    pub fn nodes(&self) -> &[CrateGraphNode] {
        &self.nodes
    }

    /// The dependency edges between workspace crates.
    pub fn edges(&self) -> &[CrateGraphEdge] {
        &self.edges
    }
}

/// A node in the module tree.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleNode {
    module_path: String,
    file_path: Option<PathBuf>,
    is_inline: bool,
    children: Vec<ModuleNode>,
}

impl ModuleNode {
    pub(crate) fn new(module_path: String, file_path: Option<PathBuf>, is_inline: bool) -> Self {
        Self {
            module_path,
            file_path,
            is_inline,
            children: Vec::new(),
        }
    }

    pub(crate) fn add_child(&mut self, child: ModuleNode) {
        self.children.push(child);
    }

    /// The fully-qualified module path (e.g., `crate::analysis::complexity`).
    pub fn module_path(&self) -> &str {
        &self.module_path
    }

    /// The file path on disk, if this is a file-backed module.
    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Whether this module is defined inline (e.g., `mod tests { ... }`).
    pub fn is_inline(&self) -> bool {
        self.is_inline
    }

    /// Child modules.
    pub fn children(&self) -> &[ModuleNode] {
        &self.children
    }
}

/// A module tree rooted at a crate.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleTree {
    crate_name: String,
    root: ModuleNode,
}

impl ModuleTree {
    pub(crate) fn new(crate_name: String, root: ModuleNode) -> Self {
        Self { crate_name, root }
    }

    /// The crate name this tree belongs to.
    pub fn crate_name(&self) -> &str {
        &self.crate_name
    }

    /// The root module node.
    pub fn root(&self) -> &ModuleNode {
        &self.root
    }
}

/// A cross-module import edge.
#[derive(Debug, Clone, Serialize)]
pub struct ImportEdge {
    source_module: String,
    target_module: String,
    items: Vec<String>,
    is_internal: bool,
}

impl ImportEdge {
    pub(crate) fn new(
        source_module: String,
        target_module: String,
        items: Vec<String>,
        is_internal: bool,
    ) -> Self {
        Self {
            source_module,
            target_module,
            items,
            is_internal,
        }
    }

    /// The module where the import appears.
    pub fn source_module(&self) -> &str {
        &self.source_module
    }

    /// The module being imported from.
    pub fn target_module(&self) -> &str {
        &self.target_module
    }

    /// The specific items imported.
    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Whether this import references an internal module (same crate/workspace).
    pub fn is_internal(&self) -> bool {
        self.is_internal
    }
}

/// The kind of entry point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPointKind {
    MainFn,
    LibPubItem,
}

impl fmt::Display for EntryPointKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntryPointKind::MainFn => write!(f, "fn main"),
            EntryPointKind::LibPubItem => write!(f, "pub item"),
        }
    }
}

/// An entry point into a crate.
#[derive(Debug, Clone, Serialize)]
pub struct EntryPoint {
    crate_name: String,
    name: String,
    kind: EntryPointKind,
    file_path: PathBuf,
}

impl EntryPoint {
    pub(crate) fn new(
        crate_name: String,
        name: String,
        kind: EntryPointKind,
        file_path: PathBuf,
    ) -> Self {
        Self {
            crate_name,
            name,
            kind,
            file_path,
        }
    }

    /// The crate this entry point belongs to.
    pub fn crate_name(&self) -> &str {
        &self.crate_name
    }

    /// The name of the entry point item.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The kind of entry point.
    pub fn kind(&self) -> &EntryPointKind {
        &self.kind
    }

    /// The file containing this entry point.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

/// The complete structural map of a codebase.
#[derive(Debug, Clone, Serialize)]
pub struct CodebaseMap {
    crate_graph: Option<CrateGraph>,
    module_trees: Vec<ModuleTree>,
    imports: Vec<ImportEdge>,
    entry_points: Vec<EntryPoint>,
}

impl CodebaseMap {
    pub(crate) fn new(
        crate_graph: Option<CrateGraph>,
        module_trees: Vec<ModuleTree>,
        imports: Vec<ImportEdge>,
        entry_points: Vec<EntryPoint>,
    ) -> Self {
        Self {
            crate_graph,
            module_trees,
            imports,
            entry_points,
        }
    }

    /// The workspace crate graph, if this is a workspace.
    pub fn crate_graph(&self) -> Option<&CrateGraph> {
        self.crate_graph.as_ref()
    }

    /// Module trees, one per crate.
    pub fn module_trees(&self) -> &[ModuleTree] {
        &self.module_trees
    }

    /// Cross-module import edges.
    pub fn imports(&self) -> &[ImportEdge] {
        &self.imports
    }

    /// Entry points into the codebase.
    pub fn entry_points(&self) -> &[EntryPoint] {
        &self.entry_points
    }

    /// Render the map as a Mermaid diagram.
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("graph TD\n");

        if let Some(graph) = &self.crate_graph {
            for edge in graph.edges() {
                out.push_str(&format!(
                    "    {}-->|{}|{}\n",
                    sanitize_mermaid_id(edge.from()),
                    edge.kind(),
                    sanitize_mermaid_id(edge.to()),
                ));
            }
            out.push('\n');
        }

        for tree in &self.module_trees {
            write_mermaid_tree(&tree.root, &mut out);
        }

        // Internal import edges
        let internal: Vec<_> = self.imports.iter().filter(|i| i.is_internal).collect();
        if !internal.is_empty() {
            out.push('\n');
            for edge in internal {
                out.push_str(&format!(
                    "    {}-.->|{}|{}\n",
                    sanitize_mermaid_id(edge.source_module()),
                    edge.items.join(", "),
                    sanitize_mermaid_id(edge.target_module()),
                ));
            }
        }

        out
    }
}

fn sanitize_mermaid_id(name: &str) -> String {
    name.replace("::", "_").replace([':', '-'], "_")
}

fn write_mermaid_tree(node: &ModuleNode, out: &mut String) {
    let id = sanitize_mermaid_id(node.module_path());
    let label = node
        .module_path()
        .rsplit("::")
        .next()
        .unwrap_or(node.module_path());
    let suffix = if node.is_inline { " (inline)" } else { "" };
    out.push_str(&format!("    {id}[\"{label}{suffix}\"]\n"));

    for child in &node.children {
        let child_id = sanitize_mermaid_id(child.module_path());
        out.push_str(&format!("    {id}-->{child_id}\n"));
        write_mermaid_tree(child, out);
    }
}

impl fmt::Display for CodebaseMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Prism Codebase Map")?;
        writeln!(f, "{}", "=".repeat(72))?;

        if let Some(graph) = &self.crate_graph {
            writeln!(f, "\nCrate Graph ({} crates):", graph.nodes.len())?;
            writeln!(f, "{}", "-".repeat(72))?;
            for edge in &graph.edges {
                writeln!(f, "  {} -> {} ({})", edge.from, edge.to, edge.kind)?;
            }
            writeln!(f)?;
        }

        for tree in &self.module_trees {
            writeln!(f, "Module Tree: {}", tree.crate_name)?;
            writeln!(f, "{}", "-".repeat(72))?;
            write_display_tree(f, &tree.root, 0)?;
            writeln!(f)?;
        }

        let internal_imports: Vec<_> = self.imports.iter().filter(|i| i.is_internal).collect();
        if !internal_imports.is_empty() {
            writeln!(f, "Internal Imports:")?;
            writeln!(f, "{}", "-".repeat(72))?;

            // Group by source->target, aggregate items
            let mut grouped: std::collections::BTreeMap<(&str, &str), Vec<&str>> =
                std::collections::BTreeMap::new();
            for imp in &internal_imports {
                let items = grouped
                    .entry((&imp.source_module, &imp.target_module))
                    .or_default();
                for item in &imp.items {
                    items.push(item);
                }
            }
            for ((from, to), items) in &grouped {
                writeln!(
                    f,
                    "  {} -> {} ({} import{})",
                    from,
                    to,
                    items.len(),
                    if items.len() == 1 { "" } else { "s" },
                )?;
            }
            writeln!(f)?;
        }

        let external_imports: Vec<_> = self.imports.iter().filter(|i| !i.is_internal).collect();
        if !external_imports.is_empty() {
            writeln!(f, "External Dependencies:")?;
            writeln!(f, "{}", "-".repeat(72))?;
            let mut ext_crates: std::collections::BTreeSet<&str> =
                std::collections::BTreeSet::new();
            for imp in &external_imports {
                let root = imp
                    .target_module
                    .split("::")
                    .next()
                    .unwrap_or(&imp.target_module);
                ext_crates.insert(root);
            }
            for crate_name in &ext_crates {
                writeln!(f, "  {crate_name}")?;
            }
            writeln!(f)?;
        }

        if !self.entry_points.is_empty() {
            writeln!(f, "Entry Points:")?;
            writeln!(f, "{}", "-".repeat(72))?;
            for ep in &self.entry_points {
                writeln!(
                    f,
                    "  [{}] {} ({})",
                    ep.kind,
                    ep.name,
                    ep.file_path.display(),
                )?;
            }
        }

        writeln!(f, "{}", "=".repeat(72))?;
        Ok(())
    }
}

fn write_display_tree(f: &mut fmt::Formatter<'_>, node: &ModuleNode, indent: usize) -> fmt::Result {
    let prefix = "  ".repeat(indent);
    let inline_tag = if node.is_inline { " [inline]" } else { "" };
    let file_info = node
        .file_path
        .as_ref()
        .map(|p| format!(" ({})", p.display()))
        .unwrap_or_default();
    writeln!(f, "{prefix}{}{inline_tag}{file_info}", node.module_path)?;
    for child in &node.children {
        write_display_tree(f, child, indent + 1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_config_default_has_no_depth_limit() {
        let config = MapConfig::new("/some/path");
        assert_eq!(config.path(), Path::new("/some/path"));
        assert_eq!(config.depth_limit(), None);
    }

    #[test]
    fn map_config_with_depth_limit() {
        let config = MapConfig::new("/some/path").with_depth_limit(3);
        assert_eq!(config.depth_limit(), Some(3));
    }

    #[test]
    fn module_node_construction_and_children() {
        let mut root = ModuleNode::new(
            "crate".to_string(),
            Some(PathBuf::from("src/lib.rs")),
            false,
        );
        assert_eq!(root.module_path(), "crate");
        assert_eq!(root.file_path(), Some(Path::new("src/lib.rs")));
        assert!(!root.is_inline());
        assert!(root.children().is_empty());

        let child = ModuleNode::new(
            "crate::foo".to_string(),
            Some(PathBuf::from("src/foo.rs")),
            false,
        );
        root.add_child(child);
        assert_eq!(root.children().len(), 1);
        assert_eq!(root.children()[0].module_path(), "crate::foo");
    }

    #[test]
    fn inline_module_has_no_file_path() {
        let node = ModuleNode::new("crate::tests".to_string(), None, true);
        assert!(node.is_inline());
        assert_eq!(node.file_path(), None);
    }

    #[test]
    fn import_edge_accessors() {
        let edge = ImportEdge::new(
            "crate::analysis".to_string(),
            "crate::types".to_string(),
            vec!["Config".to_string(), "Mode".to_string()],
            true,
        );
        assert_eq!(edge.source_module(), "crate::analysis");
        assert_eq!(edge.target_module(), "crate::types");
        assert_eq!(edge.items(), &["Config", "Mode"]);
        assert!(edge.is_internal());
    }

    #[test]
    fn external_import_edge() {
        let edge = ImportEdge::new(
            "crate::lib".to_string(),
            "serde".to_string(),
            vec!["Serialize".to_string()],
            false,
        );
        assert!(!edge.is_internal());
    }

    #[test]
    fn entry_point_accessors() {
        let ep = EntryPoint::new(
            "my-crate".to_string(),
            "main".to_string(),
            EntryPointKind::MainFn,
            PathBuf::from("src/main.rs"),
        );
        assert_eq!(ep.crate_name(), "my-crate");
        assert_eq!(ep.name(), "main");
        assert_eq!(ep.kind(), &EntryPointKind::MainFn);
        assert_eq!(ep.file_path(), Path::new("src/main.rs"));
    }

    #[test]
    fn crate_graph_construction() {
        let nodes = vec![
            CrateGraphNode::new("alpha".to_string(), PathBuf::from("crates/alpha")),
            CrateGraphNode::new("beta".to_string(), PathBuf::from("crates/beta")),
        ];
        let edges = vec![CrateGraphEdge::new(
            "alpha".to_string(),
            "beta".to_string(),
            DependencyEdgeKind::Normal,
        )];
        let graph = CrateGraph::new(nodes, edges);

        assert_eq!(graph.nodes().len(), 2);
        assert_eq!(graph.nodes()[0].name(), "alpha");
        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.edges()[0].from(), "alpha");
        assert_eq!(graph.edges()[0].to(), "beta");
        assert_eq!(graph.edges()[0].kind(), DependencyEdgeKind::Normal);
    }

    #[test]
    fn codbase_map_without_crate_graph() {
        let root = ModuleNode::new(
            "crate".to_string(),
            Some(PathBuf::from("src/main.rs")),
            false,
        );
        let tree = ModuleTree::new("my-crate".to_string(), root);
        let map = CodebaseMap::new(None, vec![tree], vec![], vec![]);

        assert!(map.crate_graph().is_none());
        assert_eq!(map.module_trees().len(), 1);
        assert_eq!(map.module_trees()[0].crate_name(), "my-crate");
        assert!(map.imports().is_empty());
        assert!(map.entry_points().is_empty());
    }

    #[test]
    fn dependency_edge_kind_display() {
        assert_eq!(DependencyEdgeKind::Normal.to_string(), "normal");
        assert_eq!(DependencyEdgeKind::Dev.to_string(), "dev");
        assert_eq!(DependencyEdgeKind::Build.to_string(), "build");
    }

    #[test]
    fn entry_point_kind_display() {
        assert_eq!(EntryPointKind::MainFn.to_string(), "fn main");
        assert_eq!(EntryPointKind::LibPubItem.to_string(), "pub item");
    }

    #[test]
    fn map_error_messages() {
        let err = MapError::InvalidPath {
            path: "/bad".to_string(),
        };
        assert_eq!(err.to_string(), "path does not exist: /bad");

        let err = MapError::NotACargoProject {
            path: "/not-cargo".to_string(),
        };
        assert_eq!(err.to_string(), "not a cargo project: /not-cargo");
    }

    fn sample_codbase_map() -> CodebaseMap {
        let mut root = ModuleNode::new(
            "crate".to_string(),
            Some(PathBuf::from("src/lib.rs")),
            false,
        );
        let child = ModuleNode::new(
            "crate::utils".to_string(),
            Some(PathBuf::from("src/utils.rs")),
            false,
        );
        root.add_child(child);

        let tree = ModuleTree::new("my-crate".to_string(), root);
        let import = ImportEdge::new(
            "crate::utils".to_string(),
            "crate".to_string(),
            vec!["Config".to_string()],
            true,
        );
        let ep = EntryPoint::new(
            "my-crate".to_string(),
            "hello".to_string(),
            EntryPointKind::LibPubItem,
            PathBuf::from("src/lib.rs"),
        );

        CodebaseMap::new(None, vec![tree], vec![import], vec![ep])
    }

    #[test]
    fn display_output_contains_module_tree() {
        let map = sample_codbase_map();
        let output = map.to_string();

        assert!(output.contains("my-crate"), "should contain crate name");
        assert!(
            output.contains("crate::utils"),
            "should contain module path"
        );
    }

    #[test]
    fn display_output_contains_imports_summary() {
        let map = sample_codbase_map();
        let output = map.to_string();

        assert!(output.contains("Import"), "should contain imports section");
    }

    #[test]
    fn display_output_contains_entry_points() {
        let map = sample_codbase_map();
        let output = map.to_string();

        assert!(
            output.contains("Entry"),
            "should contain entry points section"
        );
        assert!(output.contains("hello"), "should contain entry point name");
    }

    #[test]
    fn mermaid_output_starts_with_graph() {
        let map = sample_codbase_map();
        let mermaid = map.to_mermaid();

        assert!(
            mermaid.starts_with("graph"),
            "mermaid should start with graph keyword"
        );
        assert!(mermaid.contains("crate"), "should contain crate root node");
    }

    #[test]
    fn json_serialization_roundtrip() {
        let map = sample_codbase_map();
        let json = serde_json::to_string(&map).expect("should serialize to JSON");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse JSON");

        assert!(parsed["module_trees"].is_array());
        assert!(parsed["imports"].is_array());
        assert!(parsed["entry_points"].is_array());
        assert!(parsed["crate_graph"].is_null());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_module_path() -> impl Strategy<Value = String> {
        proptest::collection::vec("[a-z][a-z0-9_]{0,10}", 1..=4)
            .prop_map(|parts| format!("crate::{}", parts.join("::")))
    }

    proptest! {
        #[test]
        fn module_node_path_is_never_empty(
            path in arb_module_path(),
        ) {
            let node = ModuleNode::new(path.clone(), None, true);
            prop_assert!(!node.module_path().is_empty());
            prop_assert_eq!(node.module_path(), &path);
        }

        #[test]
        fn codbase_map_json_roundtrip(
            num_trees in 0usize..3,
        ) {
            let trees: Vec<ModuleTree> = (0..num_trees)
                .map(|i| {
                    let root = ModuleNode::new(
                        "crate".to_string(),
                        Some(PathBuf::from(format!("src/lib{i}.rs"))),
                        false,
                    );
                    ModuleTree::new(format!("crate-{i}"), root)
                })
                .collect();

            let map = CodebaseMap::new(None, trees, vec![], vec![]);
            let json_str = serde_json::to_string(&map).expect("serialize");
            let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("parse");
            prop_assert_eq!(
                parsed["module_trees"].as_array().unwrap().len(),
                num_trees
            );
        }

        #[test]
        fn sanitize_mermaid_id_never_contains_colons(
            name in "[a-z:_-]{1,30}",
        ) {
            let sanitized = sanitize_mermaid_id(&name);
            prop_assert!(!sanitized.contains(':'));
            prop_assert!(!sanitized.contains('-'));
        }
    }
}
