# ADR-062: ElmerIce step-0 — frozen ice-enabled baseline, no porting

Date: 2026-10-29. Scope: Slice 3 of the rs-elmer-next ranked slices
(report §8): a frozen `WITH_ElmerIce=TRUE` reference build, surface
contracts for the three ice libraries, the fem backstop in the same
tree, a named `elmerice/Tests` subset run pristine-vs-pristine twice
with byte compare, and the USF loading-contract probe. No gate
changes, no ports. Outcome: **baseline ESTABLISHED** — neither kill
criterion triggered; gate for slice 4 (ElmerIceUSF whole-lib) is open.

Lane: `rs-elmer-t024-01` (store + `lane.toml` in the lane dir; run
closed `passed` with zero grade rows — a measurement run, not a port
run; no transcript session id is exposed to the worker, so
`sessions.csv` carries the run only, as in prior Elmer lanes).
Evidence manifests committed under `data/rs-elmer-t024/`; full logs
stay in the lane `scratch/` + `proof/` dirs.

## 1. Frozen configure

- Source: ElmerCSC/elmerfem @ `9f6af2f85` (campaign pin), pristine
  (`git status` clean at configure time).
- Build dir: lane `scratch/build-ice` (durable, never `/tmp`).
- Environment (required — see §7): `HWLOC_COMPONENTS=-gl`;
  `CCACHE_DIR=/home/john/firstmate/data/elmer-lanes-cache`,
  `CCACHE_SLOPPINESS="file_macro include_file_mtime include_file_ctime
  time_macros pch_defines"`.
- cmake flags (printed in `data/rs-elmer-t024/configure-ice.cmd`):
  `-DCMAKE_BUILD_TYPE=Debug -DWITH_MPI=TRUE -DWITH_ElmerIce=TRUE
  -DWITH_ELMERGUI=FALSE -DWITH_ELMERGUILOGGER=FALSE
  -DWITH_ELMERGUITESTER=FALSE -DWITH_ELMERPOST=FALSE -DWITH_LUA=FALSE
  -DWITH_MKL=FALSE -DWITH_Mumps=FALSE -DWITH_Hypre=FALSE
  -DWITH_Trilinos=FALSE -DWITH_FETI4I=FALSE -DWITH_Zoltan=FALSE
  -DWITH_ROCALUTION=FALSE -DWITH_CHOLMOD=FALSE -DWITH_UMFPACK=TRUE
  -DWITH_CONTRIB=FALSE -DWITH_OpenMP=FALSE -DWITH_NETCDF=FALSE
  -DWITH_MMG=FALSE -DWITH_PARMMG=FALSE -DWITH_XIOS=FALSE
  -DWITH_PROJ=FALSE -DELMERGRID_WITH_MATC=OFF
  -DCMAKE_EXE_LINKER_FLAGS=-Wl,--allow-multiple-definition
  -DCMAKE_SHARED_LINKER_FLAGS=-Wl,--allow-multiple-definition`
  (campaign configure + `WITH_ElmerIce=TRUE` + frozen-FALSE optionals;
  `WITH_PROJ` is not a declared option — recorded `UNINITIALIZED=FALSE`).
- Resulting optional-dep values (`CMakeCache.txt`): `WITH_NETCDF /
  WITH_MMG / WITH_PARMMG / WITH_XIOS` all `FALSE`; `HAVE_NETCDF` /
  `HAVE_HDF5` absent (system libs not found — functionalities
  disabled, frozen as absent); `HAVE_MMG / HAVE_PARMMG / HAVE_XIOS /
  HAVE_PROJ` absent; `HAVE_ODEPack` unset anywhere in the tree (dead
  conditional — `PlumeSolver.F90` is NOT compiled);
  `ELMER_BROKEN_MPI_IN_PLACE=OFF` (measured, not assumed).
- Denominator consequences (binaries, not source lists):
  `ElmerIceSolvers` = unconditional `ElmerIce_SRC` only;
  `ElmerIceUtils` = 4 files (`proj6_interface` excluded);
  `ElmerIceUSF` = 17 files (incl. `USF_proj.F90`; only the PROJ
  *link* is conditional, and it is off).
- Registered tests at configure: 66 `elmerice`, of which 29
  `elmerice-fast`; `quick` label = 482 (ice-on adds no fem tests).

## 2. Surface contracts (`nm -D --defined-only`, committed)

| Library | Path in build | T | data (B/D/R) | total |
|---|---|---:|---:|---:|
| `ElmerIceSolvers.so` | `fem/src/modules/` | 360 | 23 | 383 |
| `ElmerIceUSF.so` | `fem/src/modules/` | 60 | 9 | 69 |
| `ElmerIceUtils.so` | `elmerice/Utils/` (own dir — its CMakeLists sets no `LIBRARY_OUTPUT_DIRECTORY`) | 16 | 22 | 38 |

Manifests: `data/rs-elmer-t024/ElmerIce{Solvers,USF,Utils}.nm`
(sorted). Spot checks: `seapressure_`/`seaspring_` T in USF;
USF has 77 undefined dynamic symbols (`nm -D -u`), dominated by
`__defutils_MOD_*` / `__coordinate*` provided by libelmersolver.

## 3. Fem backstop in the ice-on tree: 479/482, zero delta

- `ctest -L quick -j4`: **479/482, failures exactly #80
  (`ConstantBCTemperature`), #536 (`ProfileBCTemperature`), #537
  (`ProfileBCTemperatureRobin`)** — the K=3 upstream set, same test
  numbers as the ice-off campaign bar.
