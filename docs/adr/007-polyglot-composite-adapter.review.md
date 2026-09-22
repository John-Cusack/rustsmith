# Review of ADR-007 (polyglot composite adapter)

Staff-level accept/reject review of `docs/adr/007-polyglot-composite-adapter.md`,
written for the agent that revises the ADR or implements it.

- **Reviewed:** 2026-09-21, against `main @ 8adcef6` (ADR-007 untracked in the working tree).
- **Prompt:** `prompts/polyglot_review.md`.
- **Status:** accept-with-changes. The 5 blocking items (§3) must be resolved *in the ADR text* before migration step 1 merges.

## How to use this file

- Treat every **blocking** row in §3 as a required ADR change, and every **major** row as required before the step that touches it (§8 has the revised order).
- §4 holds the proposed signatures. Take them as a starting point: follow them unless the code shows a concrete reason not to, and record why when you don't.
- Claims marked **[INFERENCE]** were not verified against Elmer `release-26.2.1 @ a19504a`. Verify them before relying on them (§10).
- Path shorthand: `adapters`, `core`, `oracle`, `sandbox`, `gates`, `profile`, `harvest` = `crates/rustsmith-<x>/src/lib.rs`. `recon.rs`, `mirror.rs`, `optimize.rs`, `fixture.rs`, `porting.rs`, `heldout.rs`, `main.rs` = `crates/rustsmith-cli/src/<file>`. `ADR` = `docs/adr/007-polyglot-composite-adapter.md`.

## Input gap

The "Elmer FEM research findings" named in the review prompt do not exist on disk: nothing in the repo or under `/home/john` mentions `a19504a` or `release-26.2.1` apart from ADR:18-19 and the prompt itself. Elmer evidence below comes from the **Elmer 9.0 package installed on this host** (`dpkg`: `elmerfem-csc 9.0-0ppa0-202609101355~882b3e6a9~ubuntu24.04.1`). Anything assumed to carry over to 26.2.1 is marked [INFERENCE]. Running `prompts/cpp_fortran_deep_research.md` against the pinned repo would close this gap.

---

## 1. Verdict

**accept-with-changes.** The idea of parsing per language and deciding per repo is right. As written, though, the design can't mirror even one Elmer unit:

- BuildBridge has no contract for swapping a Rust unit into the original build.
- Unit IDs are still file stems.
- The frozen manifest can't store typed commands.
- The norm-diff oracle's pass signal is written by the code being ported.
- Repo-name dispatch (`FixtureKind`) survives the migration.

## 2. Strongest 3 points

1. **Typed `TestCommand` (ADR:39-40, 103-105) removes the root cause of the leak.** The split-on-spaces-and-drop-the-first-word pattern is duplicated at `oracle:307`, `sandbox:117`, `sandbox:177`, `mirror.rs:192` and `recon.rs:126`. The ADR is right to reject "more strings."
2. **The spine is chosen by build/test stack, not by language (ADR:38, 73-76).** One `ctest` grades all three Elmer languages, so the number of runners and images stays constant as languages are added.
3. **Stages only ever see the composite, and `languages` is a `Vec` (ADR:48-62).** Python becomes the one-frontend case, so the refactor can be proven on crc/strsimpy before any Fortran parser exists. Rejecting an `ElmerAdapter` (ADR:98-99) keeps repo names out of `src/`.

## 3. Issues

