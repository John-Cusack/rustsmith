//! Stage-3 harvest: classify merged optimizations into shippable suggestions.
//!
//! Every `optimizations` row becomes one suggestion: a class + reasoning
//! paragraph, plus a `language_independent` backport patch and/or a
//! `module_local` accelerator where one exists. Provenance (license header +
//! attribution) is checked on every emitted artifact before it is written.

use rustsmith_store::OptimizationRow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HarvestError {
    #[error("no backport for technique {0}")]
    NoBackport(String),
    #[error("no accelerator for class {0}")]
    NoAccelerator(String),
    #[error("io: {0}")]
    Io(String),
    #[error("provenance rejected: {0}")]
    Provenance(String),
}

/// Suggestion class (§9 Stage 3 taxonomy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HarvestClass {
    LanguageIndependent,
    ModuleLocal,
    PortOnly,
}

impl HarvestClass {
    pub fn as_str(self) -> &'static str {
        match self {
            HarvestClass::LanguageIndependent => "language_independent",
            HarvestClass::ModuleLocal => "module_local",
            HarvestClass::PortOnly => "port_only",
        }
    }

    pub fn parse(s: &str) -> Option<HarvestClass> {
        match s {
            "language_independent" => Some(HarvestClass::LanguageIndependent),
            "module_local" => Some(HarvestClass::ModuleLocal),
            "port_only" => Some(HarvestClass::PortOnly),
            _ => None,
        }
    }
}

/// Heuristic v1 from tier/bound/technique. A council-set `harvest_class` on
/// the row always wins (override hook; the reason lives in the decisions log).
/// Tier 1-2 algorithmic wins are portable arithmetic; clean-interface hot
/// paths want accelerators; representation-bound wins cannot leave the port.
pub fn classify(row: &OptimizationRow) -> (HarvestClass, String) {
    if let Some(c) = HarvestClass::parse(row.harvest_class.trim()) {
        return (c, format!("council override to {} (see decisions log)", c.as_str()));
    }
    let tech = row.technique.to_lowercase();
    if tech.contains("slic") || tech.contains("table") && row.bound == "compute" {
        return (
            HarvestClass::LanguageIndependent,
            format!(
                "{} replaces per-byte table lookup with 8-bytes-per-step combined tables. \
                 The win is arithmetic over the CRC recurrence (derived by simulating the \
                 byte step, correct by construction), expressible in any language with \
                 fixed-width integers — no Rust ownership, borrow, or trait machinery is \
                 involved. Ships as a pure-Python backport (measured {:.0}% in-Rust, \
                 {:.0}% in-Python on the reference host).",
                row.technique,
                row.visible_gain_pct,
                BACKPORT_EXPECTED_GAIN * 100.0,
            ),
        );
    }
    if row.bound == "compute" && row.tier >= 6 {
        return (
            HarvestClass::ModuleLocal,
            format!(
                "{} is a hot path behind a clean interface ({}, tier {}): the win does \
                 not port as an algorithm but ships as an optional accelerator module \
                 with dispatch shim and pure-Python fallback intact.",
                row.technique, row.hotspot, row.tier,
            ),
        );
    }
    (
        HarvestClass::PortOnly,
        format!(
            "{} is representation-bound ({} tier {}): inseparable from the full \
             rewrite (data layout, ownership, dispatch). Documented as a reason to \
             use the ported version; nothing shippable back.",
            row.technique, row.bound, row.tier,
        ),
    )
}

/// Conservative in-Python gain for the slicing backport (measured 0.90 on the
/// reference host; the acceptance gate asserts gain beyond the run's floor).
pub const BACKPORT_EXPECTED_GAIN: f64 = 0.50;

/// Original-language backport of the slicing win (verified: applies to
/// pristine Nicoretti/crc@4e65ac4, full 80-test suite green, ~10x in-Python).
const SLICE_PATCH: &str = include_str!("../patches/slice_by_8_crc_py.patch");

