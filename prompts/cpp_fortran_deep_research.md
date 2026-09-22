# C++/Fortran adapter deep research — background + brief

## 1. What rustsmith is

`rustsmith`: autonomous rewrite-in-Rust orchestrator. Point it at a source repo
(any language), get back a complete Rust mirror plus backportable suggestions —
no human in the loop during the run. Pipeline: Stage 0 recon (detect, call
graph, test baseline, PORTING.md, unit DAG) → Stage 1 mirror (behavior-identical
Rust, graded per unit) → Stage 2 optimize (bounded rounds, gated) → Stage 3
harvest (backport patches). Deterministic gates (`rustsmith-gates`, pure Rust,
no LLM) decide what passes; agents (`rustsmith-agent`, 4-seat
`rustsmith-council`: Architect/Verifier/Performance/Scope) do the porting.

## 2. Where we stand (audit verdict: trait-ok-but-consumers-leak)

- The seam `crates/rustsmith-adapters/src/lib.rs:127-134` (`trait Adapter`:
  `detect` / `call_graph` / `test_inventory` / `classify_dep` /
  `license_terms`, plus `BuildInfo` / `CallGraph` / `TestInventory` /
  `Attribution` / `UnitDag`) is language-neutral in name.
- Every execution path bypasses it. `cli/recon.rs:17` hardcodes
  `let adapter = PythonAdapter`. `oracle/src/lib.rs:269-365`
  (`run_pytest` / `run_invocation` / `run_heldout`), `sandbox/src/lib.rs:198-223`
  (`host_pytest`), `sandbox:117-120` (docker inner),
  `recon.rs:122-157` (`baseline_counts`), `mirror.rs:176-226`
  (`run_oracle_in_venv`), `optimize.rs:281-347` (`grade_candidate`) each spawn
  `python3 -m pytest` with `PYTHONPATH` / `PY_COLORS` / `PYTHONHASHSEED` and
  parse `pytest -v` output (4 independent parser copies: `oracle:367`,
  `sandbox:225`, `mirror:228`, `recon:160`).
- Build assumes PyO3/maturin: `mirror.rs:124-172`
  (`maturin_bin` / `ensure_grade_venv` / `build_ext`), `optimize.rs:503-579`
  (`build_release`). Profile assumes `py-spy` / `cProfile` / `tracemalloc`:
  `profile/src/lib.rs:49-107` (`capture_hotspot_baseline`), `:160-200`
  (`harness_py` / `measure_once`, fields `setup_py` / `stmt_py`).
- Content generators are Python→Rust rulebooks: `cli/porting.rs:8-272`
  (`generate_porting_md`, 100+ `Original (Python)` rules),
  `cli/heldout.rs:7-278` (`test_heldout_*.py` with `from crc import …`),
  `cli/fixture.rs:42-74` (only `src/crc/` or strsimpy layouts accepted).
- Containers pin `FROM python:3.11-slim + pip install pytest`
  (`containers/run.Dockerfile:1-3`, `grading.Dockerfile:1-3`).
- Reusable as-is: `config/default.toml`, `guidance/optimize.md`,
  `rustsmith-agent` (`sh -c` + `UNIT_WORKTREE`), `rustsmith-council`,
  `rustsmith-store`, `rustsmith-report`, core gate math
  (`oracle_integrity` / `oracle_parity` / `heldout_divergence` / `differential`).