| # | sev | location | problem | concrete fix |
|---|---|---|---|---|
| B1 | **blocking** | ADR:45 `BuildBridge` | `configure()`/`build()` take no worktree, unit or Rust artifact. But the mirror *is* substitution (§7F): `mirror.rs:154` `build_ext` installs the extension, `mirror.rs:202` drops `PYTHONPATH` so the installed extension is used, `optimize.rs:503-579` `build_release` copies the `.so` into the tree, and hand-written per-repo scaffolds come from `fixture.rs:76-78` `template_dir` (`mirror/crc/template.json`). For CMake/Fortran this becomes link substitution, and nothing in the design owns it. | Use §4 **S3** `BuildBridge { prepare, build, substitute, scaffold, link_deps, artifacts }`. `scaffold()` replaces `mirror/<fixture>/template.json`. |
| B2 | **blocking** | ADR:34 `fragment() -> CallGraph`; ADR §Reasoning 3 | `CallGraph.modules` (`adapters:31`) maps one ID to one file. `python_call_graph` already silently overwrites duplicate stems (`adapters:352-356`). The IDs are a persisted contract: `dag.json` (`recon.rs:87-99`, read at `mirror.rs:488-493`) and `recon.json` modules (`fixture.rs:252-265`). They can't express header+source units, several Fortran modules per file, submodules, `.src`→`.F90` generation, or the same stem in two languages. | Use §4 **S2** `UnitId`/`UnitDecl{files, generated_from, exports, imports}`/`Symbol{linkage, abi}`/`Fragment`. Write the canonical ID spec now (§5 Q3). |
| B3 | **blocking** | ADR:39-44; ADR migration step 2 | The frozen manifest still stores `invocation: Vec<String>` at `version: 1` (`core:6-7`). `detect_invocation` (`oracle:251`) duplicates `adapters:168-172` and recomputes the command at freeze. `config_hash()` (ADR:44) has no field to persist into. A typed command that isn't frozen gets recomputed at grade time, which is a tamper surface. | Use §4 **S1** `Manifest v2 { runner, prepare, invocation: Vec<TestCommand>, config_hash, observables }` in `rustsmith-core`. Ship the writer (`recon.rs:330` `frozen_manifest_with_benchmarks`) and every reader in one change, plus a v1 reader. |
| B4 | **blocking** | ADR:79 "discovered test artifacts"; ADR:92 "norm-diff" | `oracle_parity` (`gates:78-87`) only trusts `failed`/`exit_code`. [INFERENCE] Elmer's `TEST.PASSED` is written by the solver after it compares against `Reference Norm`, so porting anything on that path lets the Rust side certify itself. A generic CTest runner also can't "discover" `TEST.PASSED` without an Elmer-specific literal in `src/`. | Add §4 **S1** `TestRunner::observe(&[RunOutput], &[ObservableSpec]) -> Vec<Observation>`. The specs (regex/artifact plus tolerance) live in frozen recon data (`RepoFacts.observables`) and the comparison happens on the oracle side. Keep `TEST.PASSED`/CTest verdicts only as a cross-check. |
| B5 | **blocking** | ADR:17, 88-89 (only "`fixture.rs:42-74` allowlist → deleted") | `FixtureKind` drives far more than detection: recon workloads/contract/held-outs/PORTING (`recon.rs:22-84, 217, 298`), differential pairs (`mirror.rs:305-313`), optimize workloads (`optimize.rs:55-115, 672-678, 1036, 1095, 1368, 1671`), the plant gate (`main.rs:434-435`), and attribution (`harvest:111-116`, `main.rs:1257-1258`). Repo probes also sit in `profile:70` and `profile:100`, and `mirror.rs:124` hardcodes the host path `/home/john/.local/bin/maturin`. ADR:21 names "recon/*.json" but gives no schema and no author. | Add §4 **S4** `RepoFacts` (`recon/facts.json`), written by the Architect in Stage 0 and hashed into the manifest. Move it to migration step 3. Resolve `maturin` through `PATH`/config only. |
| M1 | major | ADR:39-40 `TestCommand` | No `cwd`, timeout, launcher or artifact collection, yet every spawn today sets `current_dir` (`oracle:276`, `sandbox:205`, `mirror.rs:199`, `recon.rs:135`), and ctest runs in the build dir. | §4 **S1** `TestCommand { cwd: Cwd, launcher: Option<Launcher>, timeout_secs, collect }`. |
| M2 | major | ADR:42 `parse_output(&str, i32) -> GradedResult` | It only sees stdout; CTest's machine-readable output is a file (`--output-junit`). `GradedResult` (`core:27-35`) is shaped like pytest (`xfailed`/`deselected`, no per-test IDs), so `oracle_integrity` compares counts only (`gates:24-33`) and misses "same count, different tests." | `fn grade(&self, runs: &[RunOutput]) -> Result<GradedResult, AdapterError>`, plus `GradedResult.outcomes: BTreeMap<String, Outcome>`. `oracle_integrity` compares the set of keys. |
| M3 | major | ADR:34 `fragment(&self, files: &[PathBuf])` | C/C++ parsing needs the compile DB. Fortran `USE` resolution needs preprocessed and generated `.F90`. Both exist only after configure, or after the codegen targets are built. | `fragment(&self, cx: &FragmentCtx)` (§4 **S2**). Recon order becomes probe → `bridge.prepare` → codegen build → fragments. |
| M4 | major | ADR:70 "one `perf record`"; ADR:46-47 `Profiler` | This contradicts `docs/adr/003-deterministic-instrument-fallback.md:2-4` (perf blocked; `/proc/sys/kernel/perf_event_paranoid` still reads `4` on 2026-09-21; no valgrind). `harness()` has no parser and no untimed setup step (e.g. mesh generation). `alloc_peak` comes from tracemalloc (`profile:127`, `optimize.rs:310`) and has no language-neutral equivalent. | §4 **S3** `Profiler { setup, timed, hotspots(w, perf_available) }`. Core measures child CPU via `wait4` rusage. `CpuStats.alloc_peak` and `gates::ResourceSnapshot.alloc_count` (`gates:276`) become `Option<u64>`. |
| M5 | major | ADR:36 `porting_rules() -> String`; ADR:87 "held-outs → frontend+runner" | The rulebook content is specific to crc's API (`porting.rs:16-31`: `Configuration(width=8)`, `TypeError`), not to Python. Held-outs are pytest-only (`oracle:328-360`, `heldout.rs` writes `test_heldout_*.py`), and `mirror.rs:108` writes `original_source.py`. | `Frontend::language_rules() -> Vec<Rule>` (ABI/layout only), with repo rules in `RepoFacts.porting_rules`. Add `TestRunner::heldout(&self, suite, cx) -> Vec<TestCommand>`; held-outs belong to the runner, not the frontend. |
| M6 | major | ADR:118 "parameterized images" in step 6 | Step 2 routes the sandbox through the runner, but the image is still a const (`sandbox:17`) and the artifact is mounted `:ro` (`sandbox:131`). CTest writes `Testing/` and per-test outputs into the build tree. | `CompositeAdapter::image(&self) -> ImageSpec { base, packages, writable: Vec<String> }`, pulled forward into step 2. |
| M7 | major | ADR:107-118 migration | Three groups have to land together: (a) the Manifest v2 writer and readers; (b) the UnitId format together with `dag.json`, `template.json` keys (`orig_source`/`delete_on_merge`) and `fixture.rs:233-243`; (c) `outcomes` together with `oracle_integrity`/`Baseline` (`core:19-24`). Step 1 has no parity check. | Step 1's exit criterion: the composite produces byte-identical `recon.json`/`dag.json`/`manifest.json` to HEAD for crc and strsimpy, ignoring v2-only fields. Use the revised order in §8. Write ADR-008 to supersede ADR-004. |
| m1 | minor | ADR:88 "gates:372 literal" | The same `pyproject.toml` rule also lives in `oracle:52, 82, 102, 146` (ADR-002) and in `discover_oracle_files`'s pytest patterns (`oracle:198-240`); the ADR lists neither. | `TestRunner::normalize_for_hash(&self, rel: &str, bytes: &[u8]) -> Option<Vec<u8>>` (the Python runner owns the ADR-002 behavior), plus `oracle_files()` on the runner. |
| m2 | minor | ADR:41-47 crate placement | The traits return `core::GradedResult` and `profile::HotspotBaseline`. `sandbox:226` duplicates the parser precisely because the sandbox "must not depend on oracle." | Put `TestCommand`, `RunOutput`, `Manifest` and `TestRunner` in `rustsmith-core`. oracle and sandbox then depend only on core (§7C). |
| m3 | minor | ADR:35 `Frontend::classify_dep(&str)` | C/Fortran dependencies are link targets that come from the build, not the language (this host: `/usr/lib/elmersolver/libarpack.so`, `libumfpack.a`, `libamd.a`). | `CompositeAdapter::classify_dep(&self, dep: &LinkDep) -> DepClass`, fed by `BuildBridge::link_deps`. |
| m4 | minor | ADR:49-51 drops `license_terms` (`adapters:133`) | Assumes one license per repo; this host's Elmer ships `/usr/share/elmersolver/license_texts/`. | `fn license_terms(&self, repo: &Path) -> Result<Vec<(String /*glob*/, Attribution)>, AdapterError>`. |
| m5 | minor | ADR:31 vs 41-47; ADR:54-55; ADR:60 | Only `Frontend` is `Send + Sync`. `call_graph()`/`partition()` can't fail. "Unknown extensions warn" gives a silently partial DAG. | Make all traits `Send + Sync`. `partition(&self, cx) -> Result<(UnitDag, Vec<Diagnostic>), AdapterError>`. Add `ProbeReport { unclaimed: Vec<String>, unclaimed_share: f64 }` that halts above a config threshold. |
| m6 | minor | ADR:62 `BuildInfo.language` → `languages` | `recon.rs:104` writes `build.language` into the frozen `recon.json`. | Rename the frozen JSON key in the same change as group (b) in M7. |
| m7 | minor | ADR:49 | `pub struct CompositeAdapter { pub languages: Vec<String>;` has a `;` where a `,` belongs (typo). | Fix when revising. |

