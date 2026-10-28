//! Offline `export-records` engine: training-record schema v1 (report §2).
//!
//! Reads finished runs from `store.db` (+ `events.jsonl`, frozen manifest,
//! recon license inventory, run-local `release.toml`), scrubs free text at
//! write time, validates every record, and writes versioned JSONL plus a
//! regenerated `QUALITY.md`. Deterministic crate: no LLM, no network.
//!
//! Record-kind coverage (report §2.2): `mirror_unit` (units + gate_results +
//! hunks), `optimization` (merged rows), `failed_optimization` (loser rows).
//! `harvest_item` has no stored source in the §3 tap list (retro/upstream
//! review is Phase 3): it is deferred, see `docs/adr/052-*`.
//!
//! Two honest-default rules live here (also ADR-noted):
//! - mirror units carry no patch in the store, so `parent_sha`/`patch_text`
//!   validation applies to optimization rows only; mirror rows instead
//!   require a non-empty `commit_sha` and reference `before_ref`/`after_ref`.
//! - mirror `provenance.model` is `"template"`: the units table records no
//!   model/proposal, and default-path mirrors are reference-template copies
//!   (report §7.4: label them honestly).

use crate::evidence::TranscriptIndex;
use crate::scrub::scrub_text;
use rustsmith_store::Store;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use thiserror::Error;

const SCHEMA_VERSION: u32 = 1;
const MAX_REDACTED_FRACTION: f64 = 0.5;
const EXPORTER_NAME: &str = "rustsmith";

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("store: {0}")]
    Store(String),
    #[error("io: {0}")]
    Io(String),
    #[error("no runs matched selection")]
    NoRuns,
    #[error("nothing exported: {0}")]
    NothingExported(Box<ExportStats>),
    #[error("evidence: {0}")]
    Evidence(String),
}

impl std::fmt::Display for ExportStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} run(s) matched, {} dropped invalid, {} gutted",
            self.runs_seen, self.dropped_invalid, self.dropped_gutted
        )
    }
}

impl From<rustsmith_store::StoreError> for ExportError {
    fn from(e: rustsmith_store::StoreError) -> Self {
        ExportError::Store(e.to_string())
    }
}

impl From<crate::evidence::EvidenceError> for ExportError {
    fn from(e: crate::evidence::EvidenceError) -> Self {
        ExportError::Evidence(e.to_string())
    }
}

/// CLI-facing options for one export pass.
pub struct ExportOptions {
    pub store_path: PathBuf,
    pub out_path: PathBuf,
    pub since: Option<i64>,
    pub run_ids: Option<Vec<String>>,
    pub session_map: Option<PathBuf>,
    pub transcripts: Option<PathBuf>,
    pub license_split: Option<PathBuf>,
}

/// Loud counters: every dropped record lands in `invalid_reasons` or
/// `dropped_gutted` and is printed to stderr by the CLI.
#[derive(Debug, Default)]
pub struct ExportStats {
    pub runs_seen: usize,
    pub runs_exported: usize,
    pub exported: usize,
    pub dropped_invalid: usize,
    pub dropped_gutted: usize,
    pub invalid_reasons: BTreeMap<String, usize>,
    pub scrubbed_fields: usize,
    pub redacted_spans: usize,
    pub run_spans: BTreeMap<String, Option<i64>>,
    pub gpl_rows: usize,
}

impl ExportStats {
    fn drop_invalid(&mut self, reason: &str) {
        self.dropped_invalid += 1;
        *self.invalid_reasons.entry(reason.to_string()).or_insert(0) += 1;
    }

