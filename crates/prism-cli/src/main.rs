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
        Command::Deps { path: _ } => {
            anyhow::bail!("deps subcommand is not yet implemented");
        }
    }

    Ok(())
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
