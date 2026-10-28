//! Agent-lane capture (ADR-055, audit gaps 1, 2, 4, 5, 7).
//!
//! `grade` stays a read-only probe. This command is the only writer for
//! lane-local training capture: scoped `run`/`unit`/`gate_results`/`hunks`
//! rows into a lane-local `store.db`, `(run,unit,session_id,omp,phoenix)`
//! appends to `sessions.csv`, and `lane.toml` sidecars for rights/pins.
//! Builds and venvs live under a durable lane scratch dir, never `/tmp`.

use super::flag;
use rustsmith_core::{Event, Gate};
use rustsmith_report::evidence;
use rustsmith_store::{HunkRow, Store};
use std::path::{Path, PathBuf};

pub fn cmd_lane_capture(args: &[String]) -> Result<(), String> {
    match args.first().map(|s| s.as_str()) {
        Some("init") => cmd_init(&args[1..]),
        Some("grade-unit") => cmd_grade_unit(&args[1..]),
        Some("record-failed") => cmd_record_failed(&args[1..]),
        Some("session") => cmd_session(&args[1..]),
        Some("finish") => cmd_finish(&args[1..]),
        _ => Err("usage: rustsmith lane-capture <init|grade-unit|record-failed|session|finish> …".into()),
    }
}

fn store_at(path: &str) -> Result<(Store, PathBuf), String> {
    let db = PathBuf::from(path);
    let dir = db
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    if let Some(p) = dir.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("bad store dir: {e}"))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("bad store dir: {e}"))?;
    let store = Store::open(&db).map_err(|e| e.to_string())?;
    Ok((store, dir))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Refuse evaporating build roots (audit gap 4: lanes built in `/tmp`).
fn durable_dir(path: &str) -> Result<PathBuf, String> {
    let tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
    let p = path.to_string();
    if p == "/tmp" || p.starts_with("/tmp/") || p == tmp || p.starts_with(&format!("{tmp}/")) {
        return Err(format!("refusing scratch under /tmp (evaporates on reboot): {path}"));
    }
    let dir = PathBuf::from(path);
    std::fs::create_dir_all(&dir).map_err(|e| format!("bad --scratch {path}: {e}"))?;
    Ok(dir)
}