- Failure signature verified identical: no `TEST.PASSED` (solver did
  not run to completion); per-rank log ends in HeatSolver with
  `mpiexec ... exited on signal 11 (Segmentation fault)`.
- Verdict: ice-on introduces **no fem baseline delta**; 479/482 K=3
  stands in the ice-enabled tree.

## 4. Ice oracle subset: self-deterministic modulo wall-clock

Named subset (10; all serial, `elmerice-fast`, no optional deps;
USF-touching first): Buoyancy, Friction_Weertman, Friction_Coulomb,
Grounded, SSA_Weertman, SSA_Coulomb, SSA_SEP, ComputeDevStress,
ShapeFactor, Flotation. (GroundingLineFlux excluded: MPI/NPROCS;
Proj6/UGridDataReader/Xios/MMG tests excluded: need `HAVE_*` that
are frozen off.)

Method: per test, `ctest -R ^<t>$` twice in the pristine build tree;
snapshot `test-stdout.log` / `test-stderr.log` / `TEST.PASSED` after
each run; byte-compare. Raw logs differ run-vs-run **only** in
wall-clock artifacts; after normalizing exactly these classes —
`STARTED/FINISHED AT` timestamps, `Assembly:/Solve: (s)` timings,
`SOLVER TOTAL TIME(CPU,REAL)`, `Elapsed REAL time`,
`Active coordinate mapping time`, and the speed-threshold progress
line `Estimated time left` (dropped: presence varies with machine
speed) — stdout is byte-identical in all 10 pairs:

| test | stdout ≡ mod wall-clock | TEST.PASSED | stderr note |
|---|---|---|---|
| Buoyancy | yes | 1 / 1 | identical |
| Friction_Weertman | yes | 1 / 1 | identical |
| Friction_Coulomb | yes | 1 / 1 | identical |
| Grounded | yes | 1 / 1 | identical |
| SSA_Weertman | yes | 1 / 1 | identical |
| SSA_Coulomb | yes | 1 / 1 | run1-only OpenMPI `tcp_peer_recv_connect_ack: invalid header type` transport warning |
| SSA_SEP | yes | 1 / 1 | identical |
| ComputeDevStress | yes | 1 / 1 | identical |
| ShapeFactor | yes | 1 / 1 | identical |
| Flotation | yes | 1 / 1 | identical |

Normalization script + both raw snapshots per test are in the lane
`proof/ice-subset/` (`run1/`, `run2/`, `norm1/`, `norm2/`,
`verdict.txt`); the verdict table is committed as
`data/rs-elmer-t024/ice-subset-verdict.txt`.
Kill-criterion disposition: the subset IS self-deterministic
(results + pass/fail identical; only wall-clock chatter varies) —
the ice programme does **not** stop here.

## 5. USF loading-contract probe: Rust cdylib serves entries

Mechanism pinned (both halves measured):

- (a) Symbol resolution: `ElmerIceUSF.so` DT_NEEDED-links
  `ElmerIceUtils.so` + `libelmersolver.so` (+ system) with RUNPATH
  into the build tree — 77 undefined dynsyms resolve via the link,
  **not** via host-global scope. A port cdylib replicates exactly
  this (`-lelmersolver` + RUNPATH); `Load.c` `try_dlopen` uses
  `RTLD_NOW` (LOCAL). `ElmerSolver_mpi` itself links
  `libelmersolver.so` the same way.
- (b) dlopen+call path: `Load.c` search order is `.` →
  `ELMER_LIB` dirs → `ELMER_MODULES_PATH` dirs. Staged Buoyancy in
  lane scratch: reference run `TEST.PASSED=1` (FlowSolve Result
  Norm 0.35355…, zs-top 9.99999990 = RefNorm). Shadowing
  `ElmerIceUSF.so` in the stage CWD with a 2-entry Rust cdylib
  (`seapressure_ → 1234.5`, `seaspring_ → -42.25`; same
  by-reference `(Model, nodenumber, y) → REAL(8)` ABI, `panic=abort`,
  no dependencies): solver loads it, completes, and the sentinels
  drive the solve — `TEST.PASSED=0`,
  `CompareToReferenceSolution: FAILED: Norm = 1.587E+02 vs
  RefNorm = 9.99999990E+00` (FlowSolve 20.6756…, zs-top 39.7398…).
  Probe source: lane `scratch/probe-usf/`; report committed as
  `data/rs-elmer-t024/usf-probe.txt`.

The remaining unproven step for slice 4 is a Rust cdylib that
*also calls into* libelmersolver (DT_NEEDED half + call half
together) — that is the slice-4 port's own proof, not step-0's.

## 6. Verdict and gate for slice 4

Step-0 baseline ESTABLISHED with zero deltas: frozen configure
printed, 3 surface contracts committed, fem 479/482 K=3 unchanged,
10-test ice oracle deterministic, USF contract (DT_NEEDED+RUNPATH
resolution, `.`-first shadowable dlopen path, by-ref entry ABI)
proven with a Rust cdylib. Slice 4 (ElmerIceUSF whole-lib, 17 files,
6,110 LOC) is costed against this baseline. No gate, oracle, or
threshold touched.

## 7. Environment note (reproducibility)

MPI process startup on this box hangs without
`HWLOC_COMPONENTS=-gl` (singleton `MPI_Init` and `mpiexec` alike;
six failing variants observed before the fix, incl. clean-TMPDIR
and loopback-MCA). Every prior Elmer lane uses this setting
(`backstop-t020.sh`, `backstop-femb.sh`); configure, build, and all
ctest runs in this slice export it. Without it, the cmake
`MPI_IN_PLACE` `try_run` hangs and noice-on build is possible.
