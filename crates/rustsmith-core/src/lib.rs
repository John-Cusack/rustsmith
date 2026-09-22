use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

// --- S1: polyglot composite adapter core types (ADR-008) ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Cwd {
    Tree,
    BuildDir,
    Rel(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Launcher {
    pub program: String,
    pub np_flag: String,
    pub np: u16,
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Cwd,
    pub env_set: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    pub launcher: Option<Launcher>,
    pub timeout_secs: Option<u32>,
    pub collect: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub artifacts: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    Skip,
    XFail,
    NotRun,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservableSpec {
    pub test_glob: String,
    /// "stdout" | "stderr" | artifact glob.
    pub source: String,
    pub regex: String,
    pub rel_tol: f64,
    pub abs_tol: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    pub test_id: String,
    pub key: String,
}

/// Current frozen manifest schema version.
pub const MANIFEST_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub version: u32,
    pub runner: String,
    pub languages: Vec<String>,
    pub prepare: Vec<TestCommand>,
    pub invocation: Vec<TestCommand>,
    pub config_hash: String,
    pub observables: Vec<ObservableSpec>,
    pub files: Vec<FileHash>,
    pub baseline: Baseline,
}

/// Frozen manifest schema v1: `invocation` was free-form shell strings.
/// Still parsed so old fixtures validate; upgrade with [`Manifest::from_v1`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestV1 {
    pub version: u32,
    pub invocation: Vec<String>,
    pub files: Vec<FileHash>,
    pub baseline: Baseline,
}

impl Manifest {
    /// Upgrade a v1 manifest. Each free-form v1 invocation string becomes
    /// `sh -c "<string>"` run from the tree root, preserving shell semantics
    /// (PATH lookup, quoting). The original `version` is preserved so readers
    /// can tell an upgraded manifest from a native v2 one.
    pub fn from_v1(v1: ManifestV1) -> Self {
        Manifest {
            version: v1.version,
            runner: String::new(),
            languages: Vec::new(),
            prepare: Vec::new(),
            invocation: v1
                .invocation
                .into_iter()
                .map(|s| TestCommand {
                    program: "sh".to_string(),
                    args: vec!["-c".to_string(), s],
                    cwd: Cwd::Tree,
                    env_set: Vec::new(),
                    env_remove: Vec::new(),
                    launcher: None,
                    timeout_secs: None,
                    collect: Vec::new(),
                })
                .collect(),
            config_hash: String::new(),
            observables: Vec::new(),
            files: v1.files,
            baseline: v1.baseline,
        }
    }
}

/// Parse frozen manifest JSON, accepting v2 and upgrading v1.
/// Returns the v2-shaped parse error when neither schema matches.
pub fn parse_manifest_json(text: &str) -> Result<Manifest, serde_json::Error> {
    match serde_json::from_str::<Manifest>(text) {
        Ok(m) => Ok(m),
        Err(v2_err) => match serde_json::from_str::<ManifestV1>(text) {
            Ok(v1) => Ok(Manifest::from_v1(v1)),
            Err(_) => Err(v2_err),
        },
    }
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
    /// Per-test outcomes. Counts above are derived from this map;
    /// `deselected` has no [`Outcome`] variant and stays count-only.
    #[serde(default)]
    pub outcomes: BTreeMap<String, Outcome>,
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
    /// Number of tests with the given outcome.
    pub fn count(&self, outcome: Outcome) -> usize {
        self.outcomes.values().filter(|o| **o == outcome).count()
    }
    /// Total tests recorded in the outcomes map.
    pub fn total_outcomes(&self) -> usize {
        self.outcomes.len()
    }
    /// Recompute `passed`/`failed`/`skipped`/`xfailed` from `outcomes`.
    /// `NotRun` and `Timeout` entries have no legacy counter and are visible
    /// only via [`GradedResult::count`].
    pub fn sync_counts_from_outcomes(&mut self) {
        self.passed = self.count(Outcome::Pass) as u32;
        self.failed = (self.count(Outcome::Fail) + self.count(Outcome::Timeout)) as u32;
        self.skipped = self
            .outcomes
            .iter()
            .filter(|(_, o)| **o == Outcome::Skip)
            .map(|(id, _)| id.clone())
            .collect();
        self.skipped.sort();
        self.xfailed = self
            .outcomes
            .iter()
            .filter(|(_, o)| **o == Outcome::XFail)
            .map(|(id, _)| id.clone())
            .collect();
        self.xfailed.sort();
    }
    /// Build a result from per-test outcomes, deriving the legacy counters.
    pub fn from_outcomes(
        exit_code: i32,
        outcomes: BTreeMap<String, Outcome>,
        stdout: String,
    ) -> Self {
        let mut got = GradedResult {
            exit_code,
            passed: 0,
            failed: 0,
            skipped: Vec::new(),
            xfailed: Vec::new(),
            deselected: Vec::new(),
            stdout,
            outcomes,
        };
        got.sync_counts_from_outcomes();
        got
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OracleKind {
    Spec,
    Fixture,
    /// Code compiled against the SUT.
    Harness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OracleFile {
    pub path: String,
    pub kind: OracleKind,
}
/// Grading container image: base image, extra packages, and tree paths
/// the container may write (e.g. build trees recording test outputs).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageSpec {
    pub base: String,
    pub packages: Vec<String>,
    pub writable: Vec<String>,
}
/// A benchmark workload: named snippet run `iters` times per sample.
/// Shared by `Profiler` (setup/timed/hotspots) and `RepoFacts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workload {
    pub name: String,
    pub setup: String,
    pub stmt: String,
    pub iters: usize,
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(String),
}

#[derive(Debug)]
pub struct BuildCtx<'a> {
    pub tree: &'a Path,
    pub build_dir: &'a Path,
    pub release: bool,
}

