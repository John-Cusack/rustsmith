# Polyglot composite adapter — design proposal (for review)

## Context

rustsmith ports source repos to Rust. Python is the only implemented adapter.
Audit verdict: **trait-ok-but-consumers-leak** — `trait Adapter`
(`crates/rustsmith-adapters/src/lib.rs:127-134`) is language-neutral in name,
but every execution path bypasses it: `cli/recon.rs:17` hardcodes
`let adapter = PythonAdapter`; `oracle/src/lib.rs:269-365`, `sandbox:198-223`,
`mirror:176-226`, `optimize:281-347` each spawn `python3 -m pytest` with
`PYTHONPATH`/`PY_COLORS` and parse `pytest -v` (4 parser copies); build assumes
maturin/venv (`mirror:124-172`, `optimize:503-579`); profile assumes
py-spy/cProfile/tracemalloc (`profile:49-200`); content is Python→Rust-only
(`porting.rs:8-272`, `heldout.rs:7-278`, `fixture.rs:42-74` allowlist);
containers pin `python:3.11-slim`. Reusable as-is: `config/default.toml`,
`guidance/optimize.md`, `agent`/`council`/`store`/`report`, core gate math.

Requirement: support repos with **N languages in one tree** (Elmer FEM:
Fortran+C+C++ tri-language CMake+CTest repo, `release-26.2.1 @ a19504a`, is the
forcing case), scaling to ~7, with **zero repo-specific rules in `src/`**
(repo facts live in `recon/*.json` data; code knows languages, build systems,
runners — never repo names).

## Proposal: parse per-language, decide per-repo

Two layers plus one assembly step. Extension dispatch at the leaves, single
spine per repo, composite as the only thing stages touch.

```rust
// LEAF — generic per language, extension-routed, no repo names
pub trait Frontend: Send + Sync {
    fn id(&self) -> &'static str;                     // "fortran" | "c" | "cxx" | "python"
    fn extensions(&self) -> &'static [&'static str];
    fn fragment(&self, files: &[PathBuf]) -> Result<CallGraph, AdapterError>;
    fn classify_dep(&self, dep: &str) -> DepClass;
    fn porting_rules(&self) -> String;
}
// SPINE — one per repo, selected by build/test stack, not by language
pub struct TestCommand { pub program: String, pub args: Vec<String>,
    pub env_set: Vec<(String,String)>, pub env_remove: Vec<String> }
pub trait TestRunner  { fn invocation(&self) -> Vec<TestCommand>;
    fn parse_output(&self, s: &str, code: i32) -> GradedResult;
    fn oracle_files(&self) -> Result<TestInventory, AdapterError>;
    fn config_hash(&self) -> String; }
pub trait BuildBridge { fn configure(&self) -> TestCommand; fn build(&self) -> TestCommand; }
pub trait Profiler    { fn hotspots(&self, w: &[String]) -> HotspotBaseline;
    fn harness(&self, w: &Workload) -> TestCommand; }
// COMPOSITE — per repo, assembled by probing
pub struct CompositeAdapter { pub languages: Vec<String>;
    pub frontends: Vec<Box<dyn Frontend>>; pub runner: Box<dyn TestRunner>;
    pub bridge: Box<dyn BuildBridge>; pub profiler: Box<dyn Profiler>; }
pub fn select_composite(repo: &Path) -> Result<CompositeAdapter, AdapterError>;
impl CompositeAdapter {
    fn call_graph(&self) -> CallGraph;  // union fragments + cross-edges
    fn partition(&self) -> UnitDag;     // replaces dag_from_call_graph:402
}
```

Selection by probing, never allowlists: extension census + `PROJECT()` langs +
`CTestTestfile.cmake` presence. Unknown extensions warn, never fail. `languages`
is a `Vec`, so 1-language and 7-language repos share every consumer path.
`BuildInfo.language: String` (`adapters:21`) becomes `languages: Vec<String>`.

## Reasoning

