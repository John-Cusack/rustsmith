use rustsmith_core::{Baseline, FileHash, GradedResult, Manifest, ObservableSpec, Outcome};
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
pub struct GateVerdict {
    pub passed: bool,
    pub detail: serde_json::Value,
}

fn verdict(passed: bool, detail: serde_json::Value) -> GateVerdict {
    GateVerdict { passed, detail }
}

/// All manifest hashes match AND test count + skip/xfail/deselect lists match baseline.
/// Counts checked FIRST so a skip-injection reports skip_mismatch even though its
/// file hash also changed (M0 acceptance step 4). Extras in tree are allowed
/// (workers add files); missing/mismatched manifest files are tamper.
pub fn oracle_integrity(
    manifest: &Manifest,
    tree_hashes: &[FileHash],
    base: &Baseline,
    got: &GradedResult,
) -> GateVerdict {
    let total = got.total();
    if total != base.test_count {
        return verdict(
            false,
            json!({"reason":"count_mismatch","expected":base.test_count,"got":total,
                   "passed":got.passed,"failed":got.failed,
                   "skipped":got.skipped.len(),"xfailed":got.xfailed.len(),
                   "deselected":got.deselected.len()}),
        );
    }
    if got.skipped != base.skipped {
        return verdict(
            false,
            json!({"reason":"skip_mismatch","expected":base.skipped,"got":got.skipped}),
        );
    }
    if got.xfailed != base.xfailed {
        return verdict(
            false,
            json!({"reason":"xfail_mismatch","expected":base.xfailed,"got":got.xfailed}),
        );
    }
    if got.deselected != base.deselected {
        return verdict(
            false,
            json!({"reason":"deselect_mismatch","expected":base.deselected,"got":got.deselected}),
        );
    }
    // Outcomes key-set check: counts alone miss same-count-different-tests
    // (e.g. one test renamed while the total is unchanged). When the runner
    // populated `outcomes`, its keys must account for every non-deselected
    // baseline test, and the skip/xfail vectors must equal the Skip/XFail
    // keys in the map. Empty maps (legacy fixtures) skip this check.
    if !got.outcomes.is_empty() {
        let mut skip_keys: Vec<String> = got
            .outcomes
            .iter()
            .filter(|(_, o)| **o == Outcome::Skip)
            .map(|(id, _)| id.clone())
            .collect();
        skip_keys.sort();
        if skip_keys != got.skipped {
            return verdict(
                false,
                json!({"reason":"skip_mismatch","expected":got.skipped,"got":skip_keys}),
            );
        }
        let mut xfail_keys: Vec<String> = got
            .outcomes
            .iter()
            .filter(|(_, o)| **o == Outcome::XFail)
            .map(|(id, _)| id.clone())
            .collect();
        xfail_keys.sort();
        if xfail_keys != got.xfailed {
            return verdict(
                false,
                json!({"reason":"xfail_mismatch","expected":got.xfailed,"got":xfail_keys}),
            );
        }
        if got.deselected.iter().any(|d| got.outcomes.contains_key(d)) {
            return verdict(
                false,
                json!({"reason":"deselect_overlap","deselected":got.deselected}),
            );
        }
        if got.total_outcomes() + got.deselected.len() != base.test_count as usize {
            return verdict(
                false,
                json!({"reason":"count_mismatch","expected":base.test_count,"got":got.total_outcomes() + got.deselected.len(),
                       "outcomes":got.total_outcomes(),"deselected":got.deselected.len()}),
            );
        }
    }
    let mut want: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for f in &manifest.files {
        want.insert(f.path.as_str(), f.sha256.as_str());
    }
    for h in tree_hashes {
        if let Some(expected) = want.get(h.path.as_str()) {
            if *expected != h.sha256.as_str() {
                return verdict(
                    false,
                    json!({"reason":"hash_mismatch","path":h.path.as_str(),"expected":expected,"got":h.sha256}),
                );
            }
        }
    }
    for f in &manifest.files {
        if !tree_hashes.iter().any(|h| h.path == f.path) {
            return verdict(
                false,
                json!({"reason":"missing_file","path":f.path.as_str()}),
            );
        }
    }
    verdict(true, json!({"reason":"ok","test_count":total}))
}

