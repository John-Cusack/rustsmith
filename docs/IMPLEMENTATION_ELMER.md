# Elmer FEM → Rust: scale-up strategy + worker guide

Status: the CTest mirror loop is complete and proven on a throwaway
BIND(C) fixture (`mirror: 1/1 passed divergence=0.0000`, merged port
live). This document is the execution plan for scaling that loop to
Elmer itself. Numbers below are measured on this host, not estimated.

## 1. Ground truth (measured 2026-09-22)

- Recon: `cmake` spine, 3020 units — `fortran` 2151, `c` 559, `cxx` 300,
  `python` 10. Baseline was `test_count: 0` (unbuilt tree).
- Toolchain (host): cmake 3.28, gfortran, gcc, MPI present.
- Elmer configure out-of-source: **~8 s, succeeds** (`-S <orig> -B <build>`
  `-DCMAKE_BUILD_TYPE=Debug`).
- Discovered tests: **1100 total, 482 with the `quick` label** (the frozen
  recon invocation grades `-L quick`). A recon-build therefore freezes a
  real baseline of 482 — the vacuous-baseline gap is closable here.
- Fortran shape: 1589 fixed-form `.f` + 660 free-form; **<1% of files
  contain `BIND(C)`** (19 files). ~99% of Fortran ports go file-granularity
  behind an ABI shim, not per-procedure.
- Subsystem sizes (Fortran+C/C++ sources): `mathlibs` 1587 (bundled
  LAPACK/ARPACK-class third-party), `fem` 508, `elmerice` 177,
  `umfpack` 193, `elmergrid` 131, `meshgen2d` 60, `contrib` 60,
  `matc` 26, `fhutiter` 17, `misc` 19.

## 2. Worker contract (normative for any port author, human or LLM)

The harness invokes one worker per unit; the worker produces code, the
harness grades and merges. Worker interface (`crates/rustsmith-agent`):

- Command from `RUSTSMITH_WORKER_CMD`, cwd = unit worktree, prompt
  (`unit <id>` + PORTING.md + unit bundle) on stdin, env `UNIT_ID`,
  `UNIT_WORKTREE`. Exit 0 + stdout `{"tokens_in":N,"tokens_out":M}`
  (+ optional `note`); anything else is a worker failure, never graded.
- Read inputs: `.bundles/<unit>/` (orig source, scaffold audit
  `scaffold.json`, `prompt.txt`). Never touch held-out paths (none exist
  on this path by construction).
- Write port SOURCES to tracked `rust/<crate>/` (`Cargo.toml` staticlib +
  `src/lib.rs`; crate name = `scaffold_crate_name(file stem)` —
  lowercase-alphanumeric, `crates/rustsmith-adapters`). Build outputs stay
  under `build/` (ignored, never merged).
- Build the archive yourself into the worker-contract path
  `build/rust/lib<crate>.a` (mirror `expected_rust_lib`). The grade uses
  that exact file; a missing archive halts honestly before anything merges.
- Port rules: only `BIND(C)`-clean units substitute one at a time
  (`CmakeBridge::substitute` refuses the rest — port those whole files).
  Export the original linkage names (`#[export_name = "..."]`); the
  scaffold (`CmakeBridge::scaffold`) shows the exact expected shape,
  including the file-granularity ABI note. Non-`BIND(C)` module
  procedures, assumed-shape descriptors, and `COMMON` blocks are out of
  the per-procedure scope (whole-file ports only).
- Do NOT edit `CMakeLists.txt`, test files, or other units. The merge
  owns build edits (source removal + `target_sources`/`target_link_libraries`
  splice, same commit as the deletion).

## 3. Scale-up order

1. **Vendored exclusion first.** `mathlibs/` (1587) + `umfpack/` (193)
   are bundled third-party numerics: link targets, not port scope. Land
   coverage/vendored exclusion from frozen diagnostics before porting a
   line, or the scheduler's first win is LAPACK.
