# ADR-059: T022-A deferred A–L ports — triage resolutions, staging footguns, lane mechanics

Date: 2026-10-28. Scope: lane T022-A (the 44 `deferred-lane-budget` rows
of T021A-LEDGER.md, basenames A–L, each with its own reference `.so`
in the serial build). No gate changes (no new shapes). Result: 44/44
ported, 0 refusals. Count note: the T021-A report text says "41
deferred"; the ledger lists 44 `deferred-lane-budget` rows, and all 44
are dispositioned here.

## 1. Triage-first hazards, both resolved as plain-ABI (no new-ABI refusals)

- `CoordinateTransform.so` exports four `__vecutils_MOD_*` module
  procedures. Inspection: all dummies explicit-shape `REAL(8)`
  (`a(3)`, `(3,3)` pairs/trio) — plain F77 ABI, zero descriptor
  dummies. Full port incl. the 4 procs transcribed statement-for-
  statement into `vecutils_rep.F90` (same compiler/flags → exact ABI);
  accessor hidden-visibility, vecutils object default-visibility,
  version script globalizes exactly the 6 symbols. No
  HarmonicSignals-class refusal.
- `HelmholtzBEM.so` exports `matvec_`/`precond_`. Inspection:
  assumed-size dummies only (`COMPLEX(KIND=dp) :: u(*),v(*)`,
  `INTEGER :: ipar(*)`) — plain ABI. `HUTI_*` names are CPP macros
  over `ipar`/`dpar` slots (verified in `huti_fdefs.h`); the whole
  HUTI sequence transcribes with plain indexing; `IterCallFTNC`
  (descriptor dummies) is reached via a Fortran wrapper and never
  crosses the C boundary. Full port.
- Precedent set by the BEM port: a plugin-private module *data*
  symbol (`B __globmatc_MOD_matrix`) is re-declared same-statement in
  the accessor and globalized in the version script. Data symbols are
  part of the `nm` surface and must be replicated, not merely the `T`
  set.

## 2. Boundary-shape notes (admitted, proven at their own boundary)

- `EliminateDirichlet` / `EliminatePeriodic` are linsolve-hook units
  (`INTEGER FUNCTION` via `Load.c`'s `DoLinSolveProcs` C trampoline,
  8 plain pointers, int result — backtrace-confirmed), not
  solver-entry units. Both ported at that boundary with byte-identical
  differentials. `EliminatePeriodic` delegates to the reference
  `EliminateDirichlet.so`: the port replicates `NEEDED
  EliminateDirichlet.so` + RUNPATH (build links the reference tree's
  module dir via `ELMER_MODULESDIR`; backstop builds pass it).
- `Electrokinetics`' six UDF entries verified as
  `(Model, Node, dummy-real-slot) -> real` via `execrealfunction` in
  `LoadMod.F90`.
- Reference re-entry is UB in two units (proven, documented in proof
  headers, author's evident intent implemented, defined path +
  port-vs-port rerun per bar item 3): `FixTangentVelo` (non-`SAVE`
  Mesh/Params → all-zero FixVals at Level 4, segfault at Level 10);
  `CoilSolver` (double `ALLOCATE` of entry work arrays aborts
  in-process re-entry).
- Reference reads uninitialized memory on three paths (proven by
  same-binary self-disagreement; zeroed per the library's own branch
  for that case, defined path proven): `FDiffusion3D` (`Symb`/`dSymb`
  on Cartesian — `Symbols=0` per `CoordinateSystemInfo`); sibling
  turbulence units verified clean (their `Symb`/`Metric` reads sit
  strictly in non-Cartesian ELSE branches).

## 3. Staging footguns (proven, reusable; extend T021-A/058 §3)

- Unlisted solver keywords are dropped with desync unless
  type-prefixed (`= File …`, `= Integer …`, `= Logical …`); the
  parser lowercases unquoted values (`fieldc.dat`).
- Surface-only 3-D solvers need a REAL 3-D mesh (`cube.grd`);
  runtime extrusion adds no top/bottom faces, and tagging vertical
  walls can singularize the local system (upstream slab tests tag
  top/bottom only). Singular-staging variants of this: a single-face
  phi Dirichlet leaves the hp Laplacian singular (reference itself
  diverges — full 6-face Dirichlet required); iterative BiCGStab
  diverges on the CoilSolver1 Curve matrix in both arms (Direct/
  Umfpack is the meaningful differential); unpinned pressure blows
  to uniform 1e17 deterministically (pinned is the differential).
- Mesh bodies must cover ALL bodies when core indexing is per-body:
  3-body `.sif` segfaults in core `initcond` in both arms (bodies
  1–2 bulk + 3–4 boundary); reference `init` aborts on in-process
  re-entry for CoilSolver (see §2).
- Upstream defects found-and-avoided (defined paths only, port
  mirrors code statement-for-statement): `Calculate Nodal Force`
  without `Calculate Force Density` segfaults the reference
  (`ElectricForce.F90:172`, dangling pointer); elemental-field +
  first-order-time + no-constant-bulk fails its own iterative solve
  (EMWave configs isolate the two sides).
- Norm class (lane-merged, wall-clock only): drop `Elapsed REAL
  time|SOLVER TOTAL TIME|FINISHED AT|STARTED AT|File started at`;
  mask timing *values* on per-iteration lines via `: +\(s\)`
  (structure/iterations compared). Never widen beyond wall-clock.

## 4. Lane mechanics (for future Elmer lanes)

- 3-slot `measure-slot.sh` mkdir-mutex around proof runs (max 3
  parallel measurement jobs); isolated `CARGO_TARGET_DIR` per unit;
  `PORT_SO="${CARGO_TARGET_DIR:-$PORT_DIR/target}/release/lib<lower>.so"`.
- Wave structure (4→6→6→6→10 + 2 resumed completions) with a shared
  `PORT-BRIEF.md` accumulating wave lessons; honest-halt broadcasts
  (`Symb` UB, norm refinements) adopted lane-wide within the turn.
- Build requirements beyond `ELMER_MODDIR` must be code, not lore:
  `EliminatePeriodic`'s `ELMER_MODULESDIR` link need is asserted in
  its `build.rs` (`expect`), and the campaign backstop passes it.
- Per-unit cost: transcription dominates (largest: Acoustics 4735
  lines → lib.rs 1876 + accessor 6121); proofs run in ~1–3 s on tiny
  meshes except eigensolver/ARPACK staging (CraigBampton 430 s,
  EMPort 190 s). Total: 44 ports, zero refusals, zero gate changes.
