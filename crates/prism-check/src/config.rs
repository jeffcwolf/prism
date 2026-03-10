//! Configuration loading and threshold management for release-readiness checks.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Top-level configuration for prism check.
pub struct CheckConfig {
    path: PathBuf,
    json: bool,
    no_deps: bool,
    no_coverage: bool,
    strict: bool,
    fix_suggestions: bool,
    thresholds: Thresholds,
}

impl CheckConfig {
    /// Create a config with default thresholds for the given path.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            json: false,
            no_deps: false,
            no_coverage: false,
            strict: false,
            fix_suggestions: false,
            thresholds: Thresholds::default(),
        }
    }

    pub fn with_json(mut self, json: bool) -> Self {
        self.json = json;
        self
    }

    pub fn with_no_deps(mut self, no_deps: bool) -> Self {
        self.no_deps = no_deps;
        self
    }

    pub fn with_no_coverage(mut self, no_coverage: bool) -> Self {
        self.no_coverage = no_coverage;
        self
    }

    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    pub fn with_fix_suggestions(mut self, fix_suggestions: bool) -> Self {
        self.fix_suggestions = fix_suggestions;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn json(&self) -> bool {
        self.json
    }

    pub fn no_deps(&self) -> bool {
        self.no_deps
    }

    pub fn no_coverage(&self) -> bool {
        self.no_coverage
    }

    pub fn strict(&self) -> bool {
        self.strict
    }

    pub fn fix_suggestions(&self) -> bool {
        self.fix_suggestions
    }

    pub fn thresholds(&self) -> &Thresholds {
        &self.thresholds
    }

    /// Load thresholds from a TOML config file, merging with defaults.
    pub fn load_config_file(&mut self, path: &Path) -> Result<(), ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
            path: path.to_string_lossy().into_owned(),
            source: e,
        })?;
        let file_config: FileConfig = toml::from_str(&content).map_err(|e| ConfigError::Parse {
            path: path.to_string_lossy().into_owned(),
            message: e.to_string(),
        })?;
        self.thresholds.merge_from_file(&file_config);
        Ok(())
    }

    /// Apply strict thresholds, overriding any previously set values.
    pub fn apply_strict(&mut self) {
        self.thresholds = Thresholds::strict();
    }
}

/// Errors that can occur during config loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    #[error("failed to parse config file {path}: {message}")]
    Parse { path: String, message: String },
}

/// All configurable thresholds for check evaluation.
#[derive(Debug, Clone)]
pub struct Thresholds {
    pub min_doc_coverage: f64,
    pub max_cyclomatic: u32,
    pub max_cognitive: u32,
    pub min_test_ratio: f64,
    pub require_integration_tests: bool,
    pub max_duplicate_versions: usize,
    pub check_staleness: bool,
    pub max_unsafe_blocks: u64,
    pub coverage_enabled: bool,
    pub min_line_coverage: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_doc_coverage: 80.0,
            max_cyclomatic: 20,
            max_cognitive: 30,
            min_test_ratio: 1.0,
            require_integration_tests: true,
            max_duplicate_versions: 3,
            check_staleness: true,
            max_unsafe_blocks: 0,
            coverage_enabled: true,
            min_line_coverage: 60.0,
        }
    }
}

impl Thresholds {
    fn strict() -> Self {
        Self {
            min_doc_coverage: 90.0,
            max_cyclomatic: 15,
            max_cognitive: 20,
            min_test_ratio: 2.0,
            require_integration_tests: true,
            max_duplicate_versions: 2,
            check_staleness: true,
            max_unsafe_blocks: 0,
            coverage_enabled: true,
            min_line_coverage: 75.0,
        }
    }

    fn merge_from_file(&mut self, file: &FileConfig) {
        if let Some(ref q) = file.quality {
            if let Some(v) = q.min_doc_coverage {
                self.min_doc_coverage = v;
            }
            if let Some(v) = q.max_cyclomatic {
                self.max_cyclomatic = v;
            }
            if let Some(v) = q.max_cognitive {
                self.max_cognitive = v;
            }
        }
        if let Some(ref t) = file.testing {
            if let Some(v) = t.min_test_ratio {
                self.min_test_ratio = v;
            }
            if let Some(v) = t.require_integration_tests {
                self.require_integration_tests = v;
            }
        }
        if let Some(ref d) = file.dependencies {
            if let Some(v) = d.max_duplicate_versions {
                self.max_duplicate_versions = v;
            }
            if let Some(v) = d.check_staleness {
                self.check_staleness = v;
            }
        }
        if let Some(ref s) = file.safety
            && let Some(v) = s.max_unsafe_blocks
        {
            self.max_unsafe_blocks = v;
        }
        if let Some(ref c) = file.coverage {
            if let Some(v) = c.enabled {
                self.coverage_enabled = v;
            }
            if let Some(v) = c.min_line_coverage {
                self.min_line_coverage = v;
            }
        }
    }
}