2. **Pilot: `matc` (26 files) or `fhutiter` (17).** Self-contained,
   clear I/O boundary, leaf-first over the slice DAG. Measure per-unit
   cost/time here — it extrapolates the full-port budget.
3. **Expand by subsystem** (`elmergrid` → `meshgen2d` → `fem/` core),
   F77 `COMMON`-block files last.
4. **Never**: full-tree mirror in one run before (1)–(2) report costs;
   speedup claims (safety/maintainability payoff only).

## 4. Remaining tracks (all broken ground, none papered over)

Each names the file + symbol where work starts. Python-spine behavior is
frozen by the suite (122/0); keep it byte-identical.

1. **File API target resolution** (`mirror.rs`: `owning_target`).
   Token→target heuristic fails on variable/glob source lists (all of
   Elmer: `ADD_LIBRARY(elmersolver SHARED ${solverlib_SOURCES})`) with an
   honest halt. Real fix: create the File API query in `cmake_configure`,
   resolve units via `parse_cmake_file_api_reply`
   (`rustsmith-adapters`, already parses codemodel replies).
2. **Shared pristine build per run** (`mirror.rs`: per-unit `orig_src`
   staging). Correct today, 2× full builds per unit — prohibitive at
   Elmer scale. Build once per run, reuse across units.
3. **Frozen coarsened file-units** (`rustsmith-adapters`:
   `stems_to_call_graph` collapses `UnitDecl`s). Non-`BIND(C)` Fortran
   must freeze at file granularity with exports, or grade-time decls
   stay stem-shaped. Needs the forgiving-order interplay handled.
4. **Recon-build at recon** (`recon.rs`: `discover_ctest_baseline`
   exists; Elmer run pending). Configure `~/runs/elmer-work/orig` into
   `$HOME/runs/elmer-work/recon-build`, freeze the 482-test baseline +
   out-of-source `prepare`. Then Elmer mirror counts check against
   reality instead of halting on `count_mismatch`.
5. **Workload probes per subsystem** (`repo-content.json`
   `differential.probes`, Mini-shaped precedent). Differential without
   frozen inputs is meaningless; pilot subsystems need argv-driven probe
   binaries + probe lists as data, never code.

## 5. Honest-halt catalog (error → meaning → fix)

- `unknown package … pyproject.toml` (pre-fix-recon only): Python
  identity gate before the probe. Fixed (frozen `probe.package`).
- `no mirror template for package 'X'`: needs `mirror/X/` (spine marker
  + base files). Elmer's exists.
- `substitute: rust lib … does not exist`: worker produced no archive.
  Write the port; never bypass.
- `exports non-BIND(C) Fortran …`: ABI gate fired correctly. Coarsen to
  file granularity (track 3), don't force per-procedure ports.
- `no differential probes for package`: add `differential.probes` data
  (+ argv-driven probe binary). Never a vacuous pass.
- `ambiguous/no CMakeLists match`, `cannot resolve owning target`:
  build-edit limits. Track 1 for variables; keep token edits exact.
- `oracle_tamper … count_mismatch` with empty frozen baseline: undiscovered
  baseline (unbuilt recon). Track 4; do not relax the gate.

## 6. Session guidance

- Open execution sessions per pilot phase (§3), not one big session:
  each phase ends with measured cost + a green slice.
- Gate every session on tracks in §4 it needs (pilot needs 1 at minimum
  for the first merge past a variable-list target).
- Sibling runs (packaging, charset-normalizer) share nothing but the
  machine: isolate `--fork`/`--work`/`--store`/`--run-id` explicitly.
- Never `--stage full` on Elmer until a pilot slice reports green costs.
  Never push; publishing is a separate manual step after review.

## Pilot slices (matc/fhutiter)

Data landed in `crates/rustsmith-cli/data/repo-content.json` under the
`matc` and `fhutiter` package keys (subsystem-scoped recon resolves these
names from the directory name: neither `matc/CMakeLists.txt` nor
`fhutiter/CMakeLists.txt` carries a `PROJECT()` line). No `.rs` file was
touched.

