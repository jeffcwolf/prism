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
    }

    Ok(())
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