fn toml_line(key: &str, value: &str) -> Result<String, String> {
    if value.contains('"') || value.contains('\n') {
        return Err(format!("--{key} must not contain quotes or newlines"));
    }
    Ok(format!("{key} = \"{value}\"\n"))
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

/// `lane-capture init` writes `lane.toml` + `sessions.csv` next to the store
/// and ensures the durable scratch dir. Idempotent (re-init keeps the run).
#[allow(clippy::too_many_arguments)]
fn cmd_init(args: &[String]) -> Result<(), String> {
    let store_path = flag(args, "--store").ok_or("missing --store")?;
    let run_id = flag(args, "--run").ok_or("missing --run")?;
    let repo = flag(args, "--repo").ok_or("missing --repo")?;
    let lang = flag(args, "--lang").ok_or("missing --lang")?;
    let (store, dir) = store_at(&store_path)?;
    store.create_run(&run_id, &repo, &lang, "lane").map_err(|e| e.to_string())?;

    let scratch = match flag(args, "--scratch") {
        Some(s) => durable_dir(&s)?,
        None => {
            let d = dir.join("scratch");
            std::fs::create_dir_all(&d).map_err(|e| format!("bad default scratch: {e}"))?;
            d
        }
    };

    let mut toml = "# lane-capture sidecar (ADR-055). Highest-priority export source.\n".to_string();
    if let Some(v) = flag(args, "--lane") {
        toml.push_str(&toml_line("lane", &v)?);
    }
    if let Some(v) = flag(args, "--package") {
        toml.push_str(&toml_line("package", &v)?);
    }
    for (k, v) in [
        ("upstream_repo", flag(args, "--upstream-repo")),
        ("upstream_commit", flag(args, "--upstream-commit")),
        ("upstream_license", flag(args, "--upstream-license")),
        ("record_license", flag(args, "--record-license")),
        ("rights_note", flag(args, "--rights-note")),
        ("attribution_text", flag(args, "--attribution-text")),
    ] {
        if let Some(v) = v {
            if !v.is_empty() {
                toml.push_str(&toml_line(k, &v)?);
            }
        }
    }
    toml.push_str(&toml_line("scratch", &scratch.display().to_string())?);
    std::fs::write(dir.join("lane.toml"), toml).map_err(|e| format!("lane.toml: {e}"))?;

    let sessions_path = flag(args, "--sessions").map(PathBuf::from).unwrap_or_else(|| dir.join("sessions.csv"));
    if !sessions_path.is_file() {
        std::fs::write(&sessions_path, "run,unit,session_id,omp,phoenix,tripwire\n")
            .map_err(|e| format!("sessions.csv: {e}"))?;
    }
    println!(
        "{}",
        serde_json::json!({
            "run": run_id, "store": store_path,
            "lane_toml": dir.join("lane.toml").display().to_string(),
            "sessions": sessions_path.display().to_string(),
            "scratch": scratch.display().to_string(),
        })
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// grade-unit
// ---------------------------------------------------------------------------

fn parse_gate(name: &str) -> Result<Gate, String> {
    match name {
        "oracle_parity" => Ok(Gate::OracleParity),
        "oracle_integrity" => Ok(Gate::OracleIntegrity),
        "benchmark" => Ok(Gate::Benchmark),
        "heldout_divergence" => Ok(Gate::HeldoutDivergence),
        "differential" => Ok(Gate::Differential),
        _ => Err(format!("bad --gate {name}: want oracle_parity|oracle_integrity|benchmark|heldout_divergence|differential")),
    }
}

/// Detail blob for one evidence conversion; `wall_secs` rides along when the
/// caller measured it (export maps it to `provenance.wall_seconds`).
fn with_wall(mut detail: serde_json::Value, wall_secs: Option<f64>) -> serde_json::Value {
    if let (Some(w), Some(o)) = (wall_secs, detail.as_object_mut()) {
        o.insert("wall_secs".to_string(), serde_json::json!(w));
    }
    detail
}

/// One evidence file → `(gate, passed, detail)` rows. Scope-shaped evidence
/// (scope.json / gate-verdicts) carries no oracle counts: it only fills the
/// unit spec verdict (returned separately) and records no gate.
enum EvidenceOut {
    Gates(Vec<(Gate, bool, serde_json::Value)>),
    Scope { verdict: String, evidence: String },
}

fn evidence_gates(
    kind: &str,
    evidence: &Path,
    expected: Option<&Path>,
    wall_secs: Option<f64>,
    unit: &str,
) -> Result<EvidenceOut, String> {
    match kind {
        "equiv-pair" => {
            let exp = expected.ok_or("equiv-pair needs --expected <pure-side jsonl>")?;
            let s = evidence::read_equiv_pair(exp, evidence).map_err(|e| e.to_string())?;
            let passed = s.mismatched == 0 && s.missing == 0;
            Ok(EvidenceOut::Gates(vec![(
                Gate::OracleParity,
                passed,
                with_wall(serde_json::json!({"passed": s.matched, "total": s.total, "mismatched": s.mismatched, "missing": s.missing, "source": "equiv-pair"}), wall_secs),
            )]))
        }
        "verification" => {
            let s = evidence::read_verification(evidence).map_err(|e| e.to_string())?;
            let total = s.stages.len() as i64;
            let passed_n = s.stages.iter().filter(|x| x.passed).count() as i64;
            Ok(EvidenceOut::Gates(vec![(
                Gate::OracleIntegrity,
                s.passed,
                with_wall(serde_json::json!({"passed": passed_n, "total": total, "source": "verification.json"}), wall_secs),
            )]))
        }
        "probe" => {
            let s = evidence::read_probe(evidence).map_err(|e| e.to_string())?;
            // Fingerprint, not a verdict: no gate rows. Loud on stderr so
            // the lane log records which fingerprint was attached.
            eprintln!(
                "lane-capture: probe {} {} score={:.4} verdict={} wall_secs={:.3}",
                s.package, s.version, s.score, s.verdict, s.wall_secs
            );
            Ok(EvidenceOut::Gates(vec![]))
        }
        "scope" => {
            let units = evidence::read_scope(evidence).map_err(|e| e.to_string())?;
            let u = units.iter().find(|u| u.id == unit)
                .ok_or_else(|| format!("unit {unit} not enumerated in {}", evidence.display()))?;
            Ok(EvidenceOut::Scope { verdict: u.verdict.clone(), evidence: u.evidence.clone() })
        }
        "gate-verdicts" => {
            let units = evidence::read_gate_verdicts(evidence).map_err(|e| e.to_string())?;
            let u = units.iter().find(|u| u.id == unit)
                .ok_or_else(|| format!("unit {unit} not enumerated in {}", evidence.display()))?;
            Ok(EvidenceOut::Scope { verdict: u.verdict.clone(), evidence: u.evidence.clone() })
        }
        "bench" => {
            let tracks = evidence::read_bench(evidence).map_err(|e| e.to_string())?;
            if tracks.is_empty() {
                return Err(format!("bench file has no tracks: {}", evidence.display()));
            }
            let total = tracks.len() as i64;
            let matched = tracks.iter().filter(|t| t.checksums_match).count() as i64;
            let min_speedup = tracks.iter().map(|t| t.speedup).fold(f64::INFINITY, f64::min);
            Ok(EvidenceOut::Gates(vec![
                (
                    Gate::Benchmark,
                    matched == total,
                    with_wall(serde_json::json!({"passed": matched, "total": total, "min_speedup": min_speedup, "source": "bench"}), wall_secs),
                ),
                (
                    Gate::OracleParity,
                    matched == total,
                    with_wall(serde_json::json!({"passed": matched, "total": total, "source": "bench-checksums"}), wall_secs),
                ),
            ]))
        }
        "scoped-grade" => {
            let g = evidence::read_scoped_grade(evidence).map_err(|e| e.to_string())?;
            // Keys match what the exporter counts: integrity reads
            // test_count/passed, parity reads passed/total (export.rs).
            let int_passed = if g.integrity_passed { g.integrity_tests } else { 0 };
            let par_passed = (g.integrity_tests - g.parity_failed).max(0);
            Ok(EvidenceOut::Gates(vec![
                (
                    Gate::OracleIntegrity,
                    g.integrity_passed,
                    with_wall(serde_json::json!({"passed": int_passed, "test_count": g.integrity_tests, "source": "scoped-grade"}), wall_secs),
                ),
                (
                    Gate::OracleParity,
                    g.parity_passed,
                    with_wall(serde_json::json!({"passed": par_passed, "total": g.integrity_tests, "failed": g.parity_failed, "failed_tests": g.failed_tests, "source": "scoped-grade"}), wall_secs),
                ),
            ]))
        }
        _ => Err(format!("bad --evidence-kind {kind}: want equiv-pair|verification|probe|scope|gate-verdicts|bench|scoped-grade")),
    }
}

/// Grade one lane unit from already-measured evidence (never re-runs tests).
/// Single-shot per unit: existing gate rows refuse unless the unit is fresh.
#[allow(clippy::too_many_arguments)]
fn cmd_grade_unit(args: &[String]) -> Result<(), String> {
    let store_path = flag(args, "--store").ok_or("missing --store")?;
    let run_id = flag(args, "--run").ok_or("missing --run")?;
    let unit = flag(args, "--unit").ok_or("missing --unit")?;
    let status = flag(args, "--status").ok_or("missing --status (passed|failed|parked|abandoned)")?;
    if !["passed", "failed", "parked", "abandoned"].contains(&status.as_str()) {
        return Err("bad --status: want passed|failed|parked|abandoned".into());
    }
    // parked = evidence recorded, grade deferred (probes, release checks
    // without oracle counts); abandoned = scope-refused or superseded.
    // Neither exports without gate rows carrying oracle counts.
    let commit_sha = flag(args, "--commit-sha").unwrap_or_default();
    if status == "passed" && commit_sha.is_empty() {
        return Err("passed units need --commit-sha (export requires it)".into());
    }
    let (store, _dir) = store_at(&store_path)?;

    let existing: Vec<_> = store
        .list_gate_results(&run_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|g| g.unit_id == unit)
        .collect();
    if !existing.is_empty() {
        return Err(format!("unit {unit} already has {} gate row(s): grade-unit is single-shot", existing.len()));
    }

    // Scope-shaped evidence pre-fills the spec verdict before any gate.
    let mut scope_verdict: Option<String> = flag(args, "--scope-verdict");
    let mut scope_evidence = flag(args, "--scope-evidence").unwrap_or_default();
    let mut gates: Vec<(Gate, bool, serde_json::Value)> = Vec::new();
    // Evidence files (repeatable --evidence PATH, each with --evidence-kind:
    // one kind applies to all, or zip N kinds with N files positionally).
    // --expected/--wall-secs repeat the same way (missing slots are None).
    fn all_flags(args: &[String], name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut it = args.iter().peekable();
        while let Some(a) = it.next() {
            if a == name {
                out.push(it.next().cloned().unwrap_or_default());
            } else if let Some(v) = a.strip_prefix(&format!("{name}=")) {
                out.push(v.to_string());
            }
        }
        out
    }
    let evidences = all_flags(args, "--evidence");
    let kinds = all_flags(args, "--evidence-kind");
    let expecteds = all_flags(args, "--expected");
    let walls = all_flags(args, "--wall-secs");
    if !evidences.is_empty() && kinds.is_empty() {
        return Err("missing --evidence-kind".into());
    }
    if kinds.len() > 1 && kinds.len() != evidences.len() {
        return Err("one --evidence-kind or one per --evidence".into());
    }
    if expecteds.len() > evidences.len() || walls.len() > evidences.len() {
        return Err("more --expected/--wall-secs than --evidence".into());
    }
    for (i, path) in evidences.iter().enumerate() {
        let kind = kinds.get(i).or_else(|| kinds.first()).cloned().unwrap_or_default();
        let expected = expecteds.get(i).map(|s| Path::new(s.as_str()));
        let wall_secs = walls
            .get(i)
            .map(|w| w.parse::<f64>().map_err(|_| format!("bad --wall-secs {w}")))
            .transpose()?;
        match evidence_gates(&kind, Path::new(path), expected, wall_secs, &unit)
            .map_err(|e| format!("evidence {}: {e}", Path::new(path).display()))?
        {
            EvidenceOut::Gates(rows) => gates.extend(rows),
            EvidenceOut::Scope { verdict, evidence } => {
                if scope_verdict.is_none() {
                    scope_verdict = Some(verdict.clone());
                }
                if scope_evidence.is_empty() {
                    scope_evidence = evidence;
                }
                if verdict != "admit" && status == "passed" {
                    return Err(format!("unit {unit} scope verdict is {verdict}: cannot grade passed"));
                }
            }
        }
    }
    // Explicit gate rows (repeatable --gate NAME:0|1:DETAIL_JSON).
    let mut explicit: Vec<String> = Vec::new();
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if a == "--gate" {
            explicit.push(it.next().cloned().ok_or("missing --gate value")?);
        } else if let Some(v) = a.strip_prefix("--gate=") {
            explicit.push(v.to_string());
        }
    }
    for g in explicit {
        let (name, rest) = g.split_once(':').ok_or(format!("bad --gate {g}: want NAME:0|1:DETAIL_JSON"))?;
        let (passed, detail) = rest.split_once(':').ok_or(format!("bad --gate {g}: want NAME:0|1:DETAIL_JSON"))?;
        let gate = parse_gate(name)?;
        let passed = match passed {
            "1" | "true" => true,
            "0" | "false" => false,
            _ => return Err(format!("bad --gate {g}: passed wants 0|1")),
        };
        let detail: serde_json::Value = serde_json::from_str(detail).map_err(|e| format!("bad --gate detail: {e}"))?;
        gates.push((gate, passed, detail));
    }

    let mut spec = serde_json::json!({"module": flag(args, "--spec-module").unwrap_or_else(|| unit.clone())});
    if let Some(v) = &scope_verdict {
        spec["scope_verdict"] = serde_json::Value::String(v.clone());
    }
    if !scope_evidence.is_empty() {
        let mut ev = scope_evidence.clone();
        ev.truncate(500);
        spec["scope_evidence"] = serde_json::Value::String(ev);
    }
    store.create_unit(&unit, &run_id, "mirror", &spec.to_string()).map_err(|e| e.to_string())?;
    store.set_unit_status(&unit, "running").map_err(|e| e.to_string())?;
    if let Some(t) = flag(args, "--tokens") {
        let (tin, tout) = t.split_once(',').ok_or("--tokens wants IN,OUT")?;
        store
            .set_unit_tokens(&unit, tin.parse().map_err(|_| "--tokens IN not a number")?, tout.parse().map_err(|_| "--tokens OUT not a number")?)
            .map_err(|e| e.to_string())?;
    }
    store.set_unit_status(&unit, "gated").map_err(|e| e.to_string())?;
    for (gate, passed, detail) in &gates {
        store.record_gate(&unit, *gate, *passed, detail).map_err(|e| e.to_string())?;
    }
    if !commit_sha.is_empty() {
        store.set_unit_commit(&unit, &commit_sha).map_err(|e| e.to_string())?;
    }
    store.set_unit_status(&unit, &status).map_err(|e| e.to_string())?;
    // Attribution hunk on the grade-to-record transaction (gap 7). Mirror
    // convention: source names the hunk origin. before_ref needs a real ref:
    // upstream commit (or --hunk-before); absent means no hunk, never a fake.
    let mut hunk_id = None;
    if status == "passed" && flag(args, "--no-hunk").is_none() {
        let before = flag(args, "--hunk-before").unwrap_or_default();
        let after = flag(args, "--hunk-after").unwrap_or_else(|| commit_sha.clone());
        if !before.is_empty() && !after.is_empty() {
            let package = flag(args, "--package").unwrap_or_else(|| run_id.clone());
            hunk_id = Some(
                store
                    .record_hunk(&HunkRow {
                        run_id: run_id.clone(),
                        unit_id: unit.clone(),
                        package,
                        source: "lane-port".to_string(),
                        before_ref: before,
                        after_ref: after,
                        wall_delta_pct: None,
                        workload: flag(args, "--workload"),
                        verdict: "merged".to_string(),
                    })
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    store
        .append_event(&Event {
            ts: now(),
            run_id: run_id.clone(),
            kind: "lane_grade".to_string(),
            detail: serde_json::json!({"unit": unit, "status": status, "gates": gates.len()}),
        })
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({"unit": unit, "status": status, "gates": gates.len(), "hunk": hunk_id, "scope": scope_verdict})
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// record-failed (no_gain / gate_failed attempts as rows, gap 7)
// ---------------------------------------------------------------------------

fn cmd_record_failed(args: &[String]) -> Result<(), String> {
    let store_path = flag(args, "--store").ok_or("missing --store")?;
    let run_id = flag(args, "--run").ok_or("missing --run")?;
    let hotspot = flag(args, "--hotspot").ok_or("missing --hotspot")?;
    let outcome = flag(args, "--outcome").ok_or("missing --outcome (no_gain|gate_failed|…)")?;
    let (store, _dir) = store_at(&store_path)?;
    let patch_text = match flag(args, "--patch-file") {
        Some(p) => std::fs::read_to_string(&p).map_err(|e| format!("--patch-file: {e}"))?,
        None => String::new(),
    };
    if outcome != "rejected_at_proposal" && patch_text.is_empty() {
        return Err("non-proposal failures need --patch-file (empty patch exports only for rejected_at_proposal)".into());
    }
    let detail: serde_json::Value = match flag(args, "--detail-json") {
        Some(d) => serde_json::from_str(&d).map_err(|e| format!("bad --detail-json: {e}"))?,
        None => serde_json::json!({}),
    };
    store
        .record_failed(
            &run_id,
            flag(args, "--round").map(|r| r.parse().map_err(|_| "bad --round")).transpose()?.unwrap_or(0),
            &hotspot,
            &flag(args, "--bound").unwrap_or_default(),
            flag(args, "--tier").map(|r| r.parse().map_err(|_| "bad --tier")).transpose()?.unwrap_or(0),
            &flag(args, "--technique").unwrap_or_else(|| "lane-port".to_string()),
            &outcome,
            flag(args, "--gate").as_deref(),
            flag(args, "--measured-delta").map(|d| d.parse().map_err(|_| "bad --measured-delta")).transpose()?,
            &detail.to_string(),
            flag(args, "--tokens").map(|t| t.parse().map_err(|_| "bad --tokens")).transpose()?.unwrap_or(0),
            &flag(args, "--model").unwrap_or_default(),
            &flag(args, "--prompt-version").unwrap_or_default(),
            "",
            "",
            &flag(args, "--parent-sha").unwrap_or_default(),
            &patch_text,
        )
        .map_err(|e| e.to_string())?;
    println!("{}", serde_json::json!({"run": run_id, "hotspot": hotspot, "outcome": outcome}));
    Ok(())
}

// ---------------------------------------------------------------------------
// session
// ---------------------------------------------------------------------------

fn cmd_session(args: &[String]) -> Result<(), String> {
    let sessions_path = PathBuf::from(flag(args, "--sessions").ok_or("missing --sessions")?);
    let run = flag(args, "--run").ok_or("missing --run")?;
    let unit = flag(args, "--unit").ok_or("missing --unit")?;
    let get = |k: &str| flag(args, k).unwrap_or_default();
    for (k, v) in [("session-id", get("--session-id")), ("omp", get("--omp")), ("phoenix", get("--phoenix")), ("tripwire", get("--tripwire"))] {
        if v.contains(',') || v.contains('\n') {
            return Err(format!("--{k} must not contain commas or newlines (CSV join key)"));
        }
    }
    if let Some(p) = sessions_path.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("bad --sessions dir: {e}"))?;
    }
    if !sessions_path.is_file() {
        std::fs::write(&sessions_path, "run,unit,session_id,omp,phoenix,tripwire\n")
            .map_err(|e| format!("sessions.csv: {e}"))?;
    }
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new().append(true).open(&sessions_path).map_err(|e| format!("sessions.csv: {e}"))?;
    writeln!(f, "{},{},{},{},{},{}", run, unit, get("--session-id"), get("--omp"), get("--phoenix"), get("--tripwire"))
        .map_err(|e| format!("sessions.csv: {e}"))?;
    println!("{}", serde_json::json!({"sessions": sessions_path.display().to_string(), "run": run, "unit": unit}));
    Ok(())
}

// ---------------------------------------------------------------------------
// finish
// ---------------------------------------------------------------------------

fn cmd_finish(args: &[String]) -> Result<(), String> {
    let store_path = flag(args, "--store").ok_or("missing --store")?;
    let run_id = flag(args, "--run").ok_or("missing --run")?;
    let status = flag(args, "--status").unwrap_or_else(|| "passed".to_string());
    let (store, _dir) = store_at(&store_path)?;
    store.finish_run(&run_id, &status).map_err(|e| e.to_string())?;
    println!("{}", serde_json::json!({"run": run_id, "status": status}));
    Ok(())
}
