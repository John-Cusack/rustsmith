use camino::Utf8PathBuf;
use rustsmith_core::{Baseline, FileHash, GradedResult, HaltReason, Manifest};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OracleError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("pytest failed: {0}")]
    Pytest(String),
    #[error("halt: {0}")]
    Halt(HaltReason),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("utf8: {0}")]
    Utf8(String),
}

/// Host-only held-out suite. Never mounted into containers.
#[derive(Debug, Clone)]
pub struct HeldoutSuite {
    /// Directory on host containing held-out tests (never copied into grading image).
    pub path: PathBuf,
    pub threshold: f64,
}

impl HeldoutSuite {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            threshold: 0.05,
        }
    }
}

pub struct Oracle;

impl Oracle {
    /// Freeze oracle: hash tests + configs + fixtures/data + CI workflow; record invocation + baseline.
    pub fn freeze(repo_root: &Path) -> Result<Manifest, OracleError> {
        let files = discover_oracle_files(repo_root)?;
        let mut hashed = Vec::new();
        for rel in files {
            let abs = repo_root.join(&rel);
            let bytes = std::fs::read(&abs)?;
            let digest = sha256_hex(&bytes);
            hashed.push(FileHash {
                path: Utf8PathBuf::from(rel),
                sha256: digest,
            });
        }
        hashed.sort_by(|a, b| a.path.cmp(&b.path));
        let invocation = detect_invocation(repo_root);
        let baseline = measure_baseline(repo_root, &invocation)?;
        Ok(Manifest {
            version: 1,
            invocation,
            files: hashed,
            baseline,
        })
    }

    /// Mismatch => Halt OracleTamper. Missing => tamper. Extra files in tree are allowed.
    pub fn verify_hashes(manifest: &Manifest, tree: &Path) -> Result<(), OracleError> {
        for f in &manifest.files {
            let abs = tree.join(f.path.as_str());
            let bytes = std::fs::read(&abs).map_err(|_| {
                OracleError::Halt(HaltReason::OracleTamper {
                    path: f.path.to_string(),
                })
            })?;
            let digest = sha256_hex(&bytes);
            if digest != f.sha256 {
                return Err(OracleError::Halt(HaltReason::OracleTamper {
                    path: f.path.to_string(),
                }));
            }
        }
        Ok(())
    }
    /// Re-hash the manifest-listed files in `tree`. Missing files are skipped
    /// (verify_hashes reports them as tamper); extras are ignored.
    pub fn current_hashes(manifest: &Manifest, tree: &Path) -> Vec<FileHash> {
        let mut out = Vec::new();
        for f in &manifest.files {
            let abs = tree.join(f.path.as_str());
            if let Ok(bytes) = std::fs::read(&abs) {
                out.push(FileHash { path: f.path.clone(), sha256: sha256_hex(&bytes) });
            }
        }
        out
    }
    /// Run the manifest invocation on `tree` WITHOUT hash verification.
    /// The caller evaluates `oracle_integrity` (counts-first) to get the
    /// precise failure reason. Used by `rustsmith grade` and acceptance probes.
    pub fn measure_tree(manifest: &Manifest, tree: &Path) -> Result<GradedResult, OracleError> {
        run_invocation(tree, &manifest.invocation)
    }