/// 100% of frozen oracle passes.
pub fn oracle_parity(got: &GradedResult) -> GateVerdict {
    if got.failed == 0 && got.exit_code == 0 {
        verdict(true, json!({"passed":got.passed}))
    } else {
        verdict(
            false,
            json!({"reason":"failures","failed":got.failed,"exit_code":got.exit_code,"stdout_tail":tail(&got.stdout)}),
        )
    }
}

fn tail(s: &str) -> String {
    const N: usize = 2000;
    if s.len() <= N {
        s.to_string()
    } else {
        s[s.len() - N..].to_string()
    }
}

/// divergence = visible_rate - heldout_rate; fail if over threshold.
pub fn heldout_divergence(visible_rate: f64, heldout_rate: f64, threshold: f64) -> GateVerdict {
    let divergence = visible_rate - heldout_rate;
    if divergence <= threshold {
        verdict(
            true,
            json!({"visible_rate":visible_rate,"heldout_rate":heldout_rate,"divergence":divergence,"threshold":threshold}),
        )
    } else {
        verdict(
            false,
            json!({"reason":"divergence_over_threshold","visible_rate":visible_rate,"heldout_rate":heldout_rate,"divergence":divergence,"threshold":threshold}),
        )
    }
}

/// Differential: each (original, ported) pair must match within tolerance.
/// `tols` carries optional per-observable tolerances (`rel_tol`/`abs_tol`
/// from `ObservableSpec`, frozen in the manifest; ADR-007 review §4 item 3):
/// when `Some` and non-empty, entry `i` governs pair `i` (shorter slices
/// fall back to `tolerance` for the tail). `None` (or an empty slice) runs
/// the legacy single-`tolerance` path verbatim, so fixtures graded with
/// `tolerance = 0.0` see byte-identical math.
pub fn differential(
    pairs: &[(String, String)],
    tolerance: f64,
    tols: Option<&[ObservableSpec]>,
) -> GateVerdict {
    // Empty and absent slices both run the legacy path, so fixtures graded
    // with `tolerance = 0.0` see byte-identical math either way.
    let specs: &[ObservableSpec] = tols.unwrap_or(&[]);
    if specs.is_empty() {
        if tolerance == 0.0 {
            for (i, (a, b)) in pairs.iter().enumerate() {
                if a != b {
                    return verdict(
                        false,
                        json!({"reason":"mismatch","index":i,"original":a,"ported":b}),
                    );
                }
            }
            return verdict(true, json!({"compared":pairs.len()}));
        }
        // Numeric tolerance path: try parse as f64.
        for (i, (a, b)) in pairs.iter().enumerate() {
            if a == b {
                continue;
            }
            match (a.parse::<f64>(), b.parse::<f64>()) {
                (Ok(x), Ok(y)) => {
                    let denom = x.abs().max(1.0);
                    if ((x - y).abs() / denom) > tolerance {
                        return verdict(
                            false,
                            json!({"reason":"tolerance_exceeded","index":i,"original":a,"ported":b}),
                        );
                    }
                }
                _ => {
                    return verdict(
                        false,
                        json!({"reason":"mismatch","index":i,"original":a,"ported":b}),
                    )
                }
            }
        }
        return verdict(true, json!({"compared":pairs.len()}));
    }
    // Per-observable path: |x - y| <= abs_tol + rel_tol * max(|x|, |y|, 1.0).
    // (`specs` is non-empty here; a short slice falls back to `tolerance`
    // for the tail.)
    for (i, (a, b)) in pairs.iter().enumerate() {
        if a == b {
            continue;
        }
        let (rel_tol, abs_tol) = specs
            .get(i)
            .map(|s| (s.rel_tol, s.abs_tol))
            .unwrap_or((tolerance, 0.0));
        match (a.parse::<f64>(), b.parse::<f64>()) {
            (Ok(x), Ok(y)) => {
                let allowed = abs_tol + rel_tol * x.abs().max(y.abs()).max(1.0);
                if (x - y).abs() > allowed {
                    return verdict(
                        false,
                        json!({"reason":"tolerance_exceeded","index":i,"original":a,"ported":b,
                               "rel_tol":rel_tol,"abs_tol":abs_tol}),
                    );
                }
            }
            _ => {
                return verdict(
                    false,
                    json!({"reason":"mismatch","index":i,"original":a,"ported":b}),
                )
            }
        }
    }
    verdict(true, json!({"compared":pairs.len()}))
}

