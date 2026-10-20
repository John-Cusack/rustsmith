# ADR-051: femcore lane A — re-grade of the refused fem core under the relaxed gate

Date: 2026-10-20. Scope: the first half of the fem/ core units that the
lane-fem-a and lane-fem-b ledgers recorded as refused/skipped (177 of
353), re-graded under the merged substitution gate, and the units that
admission put on the table.

Context: ADR-027 (the gfortran ABI shim and its shape matrix), ADR-045
(the Elmer campaign setup), ADR-046 (lane-fem-a's findings), ADR-047
(lane-fem-b's round-2 scope grading), ADR-049 (T017 shape A — the
F77-external substitution shape, rustsmith#105). Gate state at this
lane's done-gate: the T014 module profile plus T017 shape A are merged;
T017 shape B (the module-dummy shape, rustsmith#108) was open and is
**not** used for any number in this ADR — see §4.

## 1. The claim is mechanical, so the two femcore lanes cannot overlap

The fem core's refused/skipped units are recorded in two places: the
lane-fem-a ledger (168 rows, port repo `crates/elmer-fem-a/LEDGER.md`)
and the lane-fem-b re-grade record (`data/elmer-lanes/lane-fem-b.regrade.json`,
218 scope rows). Their union, restricted to the units that are recorded
as refused or skipped, is 353 units:

- 136 from lane-fem-a (105 `refused-gate`, 10 `refused-COMMON`, 9
  `skipped-not-in-build`, 7 `skipped-helper`, 5 `skipped-ext-dep`),
- 217 from lane-fem-b (214 `refuse` rows plus the three `admit` rows the
  fem-b ledger records as skips; the one already-graded row,
  `#harmutils`, is neither refused nor skipped and stays out).

Sorting the unit ids lexicographically and splitting in half gives lane
A the first 177 and lane B the other 176. The split, both halves and
the rule are written down in `data/elmer-lanes/femcore-claim-a.json`
**before** any grading, so the complement is exact.

The lane-fem-a ledger's `ported` and `deferred-round2` rows stay with
their own lane: they are neither refused nor skipped.

## 2. The re-grade runs the campaign's own gate, and now says why

Re-grading needs a per-unit verdict *and* a reason, because the deliverable
is a skip ledger whose reasons are current. The campaign's gate
(`CmakeBridge::check_substitutable`) returns a boolean decision plus one
pinned message; the per-export reasons lived only in an ad-hoc lane
script that re-implemented the gate (the fem-b re-grade record).

This lane changes the gate's internals **additively**: `gate_file_admits`
and `gate_f77_external_admits` become `gate_file_verdict` and
`gate_f77_external_reason`, which return the same decision plus the
reason that produced it; the boolean forms are one-line wrappers, and
`check_substitutable` still returns the pinned message. A new public
API, `rustsmith_adapters::fortran_substitution_verdict`, exposes the
decision with its reasons. Every existing gate test still passes
unchanged (`cargo test -p rustsmith-adapters`: 67 passed, 0 failed), and
a new test pins the reason channel: an admitted unit reports no reason,
a refused unit reports at least one.

The re-grade driver is a new binary, `rustsmith-adapters`'s
`elmer-regrade`: it takes the frozen campaign inventory
(`dag.json`), the source tree and a unit list, and prints one row per
unit (verdict, reasons, export counts). The unit model follows the
ledger that recorded the unit (ADR-047, ADR-049):

- a **lane-fem-a** row is a file-level campaign unit, so the frozen
  inventory owns its export list;
- a **lane-fem-b** row is a per-scope unit, so the current Fortran
  frontend's scope split owns its export list.

## 3. Result: the fem core's remaining units are not portable, and the
reasons say why

Of the 177 claimed units, the relaxed gate admits 25 and refuses 152.
Of the 25 admits:

- **19 are C/C++/header units** the gate admits vacuously (no non-BIND(C)
  Fortran export). They are not ports: eight are never compiled
  (`fem/obsolete` has no `CMakeLists.txt`; `elmer_lua_iface.c` is added
  to `solverlib_SOURCES` only under `IF(WITH_LUA)`, which defaults to
  FALSE; `fft_test.c` and the four `view3d/LinearSolve*.c` files are
  orphan sources; `minpack_kinds.F90` is added only under
  `IF(WITH_EXTOPTIM)`), five compile as **empty stubs** (HYPRE,
  SuperLU, AMGX, CHOLMOD and Rocalution are all off by default, so the
  guarded bodies are never built), and six are header-only helpers.
- **1 is a vacuous admit** (`EMWaveSolver.F90#emwavesolverutils`: the
  scope owns no portable procedure — the ADR-047 precedent).
- **2 admit the dummy shape but are not portable**: `BackwardError.F90`
  and `MGPrec.F90` read Elmer's whole-program state
  (`Types.GlobalMatrix`, `CRS_MatrixVectorMultiply`,
  `Multigrid.MultiGridSolve`, `IterSolve.FirstCall`). A file-level port
  must define every symbol of the unit, so a Rust port would
  re-implement the solver core, not these units.
- **3 are real ports**: `SolveBand.F90`, `SolveSBand.F90` and
  `Walls.F90`.

The 152 refusals are dominated by derived-type dummies (135 units) and
by recon linkages the gate parse cannot resolve (70 units) — the fem
core's numerics live behind `Model_t`/`Solver_t`, which no shim can
carry. The per-unit reason sets are in
`data/elmer-lanes/femcore-regrade-a.json` and in the port repo's
`crates/elmer-fem-a/FEMCORE-A-LEDGER.md`.

## 4. Decision: a gate-admitted unit that needs whole-program state is a
skip, not a port

T017 shape A widened the gate from "module procedures only" to "F77
global external subprograms". Shape A admits a unit whose *dummy shape*
fits; it says nothing about the unit's *body*. A unit whose body reads
Elmer's global matrix therefore passes the gate and still has no Rust
port: the port would have to re-implement the solver core, or the port
would be a veneer that calls the Fortran back (a symbol-swap, not a
port — the no-inflation ruling, ADR-047).

The lane records these as `skipped-gate-admitted-not-portable` with the
body's dependency named, rather than counting them as ports. This is a
judgement call above the implementation worker, so it is recorded here;
the captain may rule that such units should instead be ported as
call-back veneers.

**Shape B is not used for any number here.** rustsmith#108 (T017 shape
B — the module-dummy shape) was open at this lane's done-gate. Under
shape B's rule (explicit-shape and assumed-size array dummies travel as
raw base addresses), exactly **one** of the 177 claimed units flips from
refuse to admit: `fortran:fem/src/modules/CoordinateTransform.F90#vecutils`
(the `VecUtils` module: `norm2`, `normalized`, `jac`, `transform2rank`).
The lane re-reads the merged gate before its done-gate; the numbers above
are the merged gate's numbers. That one unit is not left unported: the
sibling T017 lane's port (elmer-rust#11) already covers the scope under
the shape B relaxation, and the lane ledger says so.

## 5. The LAPACK band wrappers are ports, and the corpus proves it

`SolveBand.F90` and `SolveSBand.F90` are wrappers around LAPACK's band
solvers. The port moves the wrapper into Rust — the argument
marshalling, the `N <= 0` early return, the `info` check and the
failure path — and keeps the factorization and the triangular solves in
LAPACK (`dgbtrf_`/`dgbtrs_`, `zgbtrf_`/`zgbtrs_`, `dpbtrf_`/`dpbtrs_`,
`zpbtrf_`/`zpbtrs_`), so the numerics are bit-identical by construction.
This is the same shape as the lane-fem-a `umf4_f77wrapper` port (a
wrapper around a vendored library).

Two ABI facts the corpus pins, both measured rather than assumed:

- gfortran's list-directed `PRINT*` writes a leading blank, the literal
  verbatim, then the integer right-justified in a field of 12 columns;
  `STOP 1` writes `STOP 1` to stderr and exits with status 1. The port
  reproduces both byte for byte.
- The Fortran runtime and the Rust provider write to the same file
  descriptor through **separate** buffers, so the corpus driver flushes
  unit 6 before the failure case. Without that flush the two streams
  interleave differently on the two sides — a corpus artifact, not a
  port defect, and the reason the driver flushes.

The reference objects are built against a stub `Types` module carrying
the campaign's own `dp` definition (`SELECTED_REAL_KIND(12)`, 8 bytes).
The two units take exactly one entity from `Types` — that compile-time
constant — so the reference object is the unit's own code; the real
`Types` module needs the generated `config.h` and the Lua module, which
this lane does not build.

Rust craft: the two new ports are marshalling wrappers, so there is no
per-item loop to tighten — no per-item boxing, no per-item allocation,
no dyn dispatch, and the branch on the LAPACK `info` is hoisted out of
any loop. The one allocation is the pivot array LAPACK requires, one per
call (the same automatic array the Fortran allocates on the stack). The
proof script routes the Fortran compiles through the campaign's shared
ccache recipe (`data/elmer-lanes/ccache.env`, ADR-045) when the host has
ccache; the lane compiles a handful of files, so the cache is not
load-bearing here.

## 6. Result

- Claim: 177 of 353 fem core refused/skipped units
  (`data/elmer-lanes/femcore-claim-a.json`).
- Re-grade under the merged gate: **25 admit, 152 refuse**; per-unit
  reasons in `data/elmer-lanes/femcore-regrade-a.json`.
- **3 ports** (2 new, 1 re-verified), byte-identical corpora: 5/5 and
  4/4 cases (stdout + stderr + exit status) and 12/12 cases (84 values)
  for the re-verified shape A port; zero STRONG port-level dups beyond
  the replaced symbols in all three.
- **174 skips**, each with the current reason: 152 gate refusals, 8 not
  in build, 5 empty stubs, 6 header helpers, 2 gate-admitted but not
  portable, 1 vacuous admit.
- No timing sample (lane-fem-a has no `bench_sample_unit`; the fem-b
  sample unit is not in this claim) and no headroom rows in the tree at
  the done-gate, so the bench row stays unmeasured — never a guess.

Tool PR: the gate's reason channel, the `elmer-regrade` driver and the
proof script land in rustsmith with this ADR. Port PR: the two new port
crates and the lane ledger/evidence land in `John-Cusack/elmer-rust`
(branch `elmer/lane-femcore-a`).
