# ADR-008: polyglot composite adapter (accepts ADR-007 with review changes)

Supersedes ADR-007 (proposal) and ADR-004 (fixture dispatch).
Status: accepted. Review: `docs/adr/007-polyglot-composite-adapter.review.md`
(accept-with-changes; all §9 boxes applied below).

## Context

Same as ADR-007: `trait Adapter` is language-neutral in name but every
execution path spawns `python3 -m pytest` / maturin / py-spy directly
(`recon.rs:17`, `oracle:269-365`, `sandbox:198-223`, `mirror:124-226`,
`optimize:281-579`, `profile:49-200`). Requirement: N languages per repo
(Elmer FEM tri-language CMake+CTest forcing case), zero repo-specific rules
in `src/`, scale to ~7.

## Decision

Parse per-language (generic `Frontend`s), decide per-repo (one spine:
`TestRunner` + `BuildBridge` + `Profiler`), assemble per-repo
(`CompositeAdapter`). Stages touch only the composite.

```rust
// --- rustsmith-core (leaf; oracle/sandbox/gates already depend on it) ---
pub enum Cwd { Tree, BuildDir, Rel(String) }
pub struct Launcher { pub program: String, pub np_flag: String, pub np: u16, pub extra: Vec<String> }
pub struct TestCommand { pub program: String, pub args: Vec<String>, pub cwd: Cwd,
    pub env_set: Vec<(String, String)>, pub env_remove: Vec<String>,
    pub launcher: Option<Launcher>, pub timeout_secs: Option<u32>, pub collect: Vec<String> }
pub struct RunOutput { pub exit_code: i32, pub stdout: String, pub stderr: String,
    pub artifacts: BTreeMap<String, Vec<u8>> }
pub enum Outcome { Pass, Fail, Skip, XFail, NotRun, Timeout }
pub struct ObservableSpec { pub test_glob: String, pub source: String, pub regex: String,
    pub rel_tol: f64, pub abs_tol: f64 }
pub struct Observation { pub test_id: String, pub key: String, pub value: f64 }
pub struct Manifest { pub version: u32 /* 2 */, pub runner: String, pub languages: Vec<String>,
    pub prepare: Vec<TestCommand>, pub invocation: Vec<TestCommand>, pub config_hash: String,
    pub observables: Vec<ObservableSpec>, pub files: Vec<FileHash>, pub baseline: Baseline }
// GradedResult gains outcomes: BTreeMap<String, Outcome>; counts derived from it.
pub enum OracleKind { Spec, Fixture, Harness }
pub struct OracleFile { pub path: String, pub kind: OracleKind }
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

// --- rustsmith-adapters: identity + frontends + spine + composite ---
pub struct UnitId(pub String); // "<lang>:<repo-rel authoritative source>[#<symbol>]"
pub enum Abi { C, Fortran { bind_c: bool }, Cxx, Python }
pub struct Symbol { pub linkage: String, pub abi: Abi }
pub struct UnitDecl { pub id: UnitId, pub files: Vec<String>, pub generated_from: Option<String>,
    pub exports: Vec<Symbol>, pub imports: Vec<Symbol> }
pub struct Fragment { pub units: Vec<UnitDecl>, pub edges: Vec<(UnitId, UnitId)>, pub diagnostics: Vec<String> }
pub struct FragmentCtx<'a> { pub repo: &'a Path, pub files: &'a [PathBuf],
    pub compile_db: Option<&'a Path>, pub compiler_ids: &'a BTreeMap<String, String> }
pub trait Frontend: Send + Sync {
    fn id(&self) -> &'static str;
    fn claims(&self, file: &Path, compile_db: Option<&Path>) -> bool;
    fn fragment(&self, cx: &FragmentCtx) -> Result<Fragment, AdapterError>;
    fn language_rules(&self) -> Vec<Rule>; // ABI/layout only; repo API rules live in RepoFacts
}
pub struct BuildCtx<'a> { pub tree: &'a Path, pub build_dir: &'a Path, pub release: bool }
pub trait BuildBridge: Send + Sync {
    fn prepare(&self, cx: &BuildCtx) -> Vec<TestCommand>; // ordered, fail-fast
    fn build(&self, cx: &BuildCtx) -> Vec<TestCommand>;
    fn substitute(&self, cx: &BuildCtx, unit: &UnitDecl, rust_lib: &Path)
        -> Result<Vec<TestCommand>, AdapterError>;
    fn scaffold(&self, unit: &UnitDecl) -> Result<Vec<(String, String)>, AdapterError>;
    fn link_deps(&self, cx: &BuildCtx) -> Result<Vec<LinkDep>, AdapterError>;
    fn artifacts(&self, cx: &BuildCtx) -> Vec<PathBuf>;
}
pub trait Profiler: Send + Sync {
    fn setup(&self, w: &Workload) -> Vec<TestCommand>;
    fn timed(&self, w: &Workload) -> TestCommand; // core measures via wait4 rusage
    fn hotspots(&self, w: &Workload, perf_available: bool) -> Result<HotspotBaseline, AdapterError>;
}
pub struct Workload { pub name: String, pub setup: String, pub stmt: String, pub iters: usize }
impl CompositeAdapter {
    pub fn partition(&self, cx: &BuildCtx) -> Result<(UnitDag, Vec<String>), AdapterError>;
    pub fn classify_dep(&self, dep: &LinkDep) -> DepClass;
    pub fn license_terms(&self, repo: &Path) -> Result<Vec<(String, Attribution)>, AdapterError>;
    pub fn image(&self) -> ImageSpec;
}
pub struct RepoFacts { pub workloads: Vec<Workload>, pub heldout_workloads: Vec<Workload>,
    pub differential_probes: Vec<TestCommand>, pub observables: Vec<ObservableSpec>,
    pub api_surface: Vec<String>, pub porting_rules: Vec<Rule>,
    pub attribution: Vec<(String, Attribution)> }
```

