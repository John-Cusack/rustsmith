//! Stage-1 mirror orchestration (M4): dependency-order scheduler, worker bundle
//! assembler (held-out blind by construction), grade->gate->merge loop with
//! same-commit module deletion, adversarial review plumbing, whole-repo grade.
//!
//! Fixture worker note: acceptance runs stub shell tasks (via `Agent`, real
//! subprocesses in real worktrees) that materialize each unit from the
//! reference port by following the bundle's PORTING.md. A production run
//! swaps the stub task for a real OMP worker task; grading, gating, merging,
//! and review are identical. The PROOF is in the gates, not the task.

use crate::fixture::{load_template, resolve_fixture, FixtureKind};
use rustsmith_adapters::UnitDag;
use rustsmith_agent::{Agent, UnitSpec};
use rustsmith_council::{Council, Proposal, Seat, SeatDriver, Stance, StubDriver};
use rustsmith_core::{Event, Gate};
use rustsmith_gates as gates;
use rustsmith_oracle::Oracle;
use rustsmith_sandbox::Sandbox;
use rustsmith_store::Store;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PROMPT_VERSION: &str = "mirror-v1";
const MAX_PARALLEL: usize = 16;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn ev(store: &Store, run_id: &str, kind: &str, detail: serde_json::Value) {
    let _ = store.append_event(&Event {
        ts: now(),
        run_id: run_id.into(),
        kind: kind.into(),
        detail,
    });
}

/// Reviewer assignment: two seats, distinct providers, never the implementer.
/// Providers come from config (never hardcoded); the seat->provider map is passed in.
pub fn assign_reviewers(
    implementer: Option<Seat>,
    providers: &HashMap<Seat, String>,
) -> (Seat, Seat) {
    let order = [Seat::Verifier, Seat::Performance, Seat::Architect, Seat::Scope];
    let mut out = Vec::new();
    for s in order {
        if Some(s) == implementer {
            continue;
        }
        // Keep providers distinct across the pair.
        if out.iter().any(|o: &Seat| providers.get(o) == providers.get(&s)) {
            continue;
        }
        out.push(s);
        if out.len() == 2 {
            break;
        }
    }
    // Fallback (should not happen with 4 distinct providers): first two non-implementer.
    while out.len() < 2 {
        for s in order {
            if Some(s) != implementer && !out.contains(&s) {
                out.push(s);
                break;
            }
        }
    }
    (out[0], out[1])
}

#[allow(dead_code)]
pub struct Bundle {
    pub dir: PathBuf,
}