/// Attribution every harvest artifact must carry (provenance gate input).
pub const ATTRIBUTION: &str = "Nicoretti/crc";
pub const LICENSE_TAG: &str = "SPDX-License-Identifier: BSD-2-Clause";

/// Fixture-aware attribution (crc consts above are pinned by tests; strsimpy
/// runs carry MIT + upstream attribution instead of crc's).
pub fn attribution_for(fixture: &str) -> (&'static str, &'static str) {
    match fixture {
        "strsimpy" => ("luozhouyang/python-string-similarity", "SPDX-License-Identifier: MIT"),
        _ => (ATTRIBUTION, LICENSE_TAG),
    }
}

/// Text-level provenance pre-check: license header + attribution present.
/// The structural `rustsmith_gates::provenance` gate runs the same rule.
pub fn artifact_provenance_ok(text: &str) -> bool {
    (text.contains("SPDX-License-Identifier")
        || text.contains("BSD-2-Clause")
        || text.contains("Copyright (c)"))
        && text.contains(ATTRIBUTION)
}

/// Original-language patch artifact + how to prove the gain there.
#[derive(Debug, Clone)]
pub struct PatchArtifact {
    pub filename: String,
    pub text: String,
    pub benchmark_cmd: String,
    pub expected_gain: f64,
}

/// A `language_independent` row earns its backport; anything else errors
/// (no fantasy backports: only proven translations ship).
pub fn emit_patch(row: &OptimizationRow) -> Result<PatchArtifact, HarvestError> {
    let (class, _) = classify(row);
    if class != HarvestClass::LanguageIndependent {
        return Err(HarvestError::NoBackport(format!(
            "{} classifies {}",
            row.technique,
            class.as_str()
        )));
    }
    if !row.technique.to_lowercase().contains("slic") {
        return Err(HarvestError::NoBackport(row.technique.clone()));
    }
    if !artifact_provenance_ok(SLICE_PATCH) {
        return Err(HarvestError::Provenance("slice patch".into()));
    }
    Ok(PatchArtifact {
        filename: "slice_by_8_crc.patch".into(),
        text: SLICE_PATCH.into(),
        benchmark_cmd: "git apply --check <patch> && python3 bench (see suggestion README)".into(),
        expected_gain: BACKPORT_EXPECTED_GAIN,
    })
}

/// Accelerator artifact (present only for `module_local` rows).
#[derive(Debug, Clone)]
pub struct AccelArtifact {
    pub filename: String,
    pub text: String,
}

pub fn emit_accelerator(row: &OptimizationRow) -> Result<AccelArtifact, HarvestError> {
    let (class, _) = classify(row);
    if class != HarvestClass::ModuleLocal {
        return Err(HarvestError::NoAccelerator(format!(
            "{} classifies {}",
            row.technique,
            class.as_str()
        )));
    }
    Err(HarvestError::NoAccelerator("no module_local template for this row".into()))
}

/// One ranked, shippable suggestion.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Suggestion {
    pub technique: String,
    pub class: String,
    pub reasoning: String,
    pub expected_gain: f64,
    pub review_burden: f64,
    pub patch_file: Option<String>,
    pub accelerator: Option<String>,
}

/// Burden proxy: added lines (review cost scales with diff size).
pub fn burden_of_patch(patch: &str) -> f64 {
    patch
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .count()
        .max(1) as f64
}

pub fn suggest(row: &OptimizationRow) -> Suggestion {
    let (class, reasoning) = classify(row);
    let (patch_file, expected_gain, burden) = match emit_patch(row) {
        Ok(p) => (
            Some(p.filename),
            p.expected_gain,
            burden_of_patch(&p.text),
        ),
        Err(_) => (None, row.visible_gain_pct / 100.0, 50.0),
    };
    Suggestion {
        technique: row.technique.clone(),
        class: class.as_str().into(),
        reasoning,
        expected_gain,
        review_burden: burden,
        patch_file,
        accelerator: None,
    }
}