#[derive(Debug, Clone)]
pub struct UnsafeSite {
    pub file: String,
    pub is_ffi_boundary: bool,
    pub has_safety_comment: bool,
}

/// unsafe count under budget; every unsafe at FFI boundary with // SAFETY:.
pub fn unsafe_budget(sites: &[UnsafeSite], budget_pct: f64, total_files: usize) -> GateVerdict {
    let count = sites.len();
    let budget_count = (budget_pct / 100.0) * total_files as f64;
    if count as f64 > budget_count && !(budget_count < 1.0 && count == 0) {
        // Allow zero always; otherwise enforce.
        if count > 0 && total_files > 0 && (count as f64) > budget_count.max(1.0) && budget_pct < 100.0 {
            // Strict: if budget allows e.g. 5% of files, compare counts.
            // For M0 crc (pure python mirror has 0 unsafe) this passes trivially.
        }
    }
    for s in sites {
        if !s.is_ffi_boundary {
            return verdict(
                false,
                json!({"reason":"unsafe_not_at_ffi","file":s.file}),
            );
        }
        if !s.has_safety_comment {
            return verdict(
                false,
                json!({"reason":"missing_safety_comment","file":s.file}),
            );
        }
    }
    // Budget: count must not exceed ceil(budget_pct% of 100 loc-unit)? Simplify: absolute cap derived from pct of 100.
    // For test purposes: budget_pct=5.0 allows up to 5 sites per 100 files; scale linearly.
    let allowed = ((budget_pct / 100.0) * total_files as f64).ceil() as usize;
    let allowed = allowed.max(if budget_pct > 0.0 { 1 } else { 0 });
    if count > allowed {
        return verdict(
            false,
            json!({"reason":"unsafe_over_budget","count":count,"allowed":allowed}),
        );
    }
    verdict(true, json!({"count":count,"allowed":allowed}))
}

pub fn miri(exit_code: i32, stderr: &str) -> GateVerdict {
    if exit_code == 0 {
        verdict(true, json!({"miri":"clean"}))
    } else {
        verdict(false, json!({"reason":"miri_failed","stderr_tail":tail(stderr)}))
    }
}

pub fn clippy(exit_code: i32, stderr: &str) -> GateVerdict {
    if exit_code == 0 {
        verdict(true, json!({"clippy":"clean"}))
    } else {
        verdict(false, json!({"reason":"clippy_warnings","stderr_tail":tail(stderr)}))
    }
}

pub fn provenance(headers_present: bool, notice_present: bool, attribution: &str) -> GateVerdict {
    if headers_present && notice_present && !attribution.is_empty() {
        verdict(true, json!({"attribution":attribution}))
    } else {
        verdict(
            false,
            json!({"reason":"provenance_missing","headers":headers_present,"notice":notice_present,"attribution_len":attribution.len()}),
        )
    }
}

// ---------------------------------------------------------------------------
// Stage-2 gates (SPEC_STAGE2 §10). All pure, deterministic. Held-out magnitudes
// NEVER appear in worker-visible output (see `worker_visible_message`).
// ---------------------------------------------------------------------------

/// `visible_gain - heldout_gain`. Fail if divergence > threshold (default 10pp)
/// or heldout_gain negative beyond floor (helps measured, harms rest = overfit).
/// The FAIL detail carries no magnitudes (gradient-climbing prevention).
pub fn workload_divergence(vis_gain: f64, held_gain: f64, threshold: f64, floor: f64) -> GateVerdict {
    let divergence = vis_gain - held_gain;
    if divergence > threshold || held_gain < -floor {
        verdict(false, worker_visible_rejection("workload_divergence"))
    } else {
        verdict(
            true,
            json!({"visible_gain": vis_gain, "heldout_gain": held_gain, "divergence": divergence, "threshold": threshold}),
        )
    }
}

/// Worker-visible rejection: names the gate, never magnitudes.
pub fn worker_visible_rejection(gate: &str) -> serde_json::Value {
    json!({"reason": "rejected_for_workload_divergence", "gate": gate})
}

/// Returns true iff `detail` is safe to show a worker (no held-out numbers).
/// Load-bearing redaction rule: worker-visible strings must contain NO numeric
/// magnitudes from held-out runs (a magnitude is a gradient to climb).
pub fn is_worker_safe(detail: &serde_json::Value) -> bool {
    !detail.to_string().chars().any(|c| c.is_ascii_digit())
}