- SPEC intent vs reality: `SPEC.md:27` ("only the Python adapter is implemented
  in v1"), `README:109` ("new languages arrive as plugins, not rewrites"),
  `SPEC §9` (`tree-sitter for Python`, `py-spy for Python` as instances).
  Reality hardcodes the instance as the only path — no `TestRunner` /
  `BuildBridge` / `Profiler` extension point, no adapter registry.

## 3. Target: C++ and Fortran for one pinned repo (REPO_URL_OR_PATH below)

Payoff is memory safety + maintainability, NOT speed (both already fast —
opposite of the Python 10–1000x thesis). Adapter is the hardest class:
preprocessor/macros/templates (C++), `USE`/`COMMON`/fixed-vs-free form
(Fortran), mixed-language repos, CMake/CTest matrix.

## 4. Research objectives

Produce the per-language contracts needed to implement, in dependency order:

```rust
TestRunner  { invocation() -> Vec<TestCommand>, run(), parse_output(), oracle_files(), config_hash() }
BuildBridge { prepare(), build(), runtime_env() }   // replaces maturin/venv
Profiler    { hotspots(), harness() }               // replaces py-spy/cProfile/tracemalloc
Adapter     { detect, call_graph, partition, runner, bridge, profiler,
              porting_rulebook, heldout_suite, classify_dep, license_terms }
```

## 5. Track 1 — ecosystem (C++17/20 and Fortran F90+, parallel)

For EACH language answer with evidence (paste real samples, link docs):

1. Build systems in the wild (CMake / Make / Meson / Bazel / fpm): which two
   cover 80%? Exact configure+build commands for an out-of-tree release build.
2. Test runners + machine output: `ctest`, GTest, Catch2, doctest (C++);
   pFUnit, CTest, custom shell (Fortran). Paste 10–20 lines of real verbose
   output per runner + how to get counts (pass/fail/skip lists).
3. Module/call-graph extraction WITHOUT full compile: `clang -fsyntax-only`?
   `bear` compilation DB? `tree-sitter-cpp` import regex? `fparser2` / `fortls`
   for `USE`? Cost per option (deps, accuracy on macros/templates).
4. Profiling story: `perf stat/record`, `gprof`, Intel VTune — which is the
   deterministic primary (replaces process-CPU median) and what allocation
   signal replaces `tracemalloc` (heaptrack? `massif`? none?).
5. Container base replacing `python:3.11-slim`: `gcc:X` + `gfortran` + `cmake` +
   `ninja` + `perf` — pinned versions that coexist.
6. FFI/layout traps for the PORTING.md rulebook: headers vs modules (C++20),
   `extern "C"`, name mangling, Fortran `BIND(C)` / `iso_c_binding`,
   column-major vs row-major, 1-based indexing, `COMMON` blocks.

## 6. Track 2 — pinned repo only (needs REPO_URL_OR_PATH)

1. Root listing + build file + EXACT build commands (out-of-tree, release) +
   exact test commands. 20 lines of real test output, verbatim.
2. Module map: pick 10 headers/modules, hand-trace their `#include` / `USE`
   edges. Note mixed C++↔Fortran edges (`extern "C"` / `BIND(C)`).
   Flag anything breaking file==module (headers with no `.cpp`, Fortran
   submodules, generated files).
3. Public API surface: which symbols must stay byte-identical (replaces
   `Calculator/Register` in current `workload_contract`)?
4. Hotspot guess: 3 files most likely dominating runtime + what workload
   exercises them (replaces `import crc` probes in `recon.rs:257-328`).
5. License files + layout (`src/`? `include/`? mixed dirs?) + CI workflows.
6. Dialect pins: `-std=` flag, fixed vs free Fortran, preprocessor use.

## 7. Output format (strict)

- Table per track: `question | answer (1 sentence) | evidence (file/cmd/URL/output)`.
- Appendix A: verbatim command transcripts (build, test, profile) per language
  and for the pinned repo.
- Appendix B: draft `TestCommand` vectors (program + args + env) for
  build/test/profile on the pinned repo — these become the adapter's
  `invocation()` return values.
- Appendix C: ranked blocker list (`blocks-new-language` / `friction`) with
  cheapest fix per row, same columns as the polyglot audit.
- No code changes. No guessing: every claim cites a doc URL, command transcript,
  or repo path. Mark inferences `[INFERENCE]`.

## 8. Repo under test

- URL or host path: ________________________________________
- Pinned commit: ________________________________________
- C++ standard flag: ________________________________________
- Fortran dialect: ________________________________________
- Build system + version: ________________________________________
- Test runner(s): ________________________________________
