# ADR 014: Test-local fixtures don't flip the probe spine; deferred imports aren't DAG edges; grading counts every collected test
- Context: `probe()` registered a language frontend for every claimed file.
  `packaging` ships one C fixture (`tests/hello-world.c`, compiled at test
  time by `test_manylinux`/`test_elffile`), so the census listed `cxx` and
  `select_composite` graded the pure-Python repo through the CMake/CTest
  spine with a zero-test baseline. `has_ctest`/CMakeLists repos are unaffected.
- Decision: Fortran/C++ claims for files under `test/`, `tests/`, or `bench/`
  count as claimed (no `unclaimed` noise) but register no frontend. A repo
  whose only non-Python sources are test-local probes single-Python; real
  sources outside test dirs (elmerfem `src/`) still flip to composite.
- Context: the Python import extractor (`ast.walk`) counted function-local
  deferred imports and `if TYPE_CHECKING:` imports as hard DAG edges.
  `packaging.ranges.to_specifier_set` defers `from .specifiers import ...`
  to break its cycle with `specifiers`; the false edge made strict
  `leaf_first_order` fail recon with a phantom cycle.
- Decision: the extractor only reports imports executing at module import
  time (skips function/lambda bodies and `TYPE_CHECKING` guards). Fewer
  constraints can never create cycles; other packages' DAGs only lose false
  edges.
- Consequences: `packaging` recon freezes the pytest oracle and a DAG;
  repos with only test-local C/Fortran and no other language now halt "no
  language claimed" instead of mis-spining (no such repo in the POC set).
- Context: two grading-count defects surfaced on packaging's suite (427
  marker-deselected property tests): `summary_counts` paired each number with
  the last kind in its comma-chunk, so pytest's collection line ("collected
  62910 items / 427 deselected / 62483 selected") attached 62483 to
  "deselected"; and `test_count` excluded collected-but-deselected tests, so
  the outcomes+deselected invariant could never hold for such suites.
- Decision: a number pairs only with the kind that follows it before any
  subsequent number; `baseline_of` sets `test_count` to every collected test
  (`total()` plus deselected) and the integrity head check compares the same
  sum. Suites without deselects are byte-identical.
- Alternatives: claim-count threshold (arbitrary, breaks tiny polyglots);
  fixture-name allowlist (enumerates instead of locating) — rejected.
- Spec: probe census stays extension-driven; spine choice gains the
  test-local exemption documented here.
