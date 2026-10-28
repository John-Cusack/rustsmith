//! Offline readers for the evidence formats port lanes already produce
//! (ADR-055, audit gap 6). Pure and deterministic: no store, no LLM, no
//! network. The lane helper (`lane-capture grade-unit`) converts these
//! summaries into `gate_results` rows; the exporter never reads evidence
//! directly (store rows stay authoritative).
//!
//! Formats (shapes verified against committed lane evidence 2026-10-28):
//! - `equiv/*.jsonl`: flat `{"key","status","value"}` rows per suite side
//!   (`broad/deep × pristine/pure/rust`).
//! - `verification.json`: single object, top-level `passed` + per-stage
//!   `{"passed":…}` objects (`python_install_smoke` is an array of checks).
//! - `probe.json`: cProfile headroom fingerprint (`score`, `verdict`,
//!   `wall_secs`, …).
//! - `scope.json`: single-line object with `units: [{id, verdict,
//!   evidence}]`; `gate-verdicts.json`: JSONL with `verdict: "admit" |
//!   {"refuse":[…]}`.
//! - `bench*.json`: array of `{track, pure_wall_s, rust_wall_s, speedup,
//!   pure_rss_kb, rust_rss_kb, checksum}`.
//! - `scoped-<unit>.json` (Elmer T011): single object with
//!   `integrity{passed,detail{test_count,…}}` and
//!   `parity{passed,detail{failed,…}}`.
//! - OMP session JSONL: `usage{input,output,cacheRead,cacheWrite,
//!   reasoningTokens,cost{total}}` on assistant `message` events
//!   (audit report §4 misnames it `model_usage`; the field is `usage`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvidenceError {
    #[error("io {0}: {1}")]
    Io(PathBuf, String),
    #[error("json {0}: {1}")]
    Json(PathBuf, String),
    #[error("shape {0}: {1}")]
    Shape(PathBuf, String),
}

fn read_file(path: &Path) -> Result<String, EvidenceError> {
    std::fs::read_to_string(path)
        .map_err(|e| EvidenceError::Io(path.to_path_buf(), e.to_string()))
}

fn parse_json(path: &Path, text: &str) -> Result<serde_json::Value, EvidenceError> {
    serde_json::from_str(text).map_err(|e| EvidenceError::Json(path.to_path_buf(), e.to_string()))
}

// ---------------------------------------------------------------------------
// equiv pairs: expected-vs-actual parity counts.
// ---------------------------------------------------------------------------

/// Parity summary of one `equiv` side-pair joined on `key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivSummary {
    pub total: i64,
    pub matched: i64,
    pub mismatched: i64,
    pub missing: i64,
}

fn equiv_side(path: &Path) -> Result<HashMap<String, serde_json::Value>, EvidenceError> {
    let text = read_file(path)?;
    let mut map = HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| EvidenceError::Json(path.to_path_buf(), format!("line {}: {e}", n + 1)))?;
        let key = v
            .get("key")
            .and_then(|k| k.as_str())
            .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), format!("line {} lacks string key", n + 1)))?;
        map.insert(key.to_string(), v.clone());
    }
    Ok(map)
}

/// Join `expected` and `actual` equiv sides on `key`. A key matches when both
/// rows are present and their `status`+`value` agree.
pub fn read_equiv_pair(expected: &Path, actual: &Path) -> Result<EquivSummary, EvidenceError> {
    let exp = equiv_side(expected)?;
    let act = equiv_side(actual)?;
    let mut summary = EquivSummary { total: exp.len() as i64, matched: 0, mismatched: 0, missing: 0 };
    for (key, ev) in &exp {
        match act.get(key) {
            None => summary.missing += 1,
            Some(av) => {
                if av.get("status") == ev.get("status") && av.get("value") == ev.get("value") {
                    summary.matched += 1;
                } else {
                    summary.mismatched += 1;
                }
            }
        }
    }
    Ok(summary)
}

// ---------------------------------------------------------------------------
// verification.json: release-prep gate record.
// ---------------------------------------------------------------------------

/// One gate stage of `verification.json`.
#[derive(Debug, Clone)]
pub struct VerificationStage {
    pub name: String,
    pub passed: bool,
}

/// Top-level verdict plus per-stage verdicts.
#[derive(Debug, Clone)]
pub struct VerificationSummary {
    pub passed: bool,
    pub stages: Vec<VerificationStage>,
}