## 4. Proposed signatures (referenced from §3)

```rust
// S1 — rustsmith-core (leaf crate; oracle/sandbox/gates already depend on it)
pub enum Cwd { Tree, BuildDir, Rel(String) }
pub struct Launcher { pub program: String, pub np_flag: String, pub np: u16, pub extra: Vec<String> }
pub struct TestCommand { pub program: String, pub args: Vec<String>, pub cwd: Cwd,
    pub env_set: Vec<(String, String)>, pub env_remove: Vec<String>,
    pub launcher: Option<Launcher>, pub timeout_secs: Option<u32>, pub collect: Vec<String> }
pub struct RunOutput { pub exit_code: i32, pub stdout: String, pub stderr: String,
    pub artifacts: BTreeMap<String, Vec<u8>> }
pub enum Outcome { Pass, Fail, Skip, XFail, NotRun, Timeout }
pub struct ObservableSpec { pub test_glob: String, pub source: String /* "stdout" | artifact glob */,
    pub regex: String, pub rel_tol: f64, pub abs_tol: f64 }
pub struct Observation { pub test_id: String, pub key: String, pub value: f64 }
pub struct Manifest { pub version: u32 /* 2 */, pub runner: String, pub languages: Vec<String>,
    pub prepare: Vec<TestCommand>, pub invocation: Vec<TestCommand>, pub config_hash: String,
    pub observables: Vec<ObservableSpec>, pub files: Vec<FileHash>, pub baseline: Baseline }
// GradedResult gains: pub outcomes: BTreeMap<String, Outcome>  (counts derived from it)
pub enum OracleKind { Spec, Fixture, Harness /* code compiled against the SUT */ }
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

// S2 — rustsmith-adapters: unit identity + frontends
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
    fn claims(&self, file: &Path, compile_db: Option<&Path>) -> bool; // replaces extensions()
    fn fragment(&self, cx: &FragmentCtx) -> Result<Fragment, AdapterError>;
    fn language_rules(&self) -> Vec<Rule>; // ABI/layout only
}

// S3 — spine
pub struct BuildCtx<'a> { pub tree: &'a Path, pub build_dir: &'a Path, pub release: bool }
pub trait BuildBridge: Send + Sync {
    fn prepare(&self, cx: &BuildCtx) -> Vec<TestCommand>; // ordered, fail-fast
    fn build(&self, cx: &BuildCtx) -> Vec<TestCommand>;
    fn substitute(&self, cx: &BuildCtx, unit: &UnitDecl, rust_lib: &Path)
        -> Result<Vec<TestCommand>, AdapterError>;
    fn scaffold(&self, unit: &UnitDecl) -> Result<Vec<(String, String)>, AdapterError>;
    fn link_deps(&self, cx: &BuildCtx) -> Result<Vec<LinkDep>, AdapterError>;
    fn artifacts(&self, cx: &BuildCtx) -> Vec<PathBuf>; // binary_bytes for no_regression
}
pub trait Profiler: Send + Sync {
    fn setup(&self, w: &Workload) -> Vec<TestCommand>; // untimed
    fn timed(&self, w: &Workload) -> TestCommand;      // core measures via wait4 rusage
    fn hotspots(&self, w: &Workload, perf_available: bool) -> Result<HotspotBaseline, AdapterError>;
}
pub struct Workload { pub name: String, pub setup: String, pub stmt: String, pub iters: usize }
impl CompositeAdapter {
    pub fn partition(&self, cx: &BuildCtx) -> Result<(UnitDag, Vec<String>), AdapterError>;
    pub fn classify_dep(&self, dep: &LinkDep) -> DepClass;
    pub fn license_terms(&self, repo: &Path) -> Result<Vec<(String, Attribution)>, AdapterError>;
    pub fn image(&self) -> ImageSpec; // union of frontend + spine toolchain requirements
}

// S4 — recon/facts.json, frozen and hashed in the manifest; replaces FixtureKind
pub struct RepoFacts { pub workloads: Vec<Workload>, pub heldout_workloads: Vec<Workload>,
    pub differential_probes: Vec<TestCommand>, pub observables: Vec<ObservableSpec>,
    pub api_surface: Vec<String>, pub porting_rules: Vec<Rule>,
    pub attribution: Vec<(String, Attribution)> }
```

