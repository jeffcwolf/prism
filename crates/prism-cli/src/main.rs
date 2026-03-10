use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Prism — verify LLM-generated code quality.
#[derive(Parser)]
#[command(name = "prism", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Audit a codebase for module depth, complexity, and API surface.
    Audit {
        /// Path to the codebase to audit.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Analyze dependency health.
    Deps {
        /// Path to the codebase to analyze.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show project statistics dashboard.
    Stats {
        /// Path to the codebase to analyze.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Output JSON instead of human-readable summary.
        #[arg(long)]
        json: bool,
        /// Skip dependency analysis.
        #[arg(long)]
        no_deps: bool,
    },
    /// Map the structural layout of a codebase.
    Map {
        /// Path to the codebase to map.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Output as Mermaid diagram.
        #[arg(long, conflicts_with = "json")]
        mermaid: bool,
        /// Limit module tree depth.
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Run release-readiness checks against a codebase.
    Check {
        /// Path to the codebase to check.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output as JSON (for machine consumption).
        #[arg(long)]
        json: bool,
        /// Path to a prism-check config file (TOML).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Skip dependency checks (useful offline).
        #[arg(long)]
        no_deps: bool,
        /// Skip coverage checks even if tool is available.
        #[arg(long)]
        no_coverage: bool,
        /// Use stricter thresholds.
        #[arg(long)]
        strict: bool,
        /// Include actionable fix suggestions in output.
        #[arg(long)]
        fix_suggestions: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Audit { path } => {
            let report = prism_audit::audit_codebase(&path)?;
            print_audit_report(&report);
        }
        Command::Deps { path } => {
            let report = prism_deps::analyze_dependencies(&path)?;
            print_deps_report(&report);
        }
        Command::Stats {
            path,
            json,
            no_deps,
        } => {
            let config = prism_stats::StatsConfig::new(path)
                .with_skip_deps(no_deps)
                .with_json(json);
            let stats = prism_stats::collect_stats(&config)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                print!("{stats}");
            }
        }
        Command::Map {
            path,
            json,
            mermaid,
            depth,
        } => {
            let mut config = prism_map::MapConfig::new(path);
            if let Some(d) = depth {
                config = config.with_depth_limit(d);
            }
            let map = prism_map::map_codebase(&config)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&map)?);
            } else if mermaid {
                print!("{}", map.to_mermaid());
            } else {
                print!("{map}");
            }
        }
        Command::Check {
            path,
            json,
            config,
            no_deps,
            no_coverage,
            strict,
            fix_suggestions,
        } => {
            let mut check_config = prism_check::CheckConfig::new(path)
                .with_json(json)
                .with_no_deps(no_deps)
                .with_no_coverage(no_coverage)
                .with_strict(strict)
                .with_fix_suggestions(fix_suggestions);

            // Load config file if specified, or look for prism-check.toml in project root
            if let Some(config_path) = config {
                check_config.load_config_file(&config_path)?;
            } else {
                let default_config = check_config.path().join("prism-check.toml");
                if default_config.exists() {
                    check_config.load_config_file(&default_config)?;
                }
            }

            if strict {
                check_config.apply_strict();
            }

            let report = prism_check::run_checks(&check_config);

            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_check_report(&report, fix_suggestions);
            }

            if report.overall_status() == prism_check::CheckStatus::Fail {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn print_check_report(report: &prism_check::CheckReport, fix_suggestions: bool) {
    println!("Prism Release Readiness Check");
    println!("{}", "=".repeat(72));
    if !report.project_info().is_empty() {
        println!(
            "Project: {} ({})",
            report.project_name(),
            report.project_info()
        );
    } else {
        println!("Project: {}", report.project_name());
    }
    println!();

    let mut current_category = None;
    for check in report.checks() {
        let cat = check.category();
        if current_category != Some(cat) {
            current_category = Some(cat);
            println!("{}", category_label(cat));
        }
        let icon = match check.status() {
            prism_check::CheckStatus::Pass => "\u{2713}",
            prism_check::CheckStatus::Fail => "\u{2717}",
            prism_check::CheckStatus::Warn => "\u{26A0}",
            prism_check::CheckStatus::Skip => "\u{2298}",
        };
        println!("  {icon} {}", check.message());

        if fix_suggestions
            && check.status() == prism_check::CheckStatus::Fail
            && let Some(suggestion) = fix_suggestion_for(check.name())
        {
            println!("    \u{2192} {suggestion}");
        }
    }
    println!();

    println!("{}", "=".repeat(72));

    let fail = report.total_fail();
    let warn = report.total_warn();
    if fail == 0 && warn == 0 {
        println!("Result: all checks passed \u{2014} ready for release");
    } else if fail == 0 {
        println!("Result: {warn} WARN \u{2014} ready for release (with warnings)");
    } else {
        println!("Result: {fail} FAIL, {warn} WARN \u{2014} not ready for release");
    }

    if fail > 0 {
        println!();
        println!("Failures:");
        for check in report.checks() {
            if check.status() == prism_check::CheckStatus::Fail {
                println!(
                    "  \u{2022} {}: {}",
                    category_label(check.category()),
                    check.message()
                );
            }
        }
    }

    if warn > 0 {
        println!();
        println!("Warnings:");
        for check in report.checks() {
            if check.status() == prism_check::CheckStatus::Warn {
                println!(
                    "  \u{2022} {}: {}",
                    category_label(check.category()),
                    check.message()
                );
            }
        }
    }
}

fn category_label(cat: prism_check::CheckCategory) -> &'static str {
    match cat {
        prism_check::CheckCategory::Quality => "Quality",
        prism_check::CheckCategory::Dependencies => "Dependencies",
        prism_check::CheckCategory::Testing => "Testing",
        prism_check::CheckCategory::Safety => "Safety",
        prism_check::CheckCategory::Structure => "Structure",
        prism_check::CheckCategory::Coverage => "Coverage",
    }
}