pub fn read_verification(path: &Path) -> Result<VerificationSummary, EvidenceError> {
    let text = read_file(path)?;
    let v = parse_json(path, &text)?;
    let obj = v
        .as_object()
        .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), "top level is not an object".into()))?;
    let passed = obj.get("passed").and_then(|p| p.as_bool()).unwrap_or(false);
    let mut stages = Vec::new();
    for (name, stage) in obj {
        if name == "passed" {
            continue;
        }
        if let Some(arr) = name
            .eq("python_install_smoke")
            .then(|| stage.as_array())
            .flatten()
        {
            for (i, check) in arr.iter().enumerate() {
                stages.push(VerificationStage {
                    name: format!("{name}[{i}]"),
                    passed: check.get("passed").and_then(|p| p.as_bool()).unwrap_or(false),
                });
            }
        } else if let Some(p) = stage.get("passed").and_then(|p| p.as_bool()) {
            stages.push(VerificationStage { name: name.clone(), passed: p });
        }
    }
    stages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(VerificationSummary { passed, stages })
}

// ---------------------------------------------------------------------------
// probe.json: headroom fingerprint.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ProbeSummary {
    pub package: String,
    pub version: String,
    pub score: f64,
    pub verdict: String,
    pub wall_secs: f64,
}

pub fn read_probe(path: &Path) -> Result<ProbeSummary, EvidenceError> {
    let text = read_file(path)?;
    let v = parse_json(path, &text)?;
    let str_field = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), format!("missing string {k:?}")))
    };
    let num_field = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_f64())
            .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), format!("missing number {k:?}")))
    };
    Ok(ProbeSummary {
        package: str_field("package")?,
        version: str_field("version")?,
        score: num_field("score")?,
        verdict: str_field("verdict")?,
        wall_secs: num_field("wall_secs").unwrap_or(0.0),
    })
}

// ---------------------------------------------------------------------------
// scope.json + gate-verdicts.jsonl.
// ---------------------------------------------------------------------------

/// One scoped unit: `verdict` is `"admit"` or `"refuse:<reasons>"`.
/// Refuse reasons join into the evidence string (bounded: count + first 3).
#[derive(Debug, Clone)]
pub struct ScopeUnit {
    pub id: String,
    pub verdict: String,
    pub evidence: String,
}

pub fn read_scope(path: &Path) -> Result<Vec<ScopeUnit>, EvidenceError> {
    let text = read_file(path)?;
    let v = parse_json(path, &text)?;
    let units = v
        .get("units")
        .and_then(|u| u.as_array())
        .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), "missing units[]".into()))?;
    let mut out = Vec::with_capacity(units.len());
    for u in units {
        let id = u
            .get("id")
            .and_then(|x| x.as_str())
            .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), "unit lacks string id".to_string()))?;
        let verdict = u.get("verdict").and_then(|x| x.as_str()).unwrap_or("unknown");
        let evidence = u.get("evidence").and_then(|x| x.as_str()).unwrap_or("").to_string();
        out.push(ScopeUnit { id: id.to_string(), verdict: verdict.to_string(), evidence });
    }
    Ok(out)
}

pub fn read_gate_verdicts(path: &Path) -> Result<Vec<ScopeUnit>, EvidenceError> {
    let text = read_file(path)?;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| EvidenceError::Json(path.to_path_buf(), format!("line {}: {e}", n + 1)))?;
        let id = v
            .get("unit")
            .and_then(|x| x.as_str())
            .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), format!("line {} lacks unit", n + 1)))?;
        let (verdict, evidence) = match v.get("verdict") {
            Some(serde_json::Value::String(s)) => (s.clone(), String::new()),
            Some(o) if o.get("refuse").is_some() => {
                let reasons: Vec<String> = o
                    .get("refuse")
                    .and_then(|r| r.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let mut ev = format!("refuse[{}]", reasons.len());
                for r in reasons.iter().take(3) {
                    let mut short = r.clone();
                    short.truncate(120);
                    ev.push_str("; ");
                    ev.push_str(&short);
                }
                ("refuse".to_string(), ev)
            }
            other => (
                "unknown".to_string(),
                format!("unshaped verdict: {}", other.map(|x| x.to_string()).unwrap_or_default()),
            ),
        };
        out.push(ScopeUnit { id: id.to_string(), verdict, evidence });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// bench JSONs: two_track.py outputs.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BenchTrack {
    pub track: String,
    pub pure_wall_s: f64,
    pub rust_wall_s: f64,
    pub speedup: f64,
    pub checksums_match: bool,
}

