# ADR 014: Test-local non-Python sources don't flip the probe spine
- Context: `probe()` registered a language frontend for every claimed file.
  `packaging` ships one C fixture (`tests/hello-world.c`, compiled at test
  time by `test_manylinux`/`test_elffile`), so the census listed `cxx` and
  `select_composite` graded the pure-Python repo through the CMake/CTest
  spine with a zero-test baseline. `has_ctest`/CMakeLists repos are unaffected.
- Decision: Fortran/C++ claims for files under `test/`, `tests/`, or `bench/`
  count as claimed (no `unclaimed` noise) but register no frontend. A repo
  whose only non-Python sources are test-local probes single-Python; real
  sources outside test dirs (elmerfem `src/`) still flip to composite.
- Consequences: `packaging` recon freezes the pytest oracle; repos with only
  test-local C/Fortran and no other language now halt "no language claimed"
  instead of mis-spining (no such repo in the POC set).
- Alternatives: claim-count threshold (arbitrary, breaks tiny polyglots);
  fixture-name allowlist (enumerates instead of locating) — rejected.
- Spec: probe census stays extension-driven; spine choice gains the
  test-local exemption documented here.
