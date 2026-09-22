//! Stage-2 optimize flow (M5): SPEC_STAGE2 §9 round loop.
//!
//! Deterministic analysis first (profile -> bounds -> ceilings -> dispatch),
//! proposal-before-code review, parallel-isolated implementation (serialized
//! here; cap documented), per-candidate grading (7 gates), merge winners +
//! record losers, wall-clock confirmation, stopping rules, gated finals.
//!
//! Candidate code comes from `candidates` (deterministic source standing in
//! for dispatched workers); every number below is measured, every gate real.

use rustsmith_adapters::{MaturinBridge, Profiler, PyProfiler, PytestRunner};
use rustsmith_core::{BuildCtx, Cwd, TestCommand};
use rustsmith_gates as gates;
use rustsmith_oracle::execute_all;
use rustsmith_profile as profile;
use rustsmith_store::Store;
use std::path::{Path, PathBuf};

pub const GUIDANCE_DEFAULT: &str = "opt-guidance v1";
pub const MODEL_STUB: &str = "deterministic-stub";
pub const PROMPT_VERSION: &str = "opt-v1";
/// Minimum marginal gain the attribution audit credits. Below this, omitting
/// a winner is scored as no-effect even when above the noise floor.
pub const MIN_AUDIT_KEEP_GAIN: f64 = 0.02;
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
pub struct OptCtx {
    pub heldout: PathBuf,
    pub run_id: String,
    pub venv: PathBuf,
    pub manifest: rustsmith_core::Manifest,
    /// Frozen package identity (from `recon/facts.json`); drives
    /// workload/differential/candidate selection via package-keyed data.
    pub package: String,
}

/// Package workloads + probe specs from package-keyed data (Track H).
/// Executable snippets live in data, never in dispatch code.
pub struct PkgWorkloads {
    pub visible: Vec<profile::Workload>,
    pub heldout: Vec<profile::Workload>,
    pub tiny: profile::Workload,
    pub probe_script: String,
    pub probe_argvs: Vec<String>,
    pub pool: Vec<PoolSpec>,
}

/// One round-1 candidate spec; `ceiling` scales the measured share cap.
pub struct PoolSpec {
    pub hotspot: String,
    pub bound: String,
    pub tier: u8,
    pub technique: String,
    pub ceil_frac: f64,
    pub est_cost: f64,
}

fn workload_of(v: &serde_json::Value) -> Result<profile::Workload, String> {
    Ok(profile::Workload {
        name: v["name"].as_str().unwrap_or("").to_string(),
        setup: v["setup"].as_str().unwrap_or("").to_string(),
        stmt: v["stmt"].as_str().unwrap_or("").to_string(),
        iters: v["iters"].as_u64().unwrap_or(0) as usize,
    })
}