## 5. Answers to the ADR's review questions

1. **Is `TestCommand` enough for MPI and multi-step configure?** MPI isn't a pipeline problem: `mpiexec -n 8` wraps a single command. Model it as `TestCommand.launcher: Option<Launcher>` so the rank count is data that gets hashed into `config_hash`, and the runner can skip or serialize tests needing more ranks than there are cores. Multi-step configure does need an ordered list: `BuildBridge::prepare/build -> Vec<TestCommand>` (fail-fast), frozen as `Manifest.prepare`. Don't add a DAG/pipeline type; nothing in the evidence needs branching.
2. **Paths or hashes, and who owns `TEST.PASSED`?** `oracle_files()` returns **paths plus kind** (`OracleFile`). The oracle keeps its single hashing implementation (`oracle` `sha256_hex`), and the runner only supplies `normalize_for_hash`, which is where ADR-002 moves. `TEST.PASSED`-class verdicts are written by the code being ported, so neither the runner nor the SUT should own them. The runner **extracts observables**; the oracle **compares** them against references and tolerances frozen in the manifest. [INFERENCE] For Elmer, that means parsing the computed norm and comparing it to the frozen `Reference Norm`.
3. **Canonical ID spec now or later?** Now. IDs are already a persisted contract (`dag.json` read at `mirror.rs:488-493`; `recon.json` modules at `fixture.rs:252-265`; `template.json` keys), and Python already collides on stems (`adapters:352-356`). Waiting means breaking frozen runs later. Minimal spec: `<lang>:<repo-rel path of the authoritative (pre-generation) source>[#<symbol>]`, with Fortran symbols lowercased (the language is case-insensitive) and linkage names kept in the `Symbol` table, not in the ID. Worked example for Python: `_crc` becomes `python:src/crc/_crc.py`.
4. **Any objection to deleting the `fixture.rs` dispatch (ADR-004) in step 6?** No objection to deleting it; I do object to the scope and timing. `FixtureKind` is load-bearing at more than 20 sites (B5), so deleting it in step 6 means steps 3-5 ship still dispatching on repo names. Replace it with `RepoFacts` in step 3, supersede ADR-004 in ADR-008, and keep `config/fixture.toml` as acceptance *test data* only.