pub fn read_bench(path: &Path) -> Result<Vec<BenchTrack>, EvidenceError> {
    let text = read_file(path)?;
    let v = parse_json(path, &text)?;
    // bench1-style files carry no checksum; bench2/3-style carry pure+ruts checksums.
    let rows = v
        .as_array()
        .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), "top level is not an array".into()))?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let track = r
            .get("track")
            .and_then(|x| x.as_str())
            .ok_or_else(|| EvidenceError::Shape(path.to_path_buf(), "row lacks track".to_string()))?;
        let num = |k: &str| r.get(k).and_then(|x| x.as_f64()).unwrap_or(f64::NAN);
        let checksums_match = match (r.get("checksum"), r.get("pure_checksum"), r.get("rust_checksum")) {
            (Some(a), _, _) => r.get("checksum_rust").is_none_or(|b| a == b),
            (_, Some(a), Some(b)) => a == b,
            _ => true,
        };
        out.push(BenchTrack {
            track: track.to_string(),
            pure_wall_s: num("pure_wall_s"),
            rust_wall_s: num("rust_wall_s"),
            speedup: num("speedup"),
            checksums_match,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// scoped-<unit>.json (Elmer T011 per-slice grade).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ScopedGrade {
    pub integrity_passed: bool,
    pub integrity_tests: i64,
    pub parity_passed: bool,
    pub parity_failed: i64,
    pub failed_tests: Vec<String>,
}

pub fn read_scoped_grade(path: &Path) -> Result<ScopedGrade, EvidenceError> {
    let text = read_file(path)?;
    let v = parse_json(path, &text)?;
    let integrity = v.get("integrity");
    let parity = v.get("parity");
    let detail_int = |k: &str| {
        integrity
            .and_then(|i| i.get("detail"))
            .and_then(|d| d.get(k))
            .and_then(|x| x.as_i64())
            .unwrap_or(0)
    };
    let detail_par = |k: &str| {
        parity
            .and_then(|p| p.get("detail"))
            .and_then(|d| d.get(k))
            .and_then(|x| x.as_i64())
            .unwrap_or(0)
    };
    Ok(ScopedGrade {
        integrity_passed: integrity.and_then(|i| i.get("passed")).and_then(|p| p.as_bool()).unwrap_or(false),
        integrity_tests: detail_int("test_count"),
        parity_passed: parity.and_then(|p| p.get("passed")).and_then(|p| p.as_bool()).unwrap_or(false),
        parity_failed: detail_par("failed"),
        failed_tests: v
            .get("failed_tests")
            .and_then(|t| t.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default(),
    })
}

// ---------------------------------------------------------------------------
// OMP transcript index (gap 8): model/tokens/cost from session JSONL.
// ---------------------------------------------------------------------------

/// Summed usage for one OMP session: dominant model + totals.
#[derive(Debug, Clone)]
pub struct TranscriptSpend {
    pub session_id: String,
    pub model: String,
    pub input: i64,
    pub output: i64,
    pub cost_usd: f64,
}

#[derive(Debug, Default)]
pub struct TranscriptIndex {
    inner: HashMap<String, TranscriptSpend>,
}

impl TranscriptIndex {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn lookup(&self, session_id: &str) -> Option<&TranscriptSpend> {
        self.inner.get(session_id)
    }

    /// Index every `*.jsonl` under `dir` (recursive). Files that do not look
    /// like OMP transcripts are skipped silently; a file whose session id
    /// cannot be determined is skipped loudly on stderr.
    pub fn load(dir: &Path) -> Result<Self, EvidenceError> {
        let mut idx = Self::default();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let entries = std::fs::read_dir(&d)
                .map_err(|e| EvidenceError::Io(d.clone(), e.to_string()))?;
            for e in entries {
                let e = e.map_err(|e| EvidenceError::Io(d.clone(), e.to_string()))?;
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|x| x.to_str()) == Some("jsonl")
                    && looks_like_transcript(&p)
                {
                    match index_transcript(&p) {
                        Some(spend) => {
                            idx.inner.insert(spend.session_id.clone(), spend);
                        }
                        None => eprintln!("lane-capture: transcript without session id, skipping: {p:?}"),
                    }
                }
            }
        }
        Ok(idx)
    }
}