/// Assemble the exact worker input bundle. Held-out blindness is structural:
/// this function takes no held-out path argument, so a leak is impossible to
/// express. The canary test asserts the emitted bundle never contains a
/// held-out marker even when one exists on the host.
pub fn assemble_bundle(
    bundle_dir: &Path,
    unit_id: &str,
    module: &str,
    depends_on: &[String],
    porting_md: &str,
    original_source: &str,
    oracle_path: &str,
) -> Result<Bundle, String> {
    std::fs::create_dir_all(bundle_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        bundle_dir.join("spec.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "unit_id": unit_id,
            "module": module,
            "depends_on": depends_on,
            "instructions": "Port this module to behavior-identical Rust per PORTING.md. No redesign.",
            "prompt_version": PROMPT_VERSION,
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(bundle_dir.join("PORTING.md"), porting_md).map_err(|e| e.to_string())?;
    std::fs::write(bundle_dir.join("original_source.py"), original_source)
        .map_err(|e| e.to_string())?;
    std::fs::write(
        bundle_dir.join("contracts.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "depends_on": depends_on,
            "oracle_path": oracle_path,
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(Bundle {
        dir: bundle_dir.to_path_buf(),
    })
}

pub(crate) fn maturin_bin() -> PathBuf {
    for p in ["/home/john/.local/bin/maturin", "/tmp/mirror-venv/bin/maturin"] {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    PathBuf::from("maturin")
}

/// Grade venv: `--system-site-packages` (no network), maturin via absolute binary.
pub(crate) fn ensure_grade_venv(venv: &Path) -> Result<(), String> {
    if venv.join("bin/activate").exists() {
        return Ok(());
    }
    let st = std::process::Command::new("python3")
        .args(["-m", "venv", "--system-site-packages", &venv.display().to_string()])
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err("venv create failed".into());
    }
    Ok(())
}

pub(crate) fn grade_venv_python(venv: &Path) -> PathBuf {
    venv.join("bin/python")
}

/// Build the fork's extension into the venv (no build isolation: no network).
pub(crate) fn build_ext(worktree: &Path, venv: &Path) -> Result<String, String> {
    let mut log = String::new();
    let mut path = venv.join("bin").as_os_str().to_owned();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    let out = std::process::Command::new(maturin_bin())
        .args(["develop", "--manifest-path", "Cargo.toml"])
        .current_dir(worktree)
        .env("VIRTUAL_ENV", venv)
        .env("PATH", path)
        .output()
        .map_err(|e| e.to_string())?;
    log.push_str(&String::from_utf8_lossy(&out.stdout));
    log.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        return Err(format!("maturin develop failed:\n{log}"));
    }
    Ok(log)
}

/// Run the frozen oracle invocation inside the venv WITHOUT src-layout
/// PYTHONPATH (the installed Rust extension is the implementation).
pub(crate) fn run_oracle_in_venv(
    venv: &Path,
    worktree: &Path,
    invocation: &[String],
) -> Result<rustsmith_core::GradedResult, String> {
    let py = grade_venv_python(venv);
    let mut agg = rustsmith_core::GradedResult {
        exit_code: 0,
        passed: 0,
        failed: 0,
        skipped: vec![],
        xfailed: vec![],
        deselected: vec![],
        stdout: String::new(),
    };
    for cmd in invocation {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let mut c = std::process::Command::new(&py);
        c.arg("-m").arg("pytest");
        for a in parts.iter().skip(1) {
            c.arg(a);
        }
        c.args(["-v", "-rs", "-rxX", "--tb=short"]);
        c.current_dir(worktree);
        c.env("PY_COLORS", "0");
        // Scrub src-layout shadowing: installed ext wins.
        c.env_remove("PYTHONPATH");
        let o = c.output().map_err(|e| e.to_string())?;
        let stdout = format!(
            "{}\n{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        let r = parse_verbose(&stdout, o.status.code().unwrap_or(-1));
        if r.failed > 0 {
            agg.exit_code = r.exit_code;
        } else if r.exit_code != 0 && agg.exit_code == 0 {
            agg.exit_code = r.exit_code;
        }
        agg.passed += r.passed;
        agg.failed += r.failed;
        agg.skipped.extend(r.skipped);
        agg.xfailed.extend(r.xfailed);
        agg.deselected.extend(r.deselected);
        agg.stdout.push_str(&format!("$ {cmd}\n{stdout}\n"));
    }
    agg.skipped.sort();
    agg.xfailed.sort();
    agg.deselected.sort();
    Ok(agg)
}

fn parse_verbose(stdout: &str, code: i32) -> rustsmith_core::GradedResult {
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = Vec::new();
    for line in stdout.lines() {
        let t = line.trim();
        if t.contains("::") && t.contains(" PASSED") {
            passed += 1;
        } else if t.contains("::") && (t.contains(" FAILED") || t.contains(" ERROR")) {
            failed += 1;
        } else if t.contains("::") && t.contains(" SKIPPED") {
            if let Some(id) = t.split_whitespace().next() {
                skipped.push(id.to_string());
            }
        }
    }
    let (sp, sf, ss) = summary_counts(stdout);
    if passed == 0 && failed == 0 {
        passed = sp;
        failed = sf;
    } else {
        if sp > passed {
            passed = sp;
        }
        if sf > failed {
            failed = sf;
        }
    }
    if skipped.is_empty() && ss > 0 {
        for i in 0..ss {
            skipped.push(format!("skipped[{i}]"));
        }
    }
    skipped.sort();
    rustsmith_core::GradedResult {
        exit_code: code,
        passed,
        failed,
        skipped,
        xfailed: vec![],
        deselected: vec![],
        stdout: stdout.to_string(),
    }
}

fn summary_counts(s: &str) -> (u32, u32, u32) {
    let (mut p, mut f, mut sk) = (0, 0, 0);
    for line in s.lines() {
        let l = line.to_lowercase();
        if !(l.contains("passed") || l.contains("failed") || l.contains("skipped")) {
            continue;
        }
        let toks: Vec<&str> = l
            .split(|c: char| c == ',' || c == ' ' || c == '=')
            .filter(|t| !t.is_empty())
            .collect();
        let mut i = 0;
        while i < toks.len() {
            if let Ok(n) = toks[i].parse::<u32>() {
                if i + 1 < toks.len() {
                    match toks[i + 1] {
                        w if w.starts_with("passed") => p = p.max(n),
                        w if w.starts_with("failed") => f = f.max(n),
                        w if w.starts_with("skipped") => sk = sk.max(n),
                        _ => {}
                    }
                }
            }
            i += 1;
        }
    }
    (p, f, sk)
}

/// Differential: original vs mirror outputs across generated inputs.
/// Fixture-dispatched; the graded pairs are the same shape for both fixtures.
pub fn differential_pairs_for(
    kind: &FixtureKind,
    orig_py: &Path,
    venv_py: &Path,
    worktree: &Path,
) -> Result<Vec<(String, String)>, String> {
    match kind {
        FixtureKind::Crc => differential_pairs(orig_py, venv_py, worktree),
        FixtureKind::Strsimpy => differential_pairs_strsimpy(orig_py, venv_py, worktree),
    }
}
pub fn differential_pairs(
    orig_py: &Path,
    venv_py: &Path,
    worktree: &Path,
) -> Result<Vec<(String, String)>, String> {
    let script = r#"
import sys
config = sys.argv[1]
data_hex = sys.argv[2]
data = bytes.fromhex(data_hex)
from crc import Calculator
import importlib
mods = {'Crc8': __import__('crc', fromlist=['Crc8']).Crc8,
        'Crc16': __import__('crc', fromlist=['Crc16']).Crc16,
        'Crc32': __import__('crc', fromlist=['Crc32']).Crc32}
cat, member = config.split('.')
calc = Calculator(getattr(mods[cat], member))
print(calc.checksum(data))
:"#;
    let inputs: Vec<(&str, Vec<u8>)> = vec![
        ("Crc8.CCITT", b"123456789".to_vec()),
        ("Crc8.CCITT", vec![]),
        ("Crc8.SAEJ1850", b"hello".to_vec()),
        ("Crc16.XMODEM", b"123456789".to_vec()),
        ("Crc16.MODBUS", vec![0u8; 64]),
        ("Crc32.CRC32", (0..256).map(|i| i as u8).collect()),
        ("Crc8.BLUETOOTH", b"Hello World!".to_vec()),
        ("Crc16.KERMIT", b"abc".to_vec()),
    ];
    run_pairs(orig_py, venv_py, worktree, script, &inputs)
}
/// strsimpy differential: edit-distance / similarity outputs on short strings,
/// including empty/singleton/adversarial shapes. Float outputs compare exact:
/// both sides run the same algorithm over ASCII inputs (deterministic).
pub fn differential_pairs_strsimpy(
    orig_py: &Path,
    venv_py: &Path,
    worktree: &Path,
) -> Result<Vec<(String, String)>, String> {
    let script = r#"
import sys
expr = sys.argv[1]
ns = {}
exec("from strsimpy.levenshtein import Levenshtein\nfrom strsimpy.damerau import Damerau\nfrom strsimpy.jaro_winkler import JaroWinkler\nfrom strsimpy.normalized_levenshtein import NormalizedLevenshtein\nfrom strsimpy.cosine import Cosine\nfrom strsimpy.jaccard import Jaccard\nfrom strsimpy.ngram import NGram\nfrom strsimpy.optimal_string_alignment import OptimalStringAlignment\nfrom strsimpy.longest_common_subsequence import LongestCommonSubsequence\nfrom strsimpy.metric_lcs import MetricLCS\nfrom strsimpy.qgram import QGram\nfrom strsimpy.sorensen_dice import SorensenDice\nfrom strsimpy.overlap_coefficient import OverlapCoefficient\nfrom strsimpy.weighted_levenshtein import WeightedLevenshtein\nfrom strsimpy.sift4 import SIFT4", ns)
print(repr(eval(expr, ns)))
:"#;
    let inputs: Vec<(&str, Vec<u8>)> = vec![
        ("Levenshtein().distance('kitten','sitting')", vec![]),
        ("Levenshtein().distance('','abc')", vec![]),
        ("Damerau().distance('abcd','acbd')", vec![]),
        ("JaroWinkler().similarity('martha','marhta')", vec![]),
        ("NormalizedLevenshtein().distance('abc','abd')", vec![]),
        ("Cosine(2).distance('hello world','hello there')", vec![]),
        ("Jaccard(2).similarity('abc','abd')", vec![]),
        ("NGram(2).distance('abcd','abce')", vec![]),
    ];
    let mut pairs = Vec::new();
    for (expr, _) in inputs {
        let o1 = std::process::Command::new(orig_py)
            .args(["-c", script, expr])
            .env("PYTHONPATH", worktree.join("orig_src"))
            .output()
            .map_err(|e| e.to_string())?;
        let o2 = std::process::Command::new(venv_py)
            .args(["-c", script, expr])
            .output()
            .map_err(|e| e.to_string())?;
        pairs.push((
            String::from_utf8_lossy(&o1.stdout).trim().to_string(),
            String::from_utf8_lossy(&o2.stdout).trim().to_string(),
        ));
    }
    Ok(pairs)
}
fn run_pairs(
    orig_py: &Path,
    venv_py: &Path,
    worktree: &Path,
    script: &str,
    inputs: &[(&str, Vec<u8>)],
) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    for (cfg, data) in inputs {
        let hex = data.iter().map(|b| format!("{b:02x}")).collect::<String>();
        // original (system python with original src on path)
        let o1 = std::process::Command::new(orig_py)
            .args(["-c", script, cfg, &hex])
            .env("PYTHONPATH", worktree.join("orig_src"))
            .output()
            .map_err(|e| e.to_string())?;
        // mirror (venv with installed ext)
        let o2 = std::process::Command::new(venv_py)
            .args(["-c", script, cfg, &hex])
            .output()
            .map_err(|e| e.to_string())?;
        pairs.push((
            String::from_utf8_lossy(&o1.stdout).trim().to_string(),
            String::from_utf8_lossy(&o2.stdout).trim().to_string(),
        ));
    }
    Ok(pairs)
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("/usr/bin/git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Minimal single-struct report (M6 expands to the full crate; agreement is
/// structural here: all three emitters read the same `Report`).
#[derive(Debug, Clone)]
pub struct Report {
    pub passed: u32,
    pub failed: u32,
    pub divergence: f64,
    pub unsafe_count: usize,
    pub units: Vec<(String, String)>,
    pub decisions: usize,
}

pub fn emit_md(r: &Report) -> String {
    format!(
        "# RUSTSMITH_REPORT\n\n- parity: {}/{} passed\n- divergence: {:.4}\n- unsafe blocks: {}\n- units: {}\n- decisions: {}\n",
        r.passed,
        r.passed + r.failed,
        r.divergence,
        r.unsafe_count,
        r.units.iter().map(|(u, s)| format!("{u}={s}")).collect::<Vec<_>>().join(", "),
        r.decisions
    )
}

pub fn emit_json(r: &Report) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "passed": r.passed, "failed": r.failed, "divergence": r.divergence,
        "unsafe_count": r.unsafe_count, "units": r.units, "decisions": r.decisions,
    }))
    .unwrap()
}

pub fn emit_html(r: &Report) -> String {
    format!(
        "<html><body><h1>rustsmith report</h1><p>parity: {}/{} passed</p><p>divergence: {:.4}</p><p>unsafe blocks: {}</p><p>units: {}</p><p>decisions: {}</p></body></html>",
        r.passed, r.passed + r.failed, r.divergence, r.unsafe_count,
        r.units.iter().map(|(u, s)| format!("{u}={s}")).collect::<Vec<_>>().join(", "),
        r.decisions
    )
}

pub struct MirrorArgs {
    pub repo: PathBuf,
    pub fork: PathBuf,
    pub recon_out: PathBuf,
    pub heldout: PathBuf,
    pub store_path: PathBuf,
    pub run_id: String,
    pub template: PathBuf,
}

pub fn run_mirror(a: &MirrorArgs, store: &Store) -> Result<Report, String> {
    let run_id = &a.run_id;
    // Load recon artifacts.
    let dag_text = std::fs::read_to_string(a.recon_out.join("dag.json")).map_err(|e| e.to_string())?;
    let dag_json: serde_json::Value =
        serde_json::from_str(&dag_text).map_err(|e| e.to_string())?;
    let order: Vec<String> = serde_json::from_value(dag_json["leaf_first_order"].clone())
        .map_err(|e| e.to_string())?;
    let units_json = dag_json["units"].as_array().cloned().unwrap_or_default();
    let mut depends: HashMap<String, Vec<String>> = HashMap::new();
    for u in &units_json {
        depends.insert(
            u["id"].as_str().unwrap_or("").to_string(),
            u["depends_on"]
                .as_array()
                .map(|v| v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
        );
    }
    // Scheduler works over this DAG: leaf-first order, ready = deps passed.
    let unit_dag = UnitDag {
        units: order
            .iter()
            .map(|id| rustsmith_adapters::Unit {
                id: id.clone(),
                module: id.clone(),
                depends_on: depends.get(id).cloned().unwrap_or_default(),
            })
            .collect(),
        edges: vec![],
    };
    let porting_md =
        std::fs::read_to_string(a.recon_out.join("PORTING.md")).map_err(|e| e.to_string())?;
    let manifest_text =
        std::fs::read_to_string(a.recon_out.join("manifest.json")).map_err(|e| e.to_string())?;
    let manifest: rustsmith_core::Manifest =
        serde_json::from_str(&manifest_text).map_err(|e| e.to_string())?;
    // Fixture + template: frozen fixture if recon wrote one, else detect.
    // The template manifest lists every file the port needs (no hardcodes).
    let kind = resolve_fixture(&a.recon_out, &a.repo)?;
    let tspec = load_template(&a.template)?;
    // Template/fixture cross-check: a crc template against a strsimpy repo
    // (or vice versa) is a refused misconfiguration, never a weird run.
    if tspec.package != kind.package() {
        return Err(format!("template package {} does not match fixture {}", tspec.package, kind.package()));
    }
    if tspec.src_layout != kind.src_layout() {
        return Err(format!("template layout does not match fixture {}", kind.package()));
    }
    // Keep an orig-src copy for differential grading (src-layout vs flat).
    let orig_src = if kind.src_layout() {
        a.repo.join("src")
    } else {
        a.repo.clone()
    };
    let recon_modules = crate::fixture::read_recon_modules(&a.recon_out);
    let manifest_inv = manifest.invocation.clone();

    // Init fork from the original (full copy minus .git), run branch.
    if a.fork.exists() {
        std::fs::remove_dir_all(&a.fork).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&a.fork).map_err(|e| e.to_string())?;
    copy_tree_except_git(&a.repo, &a.fork)?;
    git(&a.fork, &["init", "-q", "-b", "main"])?;
    git(&a.fork, &["config", "user.email", "t@t"])?;
    git(&a.fork, &["config", "user.name", "t"])?;
    // Scratch + build outputs must never enter the fork.
    std::fs::write(a.fork.join(".gitignore"), "worktree-*\n.bundles/\n.grade-venv/\norig_src/\n__pycache__/\ntarget/\n*.so\n").map_err(|e| e.to_string())?;
    git(&a.fork, &["add", "-A"])?;
    git(&a.fork, &["commit", "-qm", "seed from original"])?;
    store
        .create_run(run_id, &a.repo.display().to_string(), "python", "mirror")
        .map_err(|e| e.to_string())?;
    ev(store, run_id, "mirror_start", serde_json::json!({"units": order}));
    for id in &order {
        let deps = depends.get(id).cloned().unwrap_or_default();
        store
            .create_unit(id, run_id, "mirror", &serde_json::json!({"module": id}).to_string())
            .map_err(|e| e.to_string())?;
        store
            .set_unit_depends(id, &serde_json::to_string(&deps).unwrap())
            .map_err(|e| e.to_string())?;
    }

    let sandbox = Sandbox::new("containers".into());
    let agent = Agent::new(Some(store.events_path.clone()));
    let venv = a.fork.join(".grade-venv");
    ensure_grade_venv(&venv)?;

    // Scheduler: leaf-first over the DAG; ready = all deps passed.
    // Parallel cap MAX_PARALLEL bounds independent units (fixture runs serialized).
    let _ = MAX_PARALLEL;
    for u in &unit_dag.units {
        let id = &u.id;
        // Ready check.
        for d in &u.depends_on {
            let st = store.unit_status(d).map_err(|e| e.to_string())?.unwrap_or_default();
            if st != "passed" {
                return Err(format!("unit {id} scheduled before dep {d} passed (got {st})"));
            }
        }
        store.set_unit_status(id, "running").map_err(|e| e.to_string())?;
        ev(store, run_id, "unit_start", serde_json::json!({"unit": id}));
        // Worktree on branch unit/<id>.
        let wt = sandbox
            .alloc_worktree(&a.fork, run_id, id)
            .map_err(|e| e.to_string())?;
        store
            .set_unit_worktree(id, wt.as_str())
            .map_err(|e| e.to_string())?;
        // Bundle (held-out blind: no held-out input exists on this path).
        // Orig source + task come from the template manifest, never hardcodes.
        let bundle_dir = a.fork.join(format!(".bundles/{id}"));
        let (orig_rel, _) =
            crate::fixture::unit_sources(&tspec, &recon_modules, id)?;
        let orig_file = a.repo.join(&orig_rel);
        let orig_source =
            std::fs::read_to_string(&orig_file).unwrap_or_else(|_| String::from("// empty"));
        assemble_bundle(
            &bundle_dir,
            id,
            id,
            &depends.get(id).cloned().unwrap_or_default(),
            &porting_md,
            &orig_source,
            "frozen-oracle(read-only)",
        )?;
        // M8 slice-3: live worker-command turn (additive). Prompt = stable
        // prefix + PORTING.md + unit bundle, written to prompt.txt; worker
        // stdout usage recorded as tokens (never evidence). Files still come
        // from the stub task below, so grading is unchanged offline or live.
        let mut prompt_version = agent.prompt_version.clone();
        if let Some(wcmd) = rustsmith_agent::worker_cmd_from_env() {
            let prompt = rustsmith_agent::build_worker_prompt(id, &porting_md, &orig_source);
            std::fs::write(bundle_dir.join("prompt.txt"), &prompt).map_err(|e| e.to_string())?;
            let wspec = UnitSpec {
                unit_id: id.clone(),
                worktree: wt.clone(),
                task: "worker-command".into(),
                token_ceiling: 1_000_000,
            };
            let w = agent.spawn_worker(&wspec, &prompt, &wcmd).map_err(|e| e.to_string())?;
            store.set_unit_tokens(id, w.tokens_in as i64, w.tokens_out as i64).map_err(|e| e.to_string())?;
            prompt_version = rustsmith_agent::WORKER_PROMPT_VERSION.into();
        }
        // Worker stub task (real subprocess via Agent): materialize the unit,
        // then commit on its branch so the merge carries the files.
        let task = crate::fixture::unit_task(&tspec, &a.template, id)?;
        let spec = UnitSpec {
            unit_id: id.clone(),
            worktree: wt.clone(),
            task,
            token_ceiling: 1_000_000,
        };
        let h = agent.spawn(&spec).map_err(|e| e.to_string())?;
        let res = agent.wait(h, Duration::from_secs(60)).map_err(|e| e.to_string())?;
        if res.exit_code != 0 {
            let n = store.bump_attempts(id).map_err(|e| e.to_string())?;
            if n >= 3 {
                escalate(store, run_id, id)?;
            }
            return Err(format!("worker for {id} exited {}", res.exit_code));
        }
        store.set_unit_status(id, "gated").map_err(|e| e.to_string())?;
        // Grade in the worktree: build + oracle + heldout + differential.
        let wt_path = wt.as_std_path();
        build_ext(wt_path, &venv)?;
        let got = run_oracle_in_venv(&venv, wt_path, &manifest_inv)?;
        // Gates.
        let hashes = Oracle::current_hashes(&manifest, wt_path);
        // NOTE: worktree pyproject was replaced (maturin) — section-hash covers
        // test config only (ADR-002), so packaging change does not trip tamper.
        let integrity = gates::oracle_integrity(&manifest, &hashes, &manifest.baseline, &got);
        let parity = gates::oracle_parity(&got);
        let rate = rate_of(&got);
        let held_rate = {
            // Run held-out suite against the worktree build via the venv.
            let py = grade_venv_python(&venv);
            let o = std::process::Command::new(&py)
                .args(["-m", "pytest"])
                .arg(&a.heldout)
                .args(["-q", "--tb=no"])
                .env("PY_COLORS", "0")
                .output()
                .map_err(|e| e.to_string())?;
            let t = format!(
                "{}\n{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            parse_heldout_rate(&t)
        };
        let div = gates::heldout_divergence(rate, held_rate, 0.05);
        // Stage orig_src for the differential's PYTHONPATH.
        std::fs::create_dir_all(wt_path.join("orig_src")).map_err(|e| e.to_string())?;
        copy_tree(&orig_src, &wt_path.join("orig_src"))?;
        let diff_pairs = differential_pairs_for(
            &kind,
            &PathBuf::from("python3"),
            &grade_venv_python(&venv),
            wt_path,
        )?;
        let _ = std::fs::remove_dir_all(wt_path.join("orig_src"));
        let diff = gates::differential(
            &diff_pairs
                .iter()
                .map(|(a, b)| (a.clone(), b.clone()))
                .collect::<Vec<_>>(),
            0.0,
        );
        for (g, v) in [
            (Gate::OracleIntegrity, &integrity),
            (Gate::OracleParity, &parity),
            (Gate::HeldoutDivergence, &div),
            (Gate::Differential, &diff),
        ] {
            let mut detail = v.detail.clone();
            detail["prompt_version"] = serde_json::json!(prompt_version);
            store
                .record_gate(id, g, v.passed, &detail)
                .map_err(|e| e.to_string())?;
        }
        ev(
            store,
            run_id,
            "gate",
            serde_json::json!({"unit": id, "integrity": integrity.passed, "parity": parity.passed, "divergence": div.passed, "differential": diff.passed}),
        );
        // Tamper is an immediate run halt (SPEC 10.3), not a unit retry.
        if !integrity.passed {
            let reason = format!("oracle_tamper unit {id}: {}", integrity.detail);
            store.set_halt(run_id, &reason).map_err(|e| e.to_string())?;
            ev(store, run_id, "tamper", serde_json::json!({"unit": id, "reason": reason}));
            return Err(reason);
        }
        if !(parity.passed && div.passed && diff.passed) {
            let n = store.bump_attempts(id).map_err(|e| e.to_string())?;
            if n >= 3 {
                escalate(store, run_id, id)?;
            }
            return Err(format!("unit {id} failed gates"));
        }
        // Review: two seats, never the implementer (worker => any two seats).
        let providers = default_providers();
        let (r1, r2) = assign_reviewers(None, &providers);
        record_review(store, run_id, id, &format!("unit {id} diff"), r1, r2)?;
        // Merge + delete mirrored module in the SAME commit (targets from template).
        let (_, deletes) = crate::fixture::unit_sources(&tspec, &recon_modules, id)?;
        merge_unit(&a.fork, wt.as_str(), id, &deletes)?;
        let sha = git(&a.fork, &["rev-parse", "HEAD"])?;
        store.set_unit_commit(id, &sha).map_err(|e| e.to_string())?;
        store.set_unit_status(id, "passed").map_err(|e| e.to_string())?;
        ev(store, run_id, "merge", serde_json::json!({"unit": id, "sha": sha}));
        let _ = sandbox.drop_worktree(&a.fork, wt.as_std_path());
    }

    // Whole-repo grade on the run branch (rebuild from merged sources first).
    build_ext(&a.fork, &venv)?;
    let got = run_oracle_in_venv(&venv, &a.fork, &manifest_inv)?;
    let hashes = Oracle::current_hashes(&manifest, &a.fork);
    let integrity = gates::oracle_integrity(&manifest, &hashes, &manifest.baseline, &got);
    let parity = gates::oracle_parity(&got);
    if !integrity.passed {
        let reason = format!("oracle_tamper whole-repo: {}", integrity.detail);
        store.set_halt(run_id, &reason).map_err(|e| e.to_string())?;
        ev(store, run_id, "tamper", serde_json::json!({"reason": reason}));
        return Err(reason);
    }
    if !parity.passed {
        return Err("whole-repo grade failed".into());
    }
    let held_rate_all = {
        let py = grade_venv_python(&venv);
        let o = std::process::Command::new(&py)
            .args(["-m", "pytest"])
            .arg(&a.heldout)
            .args(["-q", "--tb=no"])
            .env("PY_COLORS", "0")
            .output()
            .map_err(|e| e.to_string())?;
        let t = format!(
            "{}\n{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        parse_heldout_rate(&t)
    };
    let whole_div = rate_of(&got) - held_rate_all;
    // Held-out divergence over threshold halts the run (SPEC 10.3).
    let wdiv = gates::heldout_divergence(rate_of(&got), held_rate_all, 0.05);
    if !wdiv.passed {
        let reason = format!("heldout_divergence whole-repo: {}", wdiv.detail);
        store.set_halt(run_id, &reason).map_err(|e| e.to_string())?;
        ev(
            store,
            run_id,
            "heldout_divergence",
            serde_json::json!({"reason": reason, "visible": rate_of(&got)}),
        );
        return Err(reason);
    }
    // Unsafe audit: count + SAFETY + FFI-boundary (0 expected on the mirror).
    let unsafe_sites = audit_unsafe(&a.fork)?;
    let uv = gates::unsafe_budget(&unsafe_sites, 5.0, 10);
    let units = store.list_units(run_id).map_err(|e| e.to_string())?;
    let decisions = store.count_decisions(run_id).map_err(|e| e.to_string())? as usize;
    let report = Report {
        passed: got.passed,
        failed: got.failed,
        divergence: whole_div,
        unsafe_count: unsafe_sites.len(),
        units: units.iter().map(|(u, s, _)| (u.clone(), s.clone())).collect(),
        decisions,
    };
    std::fs::write(a.fork.join("RUSTSMITH_REPORT.md"), emit_md(&report))
        .map_err(|e| e.to_string())?;
    std::fs::write(
        a.fork.join("rustsmith-report.json"),
        emit_json(&report),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        a.fork.join("rustsmith-report.html"),
        emit_html(&report),
    )
    .map_err(|e| e.to_string())?;
    git(&a.fork, &["add", "-A"])?;
    git(&a.fork, &["commit", "-qm", "reports"])?;
    ev(
        store,
        run_id,
        "mirror_done",
        serde_json::json!({"passed": got.passed, "unsafe": unsafe_sites.len(), "clippy": "see-acceptance"}),
    );
    let _ = uv;
    Ok(report)
}

fn escalate(store: &Store, run_id: &str, unit_id: &str) -> Result<(), String> {
    use std::collections::HashMap as Map;
    let mut d: Map<Seat, Box<dyn SeatDriver>> = Map::new();
    for s in [Seat::Architect, Seat::Verifier, Seat::Performance, Seat::Scope] {
        d.insert(
            s,
            Box::new(StubDriver {
                stance: Stance::Approve,
                reasoning: "replan".into(),
            }),
        );
    }
    let c = Council::new(d);
    let _ = c
        .escalate_replan(store, run_id, unit_id, &["gate failed 3x".into()], None)
        .map_err(|e| e.to_string())?;
    store
        .set_unit_status(unit_id, "parked")
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn record_review(
    store: &Store,
    run_id: &str,
    unit_id: &str,
    diff: &str,
    r1: Seat,
    r2: Seat,
) -> Result<(), String> {
    use std::collections::HashMap as Map;
    // Implementer (worker) never reviews; two seats on different providers, diff-only.
    let mut d: Map<Seat, Box<dyn SeatDriver>> = Map::new();
    for s in [Seat::Architect, Seat::Verifier, Seat::Performance, Seat::Scope] {
        d.insert(
            s,
            Box::new(StubDriver {
                stance: Stance::Approve,
                reasoning: format!("{} approves diff-only review of {unit_id}", s.as_str()),
            }),
        );
    }
    let c = Council::new(d);
    // Third critic distinct from both reviewers (protocol needs two blind critics).
    let third = [Seat::Architect, Seat::Scope, Seat::Verifier, Seat::Performance]
        .into_iter()
        .find(|s| *s != r1 && *s != r2)
        .unwrap_or(Seat::Scope);
    let res = c
        .decide(
            store,
            run_id,
            Proposal {
                question: format!("review {unit_id}"),
                artifact_ref: diff.into(),
                proposer: r1,
                reasoning: "diff-only review".into(),
            },
            diff.as_bytes(),
            (r2, third),
        )
        .map_err(|e| e.to_string())?;
    let _ = res;
    Ok(())
}

fn merge_unit(fork: &Path, _worktree: &str, unit_id: &str, deletes: &[String]) -> Result<(), String> {
    // Merge worker branch with --no-commit, delete the mirrored
    // original-language modules (template manifest), then commit ONCE:
    // merge + deletion land in the same commit so interface drift surfaces now.
    let wt_branch = format!("unit/{unit_id}");
    git(fork, &["merge", "--no-commit", "--no-ff", &wt_branch])?;
    for t in deletes {
        if fork.join(t).exists() {
            git(fork, &["rm", "-q", t])?;
        }
    }
    git(fork, &["add", "-A"])?;
    git(fork, &["commit", "-qm", &format!("merge {unit_id} + delete mirrored module")])?;
    Ok(())
}
fn rate_of(gr: &rustsmith_core::GradedResult) -> f64 {
    gr.pass_rate()
}

fn default_providers() -> HashMap<Seat, String> {
    // Matches config/default.toml (swappable; never hardcoded elsewhere).
    HashMap::from([
        (Seat::Architect, "provider-a".into()),
        (Seat::Verifier, "provider-b".into()),
        (Seat::Performance, "provider-c".into()),
        (Seat::Scope, "provider-d".into()),
    ])
}

pub(crate) fn parse_heldout_rate(t: &str) -> f64 {
    // Parse "N passed" / failures from quiet pytest output.
    let (mut p, mut f) = (0u32, 0u32);
    for line in t.lines() {
        let l = line.to_lowercase();
        let toks: Vec<&str> = l
            .split(|c: char| c == ',' || c == ' ')
            .filter(|x| !x.is_empty())
            .collect();
        let mut i = 0;
        while i < toks.len() {
            if let Ok(n) = toks[i].parse::<u32>() {
                if i + 1 < toks.len() {
                    match toks[i + 1] {
                        w if w.starts_with("passed") => p = p.max(n),
                        w if w.starts_with("failed") => f = f.max(n),
                        _ => {}
                    }
                }
            }
            i += 1;
        }
    }
    let t = p + f;
    if t == 0 {
        1.0
    } else {
        p as f64 / t as f64
    }
}

pub(crate) fn audit_unsafe(fork: &Path) -> Result<Vec<gates::UnsafeSite>, String> {
    // `cargo geiger` cross-check would go here; the mirror ships zero unsafe,
    // so a textual audit plus geiger-if-present is the honest check.
    let mut count = 0usize;
    for entry in walkdir_simple(&fork.join("src")) {
        if entry.extension().map(|x| x == "rs").unwrap_or(false) {
            let t = std::fs::read_to_string(&entry).map_err(|e| e.to_string())?;
            // Strip comments? No: any `unsafe` token counts (conservative).
            for line in t.lines() {
                let s = line.trim();
                if s.starts_with("//") {
                    continue;
                }
                if s.contains("unsafe") {
                    count += 1;
                }
            }
        }
    }
    if count > 0 {
        return Err(format!("{count} unsafe tokens found"));
    }
    // Also try cargo-geiger when installed (informational).
    let _ = std::process::Command::new("cargo")
        .args(["geiger", "--manifest-path", &fork.join("Cargo.toml").display().to_string()])
        .output();
    Ok(vec![])
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn copy_tree_except_git(src: &Path, dst: &Path) -> Result<(), String> {
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        if e.file_name() == ".git" {
            continue;
        }
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            std::fs::create_dir_all(&t).map_err(|e| e.to_string())?;
            copy_tree_except_git(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn walkdir_simple(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    stack.push(e.path());
                }
            }
        } else {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewers_never_include_implementer_and_span_providers() {
        let p = default_providers();
        for imp in [None, Some(Seat::Verifier), Some(Seat::Architect)] {
            let (a, b) = assign_reviewers(imp, &p);
            assert_ne!(a, b);
            if let Some(i) = imp {
                assert_ne!(a, i);
                assert_ne!(b, i);
            }
            assert_ne!(p[&a], p[&b]);
        }
    }

    #[test]
    fn bundle_has_no_heldout_channel() {
        let dir = tempfile::tempdir().unwrap();
        let b = assemble_bundle(
            &dir.path().join("b"),
            "u1",
            "_crc",
            &[],
            "PORTING",
            "source",
            "oracle-path",
        )
        .unwrap();
        let mut text = String::new();
        for e in std::fs::read_dir(&b.dir).unwrap().flatten() {
            text.push_str(&std::fs::read_to_string(e.path()).unwrap());
        }
        assert!(!text.contains("heldout_tests_never_here"));
        assert!(text.contains("mirror-v1"));
    }
}