1. **Why split parse from decide.** File extensions route parsing well
   (`*.F90 → fparser2`, `*.c/cxx → clang compile-DB`) but fail at everything
   else: generated files lie about extension (`.src`→`.F90`); edges cross
   extensions (`BIND(C)`, `extern "C"`); build/test/profile are unified (one
   `cmake --build`, one `ctest`, one `perf record` on the mixed binary). So
   frontends parse fragments into a shared id namespace and the composite
   merges, builds, grades, and partitions once.
2. **Why one spine.** N-language repos still have one build system and one test
   spine. Per-language runners would need N parsers per graded run and N
   container images; a single CTest-shape runner keeps grading O(1) in N.
   Flags and thread pins are captured from `CMakeCache.txt`/compile DB and
   hashed in `config_hash()`, never literals — this is what removes
   Elmer-specifics (`-fallow-argument-mismatch`, `OMP_*=1`, `TEST.PASSED`
   become instances of generic mechanisms: captured flags, deterministic-env
   policy, discovered test artifacts).
3. **Why merge via symbol index, not pairwise interop.** N² language-pair
   handlers don't scale. Fragments emit canonical units plus declared
   cross-symbols; merge resolves through one global index. Cost is
   union+sort; `leaf_first_order` stays Kahn O(V+E).
4. **Why this fixes the leak.** Each hardcoded site maps to exactly one trait
   method: pytest spawns+parsers → `runner`; maturin/venv → `bridge`;
   py-spy/harness → `profiler`; rulebooks/held-outs → `frontend`+`runner`;
   `pyproject.toml` literal in `gates:372` → runner-owned frozen list;
   `fixture.rs:42-74` allowlist → deleted. New language = new `Frontend`
   registration. New stack = new `Bridge`/`Runner`. Zero consumer edits.
5. **Why Elmer-first.** A tri-language MPI CMake+CTest repo with codegen and
   norm-diff tests exercises every seam (mixed DAG, unified build, non-xUnit
   oracle, perf profiling). Single-language repos are strict subsets: same
   composite with one frontend.

## Alternatives considered

- **One adapter per repo** (e.g. `ElmerAdapter`): rejected — repo names in
  code, combinatorial explosion, violates the no-repo-rules requirement.
- **One adapter per language, repo picks one** (status quo extended):
  rejected — cannot express mixed trees; forces the caller to choose a
  primary and silently drops other languages.
- **Keep strings, add more strings** (`invocation: Vec<String>` extended):
  rejected — string-splitting (`skip(1)` on `"pytest …"`) is what created the
  leak; typed `TestCommand` is the fix.

## Migration (dependency order)

1. `adapters`: new traits + composite + probe (no consumer change yet).
2. `oracle`+`sandbox`: route through `runner` (deletes 2 parsers, `Pytest`
   error variants).
3. `recon`: `select_composite()` (deletes `baseline_counts`, `runtime_deps`,
   hardcoded `PythonAdapter`).
4. `mirror`+`optimize`: route through `bridge`+`runner`+`profiler` (deletes
   maturin/venv paths, remaining parsers).
5. `gates`+`profile` types: `frozen_config` list, `setup/stmt` rename.
6. `porting`/`heldout`/`fixture`/containers/prompts: per-frontend content,
   parameterized images, deleted allowlist.

## Scaling to 7 languages

Fragments compute in parallel (cap at `max_parallel_workers`), merge is
linear, spine stays singular. Rulebooks grow additively per frontend.

## Questions for the reviewer

1. Is `TestCommand` expressive enough for MPI launchers (`mpiexec -n 8 …`)
   and multi-step configures, or does the spine need a `Vec<TestCommand>`
   pipeline type?
2. Should `oracle_files()` return content hashes or paths — who owns the
   `TEST.PASSED`-class artifact semantics, runner or oracle?
3. Does the symbol-index merge need a canonical ID spec now, or can it wait
   until the second frontend lands?
4. Any objection to deleting `fixture.rs` dispatch (ADR-004) as part of step 6?
