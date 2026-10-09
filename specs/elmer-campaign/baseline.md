# Elmer campaign baseline (T002 pin)

**Pinned**: 2026-10-09 | **Host**: 32-core, cmake 3.28.3, gfortran 13.3.0 + OpenMPI | **Slice**: T002

Slice PRs reference this file as the frozen oracle baseline. Any
deviation from exactly these numbers fails the slice (spec.md SC-002,
clarification 6). K is campaign-external (decision A): never
investigated inside a port session.

## Source

- Elmer checkout: `/home/john/runs/elmer-work/orig` @ `9f6af2f85`
  (`release-26.2-641-g9f6af2f85`), verified `git log` this session.
  NOT `release-26.2.1 @ a19504a`.
- Configure: `cmake -S <orig> -B <build> -DCMAKE_BUILD_TYPE=Debug`
  (default options: `WITH_ElmerIce=FALSE`, `WITH_ELMERGUI*=FALSE`;
  system BLAS/LAPACK linked, bundled sources skipped).
- Toolchain: `gfortran --version` → `GNU Fortran 13.3.0`
  (Ubuntu 13.3.0-6ubuntu2~24.04.1); descriptor layout pinned to this
  version (spec.md Assumptions).

## Oracle count

- `ctest -N -L quick`: **482 tests** (`Total Tests: 482`).
- In-harness integrity baseline: `test_count: 482`.

## Full execution (this session)

- Suite: `ctest -L quick`, wall **411 s test time** (6m51 s launcher
  wall). Log: `/tmp/elmer-baseline-c2/ctest-quick.log`
  (`LastTest.log` at
  `/tmp/elmer-baseline-c2/build/Testing/Temporary/LastTest.log`).
- Result: **479 pass / 3 fail out of 482**
  (`99% tests passed, 3 tests failed out of 482`).
- K=3 known upstream segfaults (SIGSEGV in `DefaultDirichletBCs`,
  predate the campaign; repro `ctest -R ConstantBCTemperature`):
  `ConstantBCTemperature` (#80), `ProfileBCTemperature` (#536),
  `ProfileBCTemperatureRobin` (#537) — each `***Failed` ~3.6 s,
  `test-stderr_1.log` shows `exited on signal 11 (Segmentation
  fault)`, matching S3 decision A.
- Backstop rule: the release backstop passes at exactly 479/482 with
  failures == K=3; any other failure fails the slice.

## Port scope (from T001)

- 3025 frozen − 61 `out_of_scope` − 1587 `mathlibs` − 193 `umfpack`
  ≈ **1184** port units. `RUSTSMITH_SCOPE=matc` partitions
  26 scheduled / 61 `out_of_scope` / 2938 `outside_scope`.