Canonical IDs: `<lang>:<repo-rel authoritative pre-generation source>[#<symbol>]`;
Fortran symbols lowercased; linkage names in `Symbol`, not the ID. Example:
`python:src/crc/_crc.py`. Recon order: probe → `bridge.prepare` → codegen
build → fragments. Unknown extensions warn into `ProbeReport{unclaimed,
unclaimed_share}`; halt above config threshold. `partition()` returns
diagnostics; non-`BIND(C)` Fortran units with unstable ABI coarsen
granularity. Substitution model: Rust builds as staticlib/cdylib exporting
original linkage names, spliced in place of unit objects, original suite rerun
(Python PyO3 flow is the same shape). Profiler reconciles ADR-003: no perf
dependency; `CpuStats.alloc_peak` and `ResourceSnapshot.alloc_count` become
`Option<u64>`. `RepoFacts` (`recon/facts.json`): deterministic probe writes
`facts.probe`, Architect appends `facts.rules`, both hashed into Manifest v2;
replaces `FixtureKind` everywhere; `config/fixture.toml` stays as test data.
xUnit repos use pass/fail; norm-diff repos add frozen `ObservableSpec`
tolerances (SUT-written verdicts are cross-checks only). Frozen `recon.json`
key `build.language` renames with the UnitId rollout.

## Migration (revised order, atomic groups)

1. Core types: `TestCommand`/`RunOutput`/`Outcome`/`ObservableSpec`/`Manifest
   v2` + v1 reader + `TestRunner` in `rustsmith-core`.
2. Adapters: `Frontend`/`BuildBridge`/`Profiler`/`CompositeAdapter`, UnitId
   spec, probe. Re-express Python as frontend+runner+bridge+profiler. Gate:
   byte-identical `recon/dag/manifest.json` for crc+strsimpy ignoring v2 fields.
3. Atomic: oracle+sandbox+recon manifest writer through runner;
   `oracle_integrity` → `outcomes` sets; parameterized image (writable mounts).
4. `RepoFacts` schema + Stage-0 authoring + manifest hashing; remove
   `FixtureKind` consumers; ADR-004 superseded (this file).
5. Atomic: UnitId rollout to `dag.json`/`recon.json`/`scaffold()` (ex-`template.json`).
6. mirror+optimize through `bridge.substitute`/`runner`/`profiler`; delete
   maturin/venv paths; `alloc_peak: Option`.
7. Content: `language_rules` per frontend, repo rules in RepoFacts,
   runner-owned held-outs; delete `fixture.rs`.
8. Second frontend (Fortran or C via compile DB) + CMake/CTest spine against
   the pinned tree. Grading holes closed: empty held-out halts, absolute binary
   pins, host fallback refused for non-Python runners, coverage-gated units.
