# ADR-049: measured CPU headroom for the Elmer campaign units

Date: 2026-10-20. Status: accepted.

## Context

The Elmer campaign partitioned 1157 remaining units into 8 lanes
(`data/elmer-lanes/lanes.json`, 3025 frozen − 61 out-of-scope − 1781 vendored
− 26 done `matc/`). Eight lanes have already run against that partition and
recorded a machine verdict per unit: 53 units graded/ported, the rest recorded
skips with a reason. The question this ADR answers is the one the campaign
cannot answer by grading: **which of the 1157 units actually carry CPU
headroom worth a Rust port, and which should stay refused or deferred.**

ADR-044 established the discipline for that question — measure a probe row per
candidate, split the measured cost into five classes, score
`pure × loop_factor`, apply three caps, and let the measured row move the
nomination. Its tool probes Python packages. Elmer units are Fortran, C and
C++, so ADR-044's rules are reused here and its *tool* is not: the buckets are
re-defined for compiled code, and the probe is a call-graph profile of a real
Elmer run.

## Decision

### 1. Substrate

- Source: upstream ElmerCSC/elmerfem @ `9f6af2f85` — the campaign's pinned
  revision — copied clean out of the local upstream clone and built with
  cmake 3.28.3, `RelWithDebInfo` (`-O2 -g -DNDEBUG`), gfortran 13.3.0 /
  gcc 11.5.0, system BLAS/LAPACK, bundled UMFPACK/ARPACK/PARPACK.
- **Labelled toolchain deviation**: the campaign's pinned nightly
  `gfortran 1.100.0 (feaadeeac)` is not installed on the measurement host, so
  the measurement build used gfortran 13.3.0. This moves absolute speed; it
  does not move the relative attribution a per-unit share rests on, and it
  does not touch the port work itself (ports stay gated on the campaign's
  pinned toolchain).
- **Profiler**: `valgrind --tool=callgrind`. `perf` is unavailable on this
  host (`kernel.perf_event_paranoid=4`, no sudo), so the cost metric is
  callgrind `Ir` (executed instructions) — deterministic and reproducible
  rather than sampled. Wall/user/sys are measured separately
  (`/usr/bin/time -v`) and syscall time separately (`strace -c`), which is
  what the `io_wait` class needs.
- Attribution is per **source file** (callgrind's `fi=`/`fl=` records, with
  inlined header code credited to the header), then mapped onto the campaign
  unit ids by exact path.

### 2. Workload

All 872 upstream `fem/tests` cases were timed natively first. Only 17 exceed
0.2 s of wall: the rest are smoke tests whose cost is process startup, not
solver work. Measured on the 13-case module-diverse smoke sweep, 99.4% of the
executed instructions are runtime/startup (dynamic loader, `libc`, allocator)
and only 0.5% are campaign source — profiling those would have measured
`ld.so`, not Elmer.

- **fem workload** (lanes `lane-fem-a`, `lane-fem-b`): the 16 cases with
  measured native wall ≥ 0.45 s — 45.67 s native wall, 416.6 G Ir. A
  13-case module-diverse sweep is kept as a *coverage* tier (presence only).
- **ice workload** (lane `lane-ice`): 9 elmerice cases from `elmerice/Tests`,
  profiled with the MPI rank launched under `mpirun -n 1` (a direct run
  profiles only the launcher, which forks `orted`). Only one is compute-grade
  by the ≥ 3 s native-wall rule (`Density`, 3.66 s); the other 8 are
  presence-only. The heavy ice cases (`ThicknessSolver` 44.5 s, `Glen_2D`
  11.8 s, `SIA` 10.8 s) exceeded the profiling budget at the measured
  instrumentation factor and are **not** profiled, so this pass *bounds* the
  ice lane rather than settling it: in the one compute-grade ice case the
  campaign's elmerice units account for < 0.01% of the measured CPU (60% of
  it is `libc`/allocator, ~13% the `matc` library, which is outside the
  partition).
- **grid workload** (lane `lane-grid-io`): 3 ElmerGrid mesh conversions
  (Elmer mesh dir → vtu, gmsh → vtu, `.grd` → Elmer mesh; 0.09 s native
  wall, 1.07 G Ir).
- **Not measured**: `lane-leaf` (meshgen2d), `lane-gui-a/b/c` (netgen,
  ElmerGUI). No headless CPU-bound substrate was measured for them, so every
  unit in those lanes is reported `not benchmarked` → `defer`. No number for
  them is guessed.

### 3. Buckets — ADR-044's five classes on compiled code

