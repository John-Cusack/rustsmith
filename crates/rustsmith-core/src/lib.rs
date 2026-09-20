use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: u32,
    pub invocation: Vec<String>,
    pub files: Vec<FileHash>,
    pub baseline: Baseline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileHash {
    pub path: Utf8PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Baseline {
    pub test_count: u32,
    pub skipped: Vec<String>,
    pub xfailed: Vec<String>,
    pub deselected: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GradedResult {
    pub exit_code: i32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: Vec<String>,
    pub xfailed: Vec<String>,
    pub deselected: Vec<String>,
    pub stdout: String,
}

impl GradedResult {
    pub fn total(&self) -> u32 {
        self.passed + self.failed + self.skipped.len() as u32 + self.xfailed.len() as u32
    }
    pub fn pass_rate(&self) -> f64 {
        let t = self.total();
        if t == 0 {
            return 1.0;
        }
        self.passed as f64 / t as f64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HaltReason {
    OracleTamper { path: String },
    SkipListMismatch { diff: String },
    HeldoutLeak,
    DivergenceOverThreshold { divergence: f64 },
}

impl std::fmt::Display for HaltReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HaltReason::OracleTamper { path } => write!(f, "oracle_tamper: {path}"),
            HaltReason::SkipListMismatch { diff } => write!(f, "skip_mismatch: {diff}"),
            HaltReason::HeldoutLeak => write!(f, "heldout_leak"),
            HaltReason::DivergenceOverThreshold { divergence } => {
                write!(f, "divergence_over_threshold: {divergence:.4}")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    OracleIntegrity,
    OracleParity,
    HeldoutDivergence,
    Differential,
    UnsafeBudget,
    Miri,
    Clippy,
    Benchmark,
    NoRegression,
    LoopDetector,
    Provenance,
    WorkloadDivergence,
    CausalAttribution,
    OptimizationScope,
}

impl Gate {
    pub fn as_str(&self) -> &'static str {
        match self {
            Gate::OracleIntegrity => "oracle_integrity",
            Gate::OracleParity => "oracle_parity",
            Gate::HeldoutDivergence => "heldout_divergence",
            Gate::Differential => "differential",
            Gate::UnsafeBudget => "unsafe_budget",
            Gate::Miri => "miri",
            Gate::Clippy => "clippy",
            Gate::Benchmark => "benchmark",
            Gate::NoRegression => "no_regression",
            Gate::LoopDetector => "loop_detector",
            Gate::Provenance => "provenance",
            Gate::WorkloadDivergence => "workload_divergence",
            Gate::CausalAttribution => "causal_attribution",
            Gate::OptimizationScope => "optimization_scope",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: i64,
    pub run_id: String,
    pub kind: String,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}
