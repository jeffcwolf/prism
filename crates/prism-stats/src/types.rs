use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Controls what stats to collect.
pub struct StatsConfig {
    path: PathBuf,
    skip_deps: bool,
    json: bool,
}

impl StatsConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            skip_deps: false,
            json: false,
        }
    }

    pub fn with_skip_deps(mut self, skip: bool) -> Self {
        self.skip_deps = skip;
        self
    }

    pub fn with_json(mut self, json: bool) -> Self {
        self.json = json;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn skip_deps(&self) -> bool {
        self.skip_deps
    }

    pub fn json(&self) -> bool {
        self.json
    }
}

/// Errors that can occur during stats collection.
#[derive(Debug, thiserror::Error)]
pub enum StatsError {
    #[error("not a Rust project: no Cargo.toml found at {path}")]
    NotARustProject { path: PathBuf },

    #[error("failed to read file {path}: {source}")]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {message}")]
    Parse { path: PathBuf, message: String },

    #[error("cargo metadata failed: {0}")]
    CargoMetadata(String),
}

impl StatsError {
    pub fn not_a_rust_project(path: &Path) -> Self {
        Self::NotARustProject {
            path: path.to_path_buf(),
        }
    }

    pub fn file_read(path: &Path, source: std::io::Error) -> Self {
        Self::FileRead {
            path: path.to_path_buf(),
            source,
        }
    }

    pub fn parse(path: &Path, message: String) -> Self {
        Self::Parse {
            path: path.to_path_buf(),
            message,
        }
    }
}

/// Complete project statistics.
#[derive(Debug, Serialize)]
pub struct ProjectStats {
    name: String,
    is_workspace: bool,
    timestamp: String,
    code: CodeStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    deps: Option<DepsStats>,
    tests: TestStats,
    docs: DocsStats,
    safety: SafetyStats,
    surface: SurfaceStats,
    complexity: ComplexityStats,
}