fn fix_suggestion_for(check_name: &str) -> Option<&'static str> {
    match check_name {
        "doc_coverage" => {
            Some("Add doc comments to public items (functions, structs, enums, traits)")
        }
        "cyclomatic_complexity" => Some("Consider extracting helper functions to reduce branching"),
        "cognitive_complexity" => {
            Some("Consider simplifying by extracting helper functions or using early returns")
        }
        "shallow_modules" => Some("Hide implementation details behind a narrower public API"),
        "vulnerabilities" => Some("Run `cargo audit` and update affected dependencies"),
        "staleness" => Some("Run `cargo update` to bring dependencies up to date"),
        "test_ratio" => Some("Add more unit tests to improve test coverage"),
        "integration_tests" => Some("Add integration tests in the tests/ directory"),
        "unsafe_blocks" => Some("Consider replacing unsafe code with safe alternatives"),
        "module_structure" => Some("Ensure all .rs files are reachable via mod declarations"),
        "orphan_files" => Some("Remove orphan files or add mod declarations to include them"),
        "line_coverage" => Some("Install cargo-tarpaulin and increase test coverage"),
        _ => None,
    }
}

fn print_deps_report(report: &prism_deps::DepsReport) {
    println!("Prism Dependency Health Report");
    println!("{}", "=".repeat(72));

    let graph = report.graph();
    println!(
        "Dependencies: {} direct, {} total  |  Max tree depth: {}",
        graph.direct_count(),
        graph.total_count(),
        graph.max_depth()
    );
    println!();

    if !report.dependencies().is_empty() {
        println!("Direct Dependencies:");
        println!("{}", "-".repeat(72));

        for dep in report.dependencies() {
            let health = report.health().iter().find(|h| h.name() == dep.name());

            let status = health
                .map(|h| h.status())
                .unwrap_or(prism_deps::HealthStatus::Healthy);

            let status_tag = match status {
                prism_deps::HealthStatus::Healthy => "  OK  ",
                prism_deps::HealthStatus::Stale => " STALE",
                prism_deps::HealthStatus::Bloated => " BLOAT",
                prism_deps::HealthStatus::Vulnerable => " VULN ",
            };

            println!(
                "  [{}] {:<30} v{:<12} ({}, {})",
                status_tag,
                dep.name(),
                dep.version(),
                dep.source(),
                dep.kind()
            );

            if let Some(h) = health {
                if let Some(staleness) = h.staleness() {
                    println!(
                        "         -> latest: v{} {}",
                        staleness.latest_version(),
                        if staleness.is_major_behind() {
                            "(MAJOR version behind)"
                        } else {
                            "(outdated)"
                        }
                    );
                }

                for vuln in h.vulnerabilities() {
                    println!(
                        "         -> {} [{}]: {}",
                        vuln.advisory_id(),
                        vuln.severity(),
                        vuln.title()
                    );
                }

                let tc = h.transitive_count();
                if tc >= 50 {
                    println!(
                        "         -> pulls in {} transitive dependencies (bloat risk)",
                        tc
                    );
                }
            }
        }
        println!();
    }

    if !report.duplicates().is_empty() {
        println!("Duplicate Dependencies:");
        println!("{}", "-".repeat(72));
        for dup in report.duplicates() {
            println!("  {} — versions: {}", dup.name(), dup.versions().join(", "));
        }
        println!();
    }

    println!("{}", "=".repeat(72));

    if report.is_healthy() {
        println!("All dependencies are healthy.");
    } else {
        let vuln_count = report
            .health()
            .iter()
            .filter(|h| h.status() == prism_deps::HealthStatus::Vulnerable)
            .count();
        let stale_count = report
            .health()
            .iter()
            .filter(|h| h.status() == prism_deps::HealthStatus::Stale)
            .count();
        let bloat_count = report
            .health()
            .iter()
            .filter(|h| h.status() == prism_deps::HealthStatus::Bloated)
            .count();
        let dup_count = report.duplicates().len();

        if vuln_count > 0 {
            println!("WARNING: {} vulnerable dependency(ies)", vuln_count);
        }
        if stale_count > 0 {
            println!("WARNING: {} stale dependency(ies)", stale_count);
        }
        if bloat_count > 0 {
            println!("WARNING: {} bloated dependency(ies)", bloat_count);
        }
        if dup_count > 0 {
            println!("WARNING: {} duplicate dependency(ies)", dup_count);
        }
    }
}