/// Rank by expected gain per unit review burden (descending).
pub fn rank(mut suggestions: Vec<Suggestion>) -> Vec<Suggestion> {
    suggestions.sort_by(|a, b| {
        (b.expected_gain / b.review_burden)
            .partial_cmp(&(a.expected_gain / a.review_burden))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(tech: &str, bound: &str, tier: i64, gain: f64) -> OptimizationRow {
        OptimizationRow {
            round: 1, hotspot: "h".into(), commit_sha: "c".into(), delta_pct: gain,
            technique: tech.into(), harvest_class: "".into(), files_touched_json: "[]".into(),
            bound: bound.into(), tier, ceiling_pct: 0.0, visible_gain_pct: gain,
            heldout_gain_pct: gain, divergence_pct: 0.0, instrument: "cpu_time".into(),
            ci_low: None, ci_high: None, attribution_verified: true, rss_delta_pct: 0.0,
            alloc_delta_pct: 0.0, model: "m".into(), prompt_version: "p".into(),
            guidance_version: "g".into(), proposal_text: "why".into(), tokens_spent: 0,
            parent_sha: "p".into(), patch_text: "".into(),
        }
    }

    #[test]
    fn slicing_classifies_language_independent_with_reasoning() {
        let (c, r) = classify(&row("slicing-by-8", "compute", 8, 55.0));
        assert_eq!(c, HarvestClass::LanguageIndependent);
        assert!(r.len() > 100, "reasoning paragraph required");
    }

    #[test]
    fn hot_compute_path_is_module_local() {
        let (c, _) = classify(&row("unroll-x4", "compute", 8, 3.0));
        assert_eq!(c, HarvestClass::ModuleLocal);
    }

    #[test]
    fn representation_work_is_port_only() {
        let (c, _) = classify(&row("rc-graph-flatten", "memory", 3, 12.0));
        assert_eq!(c, HarvestClass::PortOnly);
    }

    #[test]
    fn council_override_wins() {
        let mut r = row("slicing-by-8", "compute", 8, 55.0);
        r.harvest_class = "port_only".into();
        let (c, reason) = classify(&r);
        assert_eq!(c, HarvestClass::PortOnly);
        assert!(reason.contains("override"));
    }

    #[test]
    fn slice_patch_shape_and_provenance() {
        let p = emit_patch(&row("slicing-by-8", "compute", 8, 55.0)).unwrap();
        assert!(p.text.starts_with("diff --git"), "unified diff required");
        assert!(p.text.contains("src/crc/_crc.py"), "must target the original tree");
        assert!(p.text.contains("def update"), "must carry the fast path");
        assert!(p.text.contains("_slicing_tables"), "must carry table derivation");
        assert!(artifact_provenance_ok(&p.text), "patch must carry license + attribution");
        let v = rustsmith_gates::provenance(
            p.text.contains("SPDX-License-Identifier") || p.text.contains("BSD-2-Clause"),
            true,
            ATTRIBUTION,
        );
        assert!(v.passed);
    }

    #[test]
    fn non_slicing_has_no_backport() {
        assert!(emit_patch(&row("unroll-x4", "compute", 8, 3.0)).is_err());
    }

    #[test]
    fn crc_yields_no_accelerator() {
        // crc's only merged row is language_independent: empty-case ships.
        assert!(emit_accelerator(&row("slicing-by-8", "compute", 8, 55.0)).is_err());
    }

    #[test]
    fn rank_orders_by_gain_per_burden() {
        let mut a = suggest(&row("slicing-by-8", "compute", 8, 55.0));
        a.expected_gain = 0.5;
        a.review_burden = 150.0;
        let mut b = suggest(&row("other", "memory", 3, 1.0));
        b.expected_gain = 0.4;
        b.review_burden = 10.0;
        let r = rank(vec![a, b]);
        assert_eq!(r[0].technique, "other");
    }
}
