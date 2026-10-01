//! Package-keyed repo content (Track H).
//!
//! Deterministic per-repo material lives in `data/repo-content.json` — never
//! in `src/`: porting rules, held-out suites, probe scripts, workloads, plant
//! data. Stages look it up by the package name discovered from the repo's own
//! packaging metadata; adding a repo adds data, never code. Unknown packages
//! are refused, never guessed.

use std::sync::LazyLock;

static TABLE: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/repo-content.json"))
        .expect("repo-content.json parses")
});

/// Content entry for `package` (name discovered from the repo itself).
pub fn entry(package: &str) -> Result<&'static serde_json::Value, String> {
    if TABLE["packages"][package].is_object() {
        Ok(&TABLE["packages"][package])
    } else {
        Err(format!("no repo content for package '{package}'"))
    }
}

/// Generic content for repos with no package-keyed entry (non-Python spines).
/// Every consumer degrades to empty/defaults: no hotspot descriptors, an empty
/// workload contract, no held-out descriptors. Never halts; the probe report
/// already froze the repo identity. Python-spine callers keep using
/// [`entry`] (which still refuses unknowns) so their output stays byte-identical.
pub fn empty_content() -> serde_json::Value {
    serde_json::json!({
        "hotspot_descriptors": [],
        "workload_contract": {},
        "heldout_descriptors": {},
    })
}

/// Content for `package`: the package-keyed entry when present, else the
/// generic [`empty_content`]. Non-Python spines use this so unknown packages
/// degrade instead of halting once the probe succeeds.
pub fn entry_or_empty(package: &str) -> serde_json::Value {
    entry(package).cloned().unwrap_or_else(|_| empty_content())
}

/// All known package keys (tests iterate this; no package is named in code).
#[cfg(test)]
pub fn packages() -> Vec<String> {
    TABLE["packages"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}