pub trait TestRunner: Send + Sync {
    fn id(&self) -> &'static str;
    fn invocation(&self, cx: &BuildCtx) -> Vec<TestCommand>;
    fn grade(&self, runs: &[RunOutput]) -> Result<GradedResult, AdapterError>;
    fn observe(&self, runs: &[RunOutput], specs: &[ObservableSpec]) -> Vec<Observation>;
    fn oracle_files(&self, repo: &Path) -> Result<Vec<OracleFile>, AdapterError>;
    fn normalize_for_hash(&self, rel: &str, bytes: &[u8]) -> Option<Vec<u8>>;
    fn heldout(&self, suite: &Path, cx: &BuildCtx) -> Vec<TestCommand>;
    fn config_hash(&self, cx: &BuildCtx) -> Result<String, AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_command() -> TestCommand {
        TestCommand {
            program: "ctest".to_string(),
            args: vec!["-L".to_string(), "quick".to_string()],
            cwd: Cwd::BuildDir,
            env_set: vec![("CTEST_OUTPUT_ON_FAILURE".to_string(), "1".to_string())],
            env_remove: Vec::new(),
            launcher: Some(Launcher {
                program: "mpiexec".to_string(),
                np_flag: "-n".to_string(),
                np: 8,
                extra: Vec::new(),
            }),
            timeout_secs: Some(300),
            collect: vec!["Testing/**/*.xml".to_string()],
        }
    }

    #[test]
    fn s1_types_construct_and_manifest_v2_round_trips() {
        let cmd = sample_command();
        let run = RunOutput {
            exit_code: 0,
            stdout: "100% tests passed".to_string(),
            stderr: String::new(),
            artifacts: BTreeMap::from([(
                "Testing/junit.xml".to_string(),
                b"<xml/>".to_vec(),
            )]),
        };
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            runner: "ctest".to_string(),
            languages: vec!["cxx".to_string(), "fortran".to_string()],
            prepare: vec![cmd.clone()],
            invocation: vec![cmd],
            config_hash: "abc123".to_string(),
            observables: vec![ObservableSpec {
                test_glob: "*".to_string(),
                source: "stdout".to_string(),
                regex: r"norm\s*=\s*(?P<value>[0-9.eE+-]+)".to_string(),
                rel_tol: 1e-6,
                abs_tol: 1e-12,
            }],
            files: vec![FileHash {
                path: Utf8PathBuf::from("tests/runtest.cmake"),
                sha256: "deadbeef".to_string(),
            }],
            baseline: Baseline {
                test_count: 1,
                skipped: Vec::new(),
                xfailed: Vec::new(),
                deselected: Vec::new(),
            },
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, manifest);

        let graded = GradedResult::from_outcomes(
            run.exit_code,
            BTreeMap::from([
                ("t1".to_string(), Outcome::Pass),
                ("t2".to_string(), Outcome::Timeout),
            ]),
            run.stdout.clone(),
        );
        assert_eq!(graded.passed, 1);
        assert_eq!(graded.failed, 1);
        assert_eq!(graded.count(Outcome::Pass), 1);
        assert_eq!(graded.total_outcomes(), 2);
    }

    #[test]
    fn v1_manifest_still_parses() {
        let v1 = r#"{
            "version": 1,
            "invocation": ["run_tests test/unit", "run_tests test/integration"],
            "files": [{"path": "test/test_x.py", "sha256": "abc"}],
            "baseline": {"test_count": 2, "skipped": [], "xfailed": [], "deselected": []}
        }"#;
        // Raw v1 shape parses.
        let raw: ManifestV1 = serde_json::from_str(v1).unwrap();
        assert_eq!(raw.invocation.len(), 2);
        // Unified entry point upgrades it.
        let upgraded = parse_manifest_json(v1).unwrap();
        assert_eq!(upgraded.version, 1);
        assert_eq!(upgraded.invocation.len(), 2);
        assert_eq!(upgraded.invocation[0].program, "sh");
        assert_eq!(
            upgraded.invocation[0].args,
            vec!["-c".to_string(), "run_tests test/unit".to_string()]
        );
        assert_eq!(upgraded.files.len(), 1);
        // Old GradedResult JSON without `outcomes` still parses via default.
        let old_graded = r#"{"exit_code":0,"passed":2,"failed":0,"skipped":[],"xfailed":[],"deselected":[],"stdout":""}"#;
        let graded: GradedResult = serde_json::from_str(old_graded).unwrap();
        assert_eq!(graded.passed, 2);
        assert!(graded.outcomes.is_empty());
    }
}