/// TOML file structure for prism-check configuration.
#[derive(Debug, Deserialize)]
struct FileConfig {
    quality: Option<QualityConfig>,
    testing: Option<TestingConfig>,
    dependencies: Option<DependenciesConfig>,
    safety: Option<SafetyConfig>,
    coverage: Option<CoverageConfig>,
}

#[derive(Debug, Deserialize)]
struct QualityConfig {
    min_doc_coverage: Option<f64>,
    max_cyclomatic: Option<u32>,
    max_cognitive: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TestingConfig {
    min_test_ratio: Option<f64>,
    require_integration_tests: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DependenciesConfig {
    max_duplicate_versions: Option<usize>,
    check_staleness: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SafetyConfig {
    max_unsafe_blocks: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CoverageConfig {
    enabled: Option<bool>,
    min_line_coverage: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn default_thresholds_have_expected_values() {
        let t = Thresholds::default();
        assert_eq!(t.min_doc_coverage, 80.0);
        assert_eq!(t.max_cyclomatic, 20);
        assert_eq!(t.max_cognitive, 30);
        assert_eq!(t.min_test_ratio, 1.0);
        assert!(t.require_integration_tests);
        assert_eq!(t.max_duplicate_versions, 3);
        assert!(t.check_staleness);
        assert_eq!(t.max_unsafe_blocks, 0);
        assert!(t.coverage_enabled);
        assert_eq!(t.min_line_coverage, 60.0);
    }

    #[test]
    fn strict_thresholds_are_stricter() {
        let d = Thresholds::default();
        let s = Thresholds::strict();
        assert!(s.min_doc_coverage > d.min_doc_coverage);
        assert!(s.max_cyclomatic < d.max_cyclomatic);
        assert!(s.max_cognitive < d.max_cognitive);
        assert!(s.min_test_ratio > d.min_test_ratio);
        assert!(s.min_line_coverage > d.min_line_coverage);
    }

    #[test]
    fn config_file_overrides_defaults() {
        let toml_content = r#"
[quality]
min_doc_coverage = 95

[testing]
min_test_ratio = 3.0

[safety]
max_unsafe_blocks = 5
"#;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(toml_content.as_bytes()).unwrap();

        let mut config = CheckConfig::new(PathBuf::from("."));
        config.load_config_file(tmp.path()).unwrap();

        assert_eq!(config.thresholds().min_doc_coverage, 95.0);
        assert_eq!(config.thresholds().min_test_ratio, 3.0);
        assert_eq!(config.thresholds().max_unsafe_blocks, 5);
        // Unset values retain defaults
        assert_eq!(config.thresholds().max_cyclomatic, 20);
        assert_eq!(config.thresholds().max_cognitive, 30);
    }

    #[test]
    fn strict_overrides_file_config() {
        let toml_content = r#"
[quality]
min_doc_coverage = 50
"#;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(toml_content.as_bytes()).unwrap();

        let mut config = CheckConfig::new(PathBuf::from("."));
        config.load_config_file(tmp.path()).unwrap();
        assert_eq!(config.thresholds().min_doc_coverage, 50.0);

        config.apply_strict();
        assert_eq!(config.thresholds().min_doc_coverage, 90.0);
    }

    #[test]
    fn missing_config_file_returns_error() {
        let mut config = CheckConfig::new(PathBuf::from("."));
        let result = config.load_config_file(Path::new("/nonexistent/prism-check.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn invalid_toml_returns_parse_error() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"not valid { toml").unwrap();

        let mut config = CheckConfig::new(PathBuf::from("."));
        let result = config.load_config_file(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("parse"));
    }

    #[test]
    fn config_builder_methods() {
        let config = CheckConfig::new(PathBuf::from("/my/path"))
            .with_json(true)
            .with_no_deps(true)
            .with_no_coverage(true)
            .with_strict(true)
            .with_fix_suggestions(true);

        assert_eq!(config.path(), Path::new("/my/path"));
        assert!(config.json());
        assert!(config.no_deps());
        assert!(config.no_coverage());
        assert!(config.strict());
        assert!(config.fix_suggestions());
    }
}
