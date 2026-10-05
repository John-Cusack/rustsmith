# Elmer FEM → Rust: scale-up strategy + worker guide

Status: the CTest mirror loop is complete and proven on the committed
BIND(C) fixture (`tests/fixtures/mini`, `tests/elmer_mini_proof.sh`
GREEN: `mirror: 1/1 passed divergence=0.0000`, merged port live;
S1 scope-skip landed). This document is the execution plan for scaling
that loop to Elmer itself. Numbers below are measured on this host,
not estimated.

## 1. Ground truth (measured 2026-09-22)

- Recon: `cmake` spine, **3025 file-granularity units** — `fortran` 2094,
  `c` 600, `cxx` 320, `python` 11 (only 2 `#`-suffixed survivors: BIND(C)
  multi-symbol files; everything else is one unit per file). Frozen per
  unit: `exports` (linkage + `bind_c`) on 3019, `out_of_scope` (61 vendored
  `contrib/` ids, structural), `diagnostics` (2653).
- Baseline: **`test_count: 482`** — configure-only recon build
  (`work/recon-build`, ~7 s) + `ctest -N` scoped to the graded `-L quick`
  selection (1100 total; the label scoping is load-bearing, else every
  grade trips `count_mismatch`).
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
## 4. Track status (landed vs open)

Each names the file + symbol where work lives. Python-spine behavior is
frozen by the suite (132/0); keep it byte-identical.

### Landed (do not redo; extend only)

1. **File API target resolution** — query writer
   (`rustsmith-adapters::write_file_api_query`), resolver
   (`file_api_target_for_source`: exact → basename → `None`), consumed by
   merge with the token heuristic as fallback. Variable-fed targets
   (`${solverlib_SOURCES}`) now resolve via the configured reply.
2. **Shared pristine build per run** (`mirror.rs`:
   `build_shared_pristine`, one `pristine_build` event) — differential
   reuses one orig build instead of per-unit staging.
3. **Frozen coarsened file-units** (`CompositeAdapter::declared_units`;
   recon freezes canonical ids + `exports` + `out_of_scope` +
   `diagnostics`; `CallGraph.module_exports` backs the shape). Grade decls
   prefer frozen exports, live re-derivation as fallback.
4. **Recon-build baselines** (`recon.rs`: `discover_ctest_baseline` +
   `ctest_list_args`) — Elmer smoke froze 482. Configure failure still
   degrades, never halts.
5. **Build-aware merge** (`mirror.rs`: `cmake_remove_sources` →
   owning targets; merge drops sources, appends `target_sources` (shared
   empty TU, per-target guarded) + `target_link_libraries` (port archive);
   `ensure_merged_archives` before every worktree build). Whole-repo
   rebuilds archives before configuring.
6. **Merge-safe tamper** (`CtestRunner::normalize_for_hash`):
   `CMakeLists.txt` hashes test-defining lines only.