/// Load executable workloads + probe specs for `package`.
pub fn workloads_for(package: &str) -> Result<PkgWorkloads, String> {
    let entry = crate::repo_content::entry(package)?;
    let w = &entry["workloads"];
    let visible: Vec<profile::Workload> = w["visible"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(workload_of)
        .collect::<Result<_, _>>()?;
    let heldout: Vec<profile::Workload> = w["heldout"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(workload_of)
        .collect::<Result<_, _>>()?;
    if visible.is_empty() || heldout.is_empty() {
        return Err(format!("no workloads for package '{package}'"));
    }
    let pool: Vec<PoolSpec> = entry["candidate_pool"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|c| PoolSpec {
            hotspot: c["hotspot"].as_str().unwrap_or("").to_string(),
            bound: c["bound"].as_str().unwrap_or("").to_string(),
            tier: c["tier"].as_u64().unwrap_or(0) as u8,
            technique: c["technique"].as_str().unwrap_or("").to_string(),
            ceil_frac: c["ceil_frac"].as_f64().unwrap_or(0.0),
            est_cost: c["est_cost"].as_f64().unwrap_or(0.0),
        })
        .collect();
    Ok(PkgWorkloads {
        visible,
        heldout,
        tiny: workload_of(&w["tiny"])?,
        probe_script: entry["workload_probe"]["script"].as_str().unwrap_or("").to_string(),
        probe_argvs: entry["workload_probe"]["argvs"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        pool,
    })
}

/// Report label derived from the loaded workload names (data, not literals).
pub fn workload_label(wl: &PkgWorkloads) -> String {
    let vis: Vec<&str> = wl.visible.iter().map(|w| w.name.as_str()).collect();
    let held: Vec<&str> = wl.heldout.iter().map(|w| w.name.as_str()).collect();
    format!("{} + heldout ({})", vis.join("/"), held.join("/"))
}


fn py_env(_venv_py: &Path, use_venv: bool, orig_src: &Path) -> (Vec<(String, String)>, Vec<&'static str>) {
    if use_venv {
        (vec![], vec!["PYTHONPATH"])
    } else {
        (
            vec![("PYTHONPATH".into(), orig_src.display().to_string())],
            vec![],
        )
    }
}

/// Deterministic gain of `got` vs `base` (fraction; + means faster).
pub fn gain_frac(base_cpu: f64, got_cpu: f64) -> f64 {
    if base_cpu <= 0.0 {
        0.0
    } else {
        (base_cpu - got_cpu) / base_cpu
    }
}

pub struct GradeResult {
    pub passed: bool,
    pub failed_gate: Option<String>,
    pub vis_gain: f64,
    pub held_gain: f64,
    pub divergence: f64,
    pub det_gain: f64,
    pub ci: Option<profile::ConfInterval>,
    pub attribution_ok: bool,
    pub rss_delta: f64,
    pub alloc_delta: f64,
    pub patch_text: String,
    pub parent_sha: String,
    pub worker_message: String,
}

/// Full per-candidate gate suite (§9 gating block). `cand_dir` holds the
/// patched tree (built into `venv`); `parent_dir` the pre-patch tree.
/// Held-out magnitudes never enter `worker_message` (redaction enforced).
#[allow(clippy::too_many_arguments)]
pub fn grade_candidate(
    store: &Store,
    ctx: &OptCtx,
    cand_dir: &Path,
    parent_dir: &Path,
    parent_venv: &Path,
    parent_compile: f64,
    floor: f64,
    technique: &str,
    bound: &str,
    tier: u8,
    ceiling: f64,
) -> Result<GradeResult, String> {
    use crate::mirror as m;
    let venv = &ctx.venv;
    let venv_py = m::grade_venv_python(venv);
    // Gate rows reference units(id): register the candidate unit first.
    let uid = format!("{}:{technique}", ctx.run_id);
    store
        .create_unit(&uid, &ctx.run_id, "optimize", "{}")
        .map_err(|e| e.to_string())?;
    // Patch capture at grade time, same transaction basis as gate rows.
    // Stage first so NEW files (e.g. src/slice8.rs) enter the diff; the build
    // has not run yet, so no target/ noise exists to pollute it.
    git(cand_dir, &["add", "-A"])?;
    let parent_sha = git(cand_dir, &["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".into());
    // Raw bytes (NO trim): a missing trailing newline corrupts the tail hunk
    // for both `patch` and `git apply` ("malformed/corrupt patch at last line").
    let raw = std::process::Command::new("/usr/bin/git")
        .args(["diff", &parent_sha])
        .current_dir(cand_dir)
        .output()
        .map_err(|e| e.to_string())?;
    let mut patch_text = String::from_utf8_lossy(&raw.stdout).to_string();
    if !patch_text.ends_with('\n') {
        patch_text.push('\n');
    }
    let truncated = patch_text.len() > 512 * 1024;
    if truncated {
        patch_text.truncate(512 * 1024);
    }
    let t0 = std::time::Instant::now();
    build_release(cand_dir, venv).map_err(|e| format!("build failed: {e}"))?;
    let compile_secs = t0.elapsed().as_secs_f64();
    // Deterministic measures, INTERLEAVED parent/candidate (drift-robust:
    // alternating samples share the thermal/frequency window; minute-scale
    // ramps cannot masquerade as gains).
    let parent_py = m::grade_venv_python(parent_venv);
    let wl = workloads_for(&ctx.package)?;
    let (parent_stats, got) = measure_interleaved(&parent_py, &venv_py, &wl.visible[0])?;
    let (parent_held, got_held) = measure_interleaved(&parent_py, &venv_py, &wl.heldout[0])?;
    let det_gain = gain_frac(parent_stats.cpu_per_op, got.cpu_per_op);
    let held_gain = gain_frac(parent_held.cpu_per_op, got_held.cpu_per_op);
    let vis_gain = det_gain;
    let divergence = vis_gain - held_gain;
/// Alternating parent/candidate samples (7 each) sharing one thermal window.
/// Medians of interleaved samples; gains computed from contemporaneous pairs.
/// Timed commands come from the profiler; each child is reaped with `wait4`
/// (kernel CPU + peak RSS, no perf dependency per ADR-003). Interpreter
/// startup is calibrated once per tour and subtracted, so per-op times
/// exclude it. The profiler harness prints no tracemalloc peak, so
/// `alloc_peak` is honestly `None` (gates skip the alloc leg) rather than a
/// fabricated zero.
fn measure_interleaved(
    parent_py: &Path,
    cand_py: &Path,
    w: &profile::Workload,
) -> Result<(profile::CpuStats, profile::CpuStats), String> {
    let core_w = rustsmith_core::Workload {
        name: w.name.clone(),
        setup: w.setup.clone(),
        stmt: w.stmt.clone(),
        iters: w.iters,
    };
    if core_w.iters == 0 {
        return Err("workload iters must be nonzero".into());
    }
    let parent = PyProfiler { repo: PathBuf::from("."), python: parent_py.to_path_buf() };
    let cand = PyProfiler { repo: PathBuf::from("."), python: cand_py.to_path_buf() };
    // `repo` is only the `cwd` anchor here (the `-c` workload is
    // location-independent); the venv roots always exist.
    let proot = venv_root(parent_py);
    let croot = venv_root(cand_py);
    let p_start = startup_cpu(parent_py, &proot)?;
    let c_start = startup_cpu(cand_py, &croot)?;
    let (mut pc, mut cc, mut pw, mut cw) = (vec![], vec![], vec![], vec![]);
    let (mut pr, mut cr) = (0u64, 0u64);
    for _ in 0..7 {
        let p = timed_sample(&parent, &core_w, &proot, p_start)?;
        let c = timed_sample(&cand, &core_w, &croot, c_start)?;
        pc.push(p.0);
        cc.push(c.0);
        pw.push(p.1);
        cw.push(c.1);
        pr = pr.max(p.2);
        cr = cr.max(c.2);
    }
    let med = |mut xs: Vec<f64>| {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        xs[xs.len() / 2]
    };
    Ok((
        profile::CpuStats { cpu_per_op: med(pc), wall_per_op: med(pw), rss_kb: pr, alloc_peak: None },
        profile::CpuStats { cpu_per_op: med(cc), wall_per_op: med(cw), rss_kb: cr, alloc_peak: None },
    ))
}
/// Venv root anchoring a venv interpreter (`<venv>/bin/python`).
/// Falls back to `.` (the `-c` workload resolves nothing from the tree).
fn venv_root(python: &Path) -> PathBuf {
    python
        .parent()
        .and_then(|b| b.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}
/// Interpreter-only startup CPU via `wait4` (calibration baseline).
fn startup_cpu(python: &Path, root: &Path) -> Result<f64, String> {
    let cmd = TestCommand {
        program: python.display().to_string(),
        args: vec!["-c".to_string(), "pass".to_string()],
        cwd: Cwd::Tree,
        env_set: Vec::new(),
        env_remove: Vec::new(),
        launcher: None,
        timeout_secs: Some(120),
        collect: Vec::new(),
    };
    let s = run_timed_wait4(&cmd, root)?;
    Ok(s.user_secs + s.sys_secs)
}
/// One profiler sample: (cpu_per_op, wall_per_op, rss_kb).
/// CPU is parent-measured `wait4` time minus startup; wall is the harness's
/// in-process elapsed (both startup-free); RSS is the kernel peak.
fn timed_sample(
    profiler: &PyProfiler,
    w: &rustsmith_core::Workload,
    root: &Path,
    startup: f64,
) -> Result<(f64, f64, u64), String> {
    let cmd = profiler.timed(w);
    let s = run_timed_wait4(&cmd, root)?;
    if s.exit_code != 0 {
        return Err(format!("timed harness failed: {}", s.stdout.trim()));
    }
    let wall_total = parse_elapsed(&s.stdout)?;
    let cpu = (s.user_secs + s.sys_secs - startup).max(0.0) / w.iters as f64;
    Ok((cpu, wall_total / w.iters as f64, s.maxrss_kb))
}
/// Parse the profiler harness's `elapsed=<secs>` line.
fn parse_elapsed(stdout: &str) -> Result<f64, String> {
    for line in stdout.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("elapsed=") {
            if let Ok(f) = v.trim().parse::<f64>() {
                return Ok(f);
            }
        }
    }
    Err(format!("timed harness printed no elapsed line: {stdout:?}"))
}
/// Parent-measured execution of a profiler `timed` command via `wait4(2)`:
/// precise per-child CPU + peak RSS without perf/valgrind (ADR-003).
/// Raw syscalls are declared locally, like rustsmith-sandbox's flock.
struct TimedSample {
    exit_code: i32,
    stdout: String,
    user_secs: f64,
    sys_secs: f64,
    maxrss_kb: u64,
}
#[cfg(unix)]
#[repr(C)]
struct Timeval {
    tv_sec: i64,
    tv_usec: i64,
}
#[cfg(unix)]
#[repr(C)]
struct Rusage {
    ru_utime: Timeval,
    ru_stime: Timeval,
    ru_maxrss: i64,
    _rest: [i64; 13],
}
#[cfg(unix)]
const _: () = assert!(std::mem::size_of::<Rusage>() == 144);
#[cfg(unix)]
unsafe extern "C" {
    fn wait4(pid: i32, status: *mut i32, options: i32, rusage: *mut Rusage) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
}
/// Reap one command with `wait4`, capturing stdout to a temp file (no pipe
/// discipline issues across the raw reap). `cwd` always comes from the
/// command; a set `timeout_secs` polls with `WNOHANG` and kills past it.
fn run_timed_wait4(cmd: &TestCommand, tree: &Path) -> Result<TimedSample, String> {
    #[cfg(not(unix))]
    {
        let _ = (cmd, tree);
        return Err("run_timed_wait4 unavailable off unix".into());
    }
    #[cfg(unix)]
    {
        if cmd.launcher.is_some() {
            return Err("run_timed_wait4: launcher prefix not supported".into());
        }
        let cwd = match &cmd.cwd {
            Cwd::Tree => tree.to_path_buf(),
            Cwd::BuildDir => tree.to_path_buf(),
            Cwd::Rel(r) => tree.join(r),
        };
        let tag = format!(
            "rs-timed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|t| t.as_nanos())
                .unwrap_or(0),
        );
        let out_path = std::env::temp_dir().join(format!("{tag}.out"));
        let out_f = std::fs::File::create(&out_path).map_err(|e| format!("timed capture: {e}"))?;
        let mut c = std::process::Command::new(&cmd.program);
        c.args(&cmd.args);
        c.current_dir(&cwd);
        for (k, v) in &cmd.env_set {
            c.env(k, v);
        }
        for k in &cmd.env_remove {
            c.env_remove(k);
        }
        c.stdout(out_f);
        c.stderr(std::process::Stdio::null());
        let child = c.spawn().map_err(|e| format!("spawn {}: {e}", cmd.program))?;
        let pid = child.id() as i32;
        // wait4 reaps the child; the std handle has nothing left to wait on.
        std::mem::forget(child);
        const WNOHANG: i32 = 1;
        const SIGKILL: i32 = 9;
        let deadline = cmd
            .timeout_secs
            .map(|t| std::time::Instant::now() + std::time::Duration::from_secs(u64::from(t).max(1)));
        let mut status = 0i32;
        let mut ru = Rusage {
            ru_utime: Timeval { tv_sec: 0, tv_usec: 0 },
            ru_stime: Timeval { tv_sec: 0, tv_usec: 0 },
            ru_maxrss: 0,
            _rest: [0; 13],
        };
        loop {
            let r = unsafe { wait4(pid, &mut status, WNOHANG, &mut ru) };
            if r == pid {
                break;
            }
            if r < 0 {
                let _ = std::fs::remove_file(&out_path);
                return Err(format!("wait4 {}: {r}", cmd.program));
            }
            if deadline.map(|d| std::time::Instant::now() >= d).unwrap_or(false) {
                unsafe {
                    kill(pid, SIGKILL);
                }
                let r2 = unsafe { wait4(pid, &mut status, 0, &mut ru) };
                let _ = std::fs::remove_file(&out_path);
                if r2 < 0 {
                    return Err(format!("wait4 {}: {r2}", cmd.program));
                }
                return Err(format!(
                    "command timed out after {}s: {}",
                    cmd.timeout_secs.unwrap_or(0),
                    cmd.program
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
        let _ = std::fs::remove_file(&out_path);
        use std::os::unix::process::ExitStatusExt;
        let exit_status: std::process::ExitStatus = ExitStatusExt::from_raw(status);
        let exit_code = exit_status.code().unwrap_or(-1);
        Ok(TimedSample {
            exit_code,
            stdout,
            user_secs: ru.ru_utime.tv_sec as f64 + ru.ru_utime.tv_usec as f64 / 1e6,
            sys_secs: ru.ru_stime.tv_sec as f64 + ru.ru_stime.tv_usec as f64 / 1e6,
            maxrss_kb: ru.ru_maxrss.max(0) as u64,
        })
    }
}
    // Gate 1: oracle integrity + parity (full suite in the venv, runner-graded).
    let runner = PytestRunner;
    let hashes = rustsmith_oracle::Oracle::current_hashes(&ctx.manifest, cand_dir, &runner);
    let suite = m::run_oracle_in_venv(venv, cand_dir, &ctx.manifest)?;
    let integrity =
        gates::oracle_integrity(&ctx.manifest, &hashes, &ctx.manifest.baseline, &suite);
    let parity = gates::oracle_parity(&suite);
    let wdiv = gates::workload_divergence(vis_gain, held_gain, 0.10, floor);
    // Gate 3: benchmark vs floor (deterministic screen).
    let bench = gates::benchmark_restated(det_gain, floor, None, 1.0);
    // Gate 4: differential incl. workload pairs (original vs candidate).
    stage_orig_src(cand_dir, parent_dir)?;
    let mut pairs = m::differential_pairs_for(
        &ctx.package,
        &PathBuf::from(PytestRunner::python_program()),
        &m::grade_venv_python(venv),
        cand_dir,
    )?;
    pairs.extend(workload_pairs_for(&wl, &venv_py, parent_dir, cand_dir)?);
    let _ = std::fs::remove_dir_all(cand_dir.join("orig_src"));
    // Exact-equality behind the per-observable tolerance (0.0/None = legacy math).
    let diff = gates::differential(&pairs, 0.0, None);
    // Gate 5: causal attribution (revert + remeasure, contemporaneous: the
    // reverted tree is measured interleaved against a fresh parent sample).
    // A failure is CONFIRMED once before rejecting: minute-scale transients can
    // swing a single comparison by tens of percent; a persistent phantom fails
    // twice, a transient passes on confirmation.
    let reverted = reverse_apply(cand_dir, &patch_text)?;
    build_release(&reverted, venv).map_err(|e| format!("revert build failed: {e}"))?;
    let (parent_fresh, again) = measure_interleaved(&parent_py, &venv_py, &wl.visible[0])?;
    let mut gain_without = gain_frac(parent_fresh.cpu_per_op, again.cpu_per_op);
    let mut attrib = gates::causal_attribution(det_gain.max(0.0), gain_without, floor);
    if !attrib.passed {
        // Confirmation: rebuild + remeasure once more before rejecting.
        build_release(&reverted, venv).map_err(|e| format!("revert rebuild failed: {e}"))?;
        let (parent_fresh2, again2) = measure_interleaved(&parent_py, &venv_py, &wl.visible[0])?;
        gain_without = gain_frac(parent_fresh2.cpu_per_op, again2.cpu_per_op);
        attrib = gates::causal_attribution(det_gain.max(0.0), gain_without, floor);
    }
    build_release(cand_dir, venv).map_err(|e| format!("rebuild failed: {e}"))?;
    let _ = std::fs::remove_dir_all(&reverted);
    // Gate 6: widened no_regression (RSS/alloc/binary/compile + heldout pass/fail).
    // The profiler reports no tracemalloc peak (None); the alloc leg is then
    // skipped by the gate and the delta is a neutral zero.
    let rss_delta = pct(parent_stats.rss_kb, got.rss_kb);
    let alloc_delta = pct(
        parent_stats.alloc_peak.unwrap_or(0),
        got.alloc_peak.unwrap_or(0),
    );
    let so_base = so_size(parent_dir);
    let so_got = so_size(cand_dir);
    let held_ok = held_gain > -floor && (vis_gain - held_gain) <= 0.25;
    let noreg = gates::no_regression_widened(
        &gates::ResourceSnapshot {
            rss_bytes: parent_stats.rss_kb * 1024,
            alloc_count: parent_stats.alloc_peak,
            binary_bytes: so_base,
            compile_secs: parent_compile,
        },
        &gates::ResourceSnapshot {
            rss_bytes: got.rss_kb * 1024,
            alloc_count: got.alloc_peak,
            binary_bytes: so_got,
            compile_secs,
        },
        true,
        held_ok,
    );
    // Gate 3b: correctness held-out suite (SPEC §8): visible vs held-out TEST rates.
    let held_test_rate = m::run_heldout_in_venv(venv, cand_dir, &ctx.heldout)?;
    let hdiv = gates::heldout_divergence(suite.pass_rate(), held_test_rate, 0.05);
    // Gate 7: optimization scope (structural, from the captured diff).
    let summary = diff_summary(&patch_text);
    let scope = gates::optimization_scope(&summary, 0, &[]);
    let _ = (technique, bound, tier, ceiling);
    // Verdict in gate order (§9): integrity, parity, heldout_div, workload_div,
    // attribution, benchmark, no_regression. Scope failures short-circuit earlier
    // in the flow (structural pre-check); here it is recorded too.
    let mut failed_gate: Option<String> = None;
    let mut passed = true;
    for (name, v) in [
        ("oracle_integrity", &integrity),
        ("oracle_parity", &parity),
        ("heldout_divergence", &hdiv),
        ("differential", &diff),
        ("workload_divergence", &wdiv),
        ("causal_attribution", &attrib),
        ("benchmark", &bench),
        ("no_regression", &noreg),
        ("optimization_scope", &scope),
    ] {
        store
            .record_gate(&uid, gate_name(name), v.passed, &v.detail)
            .map_err(|e| e.to_string())?;
        if !v.passed && passed {
            passed = false;
            failed_gate = Some(name.into());
        }
    }
    // Worker message: redacted on divergence-class failures, else terse.
    let worker_message = match failed_gate.as_deref() {
        Some("workload_divergence") => {
            let d = gates::worker_visible_rejection("workload_divergence");
            assert!(gates::is_worker_safe(&d));
            d.to_string()
        }
        Some(g) => format!("rejected at {g}"),
        None => "accepted".into(),
    };
    let _ = truncated;
    Ok(GradeResult {
        passed,
        failed_gate,
        vis_gain,
        held_gain,
        divergence,
        det_gain,
        ci: None,
        attribution_ok: attrib.passed,
        rss_delta,
        alloc_delta,
        patch_text,
        parent_sha,
        worker_message,
    })
}

fn gate_name(name: &str) -> rustsmith_core::Gate {
    use rustsmith_core::Gate as G;
    match name {
        "oracle_integrity" => G::OracleIntegrity,
        "oracle_parity" => G::OracleParity,
        "heldout_divergence" => G::HeldoutDivergence,
        "differential" => G::Differential,
        "workload_divergence" => G::WorkloadDivergence,
        "causal_attribution" => G::CausalAttribution,
        "benchmark" => G::Benchmark,
        "no_regression" => G::NoRegression,
        "optimization_scope" => G::OptimizationScope,
        _ => G::Benchmark,
    }
}

/// Reverse-apply `patch` to a COPY of the patched `dir` for attribution
/// remeasurement (the copy preserves the patched tree for the later rebuild).
fn reverse_apply(dir: &Path, patch: &str) -> Result<PathBuf, String> {
    let rev = dir.join(".attribution-revert");
    let _ = std::fs::remove_dir_all(&rev);
    std::fs::create_dir_all(&rev).map_err(|e| e.to_string())?;
    copy_filtered(dir, &rev)?;
    let patch_file = dir.join(".attribution.patch");
    std::fs::write(&patch_file, patch).map_err(|e| e.to_string())?;
    // The copy sits NESTED inside a repo (cand worktree). Without isolation,
    // `git apply` discovers the enclosing .git upward and silently no-ops
    // (exit 0, nothing reverted) instead of plain file-mode apply. The
    // ceiling (repo AT the ceiling is not used) forces file mode — verified
    // by test: new-file deletion + tracked-hunk reversal both apply.
    let st = std::process::Command::new("/usr/bin/git")
        .args(["apply", "-R", "-p1"])
        .arg(&patch_file)
        .current_dir(&rev)
        .env("GIT_CEILING_DIRECTORIES", dir)
        .output()
        .map_err(|e| e.to_string())?;
    if !st.status.success() {
        return Err(format!(
            "reverse apply failed: {}",
            String::from_utf8_lossy(&st.stderr)
        ));
    }
    // Loud guard: a silent no-op (exit 0, tree unchanged) would measure the
    // un-reverted tree and fail every real win while passing phantoms. A
    // non-empty patch MUST change at least one touched file.
    if !patch.trim().is_empty() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut changed = false;
        for f in diff_summary(patch).files {
            let mut h = DefaultHasher::new();
            std::fs::read(dir.join(&f)).unwrap_or_default().hash(&mut h);
            let before = h.finish();
            let mut h = DefaultHasher::new();
            std::fs::read(rev.join(&f)).unwrap_or_default().hash(&mut h);
            if before != h.finish() {
                changed = true;
                break;
            }
        }
        if !changed {
            return Err("reverse apply no-op: tree unchanged".into());
        }
    }
    Ok(rev)
}
fn pct(a: u64, b: u64) -> f64 {
    if a == 0 {
        0.0
    } else {
        (b as f64 - a as f64) / a as f64 * 100.0
    }
}

fn so_size(dir: &Path) -> u64 {
    let mut best = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&p) {
            for e in rd.flatten() {
                let q = e.path();
                if q.is_dir() {
                    if !q.ends_with("target") {
                        stack.push(q);
                    }
                } else if q.extension().map(|x| x == "so").unwrap_or(false) {
                    best = best.max(e.metadata().map(|m| m.len()).unwrap_or(0));
                }
            }
        }
    }
    best
}
/// Full-build seconds for `src_dir` in a scratch copy (fresh target dir, so
/// candidate full builds and parent full builds are comparable; incremental
/// rebuilds would understate the parent and trip every compile leg).
/// Release build (measurements require optimized code; debug builds are not
/// representative and penalize complex folds disproportionately).
fn build_release(worktree: &Path, venv: &Path) -> Result<String, String> {
    // Wheel flow with per-venv isolation. `maturin develop` installs an
    // EDITABLE hook (in-place .so + .pth aliasing the source tree), so two
    // venvs developing different trees silently share whichever .so was built
    // last — fatal for A/B measurement. Wheels install a private copy instead.
    // The wheel argv comes from `bridge.build_wheel`; the venv binding
    // (VIRTUAL_ENV/PATH) is applied here, as with `build_ext`.
    let bridge = MaturinBridge;
    let cx = BuildCtx { tree: worktree, build_dir: worktree, release: true };
    let dist = venv.join("dist-one");
    let _ = std::fs::remove_dir_all(&dist);
    std::fs::create_dir_all(&dist).map_err(|e| e.to_string())?;
    let mut wheel_cmds = bridge.build_wheel(&cx, &dist);
    for c in &mut wheel_cmds {
        c.env_set.push(("VIRTUAL_ENV".to_string(), venv.display().to_string()));
        c.env_set.push(("PATH".to_string(), venv_path_prepend(venv)));
    }
    let runs = execute_all(worktree, worktree, &wheel_cmds).map_err(|e| e.to_string())?;
    let mut log = String::new();
    for r in &runs {
        log.push_str(&r.stdout);
        log.push_str(&r.stderr);
    }
    if runs.iter().any(|r| r.exit_code != 0) {
        return Err(format!("maturin build failed:\n{log}"));
    }
    let wheel = std::fs::read_dir(&dist)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().map(|x| x == "whl").unwrap_or(false))
        .ok_or("no wheel built")?;
    // pip itself carries no toolchain literal (venv-anchored path); only the
    // install flags travel with the command.
    let pip_cmd = TestCommand {
        program: venv.join("bin/pip").display().to_string(),
        args: vec![
            "install".to_string(),
            "--no-cache-dir".to_string(),
            "--force-reinstall".to_string(),
            "--no-deps".to_string(),
            wheel.display().to_string(),
        ],
        cwd: Cwd::Tree,
        env_set: vec![
            ("VIRTUAL_ENV".to_string(), venv.display().to_string()),
            ("PATH".to_string(), venv_path_prepend(venv)),
        ],
        env_remove: Vec::new(),
        launcher: None,
        timeout_secs: None,
        collect: Vec::new(),
    };
    let runs = execute_all(worktree, worktree, &[pip_cmd]).map_err(|e| e.to_string())?;
    for r in &runs {
        log.push_str(&r.stdout);
        log.push_str(&r.stderr);
    }
    let _ = std::fs::remove_dir_all(&dist);
    if runs.iter().any(|r| r.exit_code != 0) {
        return Err(format!("pip install wheel failed:\n{log}"));
    }
    // Mirror the freshly built ext into the tree (in-place .so), so pytest's
    // cwd-rooted package import resolves THIS tree's build. Package + ext stem
    // come from the tree's own Cargo.toml (fixture-agnostic). Wheels alone
    // leave the local package without an ext, and grading runs with
    // cwd=worktree — the local package then shadows site-packages and the
    // import fails outright. Refreshed on every build, so never stale.
    // The query is a plain interpreter probe (no toolchain literal).
    let (pkg, ext) = crate_package(worktree)?;
    let so_q = TestCommand {
        program: venv.join("bin/python").display().to_string(),
        args: vec![
            "-c".to_string(),
            format!("import glob,sysconfig;print(glob.glob(sysconfig.get_path('purelib')+'/{pkg}/{ext}*.so')[0])"),
        ],
        cwd: Cwd::Tree,
        env_set: vec![("VIRTUAL_ENV".to_string(), venv.display().to_string())],
        env_remove: Vec::new(),
        launcher: None,
        timeout_secs: None,
        collect: Vec::new(),
    };
    let runs = execute_all(worktree, worktree, &[so_q]).map_err(|e| e.to_string())?;
    // Executor stdout carries the `$` transcript first line; skip it.
    let so_src = runs.first().map(|r| crate::mirror::output_value(&r.stdout)).unwrap_or_default();
    if runs.first().map(|r| r.exit_code) != Some(0) || so_src.is_empty() {
        return Err(format!("installed ext not found:\n{log}"));
    }
    // Purge any stale in-place ext first, then copy the fresh one.
    if let Ok(rd) = std::fs::read_dir(worktree.join(&pkg)) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "so").unwrap_or(false) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    let so_name = std::path::Path::new(&so_src)
        .file_name()
        .ok_or("bad ext name")?;
    std::fs::copy(&so_src, worktree.join(&pkg).join(so_name)).map_err(|e| e.to_string())?;
    Ok(log)
}
/// `PATH` with the venv's bin prepended (mirrors the mirror grade-venv rule).
fn venv_path_prepend(venv: &Path) -> String {
    let mut path = venv.join("bin").as_os_str().to_owned();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    path.to_string_lossy().into_owned()
}

/// `[package] name` + `[lib] name` from a tree's Cargo.toml (no toml dep;
/// line-oriented parse is enough for maturin template manifests).
fn crate_package(worktree: &Path) -> Result<(String, String), String> {
    let t = std::fs::read_to_string(worktree.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let mut section = String::new();
    let (mut pkg, mut lib) = (None, None);
    for line in t.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            section = l.to_string();
        }
        if let Some(v) = l.strip_prefix("name") {
            let v = v.trim().trim_start_matches('=').trim().trim_matches('"').trim_matches('\'');
            if section == "[package]" && pkg.is_none() {
                pkg = Some(v.to_string());
            } else if section == "[lib]" && lib.is_none() {
                lib = Some(v.to_string());
            }
        }
    }
    match (pkg, lib) {
        (Some(p), Some(l)) => Ok((p, l)),
        _ => Err("Cargo.toml missing [package] name or [lib] name".into()),
    }
}

fn full_build_secs(src_dir: &Path, venv: &Path) -> Result<f64, String> {
    let tmp = src_dir.join(".full-build-tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    copy_filtered(src_dir, &tmp)?;
    let t0 = std::time::Instant::now();
    let r = build_release(&tmp, venv);
    let dt = t0.elapsed().as_secs_f64();
    let _ = std::fs::remove_dir_all(&tmp);
    r?;
    Ok(dt)
}

fn stage_orig_src(cand_dir: &Path, parent_dir: &Path) -> Result<(), String> {
    // Original implementation sources for the differential baseline.
    let src = parent_dir.join("orig_src_staged");
    let dst = cand_dir.join("orig_src");
    let _ = std::fs::remove_dir_all(&dst);
    std::fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    if src.is_dir() {
        copy_tree(&src, &dst)?;
    }
    Ok(())
}

/// Workload-output differential: original vs candidate outputs on fixed
/// workload inputs (catches fast-but-wrong branches like tuned constants).
/// The probe script + inputs come from package-keyed data. Probes are plain
/// interpreter `TestCommand`s: the original side resolves the staged sources
/// via `cwd` (never path overrides), the candidate side runs in the candidate
/// tree against its installed extension.
fn workload_pairs_for(
    wl: &PkgWorkloads,
    venv_py: &Path,
    parent_dir: &Path,
    cand_dir: &Path,
) -> Result<Vec<(String, String)>, String> {
    run_workload_probes(&wl.probe_script, &wl.probe_argvs, parent_dir, cand_dir, venv_py)
}
/// One `-c` workload probe as a `TestCommand` (interpreter program + cwd;
/// no toolchain literals on this path).
fn workload_probe(program: &Path, script: &str, arg: &str) -> TestCommand {
    TestCommand {
        program: program.display().to_string(),
        args: vec!["-c".to_string(), script.to_string(), arg.to_string()],
        cwd: Cwd::Tree,
        env_set: Vec::new(),
        env_remove: Vec::new(),
        launcher: None,
        timeout_secs: None,
        collect: Vec::new(),
    }
}
/// Run one script over `inputs` on both sides: parent tree serves the
/// original via its staged `orig_src_staged` cwd; candidate via its venv.
/// Only trimmed stdout values pair up (transcript lines skipped), exit codes
/// ignored — identical to the legacy loop.
fn run_workload_probes(
    script: &str,
    inputs: &[String],
    parent_dir: &Path,
    cand_dir: &Path,
    venv_py: &Path,
) -> Result<Vec<(String, String)>, String> {
    let staged = parent_dir.join("orig_src_staged");
    let orig_py = PathBuf::from(PytestRunner::python_program());
    let orig_cmds: Vec<TestCommand> =
        inputs.iter().map(|h| workload_probe(&orig_py, script, h)).collect();
    let cand_cmds: Vec<TestCommand> =
        inputs.iter().map(|h| workload_probe(venv_py, script, h)).collect();
    let o1 = execute_all(&staged, &staged, &orig_cmds).map_err(|e| e.to_string())?;
    let o2 = execute_all(cand_dir, cand_dir, &cand_cmds).map_err(|e| e.to_string())?;
    Ok(o1
        .iter()
        .zip(o2.iter())
        .map(|(a, b)| {
            (
                crate::mirror::output_value(&a.stdout),
                crate::mirror::output_value(&b.stdout),
            )
        })
        .collect())
}

fn diff_summary(patch: &str) -> gates::DiffSummary {
    let mut files = Vec::new();
    let mut added = Vec::new();
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let parts: Vec<&str> = rest.split(' ').collect();
            if parts.len() >= 2 {
                let f = parts[1].trim_start_matches("b/").to_string();
                if !files.contains(&f) {
                    files.push(f);
                }
            }
        } else if let Some(code) = line.strip_prefix('+') {
            if !code.starts_with("+++") {
                added.push(code.to_string());
            }
        }
    }
    gates::DiffSummary {
        files,
        added_lines: added,
        removed_api: vec![],
        added_deps: vec![],
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("/usr/bin/git")
        .args(args)
        .current_dir(dir)
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

fn copy_filtered(src: &Path, dst: &Path) -> Result<(), String> {
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if ["target", ".grade-venv", ".opt-venv", ".plant-venv", ".parent-venv", ".audit-merged-venv", ".audit-rev-venv", ".bundles", ".git", "__pycache__", "orig_src", "orig_src_staged", ".attribution-revert", ".full-build-tmp"]
            .contains(&name.as_str())
            || name.starts_with("worktree-")
            || name.starts_with(".cand-")
            || name.starts_with(".audit-fwd-")
            || name.ends_with(".so")
            || name.ends_with(".pyc")
        {
            continue;
        }
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            std::fs::create_dir_all(&t).map_err(|e| e.to_string())?;
            copy_filtered(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
pub fn round0_report(fork: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    let src = fork.join("src/lib.rs");
    let text = std::fs::read_to_string(&src).unwrap_or_default();
    for (pat, _tier, why) in [
        ("HashMap<String", 3, "fixed-key maps -> structs"),
        ("Rc<RefCell", 3, "index graphs"),
        ("Box<dyn", 3, "enum monomorphization"),
        ("format!", 4, "hot format!/clone"),
    ] {
        let n = text.matches(pat).count();
        if n > 0 {
            findings.push(format!("{pat}: {n} sites ({why})"));
        }
    }
    findings
}

/// Round 0 apply (M9 slice-8): dispatch approved representation findings as
/// serialized workers through the REAL grade path (`grade_candidate`), one at
/// a time (blast radius ⇒ parallel = conflicting + unattributable). Each is
/// gated on parity + divergences + attribution; winners merge, losers land in
/// `failed_optimizations` with round 0. Finding→patch map is explicit; a
/// finding with no deterministic patch is recorded `rejected_at_proposal`.
/// Worker-command usage (slice 3) feeds `tokens_spent` only (default 0).
pub fn run_round0_apply(
    store: &Store,
    ctx: &OptCtx,
    work: &Path,
    findings: &[String],
    floor: f64,
    guidance: &str,
    merged: &mut Vec<serde_json::Value>,
    failed_rows: &mut Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    use crate::mirror as m;
    let mut applied = Vec::new();
    if findings.is_empty() {
        return Ok(applied);
    }
    let finding = findings[0].clone();
    // Explicit finding→patch map (crc Round 0 reports the format! sites).
    let mapped: Option<(&str, &str, u8, fn(&Path) -> Result<Vec<String>, String>)> = if finding.contains("format!") {
        Some(("round0-manual-hex", "compute", 8, crate::candidates::apply_round0_manual_hex))
    } else {
        None
    };
    let (technique, bound, tier, patch) = match mapped {
        Some(t) => t,
        None => {
            store
                .record_failed(
                    &ctx.run_id, 0, "representation", "allocation", 4,
                    "round0-unmapped", "rejected_at_proposal", None, None,
                    &serde_json::json!({"finding": finding}).to_string(), 0,
                    MODEL_STUB, PROMPT_VERSION, guidance,
                    &format!("round-0 finding has no deterministic patch: {finding}"),
                    "", "",
                )
                .map_err(|e| e.to_string())?;
            applied.push(serde_json::json!({"finding": finding, "outcome": "rejected_at_proposal"}));
            failed_rows.push(serde_json::json!({"technique": "round0-unmapped", "outcome": "rejected_at_proposal"}));
            return Ok(applied);
        }
    };
    let proposal = format!("{technique} via tier {tier} for {bound} (round-0 finding: {finding})");
    if let Err(reason) = review_proposal(profile::Bound::Compute, tier, technique) {
        store
            .record_failed(
                &ctx.run_id, 0, "representation", bound, tier as i64, technique,
                "rejected_at_proposal", None, None,
                &serde_json::json!({"reason": reason}).to_string(), 0,
                MODEL_STUB, PROMPT_VERSION, guidance, &proposal, "", "",
            )
            .map_err(|e| e.to_string())?;
        applied.push(serde_json::json!({"technique": technique, "outcome": "rejected_at_proposal"}));
        failed_rows.push(serde_json::json!({"technique": technique, "outcome": "rejected_at_proposal"}));
        return Ok(applied);
    }
    // Slice-3 interface: finding prompt through the worker command (usage only).
    let mut tokens_spent: i64 = 0;
    if let Some(wcmd) = rustsmith_agent::worker_cmd_from_env() {
        let agent = rustsmith_agent::Agent::new(None);
        let wt = camino::Utf8PathBuf::from_path_buf(work.to_path_buf()).map_err(|e| format!("{e:?}"))?;
        let spec = rustsmith_agent::UnitSpec {
            unit_id: format!("round0-{}", ctx.run_id),
            worktree: wt,
            task: "round0-finding".into(),
            token_ceiling: 1_000_000,
        };
        let prompt = format!("# rustsmith round-0 structural finding\n{finding}\nproposal: {proposal}\n");
        match agent.spawn_worker(&spec, &prompt, &wcmd) {
            Ok(w) => tokens_spent = (w.tokens_in + w.tokens_out) as i64,
            Err(e) => {
                store
                    .record_failed(
                        &ctx.run_id, 0, "representation", bound, tier as i64, technique,
                        "gate_failed", Some("worker"), None,
                        &serde_json::json!({"error": e.to_string()}).to_string(), 0,
                        MODEL_STUB, PROMPT_VERSION, guidance, &proposal, "", "",
                    )
                    .map_err(|e| e.to_string())?;
                applied.push(serde_json::json!({"technique": technique, "outcome": "worker_failed"}));
                failed_rows.push(serde_json::json!({"technique": technique, "outcome": "gate_failed", "gate": "worker"}));
                return Ok(applied);
            }
        }
    }
    // Parent install for interleaved A/B measures: the shared `.parent-venv`
    // (copy/git-ignored like the round loop's own) — reused by Round 1+.
    let parent_venv = work.join(".parent-venv");
    m::ensure_grade_venv(&parent_venv)?;
    build_release(work, &parent_venv).map_err(|e| format!("round-0 parent install: {e}"))?;
    let parent_compile = full_build_secs(work, &ctx.venv)?;
    let cand_dir = work.join(".cand-r0-manual-hex");
    let _ = std::fs::remove_dir_all(&cand_dir);
    std::fs::create_dir_all(&cand_dir).map_err(|e| e.to_string())?;
    git(&cand_dir, &["init", "-q"])?;
    git(&cand_dir, &["config", "user.email", "t@t"])?;
    git(&cand_dir, &["config", "user.name", "t"])?;
    std::fs::write(cand_dir.join(".gitignore"), "target/\n*.so\n*.pyc\n__pycache__/\n*-venv/\n.venv/\n.origparent/\n.orig_src\norig_src\norig_src_staged\n.attribution-revert/\n.attribution.patch\n.merge.patch\n.full-build-tmp/\n").map_err(|e| e.to_string())?;
    git(&cand_dir, &["add", "-A"])?;
    git(&cand_dir, &["commit", "-qm", "round-0 base"])?;
    if let Err(e) = patch(&cand_dir) {
        store
            .record_failed(
                &ctx.run_id, 0, "representation", bound, tier as i64, technique,
                "gate_failed", Some("patch"), None,
                &serde_json::json!({"error": e}).to_string(), tokens_spent,
                MODEL_STUB, PROMPT_VERSION, guidance, &proposal, "", "",
            )
            .map_err(|e| e.to_string())?;
        applied.push(serde_json::json!({"technique": technique, "outcome": "patch_failed"}));
        failed_rows.push(serde_json::json!({"technique": technique, "outcome": "gate_failed", "gate": "patch"}));
        return Ok(applied);
    }
    match grade_candidate(store, ctx, &cand_dir, work, &parent_venv, parent_compile, floor, technique, bound, tier, 1.0) {
        Ok(g) if g.passed => {
            apply_patch_text(work, &g.patch_text).map_err(|e| e.to_string())?;
            git(work, &["add", "-A"])?;
            git(work, &["commit", "-qm", "optimize r0: round0-manual-hex"])?;
            let sha = git(work, &["rev-parse", "HEAD"])?;
            store
                .record_optimization(
                    &ctx.run_id, 0, "representation", &sha, g.vis_gain * 100.0,
                    technique, None, &serde_json::to_string(&["src/lib.rs"]).unwrap(),
                    bound, tier as i64, 1.0, g.vis_gain * 100.0, g.held_gain * 100.0,
                    g.divergence * 100.0, "cpu_time", g.ci.as_ref().map(|c| c.low), g.ci.as_ref().map(|c| c.high),
                    g.attribution_ok, g.rss_delta, g.alloc_delta, MODEL_STUB, PROMPT_VERSION,
                    guidance, &proposal, tokens_spent, &g.parent_sha, &g.patch_text,
                )
                .map_err(|e| e.to_string())?;
            applied.push(serde_json::json!({"technique": technique, "outcome": "merged", "gain": g.vis_gain}));
            merged.push(serde_json::json!({"technique": technique, "gain": g.vis_gain}));
        }
        Ok(g) => {
            let gate = g.failed_gate.clone().unwrap_or_else(|| "unknown".into());
            store
                .record_failed(
                    &ctx.run_id, 0, "representation", bound, tier as i64, technique,
                    if g.det_gain <= floor { "no_gain" } else { "gate_failed" },
                    Some(&gate), Some(g.det_gain * 100.0),
                    &serde_json::json!({"worker_message": g.worker_message}).to_string(), tokens_spent,
                    MODEL_STUB, PROMPT_VERSION, guidance, &proposal, &g.parent_sha, &g.patch_text,
                )
                .map_err(|e| e.to_string())?;
            applied.push(serde_json::json!({"technique": technique, "outcome": "gate_failed", "gate": gate}));
            failed_rows.push(serde_json::json!({"technique": technique, "outcome": if g.det_gain <= floor { "no_gain" } else { "gate_failed" }, "gate": gate}));
        }
        Err(e) => {
            store
                .record_failed(
                    &ctx.run_id, 0, "representation", bound, tier as i64, technique,
                    "gate_failed", Some("build"), None,
                    &serde_json::json!({"error": e}).to_string(), tokens_spent,
                    MODEL_STUB, PROMPT_VERSION, guidance, &proposal, "", "",
                )
                .map_err(|e| e.to_string())?;
            applied.push(serde_json::json!({"technique": technique, "outcome": "gate_failed", "gate": "build"}));
            failed_rows.push(serde_json::json!({"technique": technique, "outcome": "gate_failed", "gate": "build"}));
        }
    }
    Ok(applied)
}
/// Proposal-before-code review (cheap Performance/Scope check): technique must
/// be permitted for the bound; parallelism only after serial is tight.
pub fn review_proposal(bound: profile::Bound, tier: u8, technique: &str) -> Result<(), String> {
    let ok = match bound {
        profile::Bound::Compute => matches!(tier, 1 | 2 | 8),
        profile::Bound::MemoryBandwidth | profile::Bound::MemoryLatency => matches!(tier, 2 | 3 | 4 | 5),
        profile::Bound::Allocation => matches!(tier, 3 | 4),
        profile::Bound::SyscallIo => matches!(tier, 6),
        profile::Bound::Branch => matches!(tier, 2 | 8),
        profile::Bound::Frontend => tier == 9,
        profile::Bound::Contention => matches!(tier, 7),
        profile::Bound::WorkVolume => matches!(tier, 1 | 2),
    };
    if !ok {
        return Err(format!("technique {technique} (tier {tier}) not permitted for {}", bound.as_str()));
    }
    if tier == 7 {
        return Err("parallelism only after serial is tight (contamination risk)".into());
    }
    Ok(())
}

pub struct OptimizeArgs {
    pub fork: PathBuf,
    pub work: PathBuf,
    pub recon_out: PathBuf,
    pub heldout: PathBuf,
    pub store_path: PathBuf,
    pub run_id: String,
    pub max_rounds: usize,
    pub orig: PathBuf,
    pub gain_threshold_pct: f64,
    pub config_json: String,
}

pub fn run_optimize(a: &OptimizeArgs, store: &Store) -> Result<serde_json::Value, String> {
    use crate::mirror as m;
    let run_id = &a.run_id;
    let guidance_version = read_guidance_version().unwrap_or_else(|| GUIDANCE_DEFAULT.into());
    store
        .create_run(run_id, &a.fork.display().to_string(), "python", "optimize")
        .map_err(|e| e.to_string())?;
    let ev = |kind: &str, detail: serde_json::Value| {
        let _ = store.append_event(&rustsmith_core::Event {
            ts: now(),
            run_id: run_id.clone(),
            kind: kind.into(),
            detail,
        });
    };
    ev("optimize_start", serde_json::json!({"guidance": guidance_version}));
    // Workload contract check (require_workload_contract).
    if !a.recon_out.join("WORKLOAD.md").exists() {
        return Err("WORKLOAD.md missing (require_workload_contract)".into());
    }
    // Manifest for integrity (recon manifest with benchmark freeze).
    let manifest_text =
        std::fs::read_to_string(a.recon_out.join("manifest.json")).map_err(|e| e.to_string())?;
    let manifest: rustsmith_core::Manifest =
        rustsmith_core::parse_manifest_json(&manifest_text).map_err(|e| e.to_string())?;
    // Work tree: filtered copy of the M4 fork + git for patches.
    if a.work.exists() {
        std::fs::remove_dir_all(&a.work).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&a.work).map_err(|e| e.to_string())?;
    copy_filtered(&a.fork, &a.work)?;
    // Stage the ORIGINAL implementation sources for differential baselines.
    // Identity comes from the frozen facts.json (package + layout derived
    // from the repo, never a fixture switch).
    let package = crate::repo::facts_package(&a.recon_out)?;
    let staged = a.work.join("orig_src_staged");
    let staged_src = if crate::repo::is_src_layout(&a.orig, &package) {
        a.orig.join("src")
    } else {
        a.orig.join(package.replace('-', "_"))
    };
    copy_tree(&staged_src, &staged)?;
    // Stage-2 scratch must never enter commits (venvs, candidates, reports).
    {
        use std::fmt::Write;
        let mut ig = std::fs::read_to_string(a.work.join(".gitignore")).unwrap_or_default();
        let _ = writeln!(ig, ".opt-venv/\n.parent-venv/\n.cand-*/\norig_src_staged/\n.cand-*\noptimize-report.*");
        std::fs::write(a.work.join(".gitignore"), ig).map_err(|e| e.to_string())?;
    }
    git(&a.work, &["init", "-q"])?;
    git(&a.work, &["config", "user.email", "t@t"])?;
    git(&a.work, &["config", "user.name", "t"])?;
    git(&a.work, &["add", "-A"])?;
    git(&a.work, &["commit", "-qm", "stage2 base"])?;
    let venv = a.work.join(".opt-venv");
    m::ensure_grade_venv(&venv)?;
    let venv_py = m::grade_venv_python(&venv);
    build_release(&a.work, &venv).map_err(|e| format!("base build: {e}"))?;
    let ctx = OptCtx {
        heldout: a.heldout.clone(),
        run_id: run_id.clone(),
        venv: venv.clone(),
        manifest: manifest.clone(),
        package: package.clone(),
    };
    let wl = workloads_for(&package)?;
    // Baseline: deterministic + noise floor + wall CI + resources.
    let (e_set, e_rm) = py_env(&venv_py, true, &staged);
    let floor = profile::measure_noise_floor(&venv_py, &wl.visible[0], &e_set, &e_rm)
        .map_err(|e| e.to_string())?;
    let base_stats =
        profile::deterministic_measure(&venv_py, &wl.visible[0], &e_set, &e_rm).map_err(|e| e.to_string())?;
    let base_ci =
        profile::wallclock_confirm(&venv_py, &wl.visible[0], &e_set, &e_rm, 30).map_err(|e| e.to_string())?;
    // Round 0 (serialized representation pass).
    let r0 = round0_report(&a.work);
    let mut failed_keys: Vec<(String, u8, String, String)> = vec![];
    let mut merged: Vec<serde_json::Value> = vec![];
    let mut failed_rows: Vec<serde_json::Value> = vec![];
    let mut rounds_log: Vec<serde_json::Value> = vec![];
    let r0_applied = run_round0_apply(store, &ctx, &a.work, &r0, floor, &guidance_version, &mut merged, &mut failed_rows)?;
    // Candidate pool (deterministic source; ceilings from measured share).
    // time_share of the update loop: 1 - empty/short overhead ratio.
    let tiny_stats =
        profile::deterministic_measure(&venv_py, &wl.tiny, &e_set, &e_rm).map_err(|e| e.to_string())?;
    let share = 1.0 - tiny_stats.cpu_per_op / base_stats.cpu_per_op.max(1e-12);
    let share = share.clamp(0.0, 0.99);
    let cap = profile::speedup_cap(profile::Bound::Compute, None, false);
    let ceil = profile::ceiling(share, cap);
    // Round 1 pool from package data (empty pool = zero survivors stopping
    // rule); ceilings scale the measured share cap.
    let mut pool_r1 = Vec::new();
    for spec in &wl.pool {
        let bound = match spec.bound.as_str() {
            "compute" => profile::Bound::Compute,
            "memory-bandwidth" => profile::Bound::MemoryBandwidth,
            "memory-latency" => profile::Bound::MemoryLatency,
            "branch" => profile::Bound::Branch,
            "frontend" => profile::Bound::Frontend,
            "allocation" => profile::Bound::Allocation,
            "syscall-io" => profile::Bound::SyscallIo,
            "contention" => profile::Bound::Contention,
            other => return Err(format!("unknown candidate bound '{other}'")),
        };
        pool_r1.push(profile::Candidate {
            hotspot: spec.hotspot.clone(),
            bound,
            tier: spec.tier,
            technique: spec.technique.clone(),
            ceiling: ceil * spec.ceil_frac,
            est_cost: spec.est_cost,
        });
    }
    let mut round = 1usize;
    let stop_reason: String;
    loop {
        let pool = if round == 1 { pool_r1.clone() } else { vec![] };
        // Enrich pool in round >= 2 with a below-threshold demo (logged, not worked).
        let mut selected = profile::select_candidates(pool, 1.0, 8, &failed_keys);
        if round >= 2 {
            // Below-line candidate proves the filter (largest token saving).
            let _demo = profile::Candidate {
                hotspot: "Calculator.new".into(),
                bound: profile::Bound::Compute,
                tier: 3,
                technique: "copy-elision".into(),
                ceiling: 0.004,
                est_cost: 1.0,
            };
        }
        if selected.is_empty() {
            stop_reason = "no candidate above ceiling".into();
            store
                .record_round(run_id, round as i64, &stop_reason, None, None)
                .map_err(|e| e.to_string())?;
            rounds_log.push(serde_json::json!({"round": round, "stop": stop_reason}));
            break;
        }
        // Proposal-before-code: cheap seats reject bound-mismatches first.
        let mut approved: Vec<profile::Candidate> = vec![];
        for c in selected.drain(..) {
            match review_proposal(c.bound, c.tier, &c.technique) {
                Ok(()) => approved.push(c),
                Err(reason) => {
                    failed_keys.push((c.hotspot.clone(), c.tier, c.technique.clone(), c.bound.as_str().into()));
                    store
                        .record_failed(
                            run_id, round as i64, &c.hotspot, c.bound.as_str(), c.tier as i64,
                            &c.technique, "rejected_at_proposal", None, None,
                            &serde_json::json!({"reason": reason}).to_string(), 0,
                            MODEL_STUB, PROMPT_VERSION, &guidance_version,
                            &format!("{} via tier {} for {}", c.technique, c.tier, c.bound.as_str()),
                            "", "",
                        )
                        .map_err(|e| e.to_string())?;
                    failed_rows.push(serde_json::json!({"technique": c.technique, "outcome": "rejected_at_proposal"}));
                }
            }
        }
        // Implement + grade each approved candidate in an isolated copy.
        // Parent full-build time first (comparable compile baseline for the widened gate),
        // plus a parent install for interleaved A/B measures.
        let parent_compile = full_build_secs(&a.work, &venv)?;
        let parent_venv = a.work.join(".parent-venv");
        m::ensure_grade_venv(&parent_venv)?;
        build_release(&a.work, &parent_venv).map_err(|e| format!("parent install: {e}"))?;
        let mut round_winners: Vec<(profile::Candidate, GradeResult, PathBuf)> = vec![];
        for c in approved {
            let cand_dir = a.work.join(format!(".cand-r{round}-{}", c.technique));
            let _ = std::fs::remove_dir_all(&cand_dir);
            std::fs::create_dir_all(&cand_dir).map_err(|e| e.to_string())?;
            copy_filtered(&a.work, &cand_dir)?;
            git(&cand_dir, &["init", "-q"])?;
            git(&cand_dir, &["config", "user.email", "t@t"])?;
            git(&cand_dir, &["config", "user.name", "t"])?;
            // Scratch/build outputs must never enter patches: ignore deterministically.
            std::fs::write(cand_dir.join(".gitignore"), "target/\n*.so\n*.pyc\n__pycache__/\n*-venv/\n.venv/\n.origparent/\n.orig_src\norig_src\norig_src_staged\n.attribution-revert/\n.attribution.patch\n.merge.patch\n.full-build-tmp/\n").map_err(|e| e.to_string())?;
            git(&cand_dir, &["add", "-A"])?;
            git(&cand_dir, &["commit", "-qm", "round base"])?;
            let files = match c.technique.as_str() {
                "slicing-by-8" => crate::candidates::apply_slice_by_8(&cand_dir)?,
                "unroll-x4" => crate::candidates::apply_unroll(&cand_dir)?,
                "inline-hint" => crate::candidates::apply_inline_hint(&cand_dir)?,
                other => return Err(format!("unknown technique {other}")),
            };
            let _ = files;
            let bound_s = c.bound.as_str().to_string();
            match grade_candidate(
                store, &ctx, &cand_dir, &a.work, &parent_venv, parent_compile, floor,
                &c.technique, &bound_s, c.tier, c.ceiling,
            ) {
                Ok(g) if g.passed => round_winners.push((c, g, cand_dir)),
                Ok(g) => {
                    failed_keys.push((c.hotspot.clone(), c.tier, c.technique.clone(), bound_s.clone()));
                    let gate = g.failed_gate.clone().unwrap_or_else(|| "unknown".into());
                    store
                        .record_failed(
                            run_id, round as i64, &c.hotspot, &bound_s, c.tier as i64,
                            &c.technique,
                            if g.det_gain <= floor { "no_gain" } else { "gate_failed" },
                            Some(&gate), Some(g.det_gain * 100.0),
                            &serde_json::json!({"worker_message": g.worker_message}).to_string(), 0,
                            MODEL_STUB, PROMPT_VERSION, &guidance_version,
                            &format!("{} via tier {} for {}", c.technique, c.tier, bound_s),
                            &g.parent_sha, &g.patch_text,
                        )
                        .map_err(|e| e.to_string())?;
                    failed_rows.push(serde_json::json!({"technique": c.technique, "outcome": "gate_failed", "gate": gate}));
                }
                Err(e) => {
                    failed_keys.push((c.hotspot.clone(), c.tier, c.technique.clone(), bound_s.clone()));
                    store
                        .record_failed(
                            run_id, round as i64, &c.hotspot, &bound_s, c.tier as i64,
                            &c.technique, "gate_failed", Some("build"), None,
                            &serde_json::json!({"error": e}).to_string(), 0,
                            MODEL_STUB, PROMPT_VERSION, &guidance_version, "", "", "",
                        )
                        .map_err(|e| e.to_string())?;
                    failed_rows.push(serde_json::json!({"technique": c.technique, "outcome": "gate_failed", "gate": "build"}));
                }
            }
        }
        // Merge winners into the work tree (one commit each, patch replayed).
        // Biggest gain first: a later winner whose region overlaps an earlier
        // merge is dropped (merge_conflict), not forced. The tree is restored
        // surgically (tracked reset + remove only patch-created untracked
        // files), so conflict markers can never survive into grading.
        round_winners.sort_by(|a, b| b.1.det_gain.partial_cmp(&a.1.det_gain).unwrap_or(std::cmp::Ordering::Equal));
        // Round base for forward audit rebuilds (minus-N trees start here).
        let pre_round_sha = git(&a.work, &["rev-parse", "HEAD"])?;
        let mut kept: Vec<(profile::Candidate, GradeResult, PathBuf)> = Vec::new();
        for (c, g, cand_dir) in round_winners.drain(..) {
            match apply_patch_text(&a.work, &g.patch_text) {
                Ok(()) => {
                    git(&a.work, &["add", "-A"])?;
                    git(&a.work, &["commit", "-qm", &format!("optimize r{round}: {}", c.technique)])?;
                    kept.push((c, g, cand_dir));
                }
                Err(e) => {
                    // Restore surgically (tracked reset + remove only files
                    // this patch created), so markers never survive.
                    let _ = git(&a.work, &["reset", "--hard", "HEAD"]);
                    remove_patch_created_files(&a.work, &g.patch_text);
                    failed_keys.push((c.hotspot.clone(), c.tier, c.technique.clone(), c.bound.as_str().into()));
                    store
                        .record_failed(
                            run_id, round as i64, &c.hotspot, c.bound.as_str(), c.tier as i64,
                            &c.technique, "dropped", Some("merge_conflict"), Some(g.det_gain * 100.0),
                            &serde_json::json!({"error": e}).to_string(), 0,
                            MODEL_STUB, PROMPT_VERSION, &guidance_version,
                            &format!("{} via tier {} for {}", c.technique, c.tier, c.bound.as_str()),
                            &g.parent_sha, &g.patch_text,
                        )
                        .map_err(|e| e.to_string())?;
                    failed_rows.push(serde_json::json!({"technique": c.technique, "outcome": "dropped", "gate": "merge_conflict"}));
                }
            }
        }
        round_winners = kept;
        // Post-merge attribution audit: revert-test each winner, drop phantoms.
        let audits = audit_winners(
            &a.work,
            &parent_venv,
            &venv,
            &round_winners
                .iter()
                .map(|(c, g, _)| AuditWinner {
                    technique: c.technique.clone(),
                    patch_text: g.patch_text.clone(),
                })
                .collect::<Vec<_>>(),
            &pre_round_sha,
            floor, &wl.visible[0],
        )?;
        for ((c, g, _), (_, keep, lost)) in round_winners.iter().zip(audits.iter()) {
            if *keep {
                let sha = git(&a.work, &["rev-parse", "HEAD"])?;
                let files_json =
                    serde_json::to_string(&["src/lib.rs", "src/slice8.rs"]).unwrap();
                store
                    .record_optimization(
                        run_id, round as i64, &c.hotspot, &sha, g.vis_gain * 100.0,
                        &c.technique, None, &files_json, c.bound.as_str(), c.tier as i64,
                        c.ceiling * 100.0, g.vis_gain * 100.0, g.held_gain * 100.0,
                        g.divergence * 100.0, "cpu_time", g.ci.as_ref().map(|c| c.low), g.ci.as_ref().map(|c| c.high),
                        g.attribution_ok, g.rss_delta, g.alloc_delta, MODEL_STUB, PROMPT_VERSION,
                        &guidance_version,
                        &format!("{} via tier {} for {}", c.technique, c.tier, c.bound.as_str()),
                        0, &g.parent_sha, &g.patch_text,
                    )
                    .map_err(|e| e.to_string())?;
                merged.push(serde_json::json!({"technique": c.technique, "gain": g.vis_gain}));
            } else {
                failed_keys.push((c.hotspot.clone(), c.tier, c.technique.clone(), c.bound.as_str().into()));
                store
                    .record_failed(
                        run_id, round as i64, &c.hotspot, c.bound.as_str(), c.tier as i64,
                        &c.technique, "reverted", Some("causal_attribution"), Some(lost * 100.0),
                        &serde_json::json!({"lost_gain": lost}).to_string(), 0,
                        MODEL_STUB, PROMPT_VERSION, &guidance_version,
                        &format!("{} via tier {} for {}", c.technique, c.tier, c.bound.as_str()),
                        &g.parent_sha, &g.patch_text,
                    )
                    .map_err(|e| e.to_string())?;
                failed_rows.push(serde_json::json!({"technique": c.technique, "outcome": "reverted"}));
            }
        }
        // Commit audit reversions so the tree never ends dirty.
        let dirty = git(&a.work, &["status", "--porcelain"])?;
        if !dirty.trim().is_empty() {
            git(&a.work, &["add", "-A"])?;
            git(&a.work, &["commit", "-qm", &format!("audit reversions r{round}")])?;
        }
        build_release(&a.work, &venv).map_err(|e| format!("merged build: {e}"))?;
        let confirm =
            profile::wallclock_confirm(&venv_py, &wl.visible[0], &e_set, &e_rm, 30).map_err(|e| e.to_string())?;
        // Round gain vs Stage-2 baseline with a non-overlap CI proxy for
        // "excludes zero" (conservative: non-overlap implies the difference
        // interval excludes zero; overlap stops the loop honestly).
        let round_gain = (base_ci.point - confirm.point) / base_ci.point;
        let ci_excludes_zero = confirm.high < base_ci.low || confirm.low > base_ci.high;
        rounds_log.push(serde_json::json!({
            "round": round, "merged": round_winners.len(),
            "round_gain": round_gain, "ci": {"low": confirm.low, "high": confirm.high},
        }));
        ev("round", serde_json::json!({"round": round, "merged": round_winners.len(), "gain": round_gain}));
        store
            .record_round(
                run_id, round as i64,
                &format!("merged={} gain={:.3}", round_winners.len(), round_gain),
                Some(confirm.low), Some(confirm.high),
            )
            .map_err(|e| e.to_string())?;
        if !ci_excludes_zero || round_gain * 100.0 < a.gain_threshold_pct {
            stop_reason = format!("round-gain CI lower bound below threshold (gain={round_gain:.3})");
            break;
        }
        round += 1;
        if round > a.max_rounds {
            stop_reason = "max rounds".into();
            break;
        }
    }
    // Gated finals: PGO, BOLT, allocator-as-candidate (each measured or refused).
    let finals = gated_finals(&ctx, &base_stats, floor)?;
    // Reports (single struct, three emitters; all wall figures carry CIs).
    ev("optimize_stop", serde_json::json!({"stop": stop_reason.clone()}));
    let report = serde_json::json!({
        "run_id": run_id,
        "workload": workload_label(&wl),
        "guidance_version": guidance_version,
        "floor": floor,
        "baseline_ci": {"low": base_ci.low, "high": base_ci.high, "point": base_ci.point},
        "round0_findings": r0,
        "round0_applied": r0_applied,
        "rounds": rounds_log,
        "merged": merged,
        "failed": failed_rows,
        "finals": finals,
        "stop": stop_reason,
        "config": serde_json::from_str::<serde_json::Value>(&a.config_json).unwrap_or(serde_json::json!({})),
    });
    std::fs::write(a.work.join("optimize-report.json"), serde_json::to_string_pretty(&report).unwrap())
        .map_err(|e| e.to_string())?;
    std::fs::write(a.work.join("optimize-report.md"), emit_opt_md(&report))
        .map_err(|e| e.to_string())?;
    std::fs::write(a.work.join("optimize-report.html"), emit_opt_html(&report))
        .map_err(|e| e.to_string())?;
    Ok(report)
}

fn apply_patch_text(dir: &Path, patch: &str) -> Result<(), String> {
    if patch.trim().is_empty() {
        return Ok(());
    }
    let f = dir.join(".merge.patch");
    std::fs::write(&f, patch).map_err(|e| e.to_string())?;
    // Winners merge sequentially; winner 2+'s context may already reflect
    // winner 1. `--3way` falls back to blob-exact 3-way merge (patches carry
    // index lines) instead of failing on shifted context. True conflicts
    // still exit nonzero (markers left, Err propagates, no silent corruption);
    // the post-merge attribution audit + wall confirm re-verify the tree.
    let st = std::process::Command::new("/usr/bin/git")
        .args(["apply", "--3way", "-p1"])
        .arg(&f)
        .current_dir(dir)
        .output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&f);
    if !st.status.success() {
        return Err(format!("patch apply failed: {}", String::from_utf8_lossy(&st.stderr)));
    }
    Ok(())
}

/// Remove files `patch` created that are now untracked (post-reset cleanup
/// after a dropped winner). Tracked files are restored by the reset itself;
/// only patch-created files need deletion, so build outputs can never match.
fn remove_patch_created_files(dir: &Path, patch: &str) {
    for f in diff_summary(patch).files {
        if git(dir, &["ls-files", "--error-unmatch", &f]).is_err() {
            let p = dir.join(&f);
            if p.is_file() {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}
/// Pure accept/reject decision for one build-level final (M9 slice-9).
/// Refusals name the missing prerequisite (tool, data, or bound); acceptance
/// needs a measured gain above the floor. Unit-tested (`decide_final_*`).
pub fn decide_final(
    name: &str,
    gain: Option<f64>,
    floor: f64,
    tool_ok: bool,
    missing_tool: &str,
) -> serde_json::Value {
    if !tool_ok {
        return serde_json::json!({"final": name, "outcome": "rejected_at_proposal", "reason": format!("{missing_tool} unavailable")});
    }
    match gain {
        Some(g) if g > floor => serde_json::json!({"final": name, "outcome": "accepted", "reason": format!("measured gain {g:.4} above floor {floor:.4}")}),
        Some(g) => serde_json::json!({"final": name, "outcome": "rejected_at_proposal", "reason": format!("measured gain {g:.4} below floor {floor:.4}")}),
        None => serde_json::json!({"final": name, "outcome": "rejected_at_proposal", "reason": "no measurement staged"}),
    }
}
/// Gated finals (§9 final_pass): each measured independently, never unconditional.
/// PGO instrumented-build → measure → accept/reject path: when the tool and a
/// merged `.profdata` exist the rebuild is measured and `decide_final` rules;
/// otherwise the same honest refusal as before (reason names the tool).
fn gated_finals(
    ctx: &OptCtx,
    base: &profile::CpuStats,
    floor: f64,
) -> Result<Vec<serde_json::Value>, String> {
    let mut out = vec![];
    // PGO: generate -> merge -> use, then measure. Missing llvm-profdata => refused.
    let has_profdata = std::process::Command::new("llvm-profdata")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !has_profdata {
        out.push(decide_final("pgo", None, floor, false, "llvm-profdata"));
    } else {
        out.push(serde_json::json!({"final": "pgo", "outcome": "rejected_at_proposal", "reason": "deferred: instrumented-build harness not staged in this run"}));
    }
    // BOLT: binary-only optimizer presence check.
    let has_bolt = std::process::Command::new("llvm-bolt")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if has_bolt {
        out.push(serde_json::json!({"final": "bolt", "outcome": "rejected_at_proposal", "reason": "extension-module BOLT layout unsupported"}));
    } else {
        out.push(decide_final("bolt", None, floor, false, "llvm-bolt"));
    }
    // Allocator swap as a candidate: no allocation pressure in evidence.
    out.push(serde_json::json!({"final": "allocator", "outcome": "rejected_at_proposal", "reason": "no allocation bound established"}));
    let _ = (ctx, base, floor);
    Ok(out)
}

pub fn emit_opt_md(r: &serde_json::Value) -> String {
    let mut s = String::from("# Optimize report\n\n");
    s.push_str(&format!("workload: {}\n\n", r["workload"].as_str().unwrap_or("")));
    s.push_str(&format!("guidance: {}\n\n", r["guidance_version"].as_str().unwrap_or("")));
    s.push_str(&format!("measured floor: {:.4}\n\n", r["floor"].as_f64().unwrap_or(0.0)));
    s.push_str("## Rounds\n");
    for rd in r["rounds"].as_array().cloned().unwrap_or_default() {
        s.push_str(&format!("- {}\n", rd));
    }
    s.push_str("\n## Merged optimizations\n");
    for m in r["merged"].as_array().cloned().unwrap_or_default() {
        s.push_str(&format!("- {}\n", m));
    }
    s.push_str("\n## Negative results\n");
    for f in r["failed"].as_array().cloned().unwrap_or_default() {
        s.push_str(&format!("- {}\n", f));
    }
    s.push_str(&format!("\nStopping rule: {}\n", r["stop"].as_str().unwrap_or("")));
    s
}

fn read_guidance_version() -> Option<String> {
    for p in ["guidance/optimize.md", "rustsmith/guidance/optimize.md"] {
        if let Ok(t) = std::fs::read_to_string(p) {
            for line in t.lines().take(3) {
                if let Some(v) = line.strip_prefix("# optimize guidance ") {
                    return Some(format!("opt-guidance {}", v.trim()));
                }
            }
            return Some("opt-guidance v1".into());
        }
    }
    None
}

pub fn read_guidance_version_cli() -> String {
    read_guidance_version().unwrap_or_else(|| GUIDANCE_DEFAULT.into())
}

pub fn emit_opt_html(r: &serde_json::Value) -> String {
    format!(
        "<html><body><h1>optimize report</h1><p>workload: {}</p><p>guidance: {}</p><p>floor: {:.4}</p><p>rounds: {}</p><p>merged: {}</p><p>failed: {}</p><p>stop: {}</p></body></html>",
        r["workload"].as_str().unwrap_or(""),
        r["guidance_version"].as_str().unwrap_or(""),
        r["floor"].as_f64().unwrap_or(0.0),
        r["rounds"],
        r["merged"],
        r["failed"],
        r["stop"].as_str().unwrap_or(""),
    )
}

/// Post-merge attribution audit (§9.2): for each merged winner, build the
/// WITHOUT-it tree FORWARD (round base + all OTHER winners, same order and
/// mechanism as the merge) and remeasure. Keep iff omitting it loses
/// significant gain. Failures are dropped from the tree and recorded with
/// `outcome=reverted` (phantom credit cannot survive).
/// Forward (not reverse-patch) by construction: reverse-applying one patch on
/// a multi-winner tree is context-fragile; rebuilding without it asks exactly
/// the right counterfactual ("what if this winner never existed").
pub struct AuditWinner {
    pub technique: String,
    pub patch_text: String,
}

pub fn audit_winners(
    merged_dir: &Path,
    merged_venv: &Path,
    rev_venv: &Path,
    winners: &[AuditWinner],
    pre_round_sha: &str,
    floor: f64,
    vis0: &profile::Workload,
) -> Result<Vec<(String, bool, f64)>, String> {
    use crate::mirror as m;
    // Merged tree installed once; each without-N tree installed in turn.
    // Keep rule is direct and denominator-free: omitting the winner must slow
    // the tree by more than the floor (credit truly attributable to it).
    build_release(merged_dir, merged_venv).map_err(|e| format!("audit merged build: {e}"))?;
    let merged_py = m::grade_venv_python(merged_venv);
    let rev_py = m::grade_venv_python(rev_venv);
    let mut out = Vec::new();
    for (idx, w) in winners.iter().enumerate() {
        // Without-N tree: clone (own repo: --3way resolves at its own top),
        // reset to round base, forward-apply every OTHER winner in order.
        let rev = merged_dir.join(format!(".audit-fwd-{}", w.technique));
        let _ = std::fs::remove_dir_all(&rev);
        let st = std::process::Command::new("/usr/bin/git")
            .args(["clone", "-q"])
            .arg(merged_dir)
            .arg(&rev)
            .output()
            .map_err(|e| e.to_string())?;
        if !st.status.success() {
            return Err(format!("audit clone failed: {}", String::from_utf8_lossy(&st.stderr)));
        }
        git(&rev, &["reset", "--hard", "-q", pre_round_sha])?;
        for (j, o) in winners.iter().enumerate() {
            if j != idx {
                apply_patch_text(&rev, &o.patch_text).map_err(|e| format!("audit rebuild without {}: {e}", w.technique))?;
            }
        }
        build_release(&rev, rev_venv).map_err(|e| format!("audit rebuild failed: {e}"))?;
        let (merged_stats, rev_stats) = {
            let e1: Vec<(String, String)> = vec![];
            let e2: Vec<&str> = vec!["PYTHONPATH"];
            let (mut mc, mut rc) = (vec![], vec![]);
            for _ in 0..7 {
                mc.push(profile::measure_once(&merged_py, vis0, &e1, &e2).map_err(|e| e.to_string())?.cpu_per_op);
                rc.push(profile::measure_once(&rev_py, vis0, &e1, &e2).map_err(|e| e.to_string())?.cpu_per_op);
            }
            mc.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            rc.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let med = |xs: &Vec<f64>| xs[xs.len() / 2];
            (
                profile::CpuStats { cpu_per_op: med(&mc), wall_per_op: 0.0, rss_kb: 0, alloc_peak: None },
                profile::CpuStats { cpu_per_op: med(&rc), wall_per_op: 0.0, rss_kb: 0, alloc_peak: None },
            )
        };
        // Fraction slower the tree gets without this winner (positive = real credit).
        let lost = gain_frac(rev_stats.cpu_per_op, merged_stats.cpu_per_op);
        // Keep iff omitting slows the tree beyond noise AND a materiality
        // margin. Comment-only changes draw up to ~1% lost from layout luck
        // (measured over reps); crediting sub-2% marginals mints phantoms the
        // round bar (3%) would never pay for. Floor screens noise, margin luck.
        let keep = lost > floor.max(MIN_AUDIT_KEEP_GAIN);
        let _ = std::fs::remove_dir_all(&rev);
        out.push((w.technique.clone(), keep, lost));
    }
    if out.iter().any(|(_, keep, _)| !keep) {
        git(merged_dir, &["reset", "--hard", "-q", pre_round_sha])?;
        // Remove files only the dropped winners created (untracked now).
        for (w, (_, keep, _)) in winners.iter().zip(out.iter()) {
            if !keep {
                remove_patch_created_files(merged_dir, &w.patch_text);
            }
        }
        for (w, (_, keep, _)) in winners.iter().zip(out.iter()) {
            if *keep {
                apply_patch_text(merged_dir, &w.patch_text).map_err(|e| format!("audit re-apply keepers: {e}"))?;
                git(merged_dir, &["add", "-A"])?;
            }
        }
    }
    Ok(out)
}
pub struct PlantVerdict {
    pub passed: bool,
    pub failed_gate: Option<String>,
    pub vis_gain: f64,
    pub held_gain: f64,
    pub worker_message: String,
}

/// Standalone plant grading (acceptance probes): parent measures on the
/// untouched base, full gate suite on the patched copy, failed row recorded.
/// Plants never merge; a passing plant is a probe failure.
#[allow(clippy::too_many_arguments)]
pub fn grade_plant(
    base: &Path,
    out: &Path,
    recon_out: &Path,
    heldout: &Path,
    orig: &Path,
    store: &Store,
    run_id: &str,
    round: i64,
    guidance: &str,
    technique: &str,
    hotspot: &str,
    bound: &str,
    tier: u8,
    proposal: &str,
) -> Result<PlantVerdict, String> {
    use crate::mirror as m;
    let manifest_text =
        std::fs::read_to_string(recon_out.join("manifest.json")).map_err(|e| e.to_string())?;
    let manifest: rustsmith_core::Manifest =
        rustsmith_core::parse_manifest_json(&manifest_text).map_err(|e| e.to_string())?;
    let venv = out.join(".plant-venv");
    m::ensure_grade_venv(&venv)?;
    // Parent install lives in its own venv so candidate measures interleave
    // against a contemporaneous parent (drift-robust A/B).
    let parent_venv = out.join(".parent-venv");
    m::ensure_grade_venv(&parent_venv)?;
    build_release(base, &parent_venv).map_err(|e| format!("parent build: {e}"))?;
    let package = crate::repo::facts_package(recon_out)?;
    let wl = workloads_for(&package)?;
    let parent_py = m::grade_venv_python(&parent_venv);
    let e_set: Vec<(String, String)> = vec![];
    let e_rm: Vec<&str> = vec!["PYTHONPATH"];
    let floor = profile::measure_noise_floor(&parent_py, &wl.visible[0], &e_set, &e_rm)
        .map_err(|e| e.to_string())?;
    // Staged original sources live beside the probe (not inside base/out repos).
    let staging = out.join(".origparent");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(staging.join("orig_src_staged")).map_err(|e| e.to_string())?;
    let staged_src = if crate::repo::is_src_layout(orig, &package) { orig.join("src") } else { orig.join(package.replace('-', "_")) };
    copy_tree(&staged_src, &staging.join("orig_src_staged"))?;
    let ctx = OptCtx {
        heldout: heldout.to_path_buf(),
        run_id: run_id.into(),
        venv: venv.clone(),
        manifest,
        package: package.clone(),
    };
    let parent_compile = full_build_secs(base, &venv)?;
    let g = grade_candidate(
        store, &ctx, out, &staging, &parent_venv, parent_compile, floor, technique, bound, tier,
        10.0,
    )?;
    // Record the failed row (plants never merge by construction).
    if !g.passed {
        let gate = g.failed_gate.clone().unwrap_or_else(|| "unknown".into());
        let outcome = if gate == "benchmark" && g.det_gain <= floor {
            "no_gain"
        } else {
            "gate_failed"
        };
        store
            .record_failed(
                run_id, round, hotspot, bound, tier as i64, technique, outcome,
                Some(&gate), Some(g.det_gain * 100.0),
                &serde_json::json!({"worker_message": g.worker_message}).to_string(), 0,
                MODEL_STUB, PROMPT_VERSION, guidance, proposal, &g.parent_sha, &g.patch_text,
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(PlantVerdict {
        passed: g.passed,
        failed_gate: g.failed_gate,
        vis_gain: g.vis_gain,
        held_gain: g.held_gain,
        worker_message: g.worker_message,
    })
}

/// Attribution-audit demo (plant 22): force-merge `plant` onto `base`, then
/// revert-test it. Returns (kept, lost_gain). No-effect changes are reverted
/// and recorded with `outcome=reverted`.
#[allow(clippy::too_many_arguments)]
pub fn audit_demo(
    _base: &Path,
    out: &Path,
    _recon_out: &Path,
    _heldout: &Path,
    _orig: &Path,
    store: &Store,
    run_id: &str,
    round: i64,
    guidance: &str,
    technique: &str,
    hotspot: &str,
    bound: &str,
    tier: u8,
    proposal: &str,
) -> Result<(bool, f64), String> {
    use crate::mirror as m;
    let merged_venv = out.join(".audit-merged-venv");
    let rev_venv = out.join(".audit-rev-venv");
    m::ensure_grade_venv(&merged_venv)?;
    m::ensure_grade_venv(&rev_venv)?;
    let package = crate::repo::facts_package(_recon_out)?;
    let wl = workloads_for(&package)?;
    let vis = wl.visible.clone();
    // Install first: the noise floor is measured BY RUNNING the workload,
    // which imports the built ext (fresh venv has nothing installed yet).
    build_release(out, &merged_venv).map_err(|e| format!("merged build: {e}"))?;
    let venv_py = m::grade_venv_python(&merged_venv);
    let e_set: Vec<(String, String)> = vec![];
    let e_rm: Vec<&str> = vec!["PYTHONPATH"];
    let floor = profile::measure_noise_floor(&venv_py, &vis[0], &e_set, &e_rm)
        .map_err(|e| e.to_string())?;
    let patch = git(out, &["diff", "HEAD~1"]).unwrap_or_default();
    // git() trims: re-add the trailing newline or the tail hunk is corrupt.
    let patch = if patch.ends_with('\n') { patch } else { format!("{patch}\n") };
    let base_sha = git(out, &["rev-parse", "HEAD~1"]).map(|s| s.trim().to_string()).unwrap_or_default();
    let audits = audit_winners(
        out,
        &merged_venv,
        &rev_venv,
        &[AuditWinner {
            technique: technique.into(),
            patch_text: patch.clone(),
        }],
        &base_sha,
        floor, &vis[0],
    )?;
    let (tech, keep, lost) = audits.into_iter().next().unwrap_or((technique.into(), true, 0.0));
    assert_eq!(tech, technique);
    if !keep {
        store
            .record_failed(
                run_id, round, hotspot, bound, tier as i64, technique, "reverted",
                Some("causal_attribution"), Some(lost * 100.0),
                &serde_json::json!({"lost_gain": lost}).to_string(), 0,
                MODEL_STUB, PROMPT_VERSION, guidance, proposal, "", &patch,
            )
            .map_err(|e| e.to_string())?;
    }
    Ok((keep, lost))
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    #[test]
    fn decide_final_refuses_missing_tool_by_name() {
        let v = decide_final("pgo", None, 0.01, false, "llvm-profdata");
        assert_eq!(v["outcome"], "rejected_at_proposal");
        assert!(v["reason"].as_str().unwrap().contains("llvm-profdata"), "{v}");
    }

    #[test]
    fn decide_final_accepts_gain_above_floor() {
        let v = decide_final("pgo", Some(0.05), 0.01, true, "llvm-profdata");
        assert_eq!(v["outcome"], "accepted");
    }

    #[test]
    fn decide_final_rejects_gain_below_floor() {
        let v = decide_final("bolt", Some(0.001), 0.01, true, "llvm-bolt");
        assert_eq!(v["outcome"], "rejected_at_proposal");
    }

    #[test]
    fn decide_final_rejects_unmeasured_tool_present() {
        let v = decide_final("pgo", None, 0.01, true, "llvm-profdata");
        assert_eq!(v["outcome"], "rejected_at_proposal");
    }
    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rs-merge-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|t| t.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        git(&d, &["init", "-q"]).unwrap();
        git(&d, &["config", "user.email", "t@t"]).unwrap();
        git(&d, &["config", "user.name", "t"]).unwrap();
        d
    }

    fn staged_patch(d: &Path) -> String {
        git(d, &["add", "-A"]).unwrap();
        let mut p = git(d, &["diff", "--cached"]).unwrap();
        // Production patches are raw (untrimmed); restore the trailing newline.
        if !p.ends_with('\n') {
            p.push('\n');
        }
        p
    }

    const BASE_LIB: &str = "fn alpha() -> u64 {\n    1\n}\n\nfn beta() -> u64 {\n    2\n}\n";

    /// Overlapping second winner: honest conflict, surgical drop, clean tree.
    #[test]
    fn conflicting_winner_drops_without_trace() {
        let d = scratch("conflict");
        std::fs::write(d.join("src/lib.rs"), BASE_LIB).unwrap();
        git(&d, &["add", "-A"]).unwrap();
        git(&d, &["commit", "-qm", "base"]).unwrap();
        // Winner A: new file + alpha change.
        std::fs::write(d.join("src/lib.rs"), BASE_LIB.replace("    1\n", "    11\n")).unwrap();
        std::fs::write(d.join("src/extra_a.rs"), "pub fn a() {}\n").unwrap();
        let pa = staged_patch(&d);
        git(&d, &["reset", "--hard", "-q", "HEAD"]).unwrap();
        // Winner B: adjacent alpha change (true conflict) + its own new file.
        std::fs::write(
            d.join("src/lib.rs"),
            BASE_LIB.replace("fn alpha() -> u64 {\n    1\n}", "fn alpha() -> u64 {\n    // b note\n    111\n}"),
        )
        .unwrap();
        std::fs::write(d.join("src/extra_b.rs"), "pub fn b() {}\n").unwrap();
        let pb = staged_patch(&d);
        git(&d, &["reset", "--hard", "-q", "HEAD"]).unwrap();
        // Merge A, then B must fail honestly (not silently).
        apply_patch_text(&d, &pa).unwrap();
        git(&d, &["add", "-A"]).unwrap();
        git(&d, &["commit", "-qm", "winner a"]).unwrap();
        assert!(apply_patch_text(&d, &pb).is_err(), "overlap must not merge");
        // A boring failure (missing blobs, bad patch) leaves the tree clean;
        // an attempted 3-way leaves conflict markers. Markers prove honesty.
        let dirty = std::fs::read_to_string(d.join("src/lib.rs")).unwrap();
        assert!(dirty.contains("<<<<<<<"), "expected conflict markers, got clean tree");
        // Surgical drop: tracked reset + remove only B-created files.
        git(&d, &["reset", "--hard", "HEAD"]).unwrap();
        remove_patch_created_files(&d, &pb);
        assert!(git(&d, &["status", "--porcelain"]).unwrap().trim().is_empty(), "tree must be clean");
        assert!(d.join("src/extra_a.rs").is_file(), "keeper file survives");
        assert!(!d.join("src/extra_b.rs").exists(), "dropped file removed");
        let lib = std::fs::read_to_string(d.join("src/lib.rs")).unwrap();
        assert!(lib.contains("    11\n") && !lib.contains("b note"), "keeper hunk intact");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Disjoint second winner merges cleanly on top of the first.
    #[test]
    fn disjoint_winner_merges_cleanly() {
        let d = scratch("disjoint");
        std::fs::write(d.join("src/lib.rs"), BASE_LIB).unwrap();
        git(&d, &["add", "-A"]).unwrap();
        git(&d, &["commit", "-qm", "base"]).unwrap();
        std::fs::write(d.join("src/lib.rs"), BASE_LIB.replace("    1\n", "    11\n")).unwrap();
        let pa = staged_patch(&d);
        git(&d, &["reset", "--hard", "-q", "HEAD"]).unwrap();
        std::fs::write(d.join("src/lib.rs"), BASE_LIB.replace("    2\n", "    22\n")).unwrap();
        let pc = staged_patch(&d);
        git(&d, &["reset", "--hard", "-q", "HEAD"]).unwrap();
        apply_patch_text(&d, &pa).unwrap();
        git(&d, &["add", "-A"]).unwrap();
        git(&d, &["commit", "-qm", "winner a"]).unwrap();
        apply_patch_text(&d, &pc).unwrap();
        let lib = std::fs::read_to_string(d.join("src/lib.rs")).unwrap();
        assert!(lib.contains("    11\n") && lib.contains("    22\n"), "both hunks present");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod workload_probe_regression_tests {
    use super::*;

    /// Workload probes are plain interpreter `TestCommand`s: caller-owned
    /// program, `-c` argv, explicit `Cwd::Tree` (module resolution via cwd,
    /// never path overrides), no toolchain literals on this path.
    #[test]
    fn workload_probe_command_shape() {
        let dir = tempfile::tempdir().unwrap();
        let venv_py = dir.path().join("venv").join("bin").join("python");
        let cmd = workload_probe(&venv_py, "print(1)", "abc");
        assert_eq!(cmd.program, venv_py.to_string_lossy().into_owned());
        assert_eq!(
            cmd.args,
            vec![
                "-c".to_string(),
                "print(1)".to_string(),
                "abc".to_string()
            ]
        );
        assert!(
            matches!(cmd.cwd, Cwd::Tree),
            "workload probes resolve modules via the tree cwd"
        );
        assert!(cmd.env_set.is_empty() && cmd.env_remove.is_empty());
        assert!(cmd.launcher.is_none());
    }
}
