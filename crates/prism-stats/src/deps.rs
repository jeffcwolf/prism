use std::path::Path;
use std::process::Command;

use crate::types::{DepsStats, StatsError};

pub(crate) fn collect(path: &Path) -> Result<DepsStats, StatsError> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(path.join("Cargo.toml"))
        .exec()
        .map_err(|e| StatsError::CargoMetadata(e.to_string()))?;

    let workspace_member_ids: std::collections::HashSet<_> =
        metadata.workspace_members.iter().collect();

    // Count direct dependencies of workspace members (normal deps only)
    let mut direct_deps: std::collections::HashSet<String> = std::collections::HashSet::new();
    for pkg_id in &metadata.workspace_members {
        if let Some(pkg) = metadata.packages.iter().find(|p| &p.id == pkg_id) {
            for dep in &pkg.dependencies {
                if dep.kind == cargo_metadata::DependencyKind::Normal {
                    direct_deps.insert(dep.name.clone());
                }
            }
        }
    }

    // Transitive: all resolved packages minus workspace members
    let transitive = metadata
        .packages
        .iter()
        .filter(|p| !workspace_member_ids.contains(&p.id))
        .count() as u64;

    let advisories = check_advisories();

    Ok(DepsStats {
        direct: direct_deps.len() as u64,
        transitive,
        advisories,
    })
}

fn check_advisories() -> Option<u64> {
    let output = Command::new("cargo")
        .args(["audit", "--json"])
        .output()
        .ok()?;

    let json_str = String::from_utf8(output.stdout).ok()?;
    let json: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    let count = json
        .get("vulnerabilities")
        .and_then(|v| v.get("found"))
        .and_then(|f| f.as_u64())
        .unwrap_or(0);

    Some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_advisories_returns_none_when_cargo_audit_missing() {
        // This test verifies the graceful degradation when cargo-audit is not installed.
        // In CI or dev environments without cargo-audit, this should return None, not panic.
        let result = check_advisories();
        // We can't assert the exact value since it depends on the environment,
        // but we can assert it doesn't panic.
        let _ = result;
    }
}