### Binary layout found (read from orig CMakeLists + confirmed by build)

- `matc/src/CMakeLists.txt`: `ADD_LIBRARY(matc SHARED ...)` (19 C files)
  plus exactly one executable, `ADD_EXECUTABLE(Matc_bin main.c)` with
  `OUTPUT_NAME matc`. Full-tree build artifact: `<build>/matc/src/matc`
  alongside `libmatc.so`. No CTest registrations anywhere under `matc/`.
- `fhutiter/src/CMakeLists.txt`: `ADD_LIBRARY(fhuti SHARED ...)` (10 F90
  files + 2 headers) and **no executable at all** — artifact is only
  `<build>/fhutiter/src/libfhuti.so`. The `examples/ex1` driver
  (`hutiexample`, stdin-fed matrix file + leading dimension via
  `testmat.script`) is not wired into CMake, so it never builds.
- `main.c` (matc) ignores `argv` entirely and runs a `fgets` REPL fed by
  stdin; only the `exit`/`quit` lines terminate with status 0. fhutiter
  sources contain zero `BIND(C)` procedures (whole-file ports only).

### Probe design

- `matc` entry records one probe, `{"program": "matc/src/matc", "args":
  []}` — the real full-tree-relative binary path observed in
  `/tmp/elmer-probe-build` (`matc/src/matc` exists and runs). Empty args
  are load-bearing honesty: the binary reads no argv, so any args would
  imply an interface that does not exist.
- `fhutiter` entry records porting rules but deliberately **no**
  `differential.probes`: there is no built binary to reference, and
  inventing one would be false data. Grading package `fhutiter` therefore
  keeps today's fast honest halt (`no differential probes`) instead of a
  fabricated program path.
- Measured behavior of the real binary (from the /tmp build): `printf
  '1+2\nexit\n' | matc` exits 0 printing `         3`; `printf 'exit\n' |
  matc` exits 0 silent; bare `matc </dev/null` never terminates (infinite
  `MATC ERROR: Expecting identifier...` loop, killed by timeout, exit
  124). The differential signal exists but is stdin-fed, which the
  `{program, args}` runner shape cannot express today.

### Entry keys added

- `matc`: 4 porting rules (longjmp→Result, file-static globals→MatcCtx,
  `mtc_domath` C-ABI boundary, stdin-REPL as the differential surface) +
  the 1 probe above. Every rule carries literal `Original (C):`,
  `Rust:`, and `Example:` markers.
- `fhutiter`: 4 porting rules (whole-file non-`BIND(C)` ports with
  gfortran-mangled `export_name`, explicit-shape workspaces as sized
  slices, `external` matvec→`extern "C"` fn pointer, s/d/c/z solver
  families→one generic port) with literal `Original (Fortran):`,
  `Rust:`, and `Example:` markers.

### What a built-tree verification must check

1. Configure the pristine tree and build `Matc_bin` + `fhuti`; assert
   `<build>/matc/src/matc` and `<build>/fhutiter/src/libfhuti.so` exist
   (done once in /tmp on 2026-09-22: configure ~8 s, targets ~1 s).
2. Run the recorded `matc` probe argv-only in both trees and require exit
   0: EXPECTED TO FAIL TODAY (hang → 124) until the runner grows stdin
   support or a CTest-wired probe driver lands — do NOT grade package
   `matc` in the mirror loop before then, or every grade burns 2x600 s
   timeouts before halting.
3. The stdin-fed differential (`1+2` → `         3`, `exit` → silent 0)
   must compare byte-identical orig-vs-mirror once a stdin-capable runner
   exists; that is the real pilot differential for matc.
4. Full Elmer `cmake --build` (all targets, hours-scale) was NOT run —
   only configure + the two pilot targets. Re-verify probe paths after
   any `CMAKE_RUNTIME_OUTPUT_DIRECTORY` change, which would relocate the
   `matc` binary.