## 6. Missing cases (scenarios the ADR mishandles)

1. **Solvers loaded at runtime.** This host's Elmer ships 149 solver `.so` files in `/usr/share/elmersolver/lib` (e.g. `Acoustics.so`, `AdvectionDiffusion.so`), loaded by name at runtime [INFERENCE: via `.sif` `Procedure` entries]. The static graph has no edges to them. Fix direction: record loaded shared objects during the baseline run (`LD_DEBUG=files`) and map them to units through the CMake File API.
2. **Oracle code compiled against the SUT.** `/usr/bin/elmerf90` compiles user Fortran against 147 `.mod` files in `/usr/share/elmersolver/include` and `-lelmersolver` [INFERENCE: tests compile test-local modules the same way]. Porting a used module breaks the frozen oracle's compile. Fix direction: `OracleKind::Harness`, and have `partition()` keep an ABI-compatible `Bind` shim for any unit a Harness file imports.
3. **Floating-point parity.** `gates::differential(.., 0.0)` requires exact equality (`optimize.rs:288`, `mirror.rs:689-694`), but Fortran→Rust won't be bit-identical (FMA, reduction order, libm). Fix direction: per-observable `rel_tol`/`abs_tol` from `ObservableSpec`, frozen in the manifest.
4. **Test set depends on configure.** ctest's test list depends on what configure found (MPI, optional libraries), so the count baseline (`gates:24-33`) differs between the host and the grading image. Fix direction: record the baseline after `prepare` inside the grading image, include the found-feature set in `config_hash`, and treat a mismatch as a halt, not a failure.
5. **Units the oracle never runs, and vendored code.** Examples are GUI C++ or vendored ARPACK/UMFPACK. An extension census schedules them for porting, and per-unit grading passes them without testing anything. Fix direction: per-unit coverage from the baseline run plus vendored-target classification from `link_deps`; mark those units out of scope in `partition()`.