7. **Scheduler scope-skip + pilot subset filter** (`mirror.rs`:
   `out_of_scope_set`, `scope_prefixes`/`in_scope` from `RUSTSMITH_SCOPE`
   (empty/unset = whole tree), `skip_reason`, `dep_satisfied`; scheduler
   records status `skipped` + `unit_skip` event with reason
   `out_of_scope`/`outside_scope`, no grade, no merge; ready-check treats
   `skipped` deps as satisfied, link don't port). Headers/helpers skip
   standalone here; header-follower attachment is a later track.
   Proof `tests/elmer_mini_proof.sh` GREEN (`mirror: 1/1 passed
   divergence=0.0000`; `fortran:src/mini_add.F90` passed,
   `c:include/mini_helper.h` skipped `outside_scope`). Elmer slices on the
   frozen recon (3025 units, 61 `out_of_scope`; checkout
   `release-26.2-641-g9f6af2f85`, NOT the `release-26.2.1 @ a19504a` the
   earlier docs name): `RUSTSMITH_SCOPE=fhutiter` schedules 1
   (`fortran:fhutiter/src/huti_interfaces.F90`), skips 1601
   `outside_scope` before the honest halt, 1423 queued behind it, halts
   `substitute: … exports non-BIND(C) Fortran …` (118 s);
   `RUSTSMITH_SCOPE=matc` schedules 1 (`c:matc/src/str.h`), skips 2437
   `outside_scope`, 587 queued, halts `substitute: no built object for
   'c:matc/src/str.h'` (117 s). Neither halts on scope. Static full-slice
   expectation (no halt): fhutiter 17 scheduled / 61 `out_of_scope` / 2947
   `outside_scope`; matc 26 / 61 / 2938. Suite 165/0 (161 + 4 new S1 tests).

### Open (session-sized; see §7)

8. **Stdin-capable probes** — `matc` differential is stdin-fed; the
   `{program, args}` shape cannot express it.
9. **Real workers** — scripts hold `RUSTSMITH_WORKER_CMD`; no model backs
   it, Fortran→Rust prompt quality untested.
10. **ABI shim design** — 99% of Fortran is non-`BIND(C)` and the gate
   correctly refuses it. Needs the mangled-name/descriptor strategy plus
   proof. Never relax the gate without both.
11. **Full Elmer build here** — configure passes; nobody has compiled the
   tree on this host (compile-DB refinement, coverage, build-time numbers
   all wait on it).

## 5. Honest-halt catalog (error → meaning → fix)

- `unknown package … pyproject.toml` (pre-fix-recon only): Python
  identity gate before the probe. Fixed (frozen `probe.package`).
- `no mirror template for package 'X'`: needs `mirror/X/` (spine marker
  + base files). Elmer's exists.
- `substitute: rust lib … does not exist`: worker produced no archive.
  Write the port; never bypass.
- `exports non-BIND(C) Fortran …`: ABI gate fired correctly. Whole-file
  port behind a shim (§4 open-10); don't force per-procedure ports.
- `no differential probes for package`: add `differential.probes` data
  (+ argv-driven probe binary). Never a vacuous pass.
- `ambiguous/no CMakeLists match`, `cannot resolve owning target`:
  token heuristic limits (File API reply resolves variables; absent reply
  falls back). Keep token edits exact.
- `substitute: no built object for '<lang>:<path>'`: the unit has no
  compilable claim on this spine (helpers, headers). Not a port failure —
  needs scheduler scope-skip (§4 open-7). Do NOT force it through grade.
- `No rule to make target '…/build/rust/lib*.a'`: merged archive missing
  at worktree build. `ensure_merged_archives` covers this; seeing it now
  is an ordering regression — report, don't work around.
- `oracle_tamper … count_mismatch` with empty frozen baseline: stale
  (pre-discovery) recon. Re-run recon; do not relax the gate.
## 6. Session guidance

- Open execution sessions per brief in §7, not one big session: each ends
  with measured cost and/or a green slice. One worktree + one branch per
  session; file ownership from the brief (same-file edits do not merge).
- Gate every session on the brief's prerequisites (S1 unblocks the pilot;
  S4 is independent research and can run anytime).
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

## 7. Session briefs (self-contained execution slices)

Each brief is one session: goal, file ownership, acceptance, non-goals.
Assume the repo at `main` plus this document. Isolate all run dirs
(`--fork`/`--work`/`--store`/`--run-id` under `/tmp`, never `~/runs/*`).
Never push. Keep Python-spine output byte-identical (suite pins it).

### S1 — Scheduler scope-skip + pilot subset filter (unblocks the pilot)

- Goal: mirror skips unportable units instead of halting on the first
  helper script. Today it dies on `python:umfpack/src/umfpack/deps.py`
  (`no built object`) with 2900+ units still queued behind it.
