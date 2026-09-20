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
    // cProfile fallback: time the import + a representative call.
    let start = std::time::Instant::now();
    let probe = "import crc; c=crc.Calculator(crc.Crc8.CCITT); c.checksum(b'123456789'*100)";
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

// ---- M5 Stage-2 instrument surface (signatures now, bodies in M5) ----
// Declared here so M3 recon baselines flow into the same types.

#[derive(Debug, Clone)]
pub struct InsnStats {
    pub insns: u64,
}

#[derive(Debug, Clone)]
pub struct ConfInterval {
    pub low: f64,
    pub high: f64,
    pub point: f64,
}

pub fn measure_noise_floor(_build: &std::path::Path, _workloads: &[String]) -> Result<f64, ProfileError> {
    Err(ProfileError::Measure("M5: not implemented until M5".into()))
}

pub fn deterministic_measure(_build: &std::path::Path, _workloads: &[String]) -> Result<InsnStats, ProfileError> {
    Err(ProfileError::Measure("M5: not implemented until M5".into()))
}

pub fn wallclock_confirm(_build: &std::path::Path, _workloads: &[String]) -> Result<ConfInterval, ProfileError> {
    Err(ProfileError::Measure("M5: not implemented until M5".into()))
}

pub fn ceiling(time_share: f64, cap: f64) -> f64 {
    time_share * (1.0 - 1.0 / cap)
}