fn print_audit_report(report: &prism_audit::AuditReport) {
    println!("Prism Audit Report");
    println!("{}", "=".repeat(72));
    println!(
        "Files scanned: {}  |  Total lines: {}",
        report.total_files(),
        report.total_lines()
    );
    println!();

    for module in report.modules() {
        let depth_ratio = module.depth_ratio();
        let flag = if depth_ratio > 0.5 { " [SHALLOW]" } else { "" };
        println!("  {:<40} depth={:.2}{}", module.name(), depth_ratio, flag);
        println!(
            "    public items: {}  |  total items: {}  |  lines: {}",
            module.public_item_count(),
            module.total_item_count(),
            module.total_lines()
        );

        let complexities = module.function_complexities();
        if !complexities.is_empty() {
            println!("    functions:");
            for func in &complexities {
                let vis = if func.is_public() { "pub " } else { "" };
                println!(
                    "      {}{:<30} cyclomatic={:<3} depth={:<3} cognitive={}",
                    vis,
                    func.name(),
                    func.cyclomatic(),
                    func.nesting_depth(),
                    func.cognitive()
                );
            }
        }
    }

    println!();

    let findings = report.findings();
    if !findings.is_empty() {
        println!("Findings:");
        println!("{}", "-".repeat(72));
        for finding in findings {
            let prefix = match finding.severity() {
                prism_audit::Severity::Error => "ERROR",
                prism_audit::Severity::Warning => "WARN ",
                prism_audit::Severity::Info => "INFO ",
            };
            println!("  [{}] {}", prefix, finding.message());
        }
        println!();
    }

    println!("{}", "=".repeat(72));

    let shallow_count = report
        .modules()
        .iter()
        .filter(|m| m.depth_ratio() > 0.5)
        .count();
    let warning_count = findings
        .iter()
        .filter(|f| f.severity() >= prism_audit::Severity::Warning)
        .count();

    if shallow_count > 0 || warning_count > 0 {
        if shallow_count > 0 {
            println!(
                "WARNING: {} module(s) flagged as shallow (depth ratio > 0.5)",
                shallow_count
            );
        }
        if warning_count > 0 {
            println!(
                "WARNING: {} finding(s) at warning level or above",
                warning_count
            );
        }
    } else {
        println!("All modules have acceptable depth ratios.");
    }
}
