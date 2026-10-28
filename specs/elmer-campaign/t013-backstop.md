# T013: per-slice full-suite backstop — committed evidence

**Method** (2026-10-28, 32-core host): per merged slice, splice the slice's
merged ports into a fresh Elmer tree (`@ 9f6af2f85`, `-DCMAKE_BUILD_TYPE=Debug`,
defaults + `-Wl,--allow-multiple-definition` on EXE+SHARED per runbook
standing pins), full rebuild, `ctest --output-on-failure -L quick -j16`.
Acceptance: exactly 479/482 with failures == K=3 (`ConstantBCTemperature`
#80, `ProfileBCTemperature` #536, `ProfileBCTemperatureRobin` #537); any
other failure fails the slice. Env: `HWLOC_COMPONENTS=-gl` (T011 MPI/Xwayland
hang fix). ccache shared `elmer-lanes-cache`. Toolchain:
`rustc 1.100.0-nightly (feaadeeac 2026-09-19)` (all port archives),
gfortran 13.3.0, cmake 3.28.3. elmer-rust `@ 484ffe6`. Raw logs + build
trees under `/home/john/firstmate/data/rs-elmer-t013/` (durable lane dir,
never `/tmp`); OMP session record in `SESSION.md` there.

## Per-slice table

| slice | merged ports spliced | result | failures | test s | wall s |
|---|---|---|---|---|---|
| pristine anchor | — (harness validation) | 479/482 | exactly K=3 | 151.74 | 229 |
| T005 matc | 22 × `rust/<stem>` (`ld -r --whole-archive` per object, T006 pattern) | 479/482 ✅ | exactly K=3 | 156.52 | 156 |
| T008 elmergrid | 8 × `rust/elmergrid` (`splice-gridio.sh` merged-all verbatim; NM gates pass) | **463/482 ❌ FAIL** | 16 beyond K=3 (see below) | 207.57 | 207 |
| T008 elmergrid (fixed) | 8 × elmer-rust fixed archives (`unite_merge` `!=0` + regression test, elmer-rust#14) via `splice-gridio.sh` merged-all; NM gates pass | 479/482 ✅ | exactly K=3 | 163.60 | 164 |
| T009 meshgen2d | none (C-linkable surface EMPTY: 25 cxx TUs 100% `_Z`-mangled, 0 `extern "C"`, 14 vtable-emitters, 35 decl-only headers) | N/A (honest halt stands; pristine anchor covers the tree) | — | — | — |
| T010 fhutiter | 1 × hutiaux (whole-object `ld -r`; 12/12 symbols live in `libfhuti.so`) | 479/482 ✅ | exactly K=3 | 164.50 | 164 |
| T015/T017 fem | 18 whole-file ports (15 C fem-a + Walls + SolveBand + SolveSBand; 4 facades vacuous, vecutils/harmutils driver-level-only per their EVIDENCE) | 479/482 ✅ | exactly K=3 | 163.49 | 163 |

`LastTest.log` per slice: `<lane>/work/build-<pristine,matc,gridio,fhutiter,fem>/Testing/Temporary/LastTest.log`.
Excerpt (every slice verdict line):
`99% tests passed, 3 tests failed out of 482` (gridio: `96% tests passed, 19 tests failed out of 482`).

## T008 FAIL record (recorded, not tuned)

16 non-K failures, all mesh/boundary-output-sensitive: ContactPatch2D(×4:
#94, #95, #97, #99), HybridMeshMap #341, MeshColour #420,
MortarMixedDimensions #440, MortarPoisson2D #442, MortarSaveProjection #447,
SaveLine #625, beamer3d_box #827, periodic_nonconforming #1014,
radiator3d_box #1056, radiator3d_symm #1067, savescalars_boundary #1079,
savescalars_flux #1080.

Mesh-level cause (ContactPatch2D reproduced head-to-head,
`ElmerGrid 1 2 squares.grd`): nodes/elements byte-identical, side-location
stdout identical, but `mesh.boundary` has 28 rows (spliced) vs 52 (pristine)
— boundary types 1–4 (first cell outer) dropped. ElmerSolver (pristine code)
then diverges (`IterSolve: Numerical Error: Too many iterations` → `STOP 1`,
no `TEST.PASSED`). A diagnostic tree spliced with the elmer-rust archive set
fails the identical 16 with byte-identical meshes, so the defect is shared by
both port generations (suspect: unite/save boundary path in egmesh/egnative;
the in-repo `body→bnry` naming divergence is real but non-causal here).
No port file touched in this task; fixes belong to elmer-rust (follow-up).

## T008 FIX record (rs-elmer-gridio-fix, 2026-10-28)

Root cause (line-proven): `egmesh.c:3097-3098` scans past OCCUPIED to the
first FREE slot (`for(k=j;...;k++) if(!bound1[k].created) break;`). Both
Rust generations inverted the predicate (`while ... created == 0`), landing
on the first OCCUPIED slot and overwriting `bound1[0..3]` with `bound2[0..3]`
— 28 rows vs 52, types 1–4 lost. Counterfactuals: buggy unit test fails
(`left 5, right 1`); fixed `ElmerGrid 1 2 squares.grd` restores 52 rows
(first rows types 1–4); fixed-archives backstop 479/482 exactly K=3
(test 163.60 s). In-repo `body→bnry` divergence remains real but non-causal.

Fix: elmer-rust#14 (`==0` → `!=0` + NOTE) + `unite_merge_tests` regression
test. Lane capture `rs-elmer-gridio-fix-01`: 1 passed unit
(`c:elmergrid/src/egmesh.c` @ `2dbcb36`), 2 gate rows
(oracle_parity 479/482, oracle_integrity 482/482), 1 hunk, 1 failed row
(gate_failed, 28 vs 52), 1 session
(`01a183bc-4f19-7577-aa3a-b940db165577`). Proof:
`/home/john/firstmate/data/rs-elmer-gridio-fix/proof/`
(`LastTest-fixed.log`, `LastTestsFailed-fixed.log`, `mesh.boundary-*.log`).

## Incidents

- `splice-fem.sh` v1 failed on Fortran DependInfo absolute paths (doubled
  `$B/` prefix); v2 is absolute-aware + marker-idempotent. The 15 C objects
  were `ld -r`'d twice (identical archives — behavior-identical, final link
  clean); recorded here, not hidden.
