use rustsmith_core::{Baseline, FileHash, GradedResult, Manifest};
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
pub fn differential(pairs: &[(String, String)], tolerance: f64) -> GateVerdict {
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

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn man(files: Vec<(&str, &str)>, base: Baseline) -> Manifest {
        Manifest {
            version: 1,
            invocation: vec!["pytest test/unit".into(), "pytest test/integration".into()],
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
        };
        assert!(!oracle_integrity(&m, &[], &base, &got2).passed);
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
}
