# ADR-056: femcore lane B — re-grade of the second half of the refused fem core

Date: 2026-10-28. Scope: the second half of the fem/ core units that the
lane-fem-a and lane-fem-b ledgers recorded as refused/skipped (176 of
353), re-graded under the merged substitution gate, and the units that
admission puts on the table.

Context: ADR-027 (the gfortran ABI shim and its shape matrix), ADR-045
(the Elmer campaign setup), ADR-046 (lane-fem-a's findings), ADR-047
(lane-fem-b's round-2 scope grading), ADR-049 (T017 shape A — the
F77-external substitution shape), ADR-050 (T017 shape B — the
module-dummy shape), ADR-051 (lane A's re-grade). Gate state at this
lane's done-gate: the T014 module profile plus shapes A **and** B are
merged — unlike lane A, which graded before shape B landed.

## 1. The claim is the complement of lane A's, written before grading

The universe and the split rule are ADR-051 §1's: the refused/skipped
fem core units from the lane-fem-a ledger (136) and the lane-fem-b
re-grade record (217), sorted lexicographically; **lane B owns the last
176** (`data/elmer-lanes/femcore-claim-a.json` carries both halves).
Claim B is 172 lane-fem-b per-scope units (all `fem/src/modules/`) plus
4 lane-fem-a file units: `fortran:fem/src/solve_real.F90`,
`fortran:fem/src/solve_cmplx.F90`, `fortran:fem/src/mpif_stubs.F90`
(all refused-gate pre-relaxation) and
`python:fem/src/split_ElementDescription.py` (skipped-helper).

The re-grade driver is unchanged from lane A
(`rustsmith-adapters`'s `elmer-regrade` over the frozen `dag.json` and
the fork tree): `176 units, 6 admit, 170 refuse, 0 unresolved`.

## 2. Result: 6 admit, 170 refuse

Of the 176 claimed units, the merged gate admits 6:

- **2 are real ports**:
  - `fortran:fem/src/solve_real.F90` — the dense LAPACK wrappers
    `SolveLapack`/`SolveLapackSym` (`dgetrf_`/`dgetrs_`,
    `dpotrf_`/`dpotrs_`), admitted under T017 shape A. Port crate
    `solvereal`: the wrapper moves into Rust, the factorization and the
    solves stay in LAPACK, so the numerics are bit-identical by
    construction. Unlike the band wrappers (which `STOP` on a nonzero
    `info`), this unit only prints the list-directed diagnostic and
    continues — the port does the same, no failure path. 5-case corpus
    byte-identical (stdout + stderr + exit).
  - `fortran:fem/src/mpif_stubs.F90` — Elmer's single-rank MPI fallback
    (39 entry points + the `mpi_stub` module flag), admitted under T017
    shape A. Port crate `mpistubs`: the seven stateful stubs
    (`init`/`finalize` flag, `comm_size` = 1, `comm_rank` = 0,
    `comm_split` identity, `init_thread` passthrough, `initialized`
    always `.FALSE.`) plus the 32 empty bodies, with the module flag as
    BSS. Driver corpus byte-identical. Build fact, recorded not hidden:
    the unit compiles unconditionally into `libmpi_stubs.so`, but the
    default build (`WITH_MPI=TRUE`) does not link it into `elmersolver`.
- **1 admits but is not portable**:
  `fortran:fem/src/modules/ParticleDynamics.F90#particledynamicsstuff` —
  the gate admits the dummy shape (shape B), but the scope body reads
  Elmer whole-program state (`ValueList_t` params via `ListGet*`/
  `ListFind`, beside `Solver_t`/`Model_t`). A file-level port must define
  all 4 symbols, so a Rust port would re-implement the particle core
  (ADR-051 §4 precedent; the T017b judgment already recorded this unit
  as a skip for the same reason).
- **2 are vacuous admits** (`HelmholtzBEM.F90#globmatc`,
  `PoissonBEM.F90#globmat`: 1 export, 0 non-`BIND(C)` — the scopes own no
  portable procedure, ADR-047 precedent).
- **1 is a helper** (`split_ElementDescription.py`: build-time code
  generator, never linked).

The 170 refusals are dominated by `procedure-not-found-in-gate-parse`
(581) and `derived-type dummy (needs whole-program layout)` (448), with
`character dummy (hidden length param)` (29) behind them — the modules
solver bodies live behind `Model_t`/`Solver_t`/`Element_t`, which no shim
can carry. One refusal deserves a name: `solve_cmplx.F90` (the complex
twin of the ported `solve_real`) refuses on `undeclared dummy (implicit
typing)` because its `COMPLEX(KIND=dp)` dummies resolve `dp` through
`USE Types`, which the gate parse does not follow. That is a genuine
gate limitation, not a port defect; a future shape that resolves
`USE`-associated kind parameters would flip it, and the ledger says so.

Per-unit reason sets: `data/elmer-lanes/femcore-regrade-b.json`; port
side: `crates/elmer-fem-a/FEMCORE-B-LEDGER.md`.

## 3. Corpora and the strong-symbol gate

Proof: `tests/femcore_b_proof.sh` (same shape as the lane-A proof —
toolchain pins, reference build, substitute, three-way byte compare,
refined sym gate):

| unit | corpus | result |
|---|---|---|
| `fortran:fem/src/solve_real.F90` | 5 cases (3×3 dense solve, `N=0` early return, singular zero matrix, 2×2 SPD, singular symmetric) | 5/5 byte-identical stdout + stderr + exit status |
| `fortran:fem/src/mpif_stubs.F90` | driver calls all 39 entry points (7 stateful checked by value, 32 empty must return) | byte-identical stdout + stderr + exit status |

Refined strong-symbol gate: `solvereal` defines exactly
`solvelapack_`, `solvelapacksym_` as `T`; `mpistubs` defines exactly the
39 entry points as `T` plus `__mpi_stub_MOD_mpi_stub_init` as `B` —
**zero STRONG port-level dups** beyond the replaced symbols in both, so
each port replaces its unit's object whole (whole-file portable).

The driver flushes Fortran unit 6 around every wrapper call (ADR-051
§5): the Fortran runtime and the Rust provider write to the same
descriptor through separate buffers, and only the flush makes the
interleave deterministic. `mpistubs` needs no flush — the port performs
no I/O.

## 4. Backstop and capture

Lane backstop (T013-style, fresh tree @ `9f6af2f85`, both lane-B ports
spliced): **479/482, failures exactly K=3** (see PR body for
`LastTest.log` location + walls). The default `WITH_MPI=TRUE` build does
not link `libmpi_stubs.so`, so the backstop exercises the splice
mechanically (link-clean) while the differential corpus carries the
behavioral grade — both facts recorded.

Lane capture (`/home/john/firstmate/data/rs-elmer-t016/`, per
`docs/LANE_CAPTURE.md`): one `grade-unit` row per claimed unit (2
passed with differential + integrity gates and measured walls, 174
abandoned with the gate-verdict scope evidence), session joins to this
lane's transcript, `finish`, then `export-records`. Counts in the PR
body.

No timing sample: neither the lane-fem-a nor the lane-fem-b
`bench_sample_unit` is in claim B (the fem-b sample unit
`fem/src/modules/HeatSolve.F90` is claim-A), so the bench row stays
unmeasured — never a guess. No headroom rows were in the tree at the
done-gate, so no unit was skipped for "will not pay".

## 5. Decision (unchanged from lane A)

A gate-admitted unit that needs whole-program state is a skip, not a
port (ADR-051 §4). `particledynamicsstuff` is recorded as
`skipped-gate-admitted-not-portable` with the body's dependency named.

Tool PR: the `femcore_b` proof script, this ADR, and the re-grade
evidence land in rustsmith with this ADR. Port PR: the two new port
crates and the lane ledger land in `John-Cusack/elmer-rust` (branch
`elmer/lane-femcore-b`).