impl ProjectStats {
    pub fn new(
        code: CodeStats,
        deps: Option<DepsStats>,
        tests: TestStats,
        docs: DocsStats,
        safety: SafetyStats,
        surface: SurfaceStats,
        complexity: ComplexityStats,
    ) -> Self {
        let timestamp = chrono_free_timestamp();
        let name = code.name.clone();
        let is_workspace = code.is_workspace;
        Self {
            name,
            is_workspace,
            timestamp,
            code,
            deps,
            tests,
            docs,
            safety,
            surface,
            complexity,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Returns an RFC 3339 UTC timestamp without pulling in chrono.
fn chrono_free_timestamp() -> String {
    // We use a simple approach: shell out or use a fixed format.
    // For deterministic testing, we accept this may not have sub-second precision.
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    // Convert epoch seconds to ISO 8601 manually
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch to year/month/day
    let (year, month, day) = epoch_days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn epoch_days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn format_number(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

impl fmt::Display for ProjectStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ws = if self.is_workspace {
            " | workspace"
        } else {
            ""
        };
        let crate_info = if self.is_workspace {
            format!(" | {} crates", self.code.crates)
        } else {
            String::new()
        };

        writeln!(f, "prism stats — {}", self.name)?;
        writeln!(
            f,
            "  Code       {} lines | {} files{}{ws}",
            format_number(self.code.rust_lines),
            format_number(self.code.files),
            crate_info,
        )?;

        if let Some(ref deps) = self.deps {
            let advisory_str = match deps.advisories {
                Some(n) => n.to_string(),
                None => "unknown".to_string(),
            };
            writeln!(
                f,
                "  Deps       {} direct | {} transitive | {} advisories",
                format_number(deps.direct),
                format_number(deps.transitive),
                advisory_str,
            )?;
        }

        writeln!(
            f,
            "  Tests      {} unit | {} integration | {} doctests | ratio {:.1}:100",
            format_number(self.tests.unit),
            format_number(self.tests.integration),
            format_number(self.tests.doctests),
            self.tests.ratio_per_100_loc,
        )?;

        writeln!(
            f,
            "  Docs       {:.0}% documented ({}/{} pub items) | {} doctests",
            self.docs.coverage_pct,
            format_number(self.docs.documented_pub_items),
            format_number(self.docs.total_pub_items),
            format_number(self.docs.doctests),
        )?;

        let locations = &self.safety.locations;
        let loc_str = if locations.is_empty() {
            String::new()
        } else {
            format!(" ({})", locations.join(", "))
        };
        writeln!(
            f,
            "  Safety     {} unsafe blocks{loc_str}",
            self.safety.unsafe_blocks,
        )?;

        writeln!(
            f,
            "  Surface    {} pub items | ratio {:.2} pub:total",
            format_number(self.surface.pub_items),
            self.surface.pub_ratio,
        )?;

        let complexity_detail = if self.complexity.max_fn_lines > 0 {
            format!(
                "max fn {} lines ({}) | {} fns > 50 lines",
                self.complexity.max_fn_lines,
                self.complexity.max_fn_location,
                self.complexity.fns_over_50_lines,
            )
        } else {
            "no functions found".to_string()
        };
        write!(f, "  Complexity {complexity_detail}")?;

        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct CodeStats {
    pub(crate) name: String,
    pub(crate) is_workspace: bool,
    pub(crate) rust_lines: u64,
    pub(crate) files: u64,
    pub(crate) crates: u64,
}

impl CodeStats {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_workspace(&self) -> bool {
        self.is_workspace
    }
}

#[derive(Debug, Serialize)]
pub struct DepsStats {
    pub(crate) direct: u64,
    pub(crate) transitive: u64,
    pub(crate) advisories: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct TestStats {
    pub(crate) unit: u64,
    pub(crate) integration: u64,
    pub(crate) doctests: u64,
    pub(crate) ratio_per_100_loc: f64,
}

#[derive(Debug, Serialize)]
pub struct DocsStats {
    pub(crate) documented_pub_items: u64,
    pub(crate) total_pub_items: u64,
    pub(crate) coverage_pct: f64,
    pub(crate) doctests: u64,
}

#[derive(Debug, Serialize)]
pub struct SafetyStats {
    pub(crate) unsafe_blocks: u64,
    pub(crate) locations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SurfaceStats {
    pub(crate) pub_items: u64,
    pub(crate) total_items: u64,
    pub(crate) pub_ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct ComplexityStats {
    pub(crate) max_fn_lines: u64,
    pub(crate) max_fn_name: String,
    pub(crate) max_fn_location: String,
    pub(crate) fns_over_50_lines: u64,
    pub(crate) top_functions: Vec<FunctionInfo>,
}

#[derive(Debug, Serialize)]
pub struct FunctionInfo {
    pub(crate) name: String,
    pub(crate) location: String,
    pub(crate) lines: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format_single_crate() {
        let stats = ProjectStats {
            name: "my-crate".to_string(),
            is_workspace: false,
            timestamp: "2026-03-04T14:30:00Z".to_string(),
            code: CodeStats {
                name: "my-crate".to_string(),
                is_workspace: false,
                rust_lines: 1234,
                files: 15,
                crates: 1,
            },
            deps: Some(DepsStats {
                direct: 5,
                transitive: 42,
                advisories: Some(0),
            }),
            tests: TestStats {
                unit: 50,
                integration: 3,
                doctests: 10,
                ratio_per_100_loc: 5.1,
            },
            docs: DocsStats {
                documented_pub_items: 20,
                total_pub_items: 25,
                coverage_pct: 80.0,
                doctests: 10,
            },
            safety: SafetyStats {
                unsafe_blocks: 1,
                locations: vec!["src/ffi.rs:42".to_string()],
            },
            surface: SurfaceStats {
                pub_items: 25,
                total_items: 100,
                pub_ratio: 0.25,
            },
            complexity: ComplexityStats {
                max_fn_lines: 85,
                max_fn_name: "process_data".to_string(),
                max_fn_location: "src/engine.rs:process_data".to_string(),
                fns_over_50_lines: 2,
                top_functions: vec![],
            },
        };

        let output = stats.to_string();
        assert!(output.contains("prism stats — my-crate"), "missing header");
        assert!(
            output.contains("1,234 lines"),
            "missing comma-formatted lines"
        );
        assert!(output.contains("15 files"), "missing files count");
        assert!(
            !output.contains("workspace"),
            "should not show workspace for single crate"
        );
        assert!(output.contains("5 direct"), "missing direct deps");
        assert!(output.contains("42 transitive"), "missing transitive deps");
        assert!(output.contains("0 advisories"), "missing advisories");
        assert!(output.contains("50 unit"), "missing unit tests");
        assert!(
            output.contains("3 integration"),
            "missing integration tests"
        );
        assert!(output.contains("10 doctests"), "missing doctests");
        assert!(output.contains("ratio 5.1:100"), "missing ratio");
        assert!(output.contains("80% documented"), "missing doc coverage");
        assert!(output.contains("20/25 pub items"), "missing doc fraction");
        assert!(output.contains("1 unsafe blocks"), "missing unsafe count");
        assert!(output.contains("src/ffi.rs:42"), "missing unsafe location");
        assert!(output.contains("25 pub items"), "missing pub items");
        assert!(output.contains("ratio 0.25 pub:total"), "missing pub ratio");
        assert!(output.contains("max fn 85 lines"), "missing complexity max");
        assert!(output.contains("2 fns > 50 lines"), "missing fns over 50");
    }

    #[test]
    fn display_format_workspace() {
        let stats = ProjectStats {
            name: "my-workspace".to_string(),
            is_workspace: true,
            timestamp: "2026-03-04T14:30:00Z".to_string(),
            code: CodeStats {
                name: "my-workspace".to_string(),
                is_workspace: true,
                rust_lines: 61239,
                files: 347,
                crates: 8,
            },
            deps: None,
            tests: TestStats {
                unit: 814,
                integration: 23,
                doctests: 96,
                ratio_per_100_loc: 1.5,
            },
            docs: DocsStats {
                documented_pub_items: 412,
                total_pub_items: 474,
                coverage_pct: 86.9,
                doctests: 96,
            },
            safety: SafetyStats {
                unsafe_blocks: 0,
                locations: vec![],
            },
            surface: SurfaceStats {
                pub_items: 474,
                total_items: 1529,
                pub_ratio: 0.31,
            },
            complexity: ComplexityStats {
                max_fn_lines: 127,
                max_fn_name: "parse_module".to_string(),
                max_fn_location: "src/analysis/walk.rs:parse_module".to_string(),
                fns_over_50_lines: 12,
                top_functions: vec![],
            },
        };

        let output = stats.to_string();
        assert!(
            output.contains("61,239 lines"),
            "missing comma-formatted lines"
        );
        assert!(output.contains("8 crates"), "missing crate count");
        assert!(output.contains("workspace"), "missing workspace label");
        assert!(!output.contains("Deps"), "should not show deps when None");
    }

    #[test]
    fn json_serialization() {
        let stats = ProjectStats {
            name: "test".to_string(),
            is_workspace: false,
            timestamp: "2026-03-04T14:30:00Z".to_string(),
            code: CodeStats {
                name: "test".to_string(),
                is_workspace: false,
                rust_lines: 100,
                files: 5,
                crates: 1,
            },
            deps: Some(DepsStats {
                direct: 3,
                transitive: 10,
                advisories: None,
            }),
            tests: TestStats {
                unit: 10,
                integration: 2,
                doctests: 3,
                ratio_per_100_loc: 15.0,
            },
            docs: DocsStats {
                documented_pub_items: 8,
                total_pub_items: 10,
                coverage_pct: 80.0,
                doctests: 3,
            },
            safety: SafetyStats {
                unsafe_blocks: 0,
                locations: vec![],
            },
            surface: SurfaceStats {
                pub_items: 10,
                total_items: 30,
                pub_ratio: 0.33,
            },
            complexity: ComplexityStats {
                max_fn_lines: 20,
                max_fn_name: "main".to_string(),
                max_fn_location: "src/main.rs:main".to_string(),
                fns_over_50_lines: 0,
                top_functions: vec![],
            },
        };

        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["code"]["rust_lines"], 100);
        assert_eq!(json["deps"]["advisories"], serde_json::Value::Null);
        assert_eq!(json["tests"]["unit"], 10);
        assert_eq!(json["safety"]["unsafe_blocks"], 0);
    }

    #[test]
    fn format_number_adds_commas() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(1_234_567), "1,234,567");
        assert_eq!(format_number(61_239), "61,239");
    }

    #[test]
    fn unknown_advisories_display() {
        let deps = DepsStats {
            direct: 5,
            transitive: 10,
            advisories: None,
        };
        let stats = ProjectStats {
            name: "t".to_string(),
            is_workspace: false,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            code: CodeStats {
                name: "t".to_string(),
                is_workspace: false,
                rust_lines: 100,
                files: 1,
                crates: 1,
            },
            deps: Some(deps),
            tests: TestStats {
                unit: 0,
                integration: 0,
                doctests: 0,
                ratio_per_100_loc: 0.0,
            },
            docs: DocsStats {
                documented_pub_items: 0,
                total_pub_items: 0,
                coverage_pct: 0.0,
                doctests: 0,
            },
            safety: SafetyStats {
                unsafe_blocks: 0,
                locations: vec![],
            },
            surface: SurfaceStats {
                pub_items: 0,
                total_items: 0,
                pub_ratio: 0.0,
            },
            complexity: ComplexityStats {
                max_fn_lines: 0,
                max_fn_name: String::new(),
                max_fn_location: String::new(),
                fns_over_50_lines: 0,
                top_functions: vec![],
            },
        };
        let output = stats.to_string();
        assert!(
            output.contains("unknown advisories"),
            "should show 'unknown' when advisories is None"
        );
    }
}
