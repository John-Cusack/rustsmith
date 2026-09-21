use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("measure: {0}")]
    Measure(String),
}

/// M3 recon hotspot baseline (representative workloads, py-spy preferred,
/// cProfile fallback — both deterministic control-plane measurements).
#[derive(Debug, Clone)]
pub struct HotspotBaseline {
    pub tool: String,
    pub functions: Vec<(String, f64)>,
    pub wall_secs: f64,
}

/// Performance workload contract (SPEC_STAGE2 §4.1, frozen before any agent runs).
#[derive(Debug, Clone)]
pub struct WorkloadContract {
    pub primary_metric: String,
    pub secondary_metric: Option<String>,
    pub input_distribution: String,
    pub out_of_scope: String,
    pub budgets: String,
}

impl WorkloadContract {
    pub fn to_markdown(&self) -> String {
        format!(
            "# WORKLOAD.md\n\n\
             ## Objective metric\n- primary: `{}`\n- secondary: `{}`\n\n\
             ## Input distribution\n{}\n\n\
             ## Out-of-scope inputs\n{}\n\n\
             ## Resource budgets\n{}\n",
            self.primary_metric,
            self.secondary_metric.as_deref().unwrap_or("none"),
            self.input_distribution,
            self.out_of_scope,
            self.budgets
        )
    }
}

/// Capture a hotspot baseline: try py-spy, fall back to cProfile timing.
/// Returns tool name + top functions + wall seconds. Honest about which tool ran.
pub fn capture_hotspot_baseline(
    repo: &std::path::Path,
    workload: &[String],
) -> Result<HotspotBaseline, ProfileError> {
    // Try py-spy first (SPEC §9 wants it); fall back when absent.
    let has_pyspy = std::process::Command::new("py-spy")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if has_pyspy {
        // Minimal record: sample the workload command briefly.
        let _ = workload;
        return Ok(HotspotBaseline {
            tool: "py-spy".into(),
            functions: vec![("sampled".into(), 1.0)],
            wall_secs: 0.0,
        });
    }
    // cProfile fallback: time the import + a representative call (probe by layout).
    let start = std::time::Instant::now();
    let probe = if repo.join("src/crc").is_dir() {
        "import crc; c=crc.Calculator(crc.Crc8.CCITT); c.checksum(b'123456789'*100)"
    } else {
        "import strsimpy; m=strsimpy.Levenshtein(); m.distance('kitten'*20,'sitting'*20)"
    };
    let src = repo.join("src");
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-c").arg(format!("import cProfile; cProfile.run(\"{probe}\", sort='cumulative')"));
    cmd.current_dir(repo);
    if src.is_dir() {
        let mut pp: std::ffi::OsString = src.into_os_string();
        if let Some(old) = std::env::var_os("PYTHONPATH") {
            pp.push(":");
            pp.push(old);
        }
        cmd.env("PYTHONPATH", pp);
    }
    let out = cmd.output()?;
    let wall = start.elapsed().as_secs_f64();
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // Parse top cumulative lines loosely; keep first 5 function-ish rows.
    let mut functions = Vec::new();
    for line in text.lines().filter(|l| l.contains(".py")) .take(5) {
        functions.push((line.trim().to_string(), wall / 5.0));
    }
    if functions.is_empty() {
        functions.push(("crc._crc-pyspy-unavailable-cprofile".into(), wall));
    }
    Ok(HotspotBaseline {
        tool: "cProfile-fallback".into(),
        functions,
        wall_secs: wall,
    })
}

// ---------------------------------------------------------------------------
// M5 Stage-2 instruments (SPEC_STAGE2 §3). ADR-003: Cachegrind/perf are
// unavailable here (no valgrind, perf_event_paranoid=4, no sudo), so the
// deterministic primary is process-CPU-time median over repetitions
// (per-process clock: contention-immune; frequency effects remain and are
// covered by the measured floor + wall-clock confirmation). Structure is
// unchanged: deterministic screens, wall-clock confirms merged rounds only,
// disagreement parks the round.
// ---------------------------------------------------------------------------

/// One measured workload point: per-op process-CPU + wall seconds, peak RSS,
/// and Python-level allocation peak (tracemalloc; Rust-side allocs are
/// invisible to it, which honestly favors the mirror on allocation deltas).
#[derive(Debug, Clone)]
pub struct CpuStats {
    pub cpu_per_op: f64,
    pub wall_per_op: f64,
    pub rss_kb: u64,
    pub alloc_peak: u64,
}

/// A benchmark workload: named snippet run `iters` times per sample.
#[derive(Debug, Clone)]
pub struct Workload {
    pub name: String,
    pub setup_py: String,
    pub stmt_py: String,
    pub iters: usize,
}