- Files: `crates/rustsmith-cli/src/mirror.rs` (scheduler loop only).
- Change: read `dag.json` `out_of_scope` into a set; units in it get
  status `skipped` (recorded event, no grade, no merge); ready-check
  treats `skipped` deps as satisfied (link, don't port). Add a
  prefix allowlist from env (e.g. `RUSTSMITH_SCOPE=matc,fhutiter`;
  empty = whole tree, current behavior): units whose repo-rel matches no
  prefix are skipped the same way. Both recorded in store events.
- Acceptance: Elmer mirror with `RUSTSMITH_SCOPE=matc` (or `fhutiter`)
  schedules only the slice, skips the rest with reasons, and reaches the
  first real unit's grade (expect the honest ABI halt on non-`BIND(C)`,
  not a scope halt); 2-unit fixture proof still green; suite green.
- Non-goals: header-follower redesign (decide: standalone skip now,
  followers later — document the call); touching recon frozen shape.

### S2 — Stdin-capable probes (unblocks the matc differential)

- Goal: run a probe with stdin in both trees and compare stdout, so the
  recorded `matc` probe (`1+2` → `         3`) grades instead of hanging
  to timeout (exit 124 today).
- Files: `crates/rustsmith-core` (`TestCommand` stdin field — additive,
  executor honors it), `crates/rustsmith-cli/src/mirror.rs`
  (`differential_ctest_pairs` probe shape + runner),
  `data/repo-content.json` (`matc` probes gain `stdin`).
- Acceptance: fixture proof with a stdin-fed probe pair passing;
  argv-only probes byte-identical behavior; suite green. Document the
  exact `stdin` data shape in this section.
- Non-goals: pty/interactive sessions (pipes only); changing the
  `{program, args}` shape for existing entries.

### S3 — Real workers + pilot run (needs S1; S2 for matc, optional after)

- Goal: back `RUSTSMITH_WORKER_CMD` with a model and run the pilot slice
  green. Deterministic scripts hold the interface today (§2 contract);
  this session replaces the script with model calls and measures.
- Files: none required (worker is external); MAY add prompt-shaping
  helpers in `crates/rustsmith-agent` if the model needs stricter
  scaffolding, with tests.
- Acceptance: `fhutiter` slice (or stdin-ready `matc`) fully green in
  mirror with measured per-unit wall time; report extrapolates the
  full-port budget. Every gate honest; nothing forced.
- Non-goals: prompt-engineering marathons (two iterations max, then
  report); touching grade code to accommodate weak ports.

### S4 — ABI shim design (independent research, runs anytime)

- Goal: the strategy that lets the 99% (non-`BIND(C)` Fortran, mangled
  names, array descriptors) substitute soundly. The gate correctly
  refuses today; this designs the mechanism, not the relaxation.
- Files: none until the design lands (then adapters + mirror per the
  design). Read: `CmakeBridge::substitute` + `check_substitutable`
  (`rustsmith-adapters`), the `ld -r` object-replacement flow, gfortran
  descriptor layout docs.
- Acceptance: a design note (append `docs/adr/` entry): symbol mapping
  (mangled names via `export_name`), descriptor layout handling,
  assumed-shape vs explicit-shape matrix, what stays refused (F77
  `COMMON`), and a fixture proof plan. NO gate relaxation without the
  proof.
- Non-goals: implementing the shim (separate build session after the
  design is reviewed).

### S5 — Full Elmer build validation (machine time, runs anytime)

- Goal: compile the whole tree on this host (never done): real
  compile-DB (refines C++ claims), coverage data (activates uncovered
  scope marks), honest build-time numbers for pilot budgeting.
- Files: none (build into `/tmp`, never the repo). Commands:
  `cmake -S <orig> -B /tmp/elmer-full -DCMAKE_BUILD_TYPE=Debug` then
  `cmake --build /tmp/elmer-full --parallel`.
- Acceptance: report build endpoint (green with warnings list, or exact
  first failure), wall time, artifact size, whether
  `compile_commands.json` was emitted. On green, note it here so later
  sessions can rely on the DB/coverage paths.
- Non-goals: running the test suite (hours); fixing Elmer upstream
  build issues (report them).
