# ADR-058: T021-A solver-entry ports — regrade, bundled-library refusals, staging notes

Date: 2026-10-28. Scope: lane T021-A (fem/src/modules basenames A–L)
under the merged T020 entry shape; no gate changes (no new shapes).

## 1. Entry-shape regrade over the modules corpus

A temporary `elmer-regrade` copy pointed at
`fortran_plugin_entry_verdict` (file model, frozen dag exports; driver
removed after the run): 184 `fem/src/modules` file units → **139 admit
/ 45 refuse**. A–L slice: **61 admit / 21 refuse** (motifs recorded in
`elmer-rust` `T021A-LEDGER.md`). This is the admissible class the
report §8 projected as ~157 export-clean (whole-corpus number, same
shape). Lane T021-B owns the M–Z remainder.

## 2. Bundled `.so` discovery (new refusal class)

Five subdirectory groups do NOT build one `.so` per file (`ADD_MODULE`
per directory, `nm -D`-proven on the serial reference build):

- `LevelSet/` → one `LevelSet.so` (6 entries: 5 solvers +
  `levelsetintegrate_init_`, incl. gate-refused `levelsettimestep_`);
- `BatterySolver/` → one `BatterySolver.so` (module-procedure surface);
- `ConsistentSplitting/` → one `ConsistentSplitting.so` (4 F77 entries
  across 4 files, 3 of them lane-B);
- `MagnetoDynamics/` → one `MagnetoDynamics.so` (module procs +
  gate-refused `CalcFields`);
- `ResultOutputSolve/` → one `ResultOutputSolve.so` (6 writer entries +
  VTK module procs).

A per-file port cannot reproduce these libraries' dynamic surfaces
without porting every sibling (one sibling is gate-refused in two of
the five; one library crosses the A–L/M–Z lane split). All 10 affected
A–L admits refuse as `refused-shared-lib` with this reason. A
whole-library port (one crate, all sibling entries) is the correct
future shape, owned by no current lane.

Corollaries adopted as lane rules: the admissible-unit query must join
the gate verdict with the `nm` surface of the built reference `.so`
(source scan alone overclaims); `HarmonicSignals.F90` refuses as
`refused-new-abi` (its `.so` also exports module procedures with
assumed-shape `REAL(8)` dummies — callee-side descriptor ABI is an
unmeasured caller shape, T020 fixtures cover Rust-as-caller only).

## 3. Staging notes (proven, reusable)

- `.sif` bodies must cover the mesh's bulk bodies when an `Initial
  Condition` is present: square.grd carries bodies 1–4 and
  `SetInitialConditions` indexes `Bodies(body_id)` per element —
  single-`Body 1` + IC reads out of bounds (deterministic SIGSEGV,
  gdb-proven `i = 3`). All IC proofs in this lane declare 4 bodies
  (T020 Poisson configs already did).
- Solver equation names must be unique across solvers (loader `FATAL`
  otherwise) — owner-solver staging (e.g. three `AllocateSolver`
  owners, one per field) needs distinct names.
- Campaign invocation uses library-vs-entry names that may differ
  (`Procedure = "EnergyRelease" "ReleaseRateSolver"` → symbol
  `releaseratesolver_`); the proof must use the campaign form, and the
  loader's `Can't find procedure [EnergyRelease]` failure mode is the
  tell for a wrong entry name.
- Level-5 `RealTime` reports are nondeterministic by construction;
  `Max Output Level = 4` suppresses them structurally while keeping the
  deterministic `Level=4` `Result Norm`.
- Reference accessor recipe matches the modules' `flags.make`:
  `-fPIC -fallow-argument-mismatch` (the legacy-MPI-idiom
  `MPI_ALLREDUCE` mixed-type calls error without the latter, in the
  original file too).
- Under `set -e`, a nonzero `ElmerSolver` subshell kills the proof
  before `local rc=$?` runs — proof helpers that expect nonzero exits
  (negative controls) must use `|| rc=$?`.

## 4. Yield

6 graded A–L ports (AllocateSolver, DivProjection, ElementSizeSolver,
DivergenceSolver, ElementStats, EnergyRelease) + 76 dispositioned
(21 gate-refused with motifs, 12 shared-lib, 1 new-ABI, 1 unbuilt
optional dependency, 41 deferred in smallest-first order). Lane capture
`rs-elmer-t021a-01`: 6 passed units, 12 gate rows (oracle_integrity +
differential each). Backstop: fresh tree @ `9f6af2f85`, campaign
configure, all six ports spliced: **ctest -L quick 479/482, failures
exactly K=3 (#80/#536/#537)**.
