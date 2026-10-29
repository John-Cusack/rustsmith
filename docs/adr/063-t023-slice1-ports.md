# ADR-063: Slice-1 ports — ConsistentSplitting + SaveData whole-library cdylibs

Date: 2026-10-29. Scope: rs-elmer-next Slice 1 (§8) port PR (elmer-rust).
Tool PR: rustsmith#139 (ADR-062). No gate semantics change.

## 1. What was ported

- ConsistentSplitting.so: 4 solvers (Compressibility 238L, OptimalUpdate 1408L,
  Pressure 535L, Velocity 902L) as Rust cdylib, 4/4 entries.
- SaveData.so: 6 solvers incl. SaveLine (2332L) + SaveScalars (3513L) as Rust
  cdylib, 10/10 entries (6 solvers + 4 inits).

Both use Fortran BIND(C) accessors (compiled against target fmodules) + thin
Rust drivers. Host-associated ENTRY SAVE converted to module SAVE; per-entry
call history preserved so stale-Basis behavior matches source bit-for-bit.

## 2. Deviations

- nm -D: 10/10 (SaveData) and 4/4 (CS) Elmer entries match reference.
  9 MPI Fortran re-exports (mpi_fortran_*) present in reference, absent in port.
  These are mpif90 static-link artifacts, not SaveData/CS interface; MPI is
  provided by the already-loaded ElmerSolver at runtime. Veto does not apply
  (library verdict is PluginEntry AND + Elmer-dynsym match; MPI transitives
  excluded by definition).
- SaveLine WriteFieldsAtElement: element resolved by index (not pointer) at
  C boundary; Fortran-internal calls use source-identical optional-arg
  presence. Module SAVE arrays (Basis/LocalCoord) set by caller via
  ElementInfo, matching host-association call history.
- SaveScalars: mechanical transcription (module SAVE for 74 decls, body + 12
  helpers verbatim with Solver/Model mapped to module pointers).

## 3. Verification

- Per-entry staged differentials: in progress (backstop ctest).
- SaveData solo + combined backstop at 479/482 K=3: in progress.
- Bundled ledger: 37 files (10 ported, 27 pending future slices).

## 4. Kill criterion (2026-10-29, firstmate 031/032)

SaveData DROPPED from Slice 1. SaveScalars Norm bug: port computes
Values correctly (stdout shows 4 values) and sets Variable%Norm=1.0
(reads back 1.0, LOC(Solver)=LOC(Solvers(2))), but harness sees 0.0.
Magic 999 also invisible. Matched fmodules (T020) still fail. 1hr box
exhausted. 93 SaveScalars-user tests fail; reference passes. Evidence:
proof/LastTest-cs-only.log (CS-only run), test-stdout_1.log (debug).

Slice 1 ships CS ONLY. Backstop CS-only (T020 good build + my CS.so):
479/482, K=3 = ConstantBCTemperature, ProfileBCTemperature,
ProfileBCTemperatureRobin (all fail pristine too = upstream).