/// Revert-attribution: gain must disappear when the candidate change is
/// reverted. Pass iff the reverted gain is within floor (gone) OR below half
/// the claimed gain (drift-robust: back-to-back identical trees can differ by
/// frequency/thermal drift larger than the self-vs-self floor; only a gain
/// that substantially PERSISTS without the change is phantom credit).
pub fn causal_attribution(gain_with: f64, gain_without: f64, floor: f64) -> GateVerdict {
    if gain_without.abs() <= floor || gain_without.abs() <= 0.5 * gain_with.abs().max(1e-9) {
        verdict(true, json!({"gain_with": gain_with, "gain_without": gain_without}))
    } else {
        verdict(
            false,
            json!({"reason": "gain_persists_without_change", "gain_with": gain_with, "gain_without": gain_without}),
        )
    }
}

#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    pub rss_bytes: u64,
    /// Python-level allocation peak (tracemalloc). `None` when the profiler
    /// is unavailable (ADR-003: no perf/valgrind here); the alloc leg of
    /// `no_regression_widened` is then skipped, never failed.
    pub alloc_count: Option<u64>,
    pub binary_bytes: u64,
    pub compile_secs: f64,
}

/// Widened no_regression: other visible workloads within floor; RSS +5%;
/// alloc +10%; binary +10%; compile +20%. Held-out is pass/fail only (no magnitudes).
#[allow(clippy::too_many_arguments)]
pub fn no_regression_widened(
    base: &ResourceSnapshot,
    got: &ResourceSnapshot,
    other_workload_within_floor: bool,
    heldout_ok: bool,
) -> GateVerdict {
    if !other_workload_within_floor {
        return verdict(false, json!({"reason": "other_workload_regressed"}));
    }
    if !heldout_ok {
        return verdict(false, json!({"reason": "heldout_regressed"}));
    }
    let pct = |a: u64, b: u64| {
        if a == 0 {
            0.0
        } else {
            (b as f64 - a as f64) / a as f64 * 100.0
        }
    };
    let rss = pct(base.rss_bytes, got.rss_bytes);
    if rss > 5.0 {
        return verdict(false, json!({"reason": "rss_regression", "delta_pct": rss}));
    }
    let alloc: Option<f64> = match (base.alloc_count, got.alloc_count) {
        (Some(a), Some(b)) => Some(pct(a, b)),
        // Profiler unavailable on either side: skip the alloc leg (ADR-003).
        _ => None,
    };
    if alloc.is_some_and(|a| a > 10.0) {
        return verdict(false, json!({"reason": "alloc_regression", "delta_pct": alloc}));
    }
    let bin = pct(base.binary_bytes, got.binary_bytes);
    if bin > 10.0 {
        return verdict(false, json!({"reason": "binary_regression", "delta_pct": bin}));
    }
    let compile = if base.compile_secs <= 0.0 {
        0.0
    } else {
        (got.compile_secs - base.compile_secs) / base.compile_secs * 100.0
    };
    if compile > 20.0 {
        return verdict(false, json!({"reason": "compile_regression", "delta_pct": compile}));
    }
    verdict(true, json!({"rss_pct": rss, "alloc_pct": alloc, "binary_pct": bin, "compile_pct": compile}))
}

/// Per-candidate: deterministic gain > measured floor. Per-round (merged):
/// wall-clock CI excludes zero AND sign agrees with deterministic.
pub fn benchmark_restated(
    det_gain: f64,
    floor: f64,
    round_ci: Option<(f64, f64)>,
    det_sign: f64,
) -> GateVerdict {
    if det_gain <= floor {
        return verdict(false, json!({"reason": "below_floor", "det_gain": det_gain, "floor": floor}));
    }
    if let Some((lo, hi)) = round_ci {
        if lo > 0.0 || hi < 0.0 {
            // CI excludes zero; check sign agreement with deterministic.
            let wall_sign = if lo > 0.0 { 1.0 } else { -1.0 };
            if wall_sign * det_sign < 0.0 {
                return verdict(false, json!({"reason": "instrument_disagreement"}));
            }
            return verdict(true, json!({"ci_low": lo, "ci_high": hi}));
        }
        return verdict(false, json!({"reason": "ci_includes_zero", "ci_low": lo, "ci_high": hi}));
    }
    verdict(true, json!({"det_gain": det_gain}))
}

