# ADR-046: lane-fem-a grading findings (C-unit exports, facade splices, link flag)

Date: 2026-10-14. Lane: `lane-fem-a` (fem/ excluding src/modules/, 168 units).
Status: findings from graded runs `fem-a-batch1` (18/18 green) and the
combined SPLICE1 gate; no tool change in this ADR.

## 1. Recon C-unit exports carry module names, not symbol inventories

For C units the frozen recon `exports` list holds the module/stem name
(e.g. `c:fem/src/CPUTime.c` → `CPUTime`), while the built object defines
different symbols (`cputime`, `realtime`, `cpumemory` via `FC_FUNC`).
`CmakeBridge::scaffold` emits `#[export_name]` from the recon linkage, so
a scaffold-following port of a `FC_FUNC`-style C unit would export a name
nothing references, and substitution would fail the final link on the true
undefined symbols. Lane rule: ports export the `nm -g --defined-only`
truth (per-unit tables in elmer-rust `crates/elmer-fem-a/EVIDENCE.md`).
Recommended tool follow-up (separate PR): populate recon exports for C
units from built-object symbol inventories where available, or mark the
scaffold output as name-unverified for C units.

## 2. Facade Fortran modules splice as symbol-free objects

`ElementDescription`, `MeshUtils`, `SolverUtils` (and `Types`-style
parameter-only modules) define no link symbols of their own; their `.o`
files carry only (in two cases) gfortran MPI `COMMON` blocks also defined
by sibling objects. Replacing such an object with the port archive links
cleanly as long as ≥1 sibling still provides the toolchain commons —
observed green in the batch-1 grade (`suite_meshutils.log`,
`suite_solverutils.log`). The lane's
COMMON refusal stays source-based
(ADR-027): object-level MPI commons are toolchain-class, merged at link,
never program state.

## 3. Object discovery misses Fortran pairs (`CMAKE_DEPENDS_CHECK_*`)

`find_unit_objects`/`depinfo_objects_for` scan `CMakeFiles` for
`<target>.dir/<rel>.o` plus the `CMAKE_DEPENDS_DEPENDENCY_FILES` quadruple
entries. In this tree (CMake 3.28, Unix Makefiles) C entries are quadruples
with build-relative objects, but Fortran entries are 2-span
`(source, absolute-object)` pairs in `CMAKE_DEPENDS_CHECK_Fortran` — a
different block the scanner never opens. Every Fortran substitute-halting
"no built object" verdict in this layout is a discovery miss, not a missing
target. The lane driver scans both block shapes (exact-source match only).
Recommended tool follow-up (separate PR): teach the scanner both shapes.

## 4. Mirror `prepare()` does not pass the standing-pin link flag

The runbook pins `-Wl,--allow-multiple-definition` on both
`CMAKE_EXE_LINKER_FLAGS` and `CMAKE_SHARED_LINKER_FLAGS`, but
`CmakeBridge::prepare` configures with `-DCMAKE_BUILD_TYPE` only. Single
per-unit whole-archive splices link without the flag; the lane combined
splice uses selective member extraction (panel correction 3) for the same
reason. The graded lane tree therefore deviates from the campaign link
standard. Recommended tool follow-up (separate PR): pass the pinned flags
in `prepare()` (or record why per-unit grades must stay flag-free), then
re-run the combined gate on a flagged tree.

## 5. `check_substitutable` refuses 105 non-COMMON Fortran units

Beyond the 10 `COMMON`-block files (brief §2), 105 lane Fortran units are
refused by the gate (non-`BIND(C)` module procedures; no T014 relaxation
has landed). They are recorded as `refused-gate` in the lane ledger with
per-unit export counts — skips, never forced. This dominates the lane:
only 18/168 units are portable today, all machine-graded green
(10 batch-1 + 8 batch-2), plus a combined selective splice green
(SPLICE1, failures == K=3). 14 remain portable-pending; 136 are recorded
skips (see the lane ledger).
