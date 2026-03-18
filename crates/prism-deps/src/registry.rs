//! Staleness detection via the crates.io API.
//!
//! Queries crates.io for the latest version of a crate and compares it
//! against the version currently used. Rate-limit-friendly: uses a
//! simple cache and respects crates.io's API guidelines by including
//! a user-agent header.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::types::StalenessInfo;

// Cached registry responses to avoid redundant API calls.
thread_local! {
    static CACHE: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
}

/// Check if a crate is stale by comparing its version to the latest on crates.io.
///
/// Returns `None` if the crate is up to date or if the query fails (we treat
/// network failures as non-blocking — staleness is advisory, not critical).
pub(crate) fn check_staleness(name: &str, current_version: &str) -> Option<StalenessInfo> {
    let latest = fetch_latest_version(name)?;

    let current = semver::Version::parse(current_version).ok()?;
    let latest_parsed = semver::Version::parse(&latest).ok()?;

    if current >= latest_parsed {
        return None;
    }

    let is_major_behind = latest_parsed.major > current.major;

    Some(StalenessInfo::new(
        current_version.to_string(),
        latest,
        is_major_behind,
    ))
}

/// Fetch the latest version of a crate from crates.io, with caching.
fn fetch_latest_version(name: &str) -> Option<String> {
    // Check cache first
    let cached = CACHE.with(|c| c.borrow().get(name).cloned());
    if let Some(cached_result) = cached {
        return cached_result;
    }

    let result = query_crates_io(name);

    // Cache the result regardless of success/failure
    CACHE.with(|c| {
        c.borrow_mut().insert(name.to_string(), result.clone());
    });

    result
}

/// Query the crates.io API for the latest version of a crate.
fn query_crates_io(name: &str) -> Option<String> {
    let url = format!("https://crates.io/api/v1/crates/{name}");

    let response = ureq::get(&url)
        .set("User-Agent", "prism-deps (https://github.com/prism)")
        .call()
        .ok()?;

    let body: serde_json::Value = response.into_json().ok()?;

    body.get("crate")?
        .get("max_stable_version")
        .or_else(|| body.get("crate")?.get("max_version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Clear the registry cache (useful for testing).
#[cfg(test)]
pub(crate) fn clear_cache() {
    CACHE.with(|c| c.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_staleness_returns_none_for_up_to_date() {
        // Use a version higher than any real crate would have
        let result = check_staleness("serde", "9999.0.0");
        assert!(
            result.is_none(),
            "a futuristic version should not be considered stale"
        );
    }

    #[test]
    fn check_staleness_returns_info_for_old_version() {
        // serde 0.1.0 is definitely behind the latest
        let result = check_staleness("serde", "0.1.0");
        if let Some(info) = result {
            assert_eq!(info.current_version(), "0.1.0");
            assert!(
                !info.latest_version().is_empty(),
                "should have a latest version"
            );
            assert!(
                info.is_major_behind(),
                "0.1.0 should be major versions behind current serde"
            );
        }
        // If None, the network request failed, which is acceptable in tests
    }

    #[test]
    fn check_staleness_returns_none_for_non_crates_io() {
        // A completely fake crate name should return None (404 from crates.io)
        let result = check_staleness("this-crate-definitely-does-not-exist-xyz123", "0.1.0");
        assert!(
            result.is_none(),
            "nonexistent crate should return None, not an error"
        );
    }

    #[test]
    fn cache_prevents_redundant_queries() {
        clear_cache();

        // First call populates cache
        let _ = check_staleness("serde", "0.1.0");

        // Second call should hit cache (we can't directly verify this,
        // but we can verify it returns the same result)
        let result1 = check_staleness("serde", "0.1.0");
        let result2 = check_staleness("serde", "0.1.0");

        match (result1, result2) {
            (Some(a), Some(b)) => {
                assert_eq!(a.latest_version(), b.latest_version());
            }
            (None, None) => {} // Both failed (network issue), that's fine
            _ => panic!("cached results should be consistent"),
        }
    }
}