    /// Authoritative graded run (host-side; container isolation proved separately by
    /// `rustsmith-sandbox::grading_run`). Returns (visible result, divergence).
    pub fn graded_run(
        manifest: &Manifest,
        artifact: &Path,
        heldout: &HeldoutSuite,
    ) -> Result<(GradedResult, f64), OracleError> {
        Self::verify_hashes(manifest, artifact)?;
        let visible = run_invocation(artifact, &manifest.invocation)?;
        let heldout_result = run_heldout(artifact, heldout)?;
        let visible_rate = visible.pass_rate();
        let heldout_rate = heldout_result.pass_rate();
        let divergence = visible_rate - heldout_rate;
        Ok((visible, divergence))
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Files that define correctness. bench/ explicitly excluded (not oracle).
pub fn discover_oracle_files(repo_root: &Path) -> Result<Vec<String>, OracleError> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(repo_root)
        .into_iter()
        .filter_entry(|e| {
            let p = e.path();
            // Skip VCS, caches, build output, venvs.
            for seg in [".git", "__pycache__", ".pytest_cache", ".venv", "venv", "dist", ".eggs", "target"] {
                if p.components().any(|c| c.as_os_str() == seg) {
                    return false;
                }
            }
            // bench/ is NOT oracle.
            if p.components().any(|c| c.as_os_str() == "bench") {
                // Allow the directory itself but skip .py files under it.
                if p.extension().map(|x| x == "py").unwrap_or(false) {
                    return false;
                }
            }
            true
        })
    {
        let entry = entry.map_err(|e| OracleError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let rel = p.strip_prefix(repo_root).unwrap().to_string_lossy().replace('\\', "/");
        let is_test = rel.contains("test_")
            || rel.ends_with("_test.py")
            || rel.ends_with("conftest.py")
            || rel.contains("/test/")
            || rel.starts_with("test/");
        let is_config = matches!(
            rel.as_str(),
            "pytest.ini" | "tox.ini" | "setup.cfg" | "pyproject.toml"
        );
        let is_fixture_data = (rel.contains("fixture") || rel.contains("data"))
            && (rel.contains("test"));
        let is_ci = rel.starts_with(".github/workflows/");
        if is_test || is_config || is_fixture_data || is_ci {
            // Exclude bench files even if they match test patterns.
            if rel.contains("bench") {
                continue;
            }
            out.push(rel);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub fn detect_invocation(repo_root: &Path) -> Vec<String> {
    let unit = repo_root.join("test/unit");
    let integ = repo_root.join("test/integration");
    if unit.is_dir() && integ.is_dir() {
        return vec!["pytest test/unit".into(), "pytest test/integration".into()];
    }
    vec!["pytest".into()]
}

fn src_env(repo_root: &Path) -> Option<PathBuf> {
    let src = repo_root.join("src");
    if src.is_dir() {
        Some(src)
    } else {
        None
    }
}

fn run_pytest(repo_root: &Path, args: &[&str]) -> Result<GradedResult, OracleError> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-m").arg("pytest");
    for a in args {
        cmd.arg(a);
    }
    cmd.arg("-v").arg("-rs").arg("-rxX").arg("--tb=short");
    cmd.current_dir(repo_root);
    if let Some(src) = src_env(repo_root) {
        let mut pp: std::ffi::OsString = src.into_os_string();
        if let Some(old) = std::env::var_os("PYTHONPATH") {
            pp.push(":");
            pp.push(old);
        }
        cmd.env("PYTHONPATH", pp);
    }
    cmd.env("PY_COLORS", "0");
    let out = cmd.output().map_err(|e| OracleError::Pytest(e.to_string()))?;
    let stdout = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(parse_pytest_verbose(&stdout, out.status.code().unwrap_or(-1)))
}

fn run_invocation(repo_root: &Path, invocation: &[String]) -> Result<GradedResult, OracleError> {
    let mut agg = GradedResult {
        exit_code: 0,
        passed: 0,
        failed: 0,
        skipped: vec![],
        xfailed: vec![],
        deselected: vec![],
        stdout: String::new(),
    };
    for cmd in invocation {
        // cmd like "pytest test/unit" -> args ["test/unit"]
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let args: Vec<&str> = parts.into_iter().skip(1).collect();
        let r = run_pytest(repo_root, &args)?;
        if r.exit_code != 0 && r.failed > 0 {
            agg.exit_code = r.exit_code;
        } else if r.exit_code != 0 && agg.exit_code == 0 {
            agg.exit_code = r.exit_code;
        }
        agg.passed += r.passed;
        agg.failed += r.failed;
        agg.skipped.extend(r.skipped);
        agg.xfailed.extend(r.xfailed);
        agg.deselected.extend(r.deselected);
        agg.stdout.push_str(&format!("$ {cmd}\n{}\n", r.stdout));
    }
    agg.skipped.sort();
    agg.xfailed.sort();
    agg.deselected.sort();
    Ok(agg)
}

fn run_heldout(artifact: &Path, heldout: &HeldoutSuite) -> Result<GradedResult, OracleError> {
    if !heldout.path.is_dir() {
        // Empty held-out suite counts as full pass (M0 stub before generator exists in M3).
        return Ok(GradedResult {
            exit_code: 0,
            passed: 1,
            failed: 0,
            skipped: vec![],
            xfailed: vec![],
            deselected: vec![],
            stdout: "empty heldout: pass".into(),
        });
    }
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-m")
        .arg("pytest")
        .arg(&heldout.path)
        .arg("-v")
        .arg("--tb=short");
    cmd.current_dir(artifact);
    if let Some(src) = src_env(artifact) {
        let mut pp: std::ffi::OsString = src.into_os_string();
        if let Some(old) = std::env::var_os("PYTHONPATH") {
            pp.push(":");
            pp.push(old);
        }
        // Held-out imports original package from artifact's src.
        cmd.env("PYTHONPATH", pp);
    }
    cmd.env("PY_COLORS", "0");
    let out = cmd.output().map_err(|e| OracleError::Pytest(e.to_string()))?;
    let stdout = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(parse_pytest_verbose(&stdout, out.status.code().unwrap_or(-1)))
}

/// Parse `pytest -v` output: counts + skipped/xfailed/deselected test IDs.
pub fn parse_pytest_verbose(stdout: &str, exit_code: i32) -> GradedResult {
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = Vec::new();
    let mut xfailed = Vec::new();
    let mut deselected = Vec::new();
    for line in stdout.lines() {
        let t = line.trim();
        // Verbose per-test lines: "test_x.py::Test::test_y PASSED" / SKIPPED / FAILED / XFAIL
        if t.contains(" PASSED") || t.contains(" passed") {
            // Avoid double counting summary lines; only count lines with :: (test node ids).
            if t.contains("::") {
                passed += 1;
            }
        } else if t.contains(" FAILED") || t.contains(" ERROR") {
            if t.contains("::") {
                failed += 1;
            }
        } else if t.contains(" SKIPPED") {
            if let Some(id) = t.split_whitespace().next() {
                if id.contains("::") {
                    skipped.push(id.to_string());
                }
            }
        } else if t.contains(" XFAIL") {
            if let Some(id) = t.split_whitespace().next() {
                if id.contains("::") {
                    xfailed.push(id.to_string());
                }
            }
        } else if t.contains(" DSELECTED") || t.contains("deselected") {
            if let Some(id) = t.split_whitespace().next() {
                if id.contains("::") {
                    deselected.push(id.to_string());
                }
            }
        }
        // Short summary info lines: "SKIPPED [1] file:line: reason"
        if t.starts_with("SKIPPED") && t.contains(':') {
            // These duplicate the per-test SKIPPED above; skip to avoid double count.
        }
    }
    // Fallback: parse summary line "80 passed, 28 subtests passed in 0.18s" / "1 failed, 79 passed".
    let (s_passed, s_failed, s_skipped, s_xfail, s_deselect) = parse_summary_counts(stdout);
    // Prefer verbose per-test counts when nonzero, else summary.
    if passed == 0 && failed == 0 {
        passed = s_passed;
        failed = s_failed;
    } else {
        // If verbose undercounts parametrized summary (e.g. subtests), take max.
        if s_passed > passed {
            passed = s_passed;
        }
        if s_failed > failed {
            failed = s_failed;
        }
    }
    if skipped.is_empty() && s_skipped > 0 {
        for i in 0..s_skipped {
            skipped.push(format!("skipped[{i}]"));
        }
    }
    if xfailed.is_empty() && s_xfail > 0 {
        for i in 0..s_xfail {
            xfailed.push(format!("xfailed[{i}]"));
        }
    }
    if deselected.is_empty() && s_deselect > 0 {
        for i in 0..s_deselect {
            deselected.push(format!("deselected[{i}]"));
        }
    }
    skipped.sort();
    xfailed.sort();
    deselected.sort();
    GradedResult {
        exit_code,
        passed,
        failed,
        skipped,
        xfailed,
        deselected,
        stdout: stdout.to_string(),
    }
}

fn parse_summary_counts(s: &str) -> (u32, u32, u32, u32, u32) {
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = 0u32;
    let mut xfailed = 0u32;
    let mut deselected = 0u32;
    for line in s.lines() {
        let l = line.to_lowercase();
        // Only consider summary-ish lines to avoid matching test names.
        if !(l.contains("passed") || l.contains("failed") || l.contains("skipped") || l.contains("xfailed") || l.contains("deselected") || l.contains("error")) {
            continue;
        }
        // Tokenize "80 passed, 1 skipped, 2 xfailed, 1 deselected"
        let toks: Vec<&str> = l.split(|c: char| c == ',' || c == ' ' || c == '=').filter(|t| !t.is_empty()).collect();
        let mut i = 0;
        while i < toks.len() {
            if let Ok(n) = toks[i].parse::<u32>() {
                if i + 1 < toks.len() {
                    match toks[i + 1] {
                        w if w.starts_with("passed") => passed = passed.max(n),
                        w if w.starts_with("failed") => failed = failed.max(n),
                        w if w.starts_with("skipped") => skipped = skipped.max(n),
                        w if w.starts_with("xfailed") || w.starts_with("xpassed") => xfailed = xfailed.max(n),
                        w if w.starts_with("deselected") => deselected = deselected.max(n),
                        _ => {}
                    }
                }
            }
            i += 1;
        }
    }
    // Dedupe risk: multiple invocation outputs concatenated; max is right for per-invocation sums?
    // run_invocation sums per-invocation parses, so max-per-chunk is correct.
    let _ = HashSet::<u32>::new();
    (passed, failed, skipped, xfailed, deselected)
}

pub fn collect_baseline_lists(repo_root: &Path, invocation: &[String]) -> Result<Baseline, OracleError> {
    let r = run_invocation(repo_root, invocation)?;
    Ok(Baseline {
        test_count: r.total(),
        skipped: r.skipped,
        xfailed: r.xfailed,
        deselected: r.deselected,
    })
}

pub fn measure_baseline(repo_root: &Path, invocation: &[String]) -> Result<Baseline, OracleError> {
    collect_baseline_lists(repo_root, invocation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_summary_with_subtests() {
        let out = "test_x.py::a PASSED\ntest_x.py::b PASSED\n80 passed, 28 subtests passed in 0.18s\n";
        let r = parse_pytest_verbose(out, 0);
        assert_eq!(r.passed, 80);
    }

    #[test]
    fn parses_skipped_verbose() {
        let out = "test_x.py::Test::test_a SKIPPED\n1 skipped in 0.01s\n";
        let r = parse_pytest_verbose(out, 0);
        assert_eq!(r.skipped.len(), 1);
    }
}
