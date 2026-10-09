# ADR-028: Scoped per-unit grading (affected-tests-only) on the CTest spine

Date: 2026-10-08. Session S6. Status: accepted (matc pilot proven equivalent).

## Context

Per `docs/ELMER_S3_PILOT.md` §3, every per-unit grade re-runs the whole
482-test oracle (~390 s), so grade cost is O(units × suite): 3025 units ×
~480 s ≈ 17 days single-lane. S3 estimated scoped grading "would cut this
~100×". This ADR records what was implemented, what was measured, and why
the measured factor is ~4× rather than ~100×.

## Decision

Per-unit CTest grades run only the **affected tests** for the changed unit:

- Selection (`crates/rustsmith-adapters`): enumerate `add_test`s from the
  worktree's just-configured `CTestTestfile*.cmake` (`ctest_enumerate`),
  read each test's source-dir input files once (`ctest_test_texts`), and
  select tests colocated in the unit's subsystem, referencing the subsystem
  name, referencing the unit file stem as a whole word (case-insensitive),
  or — `matc` subsystem only — using `$`-expressions (Elmer's inline-MATC
  `.sif` syntax, e.g. `Density = $rho`; `dollar_matc_hit`).
  Unresolvable tests count as affected (conservative). An empty affected
  set falls back to the full suite (never a vacuous pass).
- Execution (`mirror::run_ctest_oracle_scoped`): clone the frozen base
  command (same program/flags/`-L quick`/timeout/collect) plus
  `-R ^(a|b|...)$` (`scoped_ctest_command`), grade with the unchanged
  `CtestRunner::grade`.
- Startup-smoke gate (`matc` only): every solver run evaluates a MATC
  prelude (`function _i2str__(i) {...}`) at startup, so a fault that breaks
  definition-parsing (parser.c proof fault: 479 failures), builtin lookup
  (matc.c `com_check` proof fault: 479 failures), or shared list chaining
  (lists.c `addhead` proof fault: 479 failures) fails the whole suite
  regardless of references. Before scoping, the grade runs a
  prelude-shaped program through the built `matc` binary — one definition,
  two variables sharing one list read back (chaining faults surface as
  undeclared-identifier errors), one over-minimum-args builtin call
  (call-phase faults like the eval.c argcount proof fault keep passing
  and stay scoped). Any `MATC ERROR` or a missing read-back/call token
  falls back to the full oracle (the whole suite IS the affected set
  then). Faults that only garble sprintf output (str.c proof fault)
  conservatively fall back too: slower, never wrong.
- Integrity on the scoped path is hash-only (`mirror::scoped_integrity`:
  changed; no frozen invocation, baseline, or tolerance was touched.
- Operator surface: `rustsmith grade ... --build-dir <dir> --unit <rel>`
  grades one unit through the shipped per-unit path (ctest only, full or
  scoped with JSON evidence); without `--unit`, a ctest manifest now
  grades the full oracle in the build dir through the same JSON shape.
  The pytest path is unchanged except an additive `failed_tests` key
  (existing keys byte-identical for acceptance).

## Why not ~100×

100× of 482 is ~5 tests. A 5-test affected set is unsound for Elmer's
shape: all 482 quick tests drive the same `ElmerSolver_mpi` binary, which
links `libmatc.so`, so linkage-based selection keeps everything; the only
sound shrink is reference-based (tests whose inputs exercise the
subsystem), and MATC usage is pervasive — the `MATC "..."` form, the
`$var` and `$ var` inline forms. Measured on the pilot (see Proof):
~175 of 482 quick tests per matc unit, ~180 s of the ~390 s suite cost,
i.e. **~2.2× oracle, ~1.8× end-to-end per unit**. The remaining levers
(batching units per suite run, `ctest -j` parallelism study,
probe-primary grading) are future sessions, each with its own honesty
cost; they are NOT claimed here.

## Proof (matc pilot, all 22 units)

Protocol (proof dir on the firstmate host, `rs-elmer-s6/proof/`): pristine
baseline (full 388 s, 479/482, the 3 known upstream solver segfaults from
S3 decision A) plus one realistic single-point behavioral fault per matc
unit, each graded both ways on the same faulted tree (fault table in the
PR body). Equivalence criterion per unit: `(scoped_failures − K) ==
((full_failures ∩ affected) − K)` and `(full_failures − K) ⊆ affected`,
with `K` the 3 known failures — i.e. identical verdicts AND no sensitive
test missed by selection. Result: **AGREE on all 22 units** (4 scoped with
nonzero exact-set match: oper 53, eval 44, matrix 1, variable 1; 4
smoke-fallback with full-suite match: parser/matc/lists 479, str 4; 14
scoped true-negatives 0/0, of which graphics/binary-only faults are
structurally ctest-invisible and lu/eig/funcs/rand have no comparing
callers in the quick corpus — verified by usage grep, not assumed).
Selection is validated, not assumed; the proof forced three mapping
fixes before any verdict counted:

- `$var` inline-MATC (no `matc` literal): first run missed 6 FilmFlow
  tests broken by the `oper.c` fault.
- `$ var` with a space: second run missed 2 Shoebox tests broken by the
  `eval.c` fault (digits excluded after blanks: `US$ 5` stays out).
- Solver-startup prelude: the `parser.c` fault broke all 479 tests via
  the startup `function _i2str__` evaluation, invisible to any
  reference rule → the startup-smoke gate (definition + list-sharing
  read-back + over-min builtin call; failure falls back to full).
  The smoke itself was proof-hardened twice: `com_check` breakage needed
  the call, `addhead` chain breakage needed the read-back.

## Consequences

- Per-unit CTest grade on matc-class leaf units: ~480 s → ~270 s
  (incremental rebuild + ~180 s scoped oracle + probes/heldout unchanged).
  Startup-breaking faults (parser/matc/lists-class) and keyword-colliding
  stems (variable.c) honestly pay full price by construction.
- Fem-core units honestly keep near-full sets (any solver test can observe
  a core change); the machinery degrades to full grading there by
  construction, never to a vacuous pass.
- Whole-repo full-suite grade retained as the backstop: a scoped miss
  merges nothing releasable without tripping the full grade.