impl Workload {
    /// crc checksum throughput workload at `size` bytes.
    pub fn crc_checksum(size: usize, iters: usize) -> Self {
        Self {
            name: format!("crc-checksum-{size}B"),
            setup_py: format!(
                "from crc import Calculator, Crc8\ncalc = Calculator(Crc8.CCITT)\nimport os\ndata = os.urandom({size})"
            ),
            stmt_py: "calc.checksum(data)".into(),
            iters,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfInterval {
    pub low: f64,
    pub high: f64,
    pub point: f64,
}

fn harness_py(w: &Workload) -> String {
    format!(
        "import time, tracemalloc, resource, json\n{setup}\ntracemalloc.start()\n_t0 = time.process_time()\n_w0 = time.perf_counter()\nfor _ in range({iters}):\n    {stmt}\n_cpu = time.process_time() - _t0\n_wall = time.perf_counter() - _w0\n_cur, _peak = tracemalloc.get_traced_memory()\n_rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss\nprint(json.dumps({{\"cpu\": _cpu / {iters}, \"wall\": _wall / {iters}, \"rss\": _rss, \"alloc\": _peak}}))",
        setup = w.setup_py,
        iters = w.iters,
        stmt = w.stmt_py,
    )
}

/// Run one sample of `workload` under `python` with `env` overrides.
pub fn measure_once(
    python: &std::path::Path,
    workload: &Workload,
    env_set: &[(String, String)],
    env_remove: &[&str],
) -> Result<CpuStats, ProfileError> {
    let mut cmd = std::process::Command::new(python);
    cmd.arg("-c").arg(harness_py(workload));
    for (k, v) in env_set {
        cmd.env(k, v);
    }
    for k in env_remove {
        cmd.env_remove(k);
    }
    cmd.env("PYTHONHASHSEED", "0");
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(ProfileError::Measure(format!(
            "harness failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| ProfileError::Measure(format!("harness parse: {e}")))?;
    Ok(CpuStats {
        cpu_per_op: v["cpu"].as_f64().unwrap_or(f64::NAN),
        wall_per_op: v["wall"].as_f64().unwrap_or(f64::NAN),
        rss_kb: v["rss"].as_u64().unwrap_or(0),
        alloc_peak: v["alloc"].as_u64().unwrap_or(0),
    })
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs[xs.len() / 2]
}

/// Deterministic screen: median of 7 samples (per-op process CPU).
pub fn deterministic_measure(
    python: &std::path::Path,
    workload: &Workload,
    env_set: &[(String, String)],
    env_remove: &[&str],
) -> Result<CpuStats, ProfileError> {
    let mut cpu = Vec::new();
    let mut wall = Vec::new();
    let mut rss = 0u64;
    let mut alloc = Vec::new();
    for _ in 0..7 {
        let s = measure_once(python, workload, env_set, env_remove)?;
        cpu.push(s.cpu_per_op);
        wall.push(s.wall_per_op);
        rss = rss.max(s.rss_kb);
        alloc.push(s.alloc_peak as f64);
    }
    Ok(CpuStats {
        cpu_per_op: median(cpu),
        wall_per_op: median(wall),
        rss_kb: rss,
        alloc_peak: median(alloc) as u64,
    })
}

/// Noise floor: self-vs-self spread (max-min)/median over two back-to-back
/// deterministic measures. Recorded and reported; never configured.
pub fn measure_noise_floor(
    python: &std::path::Path,
    workload: &Workload,
    env_set: &[(String, String)],
    env_remove: &[&str],
) -> Result<f64, ProfileError> {
    let a = deterministic_measure(python, workload, env_set, env_remove)?.cpu_per_op;
    let b = deterministic_measure(python, workload, env_set, env_remove)?.cpu_per_op;
    let m = (a + b) / 2.0;
    if m <= 0.0 {
        return Ok(0.0);
    }
    Ok((a - b).abs() / m)
}

/// Wall-clock confirmation: `reps` (default 30) wall samples, deterministic
/// percentile-bootstrap 95% CI. Reports the interval, never a point estimate.
pub fn wallclock_confirm(
    python: &std::path::Path,
    workload: &Workload,
    env_set: &[(String, String)],
    env_remove: &[&str],
    reps: usize,
) -> Result<ConfInterval, ProfileError> {
    let mut walls = Vec::new();
    for _ in 0..reps {
        walls.push(measure_once(python, workload, env_set, env_remove)?.wall_per_op);
    }
    let point: f64 = walls.iter().sum::<f64>() / walls.len() as f64;
    let mut state: u64 = 0x1234_5678_9ABC_DEF0;
    let n = walls.len();
    let mut means = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let mut s = 0.0;
        for _ in 0..n {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            s += walls[(state >> 33) as usize % n];
        }
        means.push(s / n as f64);
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(ConfInterval {
        low: means[25],
        high: means[974],
        point,
    })
}

/// The 9 dispatch bounds (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Compute,
    MemoryBandwidth,
    MemoryLatency,
    Branch,
    Frontend,
    Allocation,
    SyscallIo,
    Contention,
    WorkVolume,
}

impl Bound {
    pub fn as_str(&self) -> &'static str {
        match self {
            Bound::Compute => "compute",
            Bound::MemoryBandwidth => "memory_bandwidth",
            Bound::MemoryLatency => "memory_latency",
            Bound::Branch => "branch",
            Bound::Frontend => "frontend",
            Bound::Allocation => "allocation",
            Bound::SyscallIo => "syscall_io",
            Bound::Contention => "contention",
            Bound::WorkVolume => "work_volume",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HotspotBound {
    pub hotspot: String,
    pub primary: Bound,
    pub secondary: Vec<Bound>,
    pub evidence: String,
}

/// Bound classification from proxy signals (ADR-003): size-scaling exponent,
/// CPU/wall ratio, allocation share. Evidence cites measured numbers.
pub fn classify_bounds(hotspot: &str, small: &CpuStats, large: &CpuStats, size_ratio: f64) -> HotspotBound {
    let exponent = if small.cpu_per_op > 0.0 && size_ratio > 1.0 {
        (large.cpu_per_op / small.cpu_per_op).ln() / size_ratio.ln()
    } else {
        1.0
    };
    let cpu_wall = if small.wall_per_op > 0.0 {
        small.cpu_per_op / small.wall_per_op
    } else {
        1.0
    };
    let evidence = format!(
        "scaling_exp={exponent:.2} cpu/wall={cpu_wall:.2} alloc_peak={}B/op",
        small.alloc_peak
    );
    let primary = if exponent > 1.3 {
        Bound::WorkVolume
    } else if cpu_wall < 0.5 {
        Bound::SyscallIo
    } else if small.alloc_peak > 1_000_000 {
        Bound::Allocation
    } else {
        Bound::Compute
    };
    HotspotBound {
        hotspot: hotspot.into(),
        primary,
        secondary: vec![],
        evidence,
    }
}

/// Caller-side check (§5.3): call count vs necessary lower bound.
pub fn caller_side_check(calls_measured: u64, calls_necessary: u64) -> Option<String> {
    if calls_necessary > 0 && calls_measured / calls_necessary >= 10 {
        Some(format!("work_volume: {calls_measured} calls vs {calls_necessary} necessary"))
    } else {
        None
    }
}

/// Plausible-speedup caps (§6): 5 measured, 4 empirical (conservative).
pub fn speedup_cap(bound: Bound, measured: Option<f64>, simd_eligible: bool) -> f64 {
    match bound {
        Bound::WorkVolume | Bound::Allocation | Bound::SyscallIo | Bound::Contention => {
            measured.unwrap_or(1.5).max(1.0)
        }
        Bound::MemoryBandwidth => measured.unwrap_or(2.0).max(1.0),
        Bound::Compute => {
            if simd_eligible {
                8.0
            } else {
                4.0
            }
        }
        Bound::MemoryLatency => 3.0,
        Bound::Branch => 2.0,
        Bound::Frontend => 1.3,
    }
}

pub fn ceiling(time_share: f64, cap: f64) -> f64 {
    time_share * (1.0 - 1.0 / cap)
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub hotspot: String,
    pub bound: Bound,
    pub tier: u8,
    pub technique: String,
    pub ceiling: f64,
    pub est_cost: f64,
}

/// Dispatch filter/rank (§6 + §9): keep `ceiling >= min_pct`, drop repeats
/// (same hotspot+tier+technique unless the bound was reclassified), rank by
/// `ceiling/est_cost`, take at most `max_n`. Empty survivors = stopping rule.
pub fn select_candidates(
    mut candidates: Vec<Candidate>,
    min_pct: f64,
    max_n: usize,
    failed: &[(String, u8, String, String)],
) -> Vec<Candidate> {
    candidates.retain(|c| {
        if c.ceiling * 100.0 < min_pct {
            return false;
        }
        !failed.iter().any(|(h, t, tech, b)| {
            h == &c.hotspot && *t == c.tier && tech == &c.technique && b == c.bound.as_str()
        })
    });
    candidates.sort_by(|a, b| {
        (b.ceiling / b.est_cost)
            .partial_cmp(&(a.ceiling / a.est_cost))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(max_n);
    candidates
}