---

## 7. Additional findings (outside the review's required format)

### A. Hardcoded-site inventory: covered by ADR vs missed

| site | what | in ADR? |
|---|---|---|
| `recon.rs:17` | `let adapter = PythonAdapter` | yes (step 3) |
| `oracle:269-365` | `run_pytest`/`run_invocation`/`run_heldout` | yes (step 2) |
| `oracle:368` | `parse_pytest_verbose` | yes |
| `oracle:251` | `detect_invocation` duplicates `adapters:168-172` | **no** |
| `oracle:52, 82, 102, 146` | `pyproject.toml` section hashing (ADR-002) | **no** (only `gates:372` named) |
| `oracle:198-240` | `discover_oracle_files` hardcodes `test_`, `conftest.py`, `pytest.ini`, `tox.ini` | **no** |
| `oracle:329-339` | a missing held-out dir returns `passed: 1` | **no** (see B below) |
| `sandbox:117, 177, 198-223, 225` | docker/host pytest + parser copy | yes (step 2) |
| `sandbox:17, 131` | fixed `GRADING_IMAGE`, `:ro` artifact mount | partially (step 6 images; mount not mentioned) |
| `sandbox:85-99` | silent host fallback when docker fails | **no** (see B) |
| `mirror.rs:124` | `maturin_bin` hardcodes `/home/john/.local/bin/maturin` | **no** |
| `mirror.rs:154-226` | `build_ext`, `run_oracle_in_venv`, parser | yes (step 4) |
| `mirror.rs:108` | writes `original_source.py` | **no** |
| `mirror.rs:305-313` | `differential_pairs_for` dispatches on `FixtureKind` | **no** |
| `optimize.rs:281-347, 503-579` | grade + `build_release` | yes (step 4) |
| `optimize.rs:55-115, 672-678, 1036, 1095, 1368, 1671` | per-fixture workloads/reporting | **no** |
| `optimize.rs:288`, `mirror.rs:689-694` | differential tolerance literal `0.0` | **no** |
| `recon.rs:122-157, 160` | `baseline_counts`, `parse_collected` | yes (step 3) |
| `recon.rs:174-205` | `runtime_deps` parses `pyproject.toml` | yes (step 3) |
| `recon.rs:217-339` | per-fixture workloads, contract, crc/strsimpy `python3 -c` probes | **no** |
| `profile:49-107` | py-spy/cProfile | yes |
| `profile:70, 100, 141` | `src/crc` probe, `crc._crc…` literal, `Workload::crc_checksum` | **no** (repo-specific, not just language-specific) |
| `profile:127, 134, 160, 184` | tracemalloc `alloc_peak`, `setup_py`/`stmt_py`, `harness_py`, `PYTHONHASHSEED` | partially (step 5 "setup/stmt rename") |
| `gates:372` | `pyproject.toml` literal | yes |
| `harvest:111-116` | `attribution_for(fixture)` names strsimpy upstream | **no** |
| `main.rs:434-435, 945, 1257-1258` | `FixtureKind` gates and template lookup | **no** |
| `porting.rs:8-272`, `heldout.rs:7-278` | Python→Rust, crc/strsimpy-specific content | yes (step 6), but mis-scoped as per-language (M5) |
| `fixture.rs:17-105` | `FixtureKind`, detect/resolve, `template_dir` | only `42-74` named |
| `containers/*.Dockerfile:1-3` | `python:3.11-slim` + pytest | yes (step 6) |

### B. Places where grading can pass without really testing

- **An empty held-out suite passes** (`oracle:329-339`). Until a CTest-shaped held-out generator exists, `heldout_divergence` passes without testing anything on any non-Python repo. Change it to halt when the runner declares held-outs but none exist.
- **Docker failure silently falls back to the host** (`sandbox:85-99`). On this host `/usr/bin/ElmerSolver` (9.0) is on `PATH`. [INFERENCE] A host-fallback ctest run could resolve the *system* solver instead of the tree's build and pass against the wrong binary. Fix: the runner pins absolute build-dir binaries in `TestCommand.program`, and host fallback is refused for any non-Python runner, or at least reported as a gate failure.
- **Units the oracle never executes** pass per-unit grading (see §6 case 5).

### C. Crate dependency graph today (from `crates/*/Cargo.toml`)