    /// Machine summary for stdout (fail-loud counts live on stderr).
    pub fn summary_json(&self, out: &Path, quality: &Path) -> serde_json::Value {
        serde_json::json!({
            "records": self.exported,
            "runs_exported": self.runs_exported,
            "runs_seen": self.runs_seen,
            "dropped_invalid": self.dropped_invalid,
            "dropped_gutted": self.dropped_gutted,
            "invalid_reasons": self.invalid_reasons,
            "gpl_rows": self.gpl_rows,
            "out": out.display().to_string(),
            "quality": quality.display().to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Schema v1 records (report §2.2). Options serialize as null (NULL, never
// fake); strings that are unknown stay "" only where the schema needs a
// string and the source has no value (mirror technique/bound).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceIds {
    pub omp: Option<String>,
    pub phoenix: Option<String>,
    pub tripwire: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Provenance {
    pub model: String,
    pub prompt_version: String,
    pub guidance_version: String,
    pub proposal_text: String,
    pub technique: String,
    pub bound: String,
    pub tier: i64,
    pub tokens_spent: i64,
    pub cost_usd: Option<f64>,
    pub wall_seconds: Option<i64>,
    pub grade_count: i64,
    pub upstream_review: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Code {
    pub package: String,
    pub language: String,
    pub parent_sha: String,
    pub commit_sha: String,
    pub patch_text: String,
    pub files_touched: Vec<String>,
    pub before_ref: String,
    pub after_ref: String,
    pub workload_contract_ref: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OracleCounts {
    pub passed: i64,
    pub total: i64,
    pub skipped: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HeldoutCounts {
    pub passed: i64,
    pub total: i64,
    pub divergence: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Measurement {
    pub instrument: String,
    pub objective_metric: String,
    pub visible_gain_pct: Option<f64>,
    pub heldout_gain_pct: Option<f64>,
    pub divergence_pct: Option<f64>,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    pub attribution_verified: Option<bool>,
    pub oracle: OracleCounts,
    pub heldout: HeldoutCounts,
    pub known_failures_k: i64,
    pub rss_delta_pct: Option<f64>,
    pub alloc_delta_pct: Option<f64>,
    pub binary_delta_pct: Option<f64>,
    pub compile_delta_pct: Option<f64>,
    pub noise_floor_pct: Option<f64>,
    pub outcome: String,
    pub gate: Option<String>,
    pub harvest_class: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GateVerdict {
    pub gate: String,
    pub passed: bool,
    pub detail_summary: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Review {
    pub gate_verdicts: Vec<GateVerdict>,
    pub council_positions: Vec<serde_json::Value>,
    pub scope_verdict: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Rights {
    pub record_license: String,
    pub upstream_repo: String,
    pub upstream_license: String,
    pub upstream_commit: String,
    pub attribution_text: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportRecord {
    pub schema_version: u32,
    pub record_id: String,
    pub record_kind: String,
    pub created_at: i64,
    pub exporter_version: String,
    pub run_id: String,
    pub unit_id: String,
    pub round: i64,
    pub session_id: Option<String>,
    pub trace_ids: TraceIds,
    pub provenance: Provenance,
    pub code: Code,
    pub measurement: Measurement,
    pub review: Review,
    pub rights: Rights,
}

/// `sha256(run_id|unit_id|parent_sha|child_sha)[:16]`.
pub fn record_id(run_id: &str, unit_id: &str, parent_sha: &str, child_sha: &str) -> String {
    let mut h = Sha256::new();
    h.update(run_id.as_bytes());
    h.update(b"|");
    h.update(unit_id.as_bytes());
    h.update(b"|");
    h.update(parent_sha.as_bytes());
    h.update(b"|");
    h.update(child_sha.as_bytes());
    hex::encode(h.finalize())[..16].to_string()
}

fn exporter_version() -> String {
    format!(
        "{} {}+{}",
        EXPORTER_NAME,
        env!("CARGO_PKG_VERSION"),
        option_env!("RUSTSMITH_GIT_SHA").unwrap_or("unknown")
    )
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse `--since DATE` (`YYYY-MM-DD`, UTC midnight) to epoch seconds.
pub fn parse_since(s: &str) -> Result<i64, String> {
    let p: Vec<&str> = s.split('-').collect();
    if p.len() != 3 {
        return Err(format!("bad --since {s:?}: want YYYY-MM-DD"));
    }
    let (y, m, d): (i64, i64, i64) = (
        p[0].parse().map_err(|_| format!("bad --since {s:?}"))?,
        p[1].parse().map_err(|_| format!("bad --since {s:?}"))?,
        p[2].parse().map_err(|_| format!("bad --since {s:?}"))?,
    );
    if !(1970..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(format!("bad --since {s:?}: out of range"));
    }
    // Howard Hinnant's days-from-civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Ok((era * 146097 + doe - 719468) * 86400)
}

// ---------------------------------------------------------------------------
// Session map (`--session-map` CSV: run,unit,session_id,omp,phoenix,tripwire)
// ---------------------------------------------------------------------------

type SessionKey = (String, String);

pub struct SessionMap {
    inner: HashMap<SessionKey, (Option<String>, TraceIds)>,
}

impl SessionMap {
    pub fn empty() -> Self {
        Self { inner: HashMap::new() }
    }

    pub fn load(path: &Path) -> Result<Self, ExportError> {
        let text =
            std::fs::read_to_string(path).map_err(|e| ExportError::Io(format!("{path:?}: {e}")))?;
        let mut inner = HashMap::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || (n == 0 && line.starts_with("run,")) {
                continue;
            }
            let cols: Vec<&str> = line.split(',').map(str::trim).collect();
            if cols.len() != 6 {
                eprintln!("export-records: session-map line {} malformed, skipping", n + 1);
                continue;
            }
            let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
            inner.insert(
                (cols[0].to_string(), cols[1].to_string()),
                (
                    opt(cols[2]),
                    TraceIds { omp: opt(cols[3]), phoenix: opt(cols[4]), tripwire: opt(cols[5]) },
                ),
            );
        }
        Ok(Self { inner })
    }

    /// Exact `(run, unit)` hit, else `(run, hotspot)` for optimization rows
    /// whose unit id is `run:hotspot`. Absent map → `(None, null traces)`.
    pub fn lookup(&self, run_id: &str, unit_id: &str, hotspot: &str) -> (Option<String>, TraceIds) {
        let null = || {
            (
                None,
                TraceIds { omp: None, phoenix: None, tripwire: None },
            )
        };
        if let Some((s, t)) = self.inner.get(&(run_id.to_string(), unit_id.to_string())) {
            return (s.clone(), t.clone());
        }
        if !hotspot.is_empty() {
            if let Some((s, t)) = self.inner.get(&(run_id.to_string(), hotspot.to_string())) {
                return (s.clone(), t.clone());
            }
        }
        null()
    }
}

// ---------------------------------------------------------------------------
// Run context: manifest hash, run-level oracle totals, rights.
// ---------------------------------------------------------------------------

struct RunContext {
    run_id: String,
    source_lang: String,
    package: String,
    manifest_sha: Option<String>,
    oracle_passed: Option<i64>,
    oracle_total: Option<i64>,
    upstream_repo: String,
    upstream_commit: String,
    upstream_license: String,
    record_license: String,
    attribution_text: String,
    delivered_artifact: bool,
    run_wall: Option<i64>,
}

/// Candidate sidecar paths, in priority order, relative to the store dir.
fn first_hit(dir: &Path, rels: &[&str]) -> Option<PathBuf> {
    rels.iter().map(|r| dir.join(r)).find(|p| p.is_file())
}

fn manifest_sha(store_dir: &Path) -> Option<String> {
    let p = first_hit(store_dir, &["work/recon/manifest.json", "recon/manifest.json", "manifest.json"])?;
    let bytes = std::fs::read(p).ok()?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Some(hex::encode(h.finalize()))
}

fn recon_dir(store_dir: &Path) -> Option<PathBuf> {
    for rel in ["work/recon", "recon"] {
        let p = store_dir.join(rel);
        if p.join("facts.json").is_file() {
            return Some(p);
        }
    }
    None
}

fn release_toml(store_dir: &Path) -> Option<PathBuf> {
    first_hit(store_dir, &["fork/release.toml", "out/stage/release.toml"])
}

/// Lane-capture sidecar (`lane-capture init`): package, upstream and rights
/// pins for lanes that never ran recon/release. Highest priority source.
fn lane_toml(store_dir: &Path) -> Option<PathBuf> {
    first_hit(store_dir, &["lane.toml"])
}

fn lane_value(store_dir: &Path, key: &str) -> Option<String> {
    lane_toml(store_dir).and_then(|p| toml_string(&p, key))
}

/// Attribution line: `lane.toml` override or the run repo URL, with a
/// `rights_note` (e.g. a training-use permission grant) appended in
/// brackets when present.
fn lane_attribution(store_dir: &Path, repo_url: &str) -> String {
    let base =
        lane_value(store_dir, "attribution_text").unwrap_or_else(|| repo_url.to_string());
    match lane_value(store_dir, "rights_note") {
        Some(note) => format!("{base} [{note}]"),
        None => base,
    }
}

/// A GPL mention anywhere in the license string marks the record for the
/// opt-in `--license-split` sink (ADR-055 decision 7 as amended 2026-10-28:
/// Elmer GPL rows ship inline with a rights_note permission).
fn is_gpl(license: &str) -> bool {
    license.to_ascii_uppercase().contains("GPL")
}

fn toml_string(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        if let Some(v) = t.strip_prefix(key) {
            let v = v.trim().trim_start_matches('=').trim();
            let v = v.trim_matches('"').trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// License priority: `lane.toml` sidecar, run-local `release.toml`
/// `upstream_license`, then recon `facts.json` probe-attribution license,
/// else "" (record drops).
fn upstream_license(store_dir: &Path) -> String {
    if let Some(v) = lane_value(store_dir, "upstream_license") {
        return v;
    }
    if let Some(p) = release_toml(store_dir) {
        if let Some(v) = toml_string(&p, "upstream_license") {
            return v;
        }
    }
    if let Some(dir) = recon_dir(store_dir) {
        if let Ok(t) = std::fs::read_to_string(dir.join("facts.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                if let Some(lic) = v["probe"]["attribution"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|a| a["license"].as_str())
                {
                    return lic.to_string();
                }
            }
        }
    }
    String::new()
}

/// Record license: explicit `lane.toml`/`release.toml` `record_license`
/// wins; a GPL upstream forces the upstream value (opt-in split at emit,
/// inline otherwise); otherwise MIT. Never empty (validation drops).
fn record_license(store_dir: &Path, upstream: &str) -> String {
    if let Some(v) = lane_value(store_dir, "record_license") {
        return v;
    }
    if let Some(p) = release_toml(store_dir) {
        if let Some(v) = toml_string(&p, "record_license") {
            return v;
        }
    }
    if is_gpl(upstream) {
        return upstream.to_string();
    }
    "MIT".to_string()
}

fn run_package(store_dir: &Path, repo_url: &str) -> String {
    if let Some(v) = lane_value(store_dir, "package") {
        return v;
    }
    if let Some(p) = release_toml(store_dir) {
        if let Some(v) = toml_string(&p, "project") {
            return v;
        }
    }
    repo_url
        .rsplit('/')
        .next()
        .unwrap_or("unknown")
        .trim_end_matches(".git")
        .to_string()
}

fn language_for(unit_kind_prefix: Option<&str>, source_lang: &str) -> String {
    if let Some(prefix) = unit_kind_prefix {
        match prefix {
            "python" => return "python".to_string(),
            "fortran" => return "fortran".to_string(),
            "c" => return "c".to_string(),
            "cxx" | "c++" => return "cxx".to_string(),
            _ => {}
        }
    }
    match source_lang.to_lowercase().as_str() {
        "python" => "python".to_string(),
        "fortran" => "fortran".to_string(),
        "c" => "c".to_string(),
        "c++" | "cxx" => "cxx".to_string(),
        // Honest passthrough: the store value is anomalous (never guess).
        _ => source_lang.to_string(),
    }
}

fn split_repo_pin(repo_url: &str) -> (String, String) {
    match repo_url.split_once('#') {
        Some((repo, pin)) => (repo.to_string(), pin.to_string()),
        None => (repo_url.to_string(), String::new()),
    }
}

// ---------------------------------------------------------------------------
// Gate verdicts. Held-out magnitudes NEVER enter `detail_summary`: the
// held-out gate contributes only pass/fail + threshold (report §2.3).
// ---------------------------------------------------------------------------

const SUMMARY_KEYS: [&str; 5] = ["passed", "test_count", "total", "reason", "threshold"];

fn gate_summary(gate: &str, passed: bool, detail_json: &str, home: &str) -> GateVerdict {
    let mut parts = vec![if passed { "pass" } else { "fail" }.to_string()];
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(detail_json) {
        if gate == "heldout_divergence" {
            // Threshold only: never heldout_rate / visible_rate / divergence.
            if let Some(t) = v.get("threshold") {
                parts.push(format!("threshold={t}"));
            }
        } else if let Some(o) = v.as_object() {
            for k in SUMMARY_KEYS {
                if let Some(val) = o.get(k) {
                    let s = match val {
                        serde_json::Value::String(s) => s.clone(),
                        _ => val.to_string(),
                    };
                    if !s.is_empty() {
                        parts.push(format!("{k}={s}"));
                    }
                }
            }
        }
    }
    let mut summary = parts.join(" ");
    summary.truncate(240);
    // Scalar-derived but still scrubbed: detail blobs leak into `reason`.
    let scrubbed = scrub_text(&summary, home);
    GateVerdict { gate: gate.to_string(), passed, detail_summary: scrubbed.text }
}

fn oracle_counts(gates: &[rustsmith_store::GateRow], unit: Option<&str>) -> (Option<i64>, Option<i64>) {
    let (mut passed, mut total) = (None, None);
    for g in gates {
        if unit.is_some_and(|u| g.unit_id != u) {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&g.detail_json).unwrap_or_default();
        if g.gate == "oracle_parity" {
            if let Some(p) = v.get("passed").and_then(|x| x.as_i64()) {
                passed = Some(passed.unwrap_or(0).max(p));
            }
            if let Some(t) = v.get("total").and_then(|x| x.as_i64()) {
                total = Some(total.unwrap_or(0).max(t));
            }
        }
        if g.gate == "oracle_integrity" {
            if let Some(t) = v.get("test_count").and_then(|x| x.as_i64()) {
                total = Some(total.unwrap_or(0).max(t));
            }
            if let Some(p) = v.get("passed").and_then(|x| x.as_i64()) {
                passed = Some(passed.unwrap_or(0).max(p));
            }
        }
    }
    (passed, total)
}

/// Recursively scrub every string leaf (council reasoning ships scrubbed).
fn scrub_json(v: &serde_json::Value, home: &str, stats: &mut ExportStats) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) => {
            let o = scrub_text(s, home);
            if o.scrubbed {
                stats.scrubbed_fields += 1;
                stats.redacted_spans += o.scrub_count;
            }
            serde_json::Value::String(o.text)
        }
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(|x| scrub_json(x, home, stats)).collect())
        }
        serde_json::Value::Object(o) => serde_json::Value::Object(
            o.iter().map(|(k, x)| (k.clone(), scrub_json(x, home, stats))).collect(),
        ),
        _ => v.clone(),
    }
}

fn scrub_field(s: &str, home: &str, stats: &mut ExportStats) -> (String, crate::scrub::ScrubOutcome) {
    let o = scrub_text(s, home);
    if o.scrubbed {
        stats.scrubbed_fields += 1;
        stats.redacted_spans += o.scrub_count;
    }
    (o.text.clone(), o)
}

/// Files a unified diff touches, from `diff --git` / `---` / `+++` headers.
/// Failed rows carry no file list in the store; the patch itself is the
/// source (never guessed). `/dev/null` sides (add/remove) are dropped.
fn files_from_patch(patch: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for line in patch.lines() {
        // `+++ b/` covers modify/add; `--- a/` covers delete.
        let path = line
            .strip_prefix("+++ b/")
            .or_else(|| line.strip_prefix("--- a/"))
            .map(str::trim);
        if let Some(p) = path {
            if p != "/dev/null" && !files.iter().any(|f| f == p) {
                files.push(p.to_string());
            }
        }
    }
    files
}

fn finite_or_none(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite())
}

// ---------------------------------------------------------------------------
// Validation (report §3.3): schema version, parent/patch presence (except
// rejected_at_proposal and mirror rows, which carry commit refs instead),
// oracle.total > 0, finite-or-null gains, non-empty upstream license.
// ---------------------------------------------------------------------------

fn validate(rec: &ExportRecord) -> Result<(), String> {
    if rec.schema_version != SCHEMA_VERSION {
        return Err("schema_version != 1".to_string());
    }
    let rejected = rec.record_kind == "failed_optimization" && rec.measurement.outcome == "rejected_at_proposal";
    if rec.record_kind == "mirror_unit" {
        if rec.code.commit_sha.is_empty() {
            return Err("mirror_unit without commit_sha".to_string());
        }
    } else if !rejected
        && (rec.code.parent_sha.is_empty() || rec.code.patch_text.is_empty())
    {
        return Err("empty parent_sha/patch_text outside rejected_at_proposal".to_string());
    }
    if rec.measurement.oracle.total <= 0 {
        return Err("oracle.total <= 0".to_string());
    }
    for (name, v) in [
        ("visible_gain_pct", rec.measurement.visible_gain_pct),
        ("heldout_gain_pct", rec.measurement.heldout_gain_pct),
        ("divergence_pct", rec.measurement.divergence_pct),
        ("ci_low", rec.measurement.ci_low),
        ("ci_high", rec.measurement.ci_high),
        ("rss_delta_pct", rec.measurement.rss_delta_pct),
        ("alloc_delta_pct", rec.measurement.alloc_delta_pct),
        ("binary_delta_pct", rec.measurement.binary_delta_pct),
        ("compile_delta_pct", rec.measurement.compile_delta_pct),
        ("noise_floor_pct", rec.measurement.noise_floor_pct),
        ("cost_usd", rec.provenance.cost_usd),
    ] {
        if v.is_some_and(|x| !x.is_finite()) {
            return Err(format!("non-finite {name}"));
        }
    }
    if rec.rights.upstream_license.is_empty() {
        return Err("empty rights.upstream_license".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Main pass.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Lane-capture joins (ADR-055): transcript provenance + gate wall times +
// spec scope verdicts, and the GPL license split.
// ---------------------------------------------------------------------------

/// Transcript spend for a joined session: dominant model + summed usage.
/// Store values stand when no transcript joins (or it carries no usage).
fn join_provenance(
    transcripts: &TranscriptIndex,
    session_id: &Option<String>,
    store_model: &str,
    store_tokens: i64,
) -> (String, i64, Option<f64>) {
    let joined = session_id.as_ref().and_then(|s| transcripts.lookup(s));
    match joined {
        Some(spend) if !spend.model.is_empty() || spend.input + spend.output > 0 => {
            let model = if spend.model.is_empty() {
                store_model.to_string()
            } else {
                spend.model.clone()
            };
            let tokens = if spend.input + spend.output > 0 {
                spend.input + spend.output
            } else {
                store_tokens
            };
            let cost = (spend.cost_usd > 0.0).then_some(spend.cost_usd);
            (model, tokens, cost)
        }
        _ => (store_model.to_string(), store_tokens, None),
    }
}

/// Max `wall_secs` across gate detail_jsons (the lane helper records
/// measured grade/bench wall time there), else the run wall span.
fn wall_seconds(
    gates: &[rustsmith_store::GateRow],
    unit: Option<&str>,
    run_wall: Option<i64>,
) -> Option<i64> {
    let mut best: Option<i64> = None;
    for g in gates {
        if unit.is_some_and(|u| g.unit_id != u) {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&g.detail_json) {
            let w = v
                .get("wall_secs")
                .and_then(|x| x.as_f64())
                .filter(|x| x.is_finite() && *x > 0.0);
            if let Some(w) = w {
                let w = w as i64;
                best = Some(best.map_or(w, |b: i64| b.max(w)));
            }
        }
    }
    best.or(run_wall)
}

/// Scope verdict the lane helper filed in `units.spec_json` (ADR-055: no
/// new gate variant, no run-wide decision rows).
fn scope_verdict_from_spec(spec_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(spec_json)
        .ok()?
        .get("scope_verdict")?
        .as_str()
        .map(|s| s.to_string())
}

/// Opt-in GPL sink (`--license-split PATH`): created on the first
/// GPL-licensed row only. Without the flag, GPL rows ship inline in the
/// main file — Elmer (GPL-2.0-or-later upstream) is permitted training data
/// per the captain 2026-10-28, with the permission noted in
/// `rights.attribution_text`; see `rights_note` in `lane.toml`.
struct GplSink {
    path: PathBuf,
    file: Option<std::io::BufWriter<std::fs::File>>,
}

impl GplSink {

    fn write(&mut self, line: &str) -> Result<(), ExportError> {
        use std::io::Write as _;
        if self.file.is_none() {
            let f = std::fs::File::create(&self.path)
                .map_err(|e| ExportError::Io(format!("{:?}: {e}", self.path)))?;
            self.file = Some(std::io::BufWriter::new(f));
            eprintln!("export-records: GPL-licensed rows split to {:?}", self.path);
        }
        if let Some(f) = self.file.as_mut() {
            writeln!(f, "{line}").map_err(|e| ExportError::Io(e.to_string()))?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ExportError> {
        use std::io::Write as _;
        if let Some(f) = self.file.as_mut() {
            f.flush().map_err(|e| ExportError::Io(e.to_string()))?;
        }
        Ok(())
    }

    fn created(&self) -> bool {
        self.file.is_some()
    }
}
pub fn export_records(opts: &ExportOptions) -> Result<ExportStats, ExportError> {
    let store = Store::open(&opts.store_path)?;
    let store_dir = opts.store_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let home = std::env::var("HOME").unwrap_or_default();
    let sessions = match &opts.session_map {
        Some(p) => SessionMap::load(p)?,
        None => SessionMap::empty(),
    };
    let transcripts = match &opts.transcripts {
        Some(d) => TranscriptIndex::load(d)?,
        None => TranscriptIndex::empty(),
    };
    if !transcripts.is_empty() {
        eprintln!("export-records: indexed {} transcript session(s) from {:?}", transcripts.len(), opts.transcripts.as_ref().unwrap());
    }
    let created_at = now_epoch();
    let version = exporter_version();

    let mut run_ids = store.list_run_ids()?;
    run_ids.sort();
    if let Some(want) = &opts.run_ids {
        run_ids.retain(|r| want.iter().any(|w| w == r));
    }
    if let Some(since) = opts.since {
        run_ids.retain(|r| {
            store
                .run_times(r)
                .ok()
                .flatten()
                .is_some_and(|(start, end)| end.unwrap_or(start) >= since)
        });
    }
    if run_ids.is_empty() {
        return Err(ExportError::NoRuns);
    }

    let out_file = std::fs::File::create(&opts.out_path)
        .map_err(|e| ExportError::Io(format!("{:?}: {e}", opts.out_path)))?;
    let mut out = std::io::BufWriter::new(out_file);
    let mut gpl: Option<GplSink> =
        opts.license_split.clone().map(|path| GplSink { path, file: None });
    let mut stats = ExportStats { runs_seen: run_ids.len(), ..Default::default() };
    // Per-window aggregates for QUALITY.md.
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_btt: BTreeMap<(String, i64, String), (usize, usize)> = BTreeMap::new();
    let mut calib: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    let mut divergence_failures = 0usize;
    let mut run_delivered: Vec<(String, bool)> = Vec::new();

    // events.jsonl is a backstop reader: run wall spans for QUALITY.md.
    // Missing file is fine (DB rows are the primary source).
    let event_spans = read_event_spans(&store.events_path);

    for run_id in &run_ids {
        let Some(run) = store.get_run(run_id)? else { continue };
        // lane.toml pins the upstream project; otherwise the run repo URL
        // doubles as both port and upstream (pipeline default).
        let (repo_from_url, commit_from_url) = split_repo_pin(&run.repo_url);
        let upstream_repo =
            lane_value(&store_dir, "upstream_repo").unwrap_or(repo_from_url);
        let upstream_commit = lane_value(&store_dir, "upstream_commit")
            .filter(|s| !s.is_empty())
            .unwrap_or(commit_from_url);
        let upstream_license = upstream_license(&store_dir);
        let run_wall = store
            .run_times(run_id)
            .ok()
            .flatten()
            .and_then(|(start, end)| end.filter(|e| *e > start).map(|e| e - start));
        let ctx = RunContext {
            oracle_passed: None,
            oracle_total: None,
            manifest_sha: manifest_sha(&store_dir),
            package: run_package(&store_dir, &run.repo_url),
            source_lang: run.source_lang.clone(),
            run_id: run_id.clone(),
            upstream_repo,
            upstream_commit,
            upstream_license: upstream_license.clone(),
            record_license: record_license(&store_dir, &upstream_license),
            attribution_text: lane_attribution(&store_dir, &run.repo_url),
            delivered_artifact: first_hit(
                &store_dir,
                &["fork/ACCEPTED.json", "out/ACCEPTED.json", "ACCEPTED.json"],
            )
            .is_some(),
            run_wall,
        };
        let gates = store.list_gate_results(run_id).unwrap_or_default();
        let (run_passed, run_total) = oracle_counts(&gates, None);
        let ctx = RunContext { oracle_passed: run_passed, oracle_total: run_total, ..ctx };
        stats.run_spans.insert(run_id.clone(), event_spans.get(run_id).copied());

        let decisions = store.list_decisions(run_id).unwrap_or_default();
        let hunks = store.list_hunks(run_id).unwrap_or_default();

        let mut run_exported = 0;
        let units = store.list_units_full(run_id).unwrap_or_default();
        for u in &units {
            // Queued/running/gated units are incomplete work, not training
            // signal. Failed/parked/abandoned units export as negative rows
            // (gap 7); rows with oracle.total == 0 still drop loudly.
            if !matches!(u.status.as_str(), "passed" | "failed" | "parked" | "abandoned") {
                continue;
            }
            if let Some(rec) = mirror_record(&ctx, u, &gates, &hunks, &decisions, &sessions, &transcripts, &home, created_at, &version, &mut stats) {
                if emit(&mut out, &mut gpl, &rec, None, &mut stats, &mut kinds, &mut outcomes, &mut by_btt, &mut calib, &mut divergence_failures)? {
                    run_exported += 1;
                }
            }
        }
        for o in store.list_optimizations(run_id)? {
            if let Some(rec) = optimization_record(&ctx, &o, &gates, &decisions, &sessions, &transcripts, &home, created_at, &version, &mut stats) {
                if emit(&mut out, &mut gpl, &rec, Some(o.ceiling_pct), &mut stats, &mut kinds, &mut outcomes, &mut by_btt, &mut calib, &mut divergence_failures)? {
                    run_exported += 1;
                }
            }
        }
        for f in store.list_failed(run_id)? {
            if let Some(rec) = failed_record(&ctx, &f, &decisions, &sessions, &transcripts, &home, created_at, &version, &mut stats) {
                if emit(&mut out, &mut gpl, &rec, None, &mut stats, &mut kinds, &mut outcomes, &mut by_btt, &mut calib, &mut divergence_failures)? {
                    run_exported += 1;
                }
            }
        }
        run_delivered.push((run_id.clone(), ctx.delivered_artifact));
        if run_exported > 0 {
            stats.runs_exported += 1;
        }
    }
    use std::io::Write as _;
    out.flush().map_err(|e| ExportError::Io(e.to_string()))?;
    drop(out);
    if let Some(g) = gpl.as_mut() {
        g.flush()?;
    }
    let gpl_path = gpl.as_ref().filter(|g| g.created()).map(|g| g.path.clone());
    if stats.exported == 0 {
        return Err(ExportError::NothingExported(Box::new(stats)));
    }
    let quality_path = opts.out_path.parent().map(|p| p.join("QUALITY.md")).unwrap_or_else(|| PathBuf::from("QUALITY.md"));
    let md = quality_md(&stats, &kinds, &outcomes, &by_btt, &calib, divergence_failures, &run_delivered, gpl_path, created_at);
    std::fs::write(&quality_path, md).map_err(|e| ExportError::Io(format!("{quality_path:?}: {e}")))?;
    Ok(stats)
}

#[allow(clippy::too_many_arguments)]
fn emit(
    out: &mut std::io::BufWriter<std::fs::File>,
    gpl: &mut Option<GplSink>,
    rec: &ExportRecord,
    ceiling: Option<f64>,
    stats: &mut ExportStats,
    kinds: &mut BTreeMap<String, usize>,
    outcomes: &mut BTreeMap<String, usize>,
    by_btt: &mut BTreeMap<(String, i64, String), (usize, usize)>,
    calib: &mut BTreeMap<String, (f64, usize)>,
    divergence_failures: &mut usize,
) -> Result<bool, ExportError> {
    if let Err(reason) = validate(rec) {
        stats.drop_invalid(&reason);
        return Ok(false);
    }
    let line = serde_json::to_string(rec).map_err(|e| ExportError::Io(e.to_string()))?;
    use std::io::Write as _;
    // GPL-licensed rows take the opt-in `--license-split` sink when present;
    // otherwise they ship inline (Elmer is permitted training data per the
    // captain 2026-10-28; permission noted in rights.attribution_text).
    if is_gpl(&rec.rights.record_license) {
        stats.gpl_rows += 1;
        if let Some(g) = gpl.as_mut() {
            g.write(&line)?;
        } else {
            writeln!(out, "{line}").map_err(|e| ExportError::Io(e.to_string()))?;
        }
    } else {
        writeln!(out, "{line}").map_err(|e| ExportError::Io(e.to_string()))?;
    }
    stats.exported += 1;
    *kinds.entry(rec.record_kind.clone()).or_insert(0) += 1;
    *outcomes.entry(rec.measurement.outcome.clone()).or_insert(0) += 1;
    if rec.record_kind != "mirror_unit" {
        let key = (rec.provenance.bound.clone(), rec.provenance.tier, rec.provenance.technique.clone());
        let e = by_btt.entry(key).or_insert((0, 0));
        e.1 += 1;
        if rec.measurement.outcome == "merged" {
            e.0 += 1;
        }
        if let (Some(c), Some(v)) = (ceiling, rec.measurement.visible_gain_pct) {
            let e = calib.entry(rec.provenance.bound.clone()).or_insert((0.0, 0));
            e.0 += c - v;
            e.1 += 1;
        }
    }
    if rec.review.gate_verdicts.iter().any(|g| g.gate == "heldout_divergence" && !g.passed) {
        *divergence_failures += 1;
    }
    Ok(true)
}

fn council_positions(
    decisions: &[rustsmith_store::DecisionRow],
    home: &str,
    stats: &mut ExportStats,
) -> Vec<serde_json::Value> {
    decisions
        .iter()
        .map(|d| {
            let v: serde_json::Value =
                serde_json::from_str(&d.seat_positions_json).unwrap_or_default();
            scrub_json(&v, home, stats)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn mirror_record(
    ctx: &RunContext,
    u: &rustsmith_store::UnitRow,
    gates: &[rustsmith_store::GateRow],
    hunks: &[rustsmith_store::HunkRow],
    decisions: &[rustsmith_store::DecisionRow],
    sessions: &SessionMap,
    transcripts: &TranscriptIndex,
    home: &str,
    created_at: i64,
    version: &str,
    stats: &mut ExportStats,
) -> Option<ExportRecord> {
    let (passed, total) = oracle_counts(gates, Some(&u.id));
    let unit_gates: Vec<GateVerdict> = gates
        .iter()
        .filter(|g| g.unit_id == u.id)
        .map(|g| gate_summary(&g.gate, g.passed, &g.detail_json, home))
        .collect();
    let (session_id, trace_ids) = sessions.lookup(&ctx.run_id, &u.id, "");
    // Transcript join wins for model/tokens/cost when present (gap 8);
    // otherwise the store values stand ("template" + unit token sums).
    let (model, tokens_spent, cost_usd) =
        join_provenance(transcripts, &session_id, "template", u.tokens_in + u.tokens_out);
    let commit_sha = u.commit_sha.clone().unwrap_or_default();
    // Attribution-ledger refs when the grade-to-merge transaction recorded them.
    let mut before_ref = String::new();
    let mut after_ref = commit_sha.clone();
    if let Some(h) = hunks.iter().find(|h| h.unit_id == u.id) {
        before_ref = h.before_ref.clone();
        after_ref = h.after_ref.clone();
    }
    let kind_prefix = u.id.split(':').next();
    let rec = ExportRecord {
        schema_version: SCHEMA_VERSION,
        record_id: record_id(&ctx.run_id, &u.id, "", &commit_sha),
        record_kind: "mirror_unit".to_string(),
        created_at,
        exporter_version: version.to_string(),
        run_id: ctx.run_id.clone(),
        unit_id: u.id.clone(),
        round: 0,
        session_id,
        trace_ids,
        provenance: Provenance {
            model,
            prompt_version: String::new(),
            guidance_version: String::new(),
            proposal_text: String::new(),
            technique: String::new(),
            bound: String::new(),
            tier: 0,
            tokens_spent,
            cost_usd,
            wall_seconds: wall_seconds(gates, Some(&u.id), ctx.run_wall),
            grade_count: unit_gates.len() as i64,
            upstream_review: None,
        },
        code: Code {
            package: ctx.package.clone(),
            language: language_for(kind_prefix, &ctx.source_lang),
            parent_sha: String::new(),
            commit_sha: commit_sha.clone(),
            patch_text: String::new(),
            files_touched: vec![u.id.clone()],
            before_ref,
            after_ref,
            workload_contract_ref: ctx.manifest_sha.clone().unwrap_or_default(),
        },
        measurement: Measurement {
            instrument: "process_cpu_median".to_string(),
            objective_metric: "wall_time_p50".to_string(),
            visible_gain_pct: None,
            heldout_gain_pct: None,
            divergence_pct: None,
            ci_low: None,
            ci_high: None,
            attribution_verified: None,
            oracle: OracleCounts {
                passed: passed.unwrap_or(0),
                total: total.unwrap_or(0),
                skipped: 0,
            },
            heldout: HeldoutCounts { passed: 0, total: 0, divergence: None },
            known_failures_k: 0,
            rss_delta_pct: None,
            alloc_delta_pct: None,
            binary_delta_pct: None,
            compile_delta_pct: None,
            noise_floor_pct: None,
            outcome: if u.status == "passed" {
                if unit_gates.iter().all(|g| g.passed) { "merged" } else { "gate_failed" }.to_string()
            } else if u.status == "failed" {
                "gate_failed".to_string()
            } else {
                // parked | abandoned: negative rows (gap 7); oracle.total == 0
                // still drops them loudly below.
                u.status.clone()
            },
            gate: unit_gates.iter().find(|g| !g.passed).map(|g| g.gate.clone()),
            harvest_class: None,
        },
        review: Review {
            gate_verdicts: unit_gates,
            council_positions: council_positions(decisions, home, stats),
            scope_verdict: scope_verdict_from_spec(&u.spec_json),
        },
        rights: Rights {
            record_license: ctx.record_license.clone(),
            upstream_repo: ctx.upstream_repo.clone(),
            upstream_license: ctx.upstream_license.clone(),
            upstream_commit: ctx.upstream_commit.clone(),
            attribution_text: ctx.attribution_text.clone(),
        },
    };
    finalize(rec, home, stats, true)
}

/// Scrub free-text fields; enforce the gutted-patch drop. Returns `None`
/// when the record must be dropped (counted by the caller).
fn finalize(mut rec: ExportRecord, home: &str, stats: &mut ExportStats, check_gutted: bool) -> Option<ExportRecord> {
    let (proposal, _) = scrub_field(&rec.provenance.proposal_text, home, stats);
    rec.provenance.proposal_text = proposal;
    let (patch, outcome) = scrub_field(&rec.code.patch_text, home, stats);
    rec.code.patch_text = patch;
    if check_gutted && !rec.code.patch_text.is_empty() && outcome.redacted_fraction() > MAX_REDACTED_FRACTION {
        stats.dropped_gutted += 1;
        return None;
    }
    Some(rec)
}

#[allow(clippy::too_many_arguments)]
fn optimization_record(
    ctx: &RunContext,
    o: &rustsmith_store::OptimizationRow,
    gates: &[rustsmith_store::GateRow],
    decisions: &[rustsmith_store::DecisionRow],
    sessions: &SessionMap,
    transcripts: &TranscriptIndex,
    home: &str,
    created_at: i64,
    version: &str,
    stats: &mut ExportStats,
) -> Option<ExportRecord> {
    let unit_id = format!("{}:{}", ctx.run_id, o.hotspot);
    let (session_id, trace_ids) = sessions.lookup(&ctx.run_id, &unit_id, &o.hotspot);
    let (model, tokens_spent, cost_usd) =
        join_provenance(transcripts, &session_id, &o.model, o.tokens_spent);
    let files_touched: Vec<String> =
        serde_json::from_str(&o.files_touched_json).unwrap_or_default();
    let run_gate_count = gates.len() as i64;
    let rec = ExportRecord {
        schema_version: SCHEMA_VERSION,
        record_id: record_id(&ctx.run_id, &unit_id, &o.parent_sha, &o.commit_sha),
        record_kind: "optimization".to_string(),
        created_at,
        exporter_version: version.to_string(),
        run_id: ctx.run_id.clone(),
        unit_id: unit_id.clone(),
        round: o.round,
        session_id,
        trace_ids,
        provenance: Provenance {
            model,
            prompt_version: o.prompt_version.clone(),
            guidance_version: o.guidance_version.clone(),
            proposal_text: o.proposal_text.clone(),
            technique: o.technique.clone(),
            bound: o.bound.clone(),
            tier: o.tier,
            tokens_spent,
            cost_usd,
            wall_seconds: wall_seconds(gates, None, ctx.run_wall),
            grade_count: run_gate_count,
            upstream_review: None,
        },
        code: Code {
            package: ctx.package.clone(),
            language: language_for(None, &ctx.source_lang),
            parent_sha: o.parent_sha.clone(),
            commit_sha: o.commit_sha.clone(),
            patch_text: o.patch_text.clone(),
            files_touched,
            before_ref: o.parent_sha.clone(),
            after_ref: o.commit_sha.clone(),
            workload_contract_ref: ctx.manifest_sha.clone().unwrap_or_default(),
        },
        measurement: Measurement {
            instrument: o.instrument.clone(),
            objective_metric: "wall_time_p50".to_string(),
            visible_gain_pct: finite_or_none(Some(o.visible_gain_pct)),
            heldout_gain_pct: finite_or_none(Some(o.heldout_gain_pct)),
            divergence_pct: finite_or_none(Some(o.divergence_pct)),
            ci_low: finite_or_none(o.ci_low),
            ci_high: finite_or_none(o.ci_high),
            attribution_verified: Some(o.attribution_verified),
            oracle: OracleCounts {
                passed: ctx.oracle_passed.unwrap_or(0),
                total: ctx.oracle_total.unwrap_or(0),
                skipped: 0,
            },
            heldout: HeldoutCounts { passed: 0, total: 0, divergence: None },
            known_failures_k: 0,
            rss_delta_pct: finite_or_none(Some(o.rss_delta_pct)),
            alloc_delta_pct: finite_or_none(Some(o.alloc_delta_pct)),
            binary_delta_pct: None,
            compile_delta_pct: None,
            noise_floor_pct: None,
            outcome: "merged".to_string(),
            gate: None,
            harvest_class: (!o.harvest_class.is_empty()).then(|| o.harvest_class.clone()),
        },
        review: Review {
            gate_verdicts: vec![],
            council_positions: council_positions(decisions, home, stats),
            scope_verdict: None,
        },
        rights: Rights {
            record_license: ctx.record_license.clone(),
            upstream_repo: ctx.upstream_repo.clone(),
            upstream_license: ctx.upstream_license.clone(),
            upstream_commit: ctx.upstream_commit.clone(),
            attribution_text: ctx.attribution_text.clone(),
        },
    };
    finalize(rec, home, stats, true)
}

#[allow(clippy::too_many_arguments)]
fn failed_record(
    ctx: &RunContext,
    f: &rustsmith_store::FailedRow,
    decisions: &[rustsmith_store::DecisionRow],
    sessions: &SessionMap,
    transcripts: &TranscriptIndex,
    home: &str,
    created_at: i64,
    version: &str,
    stats: &mut ExportStats,
) -> Option<ExportRecord> {
    let unit_id = format!("{}:{}", ctx.run_id, f.hotspot);
    let (session_id, trace_ids) = sessions.lookup(&ctx.run_id, &unit_id, &f.hotspot);
    let (model, tokens_spent, cost_usd) =
        join_provenance(transcripts, &session_id, &f.model, f.tokens_spent);
    let (detail_text, _) = scrub_field(&f.detail_json, home, stats);
    let mut detail = detail_text;
    detail.truncate(240);
    let verdicts = match &f.gate {
        Some(g) => vec![GateVerdict { gate: g.clone(), passed: false, detail_summary: detail }],
        None => vec![],
    };
    let rec = ExportRecord {
        schema_version: SCHEMA_VERSION,
        record_id: record_id(&ctx.run_id, &unit_id, &f.parent_sha, ""),
        record_kind: "failed_optimization".to_string(),
        created_at,
        exporter_version: version.to_string(),
        run_id: ctx.run_id.clone(),
        unit_id: unit_id.clone(),
        round: f.round,
        session_id,
        trace_ids,
        provenance: Provenance {
            model,
            prompt_version: f.prompt_version.clone(),
            guidance_version: f.guidance_version.clone(),
            proposal_text: f.proposal_text.clone(),
            technique: f.technique.clone(),
            bound: f.bound.clone(),
            tier: f.tier,
            tokens_spent,
            cost_usd,
            wall_seconds: ctx.run_wall,
            grade_count: 0,
            upstream_review: None,
        },
        code: Code {
            package: ctx.package.clone(),
            language: language_for(None, &ctx.source_lang),
            parent_sha: f.parent_sha.clone(),
            commit_sha: String::new(),
            patch_text: f.patch_text.clone(),
            files_touched: files_from_patch(&f.patch_text),
            before_ref: f.parent_sha.clone(),
            after_ref: String::new(),
            workload_contract_ref: ctx.manifest_sha.clone().unwrap_or_default(),
        },
        measurement: Measurement {
            instrument: String::new(),
            objective_metric: "wall_time_p50".to_string(),
            visible_gain_pct: finite_or_none(f.measured_delta_pct),
            heldout_gain_pct: None,
            divergence_pct: None,
            ci_low: None,
            ci_high: None,
            attribution_verified: None,
            oracle: OracleCounts {
                passed: ctx.oracle_passed.unwrap_or(0),
                total: ctx.oracle_total.unwrap_or(0),
                skipped: 0,
            },
            heldout: HeldoutCounts { passed: 0, total: 0, divergence: None },
            known_failures_k: 0,
            rss_delta_pct: None,
            alloc_delta_pct: None,
            binary_delta_pct: None,
            compile_delta_pct: None,
            noise_floor_pct: None,
            outcome: f.outcome.clone(),
            gate: f.gate.clone(),
            harvest_class: None,
        },
        review: Review {
            gate_verdicts: verdicts,
            council_positions: council_positions(decisions, home, stats),
            scope_verdict: None,
        },
        rights: Rights {
            record_license: ctx.record_license.clone(),
            upstream_repo: ctx.upstream_repo.clone(),
            upstream_license: ctx.upstream_license.clone(),
            upstream_commit: ctx.upstream_commit.clone(),
            attribution_text: ctx.attribution_text.clone(),
        },
    };
    finalize(rec, home, stats, true)
}

// ---------------------------------------------------------------------------
// events.jsonl backstop: per-run wall spans for QUALITY.md (never per-record
// facts: event coverage is advisory, DB rows are authoritative).
// ---------------------------------------------------------------------------

fn read_event_spans(events_path: &Path) -> HashMap<String, i64> {
    let text = match std::fs::read_to_string(events_path) {
        Ok(t) => t,
        Err(_) => return HashMap::new(),
    };
    let mut min_ts: HashMap<String, i64> = HashMap::new();
    let mut max_ts: HashMap<String, i64> = HashMap::new();
    for line in text.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let (Some(run), Some(ts)) =
            (v.get("run_id").and_then(|x| x.as_str()), v.get("ts").and_then(|x| x.as_i64()))
        else {
            continue;
        };
        min_ts.entry(run.to_string()).and_modify(|m| *m = (*m).min(ts)).or_insert(ts);
        max_ts.entry(run.to_string()).and_modify(|m| *m = (*m).max(ts)).or_insert(ts);
    }
    min_ts
        .into_iter()
        .map(|(r, lo)| {
            let hi = max_ts.get(&r).copied().unwrap_or(lo);
            (r, hi.saturating_sub(lo))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// QUALITY.md regeneration (report §4/§6.2): window base rates, calibration,
// divergence failures, drop accounting, null-honesty note.
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn quality_md(
    stats: &ExportStats,
    kinds: &BTreeMap<String, usize>,
    outcomes: &BTreeMap<String, usize>,
    by_btt: &BTreeMap<(String, i64, String), (usize, usize)>,
    calib: &BTreeMap<String, (f64, usize)>,
    divergence_failures: usize,
    run_delivered: &[(String, bool)],
    gpl_path: Option<PathBuf>,
    created_at: i64,
) -> String {
    let mut md = String::new();
    md.push_str("# Training-record quality\n\n");
    md.push_str(&format!("Exporter window generated at epoch {created_at}.\n\n"));
    md.push_str(&format!(
        "- runs seen: {}, runs exported: {}\n- records: {}\n- dropped invalid: {}, dropped gutted (>50% redacted): {}\n\n",
        stats.runs_seen, stats.runs_exported, stats.exported, stats.dropped_invalid, stats.dropped_gutted
    ));
    md.push_str("## Records by kind\n\n");
    for (k, n) in kinds {
        md.push_str(&format!("- {k}: {n}\n"));
    }
    md.push_str("\n## Outcomes\n\n");
    for (k, n) in outcomes {
        md.push_str(&format!("- {k}: {n}\n"));
    }
    md.push_str("\n## Merge rate by (bound, tier, technique)\n\n");
    md.push_str("| bound | tier | technique | merged | attempts |\n|---|---|---|---:|---:|\n");
    for ((b, t, tech), (m, a)) in by_btt {
        md.push_str(&format!("| {b} | {t} | {tech} | {m} | {a} |\n"));
    }
    md.push_str("\n## Ceiling calibration (avg ceiling − visible gain, merged)\n\n");
    if calib.is_empty() {
        md.push_str("No merged rows with ceiling data in this window.\n");
    }
    for (b, (sum, n)) in calib {
        md.push_str(&format!("- {b}: {:.4} over {n} row(s)\n", sum / (*n as f64)));
    }
    md.push_str(&format!("\n## Soundness\n\n- heldout-divergence failures: {divergence_failures}\n"));
    if let Some(p) = gpl_path {
        md.push_str(&format!("\n## License split\n\n- GPL-licensed rows: {} (opt-in separate split: {})\n", stats.gpl_rows, p.display()));
    } else if stats.gpl_rows > 0 {
        md.push_str(&format!("\n## License split\n\n- GPL-licensed rows: {} (inline in main file; rights.attribution_text records the training-use permission)\n", stats.gpl_rows));
    }
    if !run_delivered.is_empty() {
        md.push_str("\n## Runs (delivered artifact = bench/ACCEPTED.json present)\n\n");
        for (r, d) in run_delivered {
            md.push_str(&format!("- {r}: delivered={}\n", if *d { "yes" } else { "no" }));
        }
    }
    if !stats.invalid_reasons.is_empty() {
        md.push_str("\n## Invalid drops (fail-loud accounting)\n\n");
        for (r, n) in &stats.invalid_reasons {
            md.push_str(&format!("- {r}: {n}\n"));
        }
    }
    if !stats.run_spans.is_empty() {
        md.push_str("\n## Run wall spans (events.jsonl backstop, advisory)\n\n");
        for (r, s) in &stats.run_spans {
            match s {
                Some(span) => md.push_str(&format!("- {r}: {span}s\n")),
                None => md.push_str(&format!("- {r}: no events\n")),
            }
        }
    }
    md.push_str(
        "\n## Honesty note\n\nUnmeasured fields are null, never zero-filled. \
         Slower-than-original and failed rows ship alongside wins; base rates above are the window as graded.\n",
    );
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_home() -> String {
        "/home/tester".to_string()
    }

    fn minimal_record() -> ExportRecord {
        ExportRecord {
            schema_version: 1,
            record_id: "0123456789abcdef".to_string(),
            record_kind: "optimization".to_string(),
            created_at: 0,
            exporter_version: "rustsmith 0.1.0+test".to_string(),
            run_id: "r".to_string(),
            unit_id: "u".to_string(),
            round: 1,
            session_id: None,
            trace_ids: TraceIds { omp: None, phoenix: None, tripwire: None },
            provenance: Provenance {
                model: "m".to_string(),
                prompt_version: "p".to_string(),
                guidance_version: "g".to_string(),
                proposal_text: "why".to_string(),
                technique: "t".to_string(),
                bound: "compute".to_string(),
                tier: 8,
                tokens_spent: 0,
                cost_usd: None,
                wall_seconds: None,
                grade_count: 0,
                upstream_review: None,
            },
            code: Code {
                package: "crc".to_string(),
                language: "python".to_string(),
                parent_sha: "aaa".to_string(),
                commit_sha: "bbb".to_string(),
                patch_text: "diff".to_string(),
                files_touched: vec![],
                before_ref: "aaa".to_string(),
                after_ref: "bbb".to_string(),
                workload_contract_ref: String::new(),
            },
            measurement: Measurement {
                instrument: "cpu_time".to_string(),
                objective_metric: "wall_time_p50".to_string(),
                visible_gain_pct: Some(1.5),
                heldout_gain_pct: None,
                divergence_pct: Some(0.0),
                ci_low: None,
                ci_high: None,
                attribution_verified: Some(true),
                oracle: OracleCounts { passed: 80, total: 80, skipped: 0 },
                heldout: HeldoutCounts { passed: 0, total: 0, divergence: None },
                known_failures_k: 0,
                rss_delta_pct: None,
                alloc_delta_pct: None,
                binary_delta_pct: None,
                compile_delta_pct: None,
                noise_floor_pct: None,
                outcome: "merged".to_string(),
                gate: None,
                harvest_class: None,
            },
            review: Review { gate_verdicts: vec![], council_positions: vec![], scope_verdict: None },
            rights: Rights {
                record_license: "MIT".to_string(),
                upstream_repo: "https://github.com/x/y".to_string(),
                upstream_license: "MIT".to_string(),
                upstream_commit: "abc".to_string(),
                attribution_text: "x/y".to_string(),
            },
        }
    }

    #[test]
    fn valid_record_passes() {
        assert!(validate(&minimal_record()).is_ok());
    }

    #[test]
    fn empty_patch_rejected_outside_rejection() {
        let mut r = minimal_record();
        r.code.patch_text = String::new();
        assert!(validate(&r).is_err());
    }

    #[test]
    fn rejected_at_proposal_may_carry_empty_patch() {
        let mut r = minimal_record();
        r.record_kind = "failed_optimization".to_string();
        r.measurement.outcome = "rejected_at_proposal".to_string();
        r.code.patch_text = String::new();
        r.code.parent_sha = String::new();
        assert!(validate(&r).is_ok());
    }

    #[test]
    fn zero_oracle_total_rejected() {
        let mut r = minimal_record();
        r.measurement.oracle.total = 0;
        assert!(validate(&r).is_err());
    }

    #[test]
    fn nan_gain_rejected() {
        let mut r = minimal_record();
        r.measurement.visible_gain_pct = Some(f64::NAN);
        assert!(validate(&r).is_err());
    }

    #[test]
    fn empty_upstream_license_rejected() {
        let mut r = minimal_record();
        r.rights.upstream_license = String::new();
        assert!(validate(&r).is_err());
    }

    #[test]
    fn mirror_requires_commit_sha() {
        let mut r = minimal_record();
        r.record_kind = "mirror_unit".to_string();
        r.code.commit_sha = String::new();
        assert!(validate(&r).is_err());
        r.code.commit_sha = "ccc".to_string();
        r.code.patch_text = String::new();
        assert!(validate(&r).is_ok());
    }

    #[test]
    fn record_id_is_stable_16_hex() {
        let a = record_id("r", "u", "aaa", "bbb");
        assert_eq!(a, record_id("r", "u", "aaa", "bbb"));
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, record_id("r", "u", "aaa", "bbc"));
    }

    #[test]
    fn files_from_patch_reads_diff_headers() {
        let patch = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n\
                     diff --git a/new.rs b/new.rs\n--- /dev/null\n+++ b/new.rs\n";
        assert_eq!(files_from_patch(patch), vec!["x.rs".to_string(), "new.rs".to_string()]);
        assert!(files_from_patch("").is_empty());
    }

    #[test]
    fn since_parses_utc_midnight() {
        // 2026-10-26 00:00:00 UTC.
        assert_eq!(parse_since("2026-10-26").unwrap(), 1792972800);
        assert!(parse_since("26-10-2026").is_err());
        assert!(parse_since("2026-13-01").is_err());
    }

    #[test]
    fn heldout_summary_never_carries_magnitudes() {
        let v = gate_summary(
            "heldout_divergence",
            true,
            r#"{"divergence":0.0,"heldout_rate":1.0,"visible_rate":1.0,"threshold":0.05}"#,
            &test_home(),
        );
        assert!(v.passed);
        assert!(!v.detail_summary.contains("heldout_rate"));
        assert!(!v.detail_summary.contains("visible_rate"));
        assert!(!v.detail_summary.contains("divergence="));
    }

    #[test]
    fn session_map_lookup_falls_back_to_hotspot() {
        let mut m = SessionMap::empty();
        m.inner.insert(
            ("r".to_string(), "hot".to_string()),
            (Some("s1".to_string()), TraceIds { omp: Some("o".to_string()), phoenix: None, tripwire: None }),
        );
        let (s, t) = m.lookup("r", "r:hot", "hot");
        assert_eq!(s.as_deref(), Some("s1"));
        assert_eq!(t.omp.as_deref(), Some("o"));
        let (s2, _) = m.lookup("r", "nope", "alsono");
        assert!(s2.is_none());
    }
    #[test]
    fn gpl_detection_drives_split() {
        assert!(is_gpl("GPL-2.0-or-later"));
        assert!(is_gpl("GPL-3.0-only"));
        assert!(!is_gpl("MIT"));
        assert!(!is_gpl("BSD-3-Clause"));
        assert!(!is_gpl(""));
    }

    #[test]
    fn record_license_defaults_and_lane_override() {
        let d = std::env::temp_dir().join(format!("rs-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        // No sidecar: MIT unless GPL upstream.
        assert_eq!(record_license(&d, "BSD-3-Clause"), "MIT");
        assert_eq!(record_license(&d, "GPL-2.0-or-later"), "GPL-2.0-or-later");
        // lane.toml wins over everything.
        std::fs::write(d.join("lane.toml"), "record_license = \"Apache-2.0\"\nupstream_license = \"GPL-2.0-or-later\"\n").unwrap();
        assert_eq!(record_license(&d, "GPL-2.0-or-later"), "Apache-2.0");
        assert_eq!(upstream_license(&d), "GPL-2.0-or-later");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn transcript_join_keeps_store_values_when_absent() {
        let idx = TranscriptIndex::empty();
        let (model, tokens, cost) = join_provenance(&idx, &None, "template", 42);
        assert_eq!((model.as_str(), tokens, cost), ("template", 42, None));
        let (model, tokens, cost) =
            join_provenance(&idx, &Some("nope".to_string()), "template", 42);
        assert_eq!((model.as_str(), tokens, cost), ("template", 42, None));
    }

    #[test]
    fn wall_seconds_prefers_gate_detail_over_run_span() {
        let gates = vec![rustsmith_store::GateRow {
            unit_id: "u".to_string(),
            gate: "oracle_parity".to_string(),
            passed: true,
            detail_json: r#"{"passed":80,"total":80,"wall_secs":12.7}"#.to_string(),
        }];
        assert_eq!(wall_seconds(&gates, Some("u"), Some(99)), Some(12));
        assert_eq!(wall_seconds(&gates, Some("other"), Some(99)), Some(99));
        assert_eq!(wall_seconds(&[], None, None), None);
    }

    #[test]
    fn scope_verdict_comes_from_spec_json() {
        assert_eq!(
            scope_verdict_from_spec(r#"{"module":"x","scope_verdict":"admit"}"#).as_deref(),
            Some("admit")
        );
        assert!(scope_verdict_from_spec(r#"{"module":"x"}"#).is_none());
        assert!(scope_verdict_from_spec("not json").is_none());
    }

    fn lane_ctx() -> RunContext {
        RunContext {
            run_id: "r".to_string(),
            source_lang: "python".to_string(),
            package: "p".to_string(),
            manifest_sha: None,
            oracle_passed: None,
            oracle_total: None,
            upstream_repo: "https://github.com/x/y".to_string(),
            upstream_commit: String::new(),
            upstream_license: "MIT".to_string(),
            record_license: "MIT".to_string(),
            attribution_text: "x/y".to_string(),
            delivered_artifact: false,
            run_wall: Some(60),
        }
    }

    #[test]
    fn mirror_failed_unit_exports_negative_row() {
        let ctx = lane_ctx();
        let u = rustsmith_store::UnitRow {
            id: "python:mod".to_string(),
            kind: "mirror".to_string(),
            spec_json: r#"{"module":"mod","scope_verdict":"admit"}"#.to_string(),
            status: "failed".to_string(),
            attempts: 1,
            commit_sha: Some("ccc".to_string()),
            tokens_in: 10,
            tokens_out: 5,
        };
        let gates = vec![rustsmith_store::GateRow {
            unit_id: "python:mod".to_string(),
            gate: "oracle_parity".to_string(),
            passed: false,
            detail_json: r#"{"passed":70,"total":80,"wall_secs":9.0}"#.to_string(),
        }];
        let rec = mirror_record(
            &ctx, &u, &gates, &[], &[], &SessionMap::empty(), &TranscriptIndex::empty(),
            &test_home(), 0, "v", &mut ExportStats::default(),
        )
        .unwrap();
        assert_eq!(rec.measurement.outcome, "gate_failed");
        assert_eq!(rec.measurement.oracle.total, 80);
        assert_eq!(rec.provenance.wall_seconds, Some(9));
        assert_eq!(rec.provenance.model, "template");
        assert_eq!(rec.review.scope_verdict.as_deref(), Some("admit"));
        assert_eq!(rec.rights.record_license, "MIT");
    }

    #[test]
    fn nan_cost_rejected() {
        let mut r = minimal_record();
        r.provenance.cost_usd = Some(f64::NAN);
        assert!(validate(&r).is_err());
        r.provenance.cost_usd = Some(0.02);
        assert!(validate(&r).is_ok());
    }
}