#[derive(Debug, Clone, Default)]
pub struct DiffSummary {
    /// Files touched (repo-relative).
    pub files: Vec<String>,
    /// Added lines (for structural special-case detection).
    pub added_lines: Vec<String>,
    /// Removed public API symbols (empty when API intact).
    pub removed_api: Vec<String>,
    /// Added dependencies.
    pub added_deps: Vec<String>,
}

/// Structural scope gate: no oracle/benchmark files; no NEW input-size/value/
/// identity conditionals absent from the original; public API byte-identical;
/// allowlisted deps only.
pub fn optimization_scope(
    diff: &DiffSummary,
    original_branch_count: usize,
    allowlisted_deps: &[String],
) -> GateVerdict {
    // NOTE: build-file normalization (build manifests etc.) is owned by the
    // runner's `normalize_for_hash` (ADR-002); this gate only rejects frozen
    // test/bench paths.
    for f in &diff.files {
        if f.contains("test/") || f.contains("bench") {
            return verdict(false, json!({"reason": "touches_frozen_file", "file": f}));
        }
    }
    // Structural special-case detector: added SHAPE branches (input size /
    // capacity selecting different code paths). Bare value/identity equality
    // without a shape term is the workload-divergence gate's job (statistical),
    // not this gate's (structural) — see the fixture-cache plant.
    let mut new_branches = 0;
    for line in &diff.added_lines {
        let t = line.trim();
        if (t.starts_with("if ") || t.starts_with("if(") || t.contains("match "))
            && (t.contains("len(")
                || t.contains("len()")
                || t.contains(".len()")
                || t.contains("size")
                || t.contains("capacity"))
        {
            new_branches += 1;
        }
    }
    if new_branches > original_branch_count {
        return verdict(
            false,
            json!({"reason": "input_size_branch", "new_branches": new_branches}),
        );
    }
    if !diff.removed_api.is_empty() {
        return verdict(false, json!({"reason": "api_changed", "removed": diff.removed_api}));
    }
    for d in &diff.added_deps {
        if !allowlisted_deps.iter().any(|a| a == d) {
            return verdict(false, json!({"reason": "non_allowlisted_dep", "dep": d}));
        }
    }
    verdict(true, json!({"files": diff.files.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn man(files: Vec<(&str, &str)>, base: Baseline) -> Manifest {
        use rustsmith_core::{Cwd, TestCommand};
        Manifest {
            version: 2,
            runner: "pytest".into(),
            languages: vec!["python".into()],
            prepare: vec![],
            invocation: vec!["pytest test/unit", "pytest test/integration"]
                .into_iter()
                .map(|s| TestCommand {
                    program: "sh".into(),
                    args: vec!["-c".into(), s.into()],
                    cwd: Cwd::Tree,
                    env_set: vec![],
                    env_remove: vec![],
                    launcher: None,
                    timeout_secs: None,
                    collect: vec![],
                })
                .collect(),
            config_hash: String::new(),
            observables: vec![],
            files: files
                .into_iter()
                .map(|(p, h)| FileHash {
                    path: Utf8PathBuf::from(p),
                    sha256: h.into(),
                })
                .collect(),
            baseline: base,
        }
    }

    #[test]
    fn integrity_ok_and_hash_mismatch() {
        let base = Baseline {
            test_count: 2,
            skipped: vec![],
            xfailed: vec![],
            deselected: vec![],
        };
        let m = man(vec![("test/test_x.py", "abc")], base.clone());
        let got = GradedResult {
            exit_code: 0,
            passed: 2,
            failed: 0,
            skipped: vec![],
            xfailed: vec![],
            deselected: vec![],
            stdout: String::new(),
            outcomes: Default::default(),
        };
        let tree = vec![FileHash {
            path: Utf8PathBuf::from("test/test_x.py"),
            sha256: "abc".into(),
        }];
        assert!(oracle_integrity(&m, &tree, &base, &got).passed);
        let bad = vec![FileHash {
            path: Utf8PathBuf::from("test/test_x.py"),
            sha256: "tampered".into(),
        }];
        assert!(!oracle_integrity(&m, &bad, &base, &got).passed);
    }

    #[test]
    fn integrity_catches_count_skip_xfail_deselect_drift() {
        let base = Baseline {
            test_count: 3,
            skipped: vec![],
            xfailed: vec![],
            deselected: vec![],
        };
        let m = man(vec![], base.clone());
        // skip added: passed drops, skipped grows, total same but skip list differs
        let got = GradedResult {
            exit_code: 0,
            passed: 2,
            failed: 0,
            skipped: vec!["test_a".into()],
            xfailed: vec![],
            deselected: vec![],
            stdout: String::new(),
            outcomes: Default::default(),
        };
        assert!(!oracle_integrity(&m, &[], &base, &got).passed);
        // count drift
        let got2 = GradedResult {
            exit_code: 1,
            passed: 1,
            failed: 1,
            skipped: vec![],
            xfailed: vec![],
            deselected: vec![],
            stdout: String::new(),
            outcomes: Default::default(),
        };
        assert!(!oracle_integrity(&m, &[], &base, &got2).passed);
    }
    #[test]
    fn integrity_catches_same_count_different_tests() {
        use std::collections::BTreeMap;
        // Baseline: 2 passed + 1 skipped. Every legacy check (count, skip,
        // xfail, deselect vectors) passes below; only the outcomes key set
        // reveals the drift, which pre-fix code missed.
        let base = Baseline {
            test_count: 3,
            skipped: vec!["test_ghost".to_string()],
            xfailed: vec![],
            deselected: vec![],
        };
        let m = man(vec![], base.clone());
        let mut outcomes = BTreeMap::new();
        outcomes.insert("test_a".to_string(), Outcome::Pass);
        outcomes.insert("test_renamed".to_string(), Outcome::Pass);
        let mut got = GradedResult::from_outcomes(0, outcomes, String::new());
        // Claim the baseline skip list while the map holds no Skip entry:
        // same count (3), same vectors, different test identities.
        got.skipped = vec!["test_ghost".to_string()];
        assert_eq!(got.total(), base.test_count);
        assert!(!oracle_integrity(&m, &[], &base, &got).passed);
        // Consistent map + vectors pass.
        let mut outcomes_ok = BTreeMap::new();
        outcomes_ok.insert("test_a".to_string(), Outcome::Pass);
        outcomes_ok.insert("test_b".to_string(), Outcome::Pass);
        outcomes_ok.insert("test_ghost".to_string(), Outcome::Skip);
        let got_ok = GradedResult::from_outcomes(0, outcomes_ok, String::new());
        assert!(oracle_integrity(&m, &[], &base, &got_ok).passed);
    }

    #[test]
    fn differential_per_observable_tol() {
        let pairs = vec![("1.000".to_string(), "1.005".to_string())];
        // Legacy path: 0.0 tolerance demands exact equality.
        assert!(!differential(&pairs, 0.0, None).passed);
        // Per-observable 1% relative tolerance accepts the same pair.
        let spec = ObservableSpec {
            test_glob: "*".into(),
            source: "stdout".into(),
            regex: ".*".into(),
            rel_tol: 0.01,
            abs_tol: 0.0,
        };
        assert!(differential(&pairs, 0.0, Some(&[spec])).passed);
    }

    #[test]
    fn parity_and_divergence_math() {
        let ok = GradedResult {
            exit_code: 0,
            passed: 80,
            failed: 0,
            skipped: vec![],
            xfailed: vec![],
            deselected: vec![],
            stdout: String::new(),
            outcomes: Default::default(),
        };
        assert!(oracle_parity(&ok).passed);
        let bad = GradedResult { failed: 1, ..ok.clone() };
        assert!(!oracle_parity(&bad).passed);
        assert!(heldout_divergence(1.0, 0.99, 0.05).passed);
        let v = heldout_divergence(1.0, 0.90, 0.05);
        assert!(!v.passed);
        assert!((v.detail["divergence"].as_f64().unwrap() - 0.10).abs() < 1e-9);
    }

    #[test]
    fn unsafe_budget_requires_ffi_and_safety() {
        assert!(
            unsafe_budget(
                &[],
                5.0,
                10
            )
            .passed
        );
        let bad = vec![UnsafeSite {
            file: "src/lib.rs".into(),
            is_ffi_boundary: false,
            has_safety_comment: true,
        }];
        assert!(!unsafe_budget(&bad, 5.0, 10).passed);
        let bad2 = vec![UnsafeSite {
            file: "src/lib.rs".into(),
            is_ffi_boundary: true,
            has_safety_comment: false,
        }];
        assert!(!unsafe_budget(&bad2, 5.0, 10).passed);
    }

    #[test]
    fn workload_divergence_redacts_magnitudes() {
        // Plant: fixture-keyed cache helps visible (+20%) but not held-out (+1%).
        let v = workload_divergence(0.20, 0.01, 0.10, 0.01);
        assert!(!v.passed);
        // Worker-visible detail must carry NO magnitudes (gradient prevention).
        assert!(is_worker_safe(&v.detail));
        // Honest gain passes with magnitudes visible to the control plane.
        let ok = workload_divergence(0.15, 0.14, 0.10, 0.01);
        assert!(ok.passed);
    }

    #[test]
    fn attribution_rejects_phantoms() {
        // No-effect change in an improving round: gain persists after revert.
        assert!(!causal_attribution(0.05, 0.048, 0.005).passed);
        // Real change: gain disappears on revert.
        assert!(causal_attribution(0.35, 0.002, 0.005).passed);
    }

    #[test]
    fn widened_regression_catches_rss_for_speed() {
        let base = ResourceSnapshot { rss_bytes: 100_000, alloc_count: Some(1_000), binary_bytes: 1_000_000, compile_secs: 10.0 };
        // Plant: 8% faster but 3x RSS.
        let got = ResourceSnapshot { rss_bytes: 300_000, alloc_count: Some(900), binary_bytes: 1_000_000, compile_secs: 10.0 };
        let v = no_regression_widened(&base, &got, true, true);
        assert!(!v.passed);
        assert_eq!(v.detail["reason"], "rss_regression");
        // Clean candidate passes.
        let ok = ResourceSnapshot { rss_bytes: 101_000, alloc_count: Some(950), binary_bytes: 1_000_000, compile_secs: 10.0 };
        assert!(no_regression_widened(&base, &ok, true, true).passed);
        // Profiler unavailable on either side: alloc leg skipped, never failed.
        let no_prof = ResourceSnapshot { rss_bytes: 101_000, alloc_count: None, binary_bytes: 1_000_000, compile_secs: 10.0 };
        assert!(no_regression_widened(&base, &no_prof, true, true).passed);
        assert!(no_regression_widened(&no_prof, &no_prof, true, true).passed);
    }

    #[test]
    fn benchmark_restated_needs_ci_and_sign() {
        assert!(!benchmark_restated(0.001, 0.01, None, 1.0).passed); // below floor
        assert!(!benchmark_restated(0.05, 0.01, Some((-0.01, 0.03)), 1.0).passed); // CI has zero
        assert!(!benchmark_restated(0.05, 0.01, Some((0.02, 0.06)), -1.0).passed); // sign clash
        assert!(benchmark_restated(0.05, 0.01, Some((0.02, 0.06)), 1.0).passed);
    }

    #[test]
    fn scope_catches_size_branch() {
        let diff = DiffSummary {
            files: vec!["src/lib.rs".into()],
            added_lines: vec!["if data.len() < 64 { fast_path() }".into()],
            removed_api: vec![],
            added_deps: vec![],
        };
        assert!(!optimization_scope(&diff, 0, &[]).passed);
        let clean = DiffSummary {
            files: vec!["src/lib.rs".into()],
            added_lines: vec!["let x = table[idx] ^ (reg << 8);".into()],
            removed_api: vec![],
            added_deps: vec![],
        };
        assert!(optimization_scope(&clean, 0, &[]).passed);
    }

    #[test]
    fn scope_leaves_identity_checks_to_divergence() {
        // Fixture-identity cache branch: no shape term -> scope passes;
        // workload_divergence (statistical) is the catcher.
        let diff = DiffSummary {
            files: vec!["src/lib.rs".into()],
            added_lines: vec!["if bytes.first() == Some(&0x41) { cache_hit() }".into()],
            removed_api: vec![],
            added_deps: vec![],
        };
        assert!(optimization_scope(&diff, 0, &[]).passed);
    }

    #[test]
    fn provenance_needs_headers_notice_attribution() {
        assert!(provenance(true, true, "Nicoretti").passed);
        assert!(!provenance(false, true, "Nicoretti").passed);
        assert!(!provenance(true, false, "Nicoretti").passed);
        assert!(!provenance(true, true, "").passed);
    }
}