fn looks_like_transcript(path: &Path) -> bool {
    let Ok(f) = std::fs::File::open(path) else { return false };
    use std::io::BufRead as _;
    let mut reader = std::io::BufReader::new(f);
    let mut line = String::new();
    for _ in 0..5 {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if line.contains("\"type\":\"session\"") || line.contains("\"type\": \"session\"") {
                    return true;
                }
            }
            Err(_) => return false,
        }
    }
    false
}

/// Session id from the `{"type":"session","id":…}` line, else from the
/// `<ts>_<session-id>.jsonl` filename tail.
fn index_transcript(path: &Path) -> Option<TranscriptSpend> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut session_id: Option<String> = None;
    // model -> (output_tokens, input_tokens, cost)
    let mut by_model: HashMap<String, (i64, i64, f64)> = HashMap::new();
    for line in text.lines() {
        // One bad line never drops the session: skip unparseable lines
        // and non-message events (title/custom/credential lines carry no
        // usage).
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) == Some("session") {
            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                session_id = Some(id.to_string());
            }
            continue;
        }
        let Some(msg) = v.get("message") else { continue };
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        // Usage nests INSIDE the message envelope
        // (`message.usage`, `message.model`; verified against real
        // transcripts 2026-10-28 — not top-level siblings).
        let Some(usage) = msg.get("usage") else { continue };
        let model = msg
            .get("model")
            .and_then(|m| m.as_str())
            .map(|m| match msg.get("provider").and_then(|p| p.as_str()) {
                Some(p) => format!("{p}/{m}"),
                None => m.to_string(),
            })
            .unwrap_or_default();
        let input = usage.get("input").and_then(|x| x.as_i64()).unwrap_or(0);
        let output = usage.get("output").and_then(|x| x.as_i64()).unwrap_or(0);
        let cost = usage.get("cost").and_then(|c| c.get("total")).and_then(|x| x.as_f64()).unwrap_or(0.0);
        if model.is_empty() && input + output == 0 {
            continue;
        }
        let e = by_model.entry(model).or_insert((0, 0, 0.0));
        e.0 += output;
        e.1 += input;
        e.2 += cost;
    }
    let session_id = session_id.or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.rsplit('_').next())
            .map(|s| s.to_string())
    })?;
    // Dominant model = most output tokens; totals summed across models.
    let mut model = String::new();
    let (mut best_out, mut total_in, mut total_out, mut total_cost) = (0i64, 0i64, 0i64, 0.0);
    for (m, (out, inp, cost)) in &by_model {
        total_out += *out;
        total_in += *inp;
        total_cost += *cost;
        if *out > best_out {
            best_out = *out;
            model = m.clone();
        }
    }
    Some(TranscriptSpend { session_id, model, input: total_in, output: total_out, cost_usd: total_cost })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rs-evidence-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_tmp(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        p
    }

    #[test]
    fn equiv_pair_joins_on_key() {
        let d = tmpdir("equiv");
        let exp = write_tmp(&d, "pure.jsonl",
            "{\"key\":\"num|aa|0\",\"status\":\"ok\",\"value\":\"0\"}\n{\"key\":\"num|aa|1\",\"status\":\"ok\",\"value\":\"1\"}\n{\"key\":\"num|aa|2\",\"status\":\"ok\",\"value\":\"2\"}\n");
        let act = write_tmp(&d, "rust.jsonl",
            "{\"key\":\"num|aa|0\",\"status\":\"ok\",\"value\":\"0\"}\n{\"key\":\"num|aa|1\",\"status\":\"ok\",\"value\":\"WRONG\"}\n");
        let s = read_equiv_pair(&exp, &act).unwrap();
        assert_eq!(s, EquivSummary { total: 3, matched: 1, mismatched: 1, missing: 1 });
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn verification_reads_stages_and_arrays() {
        let d = tmpdir("verify");
        let p = write_tmp(&d, "verification.json", r#"{
            "passed": false,
            "rust_core_tests": {"passed": true, "tail": "ok"},
            "sdist_contents": {"passed": false, "files": 3},
            "python_install_smoke": [{"check": "a", "passed": true}, {"check": "b", "passed": false}]
        }"#);
        let s = read_verification(&p).unwrap();
        assert!(!s.passed);
        assert_eq!(s.stages.len(), 4);
        assert!(s.stages.iter().any(|x| x.name == "python_install_smoke[1]" && !x.passed));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn probe_reads_headroom_fields() {
        let d = tmpdir("probe");
        let p = write_tmp(&d, "probe.json", r#"{
            "package": "babel", "version": "2.18.0",
            "workload": {"name": "w", "source": "s", "iters": 8},
            "wall_secs": 0.75, "score": 0.78, "verdict": "rewrite", "why": "x"
        }"#);
        let s = read_probe(&p).unwrap();
        assert_eq!((s.package, s.verdict), ("babel".to_string(), "rewrite".to_string()));
        assert!((s.score - 0.78).abs() < 1e-12);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn scope_and_gate_verdicts_shapes() {
        let d = tmpdir("scope");
        let scope = write_tmp(&d, "lane.scope.json",
            r#"{"lane":"l","units":[{"id":"cxx:a","verdict":"skip-cpp-only","evidence":"e"}]}"#);
        let units = read_scope(&scope).unwrap();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].verdict, "skip-cpp-only");
        let gv = write_tmp(&d, "gate-verdicts.json",
            "{\"unit\":\"c:x\",\"files\":[\"x\"],\"has_built_object\":false,\"verdict\":\"admit\"}\n{\"unit\":\"f:y\",\"files\":[\"y\"],\"has_built_object\":false,\"verdict\":{\"refuse\":[\"pointer dummy\"]}}\n");
        let units = read_gate_verdicts(&gv).unwrap();
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].verdict, "admit");
        assert_eq!(units[1].verdict, "refuse");
        assert!(units[1].evidence.contains("pointer dummy"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn bench_reads_tracks() {
        let d = tmpdir("bench");
        let p = write_tmp(&d, "bench2.json", r#"[
            {"track": "decimal", "pure_wall_s": 0.28, "rust_wall_s": 0.12, "speedup": 2.3,
             "pure_rss_kb": 1, "rust_rss_kb": 2, "checksum": "549330", "checksum_rust": "549330"}
        ]"#);
        let tracks = read_bench(&p).unwrap();
        assert_eq!(tracks.len(), 1);
        assert!(tracks[0].checksums_match);
        assert!((tracks[0].speedup - 2.3).abs() < 1e-12);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn scoped_grade_reads_t011_shape() {
        let d = tmpdir("scoped");
        let p = write_tmp(&d, "scoped-x.json", r#"{
            "affected": [], "failed": 3, "failed_tests": ["A"],
            "integrity": {"passed": true, "detail": {"reason": "ok", "test_count": 482}},
            "parity": {"passed": false, "detail": {"exit_code": 8, "failed": 3}}
        }"#);
        let g = read_scoped_grade(&p).unwrap();
        assert!(g.integrity_passed && g.integrity_tests == 482);
        assert!(!g.parity_passed && g.parity_failed == 3);
        assert_eq!(g.failed_tests, vec!["A".to_string()]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn transcript_index_sums_usage_and_dominant_model() {
        // Real envelope shape: usage/model/provider nest INSIDE message.
        let d = tmpdir("transcript");
        let _p = write_tmp(&d, "2026-10-28T14-30-23-635Z_01a182eb-5693-716e-ac0e-24bd28d790ab.jsonl",
            "{\"type\":\"session\",\"version\":3,\"id\":\"01a182eb-5693-716e-ac0e-24bd28d790ab\"}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[],\"provider\":\"p\",\"model\":\"m/a\",\"usage\":{\"input\":100,\"output\":50,\"cacheRead\":0,\"cacheWrite\":0,\"totalTokens\":150,\"reasoningTokens\":5,\"cost\":{\"input\":0.001,\"output\":0.0005,\"cacheRead\":0,\"cacheWrite\":0,\"total\":0.0015}}}}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[],\"provider\":\"p\",\"model\":\"m/b\",\"usage\":{\"input\":10,\"output\":200,\"cacheRead\":0,\"cacheWrite\":0,\"totalTokens\":210,\"reasoningTokens\":0,\"cost\":{\"input\":0.0,\"output\":0.002,\"cacheRead\":0,\"cacheWrite\":0,\"total\":0.002}}}}\n\
             {\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":[]}}\n");
        let idx = TranscriptIndex::load(&d).unwrap();
        assert_eq!(idx.len(), 1);
        let s = idx.lookup("01a182eb-5693-716e-ac0e-24bd28d790ab").unwrap();
        assert_eq!(s.model, "p/m/b");
        assert_eq!((s.input, s.output), (110, 250));
        assert!((s.cost_usd - 0.0035).abs() < 1e-12);
        let _ = std::fs::remove_dir_all(&d);
    }
}