- `adapters`: no deps. `profile`: no deps. `core`: no deps.
- `oracle` → core, store. `sandbox` → core. `gates` → core. `store` → core. `harvest` → store, gates. `report` → store, harvest. `agent` → core. `council` → core, store.
- `cli` → everything.

Implication: if `TestRunner` stays in `adapters`, then oracle and sandbox must add an `adapters` dependency, and `adapters` must add `core` (for `GradedResult`) and `profile` (for `HotspotBaseline`/`Workload`). No cycle results, but putting the spine traits and command types in `core` (m2) keeps oracle and sandbox free of adapters and removes the reason the parser was duplicated at `sandbox:226`.

### D. Host environment facts (verified 2026-09-21)

- `/proc/sys/kernel/perf_event_paranoid` = `4`, so perf is still blocked and ADR-003 still holds. ADR-007's perf-based profiling (ADR:70) can't run here.
- On `PATH`: `/usr/bin/cmake`, `/usr/bin/ctest`, `/usr/bin/gfortran`, `/usr/bin/mpiexec`, `/usr/bin/ElmerSolver`, `/usr/bin/ElmerSolver_mpi`, `/usr/bin/ElmerGrid`, `/usr/bin/elmerf90`.
- `elmerf90` embedded flags: `-fopenmp -fallow-argument-mismatch -DCONTIG= -DHAVE_EXECUTECOMMANDLINE -DUSE_ISO_C_BINDINGS -DUSE_ARPACK -O2 -g -DNDEBUG -fPIC -shared`, and it links `-lelmersolver`. This confirms ADR:78's point that `-fallow-argument-mismatch` must come from captured flags, not literals.
- `/usr/share/elmersolver/lib`: 149 `.so` solver modules. `/usr/share/elmersolver/include`: 147 `.mod` files. `/usr/lib/elmersolver`: `libelmersolver.so`, `libmatc.so`, `libfhuti.so`, `libarpack.so`, `libparpack.so`, `libumfpack.a`, `libamd.a`, `libamdf77.a`.
- The installed version is 9.0 (PPA), not 26.2.1. Treat layout carry-over as [INFERENCE].

### E. ADR citation check

The ADR's line references are accurate: `adapters:127-134` `trait Adapter`; `adapters:402` `dag_from_call_graph`; `adapters:21-22` `BuildInfo.language`; `recon.rs:17`; `oracle:269-365`; `sandbox:198-223`; `mirror.rs:124-172`, `176-226`; `optimize.rs:281-347`, `503-579`; `gates:372`; `fixture.rs:42-74`. The "4 parser copies" claim is correct: `oracle:368`, `sandbox:225`, `mirror.rs:228`, `recon.rs:160`. The ADR's weakness is what it *leaves out* (§7A), not what it cites.

### F. How the Python mirror actually works (context for redesigning BuildBridge)

Stage 1 never runs Rust tests. It builds the Rust port as a PyO3 extension (`maturin develop`, `mirror.rs:154-174`) into a grade venv (`mirror.rs:135-148`), removes `PYTHONPATH` so the *installed extension* shadows the original module (`mirror.rs:202`), then runs the **original, frozen pytest suite** against it (`mirror.rs:176-226`). Stage 2 does the same with release wheels and copies the `.so` into the tree (`optimize.rs:503-579`). The unit→file mapping and the Rust scaffold come from hand-written per-repo `mirror/<fixture>/template.json` (`fixture.rs:107-119` `TemplateSpec`, `233-250` `unit_sources`).

For a CMake/CTest repo the equivalent is: build the Rust port as a `staticlib`/`cdylib` exporting the original linkage names, splice it into the original build in place of the unit's object files, rebuild, and run the original `ctest`. That is what `BuildBridge::substitute` + `scaffold` (S3) must express. Constraint to design for: [INFERENCE] non-`BIND(C)` Fortran module procedures (gfortran `__mod_MOD_proc` mangling, array descriptors for assumed-shape arrays) have no stable C ABI. That limits which Fortran units can be substituted one at a time, and `partition()` has to take it into account when choosing unit granularity.

## 8. Revised migration order

