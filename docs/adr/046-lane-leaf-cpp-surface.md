# ADR-046: lane-leaf scopes to zero ports — meshgen2d has no C-linkable surface

Date: 2026-10-14. Lane: `lane-leaf` (60 `meshgen2d/` units). This records a
spec deviation per repo rule (all deviations get an ADR): the lane grades
**0 units, skips 60 with reason** — 25 `skip-cpp-only`, 35 `skip-helper`
(`data/elmer-lanes/lane-leaf.scope.json` carries the per-unit table).

## Context

The brief ordered scoping the C-linkable surface first (spike §7 item 8:
Rust cannot provide C++-mangled symbols). meshgen2d builds exactly one
target, the `Mesh2D` executable (25 TUs, `ADD_EXECUTABLE`, no library);
the recon `dag.json` marks every unit `bind_c: true`, which is wrong for
this subsystem and was re-checked against source + built objects.

## Evidence (native build `orig @ 9f6af2f85`, proof logs outside the worktree)

- `nm -g --defined-only` over all 25 `Mesh2D.dir` objects (`NF==3`):
  every defined global is a `_Z` C++-mangled class member, except the
  toolchain `DW.ref.__gxx_personality_v0` (V) and one `T main`.
- 14/25 TUs emit vtables; `Body.cpp` is a 1-line include emitting 0
  globals; 24/35 headers use `std::` directly; all 35 headers declare
  C++ classes/templates only. `grep extern "C"` over `src/`: 0 hits.
- `main.cpp`'s unmangled `main` is inseparable from in-TU C++
  instantiations (`Mesh`, `MeshParser` with `std::ifstream/vector/map`
  members), so it is not an independently portable C surface either.
- No `METIS` references: the ADR-031 per-config rule is vacuous here.

## Decision

Port nothing; record all 60 as skips (reasons in scope.json, never
forced). `bench_sample_unit` stays null — ADR-045 sampling needs a graded
unit. No `store.db` machine rows: no grading transaction ran (zero graded
units); the JSON record is the honest substitute, not backfilled rows.

## Pipeline validation (brief item 2, done first)

`proof-minimal.sh` (log outside worktree): real 22-unit minimal batch
over `libmatc.so` + `matc` exe — refined gate (0 strong port-level dups
over 263 port symbols; weak libm `W` dups informational; same-toolchain
`compiler_builtins cgu.175` member `cmp` IDENTICAL eval vs parser),
`SPLICE_MIN_GREEN` 20/20 corpus byte-identical incl. stderr+exit,
splice+relink 1–2 s. Negative controls, both observed: NC1
missing-provider → `.so` link fails (84× hidden `__rdl_alloc*`
undefined; default-visibility panicking refs stay dynamic in shared
links); NC2 missing `lu` member → `.so` link rc=0 (shared tolerance),
gate shows 0/4 `lu` symbols vs pristine 4/4, exe link fails naming
exactly `mtr_det`/`mtr_inv`/`mtr_LUD`.

## Consequence

Campaign-complete item 1 ("every in-scope unit graded green or explicitly
skipped with a recorded reason") holds for lane-leaf via these 60 skip
records. A future C++-substitution design (e.g. `#[export_name]`-mangled
providers — rejected: STL/vtable ABIs unreproducible) would re-open the
25 `skip-cpp-only` units; headers stay helpers under any design.