| ADR-044 class | Elmer meaning (all measured) |
|---|---|
| `pure_python` → `pure_source` | CPU in the unit's own source (and in other Elmer source it calls) — the work a Rust rewrite would own |
| `native` | CPU already in compiled/vendored math code: `mathlibs/`, `umfpack/`, system BLAS/LAPACK/libm |
| `io_wait` | blocked time: `wall − (user + sys)` over the measured run, cross-checked with `strace -c` syscall time |
| `regex` | shared C regex engine: **0.0** — no regex engine appears in any measured call graph (absence measured, not assumed; Elmer has no shared regex engine on these paths) |
| `overhead` | runtime glue and unattributed code: libc/libstdc++/libgfortran, dynamic loader, allocator, I/O runtime |

### 4. Scoring — ADR-044 rules, unchanged except where stated

- `score = pure_source × loop_factor`.
- `loop_factor` from the unit's own concentration (ADR-044's rules, computed
  on the unit's hot functions): top1 ≥ 25% or (top5 ≥ 55% over ≤ 3 hot
  functions) → `tight_loop` 1.0; ≥ 8 hot functions or top5 < 25% → `dispatch`
  0.5; otherwise `pipeline` 0.85.
- Caps, applied as measured: **7a** thin profile (every measured case < 2 s
  wall and the whole workload < 0.5 s CPU) over a shipped compiled cover (the
  unit's cost dominated by native calls) → score ≤ 0.10, `exclude`;
  **7b** `regex` ≥ 0.35 → at best `upstream_only`; **7c** `io_wait` ≥ 0.50 →
  at best `upstream_only`.
- Bands: score ≥ 0.50 → `rewrite`; ≥ 0.25 → `upstream_only`; else `exclude`.
- **Campaign cut (new, stated here)**: a unit is a port candidate only if a
  port could move ≥ 1% of its lane's measured workload CPU
  (`headroom_share ≥ 0.01`). Below that the row is measured but the unit is
  not worth a port → `exclude`. A share is only comparable *inside* a lane:
  each lane's shares are taken in its own workload population.
- Standing exclusions: units already ported/graded (prior lane verdicts
  `passed` / `gated` / `grade` / `link-closure-only`) → `exclude`; units the
  prior ledger marks not-in-build / header-helper / absent-dependency →
  `exclude`; units a prior lane refused → `defer` (unblock the seam first,
  then port); units never measured → `defer`.

### 5. Machine rows

`data/headroom/elmer-<lane>.json`, one file per lane, one row per unit
(1157 rows total), in the ADR-044 row shape: `unit`, `lane`, `shape`,
`shape_class`, `tool`, `cost_metric`, `machine`, `toolchain`, `workload`,
`fractions{pure_source, native, io_wait, regex, overhead}`,
`concentration{top1, top5, hot_modules}`, `top[]`, `loop_nature`,
`loop_factor`, `score`, `verdict`, `why`, plus `cpu_share`,
`coverage_share`, `headroom_share`, `caps`, `evidence[]` and the prior lane
record. The five ADR-044 fields that have no meaning for compiled code are
carried as measured, not as blanks: `regex` is 0.0 because no regex engine is
in the measured call graph, and `io_wait` is the lane workload's measured
blocked-time share.

## Measured results
### Per-lane counts

| lane | shape | measured workload | units | measured | rewrite | upstream_only | exclude | defer |
|---|---|---|---|---|---|---|---|---|
| lane-fem-a | foreign-shape | fem | 168 | 69 | 0 | 1 | 98 | 69 |
| lane-fem-b | foreign-shape | fem | 184 | 23 | 0 | 0 | 21 | 163 |
| lane-grid-io | io-boundary | grid | 166 | 7 | 0 | 0 | 8 | 158 |
| lane-gui-a | interactive | none | 124 | 0 | 0 | 0 | 0 | 124 |
| lane-gui-b | interactive | none | 123 | 0 | 0 | 0 | 0 | 123 |
| lane-gui-c | interactive | none | 148 | 0 | 0 | 0 | 21 | 127 |
| lane-ice | foreign-shape | ice | 184 | 1 | 0 | 0 | 6 | 178 |
| lane-leaf | matc-like | none | 60 | 0 | 0 | 0 | 0 | 60 |

1157 rows over 8 lanes; 100 units carry a measured CPU presence in their lane's workload.

### Measured workload populations

- **lane-fem-a** (fem): compute set: 16 cases, summed wall 45.67s, user 44.20s, sys 1.32s, measured io_wait share 0.0033; coverage sweep: 13 further upstream cases (presence only, not part of the CPU aggregate).
- **lane-fem-b** (fem): compute set: 16 cases, summed wall 45.67s, user 44.20s, sys 1.32s, measured io_wait share 0.0033; coverage sweep: 13 further upstream cases (presence only, not part of the CPU aggregate).
- **lane-grid-io** (grid): compute set: 3 cases, summed wall 0.09s, user 0.07s, sys 0.00s, measured io_wait share 0.2222; coverage sweep: 0 further upstream cases (presence only, not part of the CPU aggregate).
- **lane-gui-a**: no measured workload - lane not benchmarked: no headless CPU-bound substrate measured.
- **lane-gui-b**: no measured workload - lane not benchmarked: no headless CPU-bound substrate measured.
- **lane-gui-c**: no measured workload - lane not benchmarked: no headless CPU-bound substrate measured.
- **lane-ice** (ice): compute set: 1 cases, summed wall 3.66s, user 3.05s, sys 0.20s, measured io_wait share 0.1120; coverage sweep: 11 further upstream cases (presence only, not part of the CPU aggregate).
- **lane-leaf**: no measured workload - lane not benchmarked: no headless CPU-bound substrate measured.

### Top-10 measured units (ranked by measured headroom)

The share is of *that lane's* measured workload: lanes are measured against different substrates, so the columns are not comparable across lanes - the ranking is the campaign weight inside each lane.

| # | lane | unit | headroom % of lane workload | measured Ir | score | loop nature | verdict |
|---|---|---|---|---|---|---|---|
| 1 | lane-fem-a | `fortran:fem/src/CRSMatrix.F90` | 30.05% | 125,361,746,949 | 1.00 | tight_loop | defer |
| 2 | lane-grid-io | `c:elmergrid/src/egextra.c` | 6.02% | 64,298,090 | 1.00 | tight_loop | exclude |
| 3 | lane-grid-io | `c:elmergrid/src/egmesh.c` | 5.73% | 61,606,527 | 0.99 | tight_loop | exclude |
| 4 | lane-fem-a | `fortran:fem/src/LinearForms.F90` | 3.94% | 16,764,143,552 | 0.98 | tight_loop | defer |
| 5 | lane-fem-a | `fortran:fem/src/Smoothers.F90` | 2.31% | 9,644,811,880 | 1.00 | tight_loop | defer |
| 6 | lane-fem-a | `fortran:fem/src/NavierStokes.F90` | 1.92% | 8,078,226,475 | 0.99 | tight_loop | defer |
| 7 | lane-fem-a | `fortran:fem/src/ElemInfo.F90` | 1.61% | 13,663,549,113 | 0.49 | dispatch | upstream_only |
| 8 | lane-fem-a | `fortran:fem/src/Lists.F90` | 1.51% | 6,475,944,737 | 0.97 | tight_loop | defer |
| 9 | lane-fem-b | `fortran:fem/src/modules/MagnetoDynamics/WhitneyAVHarmonicSolver.F90` | 1.35% | 5,651,148,120 | 0.99 | tight_loop | defer |
| 10 | lane-fem-b | `fortran:fem/src/modules/TurbulenceModels/SSTKomega.F90` | 1.24% | 5,795,184,461 | 0.89 | tight_loop | defer |

### Port next (`rewrite`)

- none: every measured unit above the 1% campaign cut is blocked by a prior lane verdict (see below).

### Unblock then port (measured headroom, refused by the port seam)

- `fortran:fem/src/CRSMatrix.F90` (lane-fem-a): 30.05% of the lane workload, 125,361,746,949 Ir, score 1.00, tight_loop - blocked: check_substitutable: non-BIND(C) Fortran exports (77); no T014 relaxation landed
- `fortran:fem/src/LinearForms.F90` (lane-fem-a): 3.94% of the lane workload, 16,764,143,552 Ir, score 0.98, tight_loop - blocked: check_substitutable: non-BIND(C) Fortran exports (8); no T014 relaxation landed
- `fortran:fem/src/Smoothers.F90` (lane-fem-a): 2.31% of the lane workload, 9,644,811,880 Ir, score 1.00, tight_loop - blocked: check_substitutable: non-BIND(C) Fortran exports (35); no T014 relaxation landed
- `fortran:fem/src/NavierStokes.F90` (lane-fem-a): 1.92% of the lane workload, 8,078,226,475 Ir, score 0.99, tight_loop - blocked: check_substitutable: non-BIND(C) Fortran exports (7); no T014 relaxation landed
- `fortran:fem/src/Lists.F90` (lane-fem-a): 1.51% of the lane workload, 6,475,944,737 Ir, score 0.97, tight_loop - blocked: check_substitutable: non-BIND(C) Fortran exports (176); no T014 relaxation landed
- `fortran:fem/src/modules/MagnetoDynamics/WhitneyAVHarmonicSolver.F90` (lane-fem-b): 1.35% of the lane workload, 5,651,148,120 Ir, score 0.99, tight_loop - blocked: external (F77-global) subroutine scope: no MODULE, synthetic MOD linkage unprovable by gate parse (T017)
- `fortran:fem/src/modules/TurbulenceModels/SSTKomega.F90` (lane-fem-b): 1.24% of the lane workload, 5,795,184,461 Ir, score 0.89, tight_loop - blocked: exports non-BIND(C) Fortran (13): no stable C ABI; needs T014 shim-shape relaxation
- `fortran:fem/src/IterativeMethods.F90` (lane-fem-a): 1.18% of the lane workload, 4,927,389,455 Ir, score 1.00, tight_loop - blocked: check_substitutable: non-BIND(C) Fortran exports (27); no T014 relaxation landed

### Stop pursuing

- 155 units carry `exclude` or `upstream_only`; 21 of them the prior lane ledger marks not-in-build / header-helper / absent-dependency (never a port target: no compilation unit, or the optional dependency is off).
- the rest of the `exclude` set is measured and below the 1% campaign cut, already ported, or a dispatch shape rather than a portable loop.
- 1057 units with no measured CPU at all (not benchmarked, never guessed): the `lane-gui-a/b/c` and `lane-leaf` lanes have no headless CPU-bound substrate, and the rest were not exercised by the measured workload set.

## Consequences

- **Port next**: the measured headroom that clears the campaign cut is real,
  but *every* unit that clears it is refused by a prior lane verdict, so no
  unit is a straight "rewrite" today. The campaign's next porting move is
  therefore not another unit — it is the **Fortran port seam** that refuses
  these units.
- **Unblock then port**: the fem-a core units carry the headroom
  (`CRSMatrix.F90` alone is 30.05% of the measured fem workload CPU, in a
  `tight_loop` hot path, 99.9% portable source). The prior lane refused them
  with `check_substitutable: non-BIND(C) Fortran exports (N); no T014
  relaxation landed`. The T014 relaxation (rustsmith#92) has since landed but
  admits exactly one narrow shape, and the fem-b round-2 regrade *with* T014
  relaxed still refused 217 of 218 scopes — so the blocker is real, not stale.
  Unblocking it (ADR-027 phase-2 ABI shim) buys far more CPU than any
  remaining unit-by-unit port.
- **Stop pursuing**: the units the prior ledger marks not-in-build /
  header-helper / absent-dependency (never compiled, never a port target);
  the measured-but-tiny units (below the 1% cut); and the `lane-gui-*` and
  `lane-leaf` lanes, where no CPU-bearing port target was measured at all.
- **Follow-ups** (not this PR): fold the callgrind harness into
  `crates/rustsmith-headroom` as a profile mode (a tool PR, separate from
  this one per the repo's port/tool split); measure netgen/ElmerGUI before
  any interactive-lane port decision; profile the heavy elmerice solves
  (`ThicknessSolver`, `Glen_2D`, `SIA`) before the ice lane is called
  headroom-free; re-measure with the campaign's pinned nightly toolchain when
  a port is actually proposed.

## Validation and limits

- Repo gates green on this commit: `cargo build --release`, `cargo test`,
  `cargo clippy -- -D warnings`, `tests/headroom_acceptance.sh` and
  `tests/m0_acceptance.sh` … `tests/m9_acceptance.sh`.
- Every number in the rows and above is measured: callgrind `Ir` per source
  file, `/usr/bin/time -v` wall/user/sys per case, `strace -c` syscall time
  per case, and the prior lanes' own machine records for the prior verdicts.
  Nothing is estimated; unmeasured units say `not benchmarked`.
- Limits, stated so the numbers are not over-read:
  - The measured compute set is the compute-heavy tail of the upstream test
    suite, which is dominated by the SA/SST/V2F field-solver family; a
    different workload mix would move the per-unit shares.
  - `Ir` is an instruction count, not cycles; it is a time proxy, chosen
    because it is deterministic on a host where `perf` is blocked.
  - The heaviest upstream case (`diffuser_sa`, 37.7 s native) was measured
    natively but not callgrind-profiled — 37.7 s × the instrumentation factor
    exceeds the measurement budget; its sibling `diffuser_sa_vec` (1.46 s)
    carries the same solver family and is profiled.
  - The elmerice profile is taken with the MPI rank launched under `mpirun
    -n 1` (a direct run profiles only the launcher process, which forks
    `orted`); the native timing for those cases is a direct run. Only one
    ice case is compute-grade, so the ice lane is **bounded, not settled**:
    `lane-ice` carries no `rewrite` claim, and a follow-up must profile the
    heavy ice solves (`ThicknessSolver`, `Glen_2D`, `SIA`) before the lane
    is called empty.
  - The `lane-gui-*` and `lane-leaf` lanes carry no measured CPU at all.