1. **core types:** `TestCommand`, `RunOutput`, `Outcome`, `ObservableSpec`, `Manifest v2` + a v1 reader, and the `TestRunner` trait in `rustsmith-core`.
2. **adapters:** `Frontend`/`BuildBridge`/`Profiler`/`CompositeAdapter`, the `UnitId` spec, and the probe. Re-express `PythonAdapter` as `PythonFrontend + PytestRunner + MaturinBridge + PyProfiler`. **Exit gate:** the composite produces byte-identical `recon.json`/`dag.json`/`manifest.json` to HEAD for crc and strsimpy, ignoring v2-only fields.
3. **Atomic group:** oracle + sandbox + recon's manifest writer route through the runner, `oracle_integrity` switches to `outcomes`, and the image is parameterized (M6). This deletes `oracle:368`, `sandbox:225`, `recon.rs:160` and `detect_invocation`.
4. **RepoFacts:** `recon/facts.json` schema, Stage-0 authoring, manifest hashing. Remove `FixtureKind` consumers in recon/mirror/optimize/harvest/main/profile. Supersede ADR-004 with ADR-008.
5. **Atomic group:** `UnitId` rollout to `dag.json`, `recon.json`, `template.json`→`scaffold()`, and `fixture.rs:233-265`.
6. **mirror + optimize** through `bridge.substitute`/`runner`/`profiler`. Delete the maturin/venv paths and `mirror.rs:228`. Make `alloc_peak` an `Option`.
7. **Content:** `language_rules` per frontend, repo rules in RepoFacts, held-outs owned by the runner. Delete `fixture.rs`.
8. **Second frontend** (Fortran or C via compile DB) plus the CMake/CTest spine, against the pinned Elmer tree.

## 9. Checklist of required ADR edits

- [ ] Replace the `TestCommand` definition with S1 (cwd, launcher, timeout, collect) and state that it lives in `rustsmith-core`.
- [ ] Add Manifest v2 (frozen `prepare`, `invocation`, `config_hash`, `observables`, `runner`) and the v1-compat reader.
- [ ] Replace `parse_output` with `grade(&[RunOutput])`, and add `observe`, `normalize_for_hash`, `heldout` to `TestRunner`.
- [ ] Add `GradedResult.outcomes` and state that `oracle_integrity` compares test-ID sets.
- [ ] Replace `BuildBridge` with S3, including `substitute` and `scaffold`, and describe the substitution model for CMake (§7F).
- [ ] Replace `fragment() -> CallGraph` with `fragment(&FragmentCtx) -> Fragment`, and add the canonical `UnitId` spec.
- [ ] Specify recon ordering: probe → prepare → codegen build → fragments.
- [ ] Replace the `Profiler` with S3. Reconcile with ADR-003: no perf dependency; core measures via `wait4`; `alloc_peak: Option<u64>`.
- [ ] Add a `RepoFacts` section: schema, who authors it (Architect, Stage 0), freezing, and the full list of `FixtureKind` sites it replaces (§7A).
- [ ] Move repo-API porting rules out of `Frontend` into `RepoFacts`.
- [ ] Add `image()` on the composite and the writable-mount policy.
- [ ] Add the §6 missing cases as explicit "handled by" lines.
- [ ] Rewrite the migration section to match §8, with the atomic groups and the step-2 parity gate.
- [ ] Fix the ADR:49 typo (`;` → `,`).

## 10. Uncertainty register

| claim | status | how to verify |
|---|---|---|
| Elmer's `TEST.PASSED` is written by ElmerSolver after the reference-norm comparison | [INFERENCE] | Read `fem/src` for the reference-norm check, and `fem/tests/test_macros.cmake` / `runtest.cmake` at `a19504a`. |
| CTest pass/fail for Elmer tests is derived from `TEST.PASSED` via `runtest.cmake` | [INFERENCE] | Same files as above. |
| Solver modules are loaded by name at runtime from `.sif` `Procedure` entries | [INFERENCE]; the 149 `.so` files in the 9.0 install support it | Grep `Procedure` in `fem/tests/*/case.sif`. |
| Tests compile test-local Fortran against solver `.mod` files | [INFERENCE]; `elmerf90` in 9.0 supports it | Grep the test CMake for elmerf90/module macros. |
| `.src` → `.F90` generation exists at 26.2.1 | taken from ADR:67-68 | Look for `*.src` in `fem/src` and the CMake rule that generates from them. |
| The ctest test set changes with configure options (MPI, optional libs) | [INFERENCE] | Diff `ctest -N` output between `WITH_MPI=ON` and `OFF`. |
| Non-`BIND(C)` Fortran procedures have no stable C ABI for substitution | [INFERENCE] (standard gfortran behavior) | Try substituting one module procedure in a spike. |
