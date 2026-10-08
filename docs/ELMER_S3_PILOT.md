# Elmer S3 pilot report: model-backed worker on the matc slice

Session S3 backs `RUSTSMITH_WORKER_CMD` with a real model and runs the
pilot slice green. Scope: all 22 `matc/src/*.c` units (stdin-capable REPL;
S2 probes landed). Run dirs isolated under `/tmp/s3` (never `~/runs`).

## Outcome

- All 22 matc units ported (model worker, ≤2 iterations for 19/22;
  eval 17 attempts, matc 18, parser 14 — difficulty signal, see §4).
- Every port individually green: builds, provides all required C symbols,
  single-splices into a pristine matc tree, and passes the extended stdin
  corpus byte-identical to pristine `matc` (12 cases + paired-`rand`).
- All 22 ports combined-spliced into ONE `matc` binary: corpus green
  (`SPLICE22_GREEN`, see §2).
- Mirror harness: worker → substitute → integrity all pass for the slice.
  `oracle_parity` halts on **3 pristine ElmerSolver failures**
  (decision A: documented below, not investigated — upstream, out of scope).

## 1. Measured numbers (this host)

| quantity | value | source |
| oracle full quick suite wall (pristine) | 388.55 s / 390.46 s (2 runs) | `ctest -L quick`, 482 tests |
| oracle suite result (pristine) | 479 pass, 3 fail | `LastTest.log` |
| mirror run wall (`--run-id s3-matc-green3`) | 523 s (fresh fork configure+build + unit-1 pipeline incl. ~390 s oracle) | `mirror-green3.log` |
| preflight per port (build+splice+13-case corpus) | ≈10 s | `preflight.sh` |
| combined 22-port splice + corpus | GREEN: 13 basic + 6 scanner cases (`:`, `,`, `;`, `while`, reverse range) byte-identical | `splice22.sh` + extended checks |
| mirror unit overhead (worker+substitute, excl. oracle) | ≈130 s (523 − 390 oracle) | green3 run log |
| parser column-alignment fix effect | 61 → 482 tests counted; integrity `ok`, `test_count: 482` in-harness | green3 `gate_results` |
Per-unit oracle cost dominates: each unit grade runs the FULL 482-test suite (~390 s). Extrapolation in §3.

## 2. Evidence

- Per-port preflights: 21/22 overlay ports byte-identical to their
  preflight-green worker versions (`w-eval12`, `w-lists2`, `w-lu2`,
  `w-matc18`, `w-parser10`, `w-variable2`, single-attempt rest);
  `error` re-preflighted green (`preflight.sh matc/src/error.c …`).
- `splice22`: all 22 archives `ld -r` over a fresh Debug tree, one relink
  (`-Wl,--allow-multiple-definition` for same-toolchain std/alloc dups,
  §6), 13-case corpus byte-identical (`SPLICE22 GREEN`) plus 6
  scanner-sensitive cases (`1:10`, `sum(1:10)`, `ones(2,2)`, `[1,2;3,4]`,
  `while` loop, `10:1`) byte-identical after the §6 table fix.
- Mirror green3 (`--run-id s3-matc-green3`, wall 523 s, exit 1): integrity
  `482/482` PASS with the column-alignment parser fix (`reason: ok`);
  parity FAILs honestly on the 3 pristine solver segfaults (`failed: 3`,
  `reason: failures`, exit 8); heldout divergence PASS (`-0.006` vs 0.05
  threshold); run halts at unit 1 per the honest-gate rule. The substituted
  tree (Rust `main.o`) passes 479/482 — the port introduces zero regressions.
- Full suite: `cargo test --workspace` 228 passed / 0 failed;
  `cargo clippy --workspace --all-targets -- -D warnings` clean.


## 3. Full-port budget extrapolation

Grade cost per unit ≈ oracle wall (~390 s on this host) + harness overhead
(≈60–120 s: substitute rebuild + differential probes). Take ~480 s/unit.

- 3025 file-granularity units × ~480 s ≈ 1.45 M s ≈ **17 days** of
  single-lane oracle+grade time, serial. Parallel lanes divide wall time;
  the oracle build itself is the serial bottleneck per grade.
- Model porting effort (interactive this pilot): 19/22 units green in ≤2
  worker iterations; eval/matc/parser needed 14–18 (REPL-loop + table
  shadowing + token-stream corners). Expect the long tail (solver Fortran,
  MPI ranks) to dominate model cost, not the C leaves.
- Structural finding: per-unit grades re-run the whole-suite oracle, so
  grade cost is O(units × suite). Scoped per-unit grading (only affected
  tests) would cut this ~100× and is the evident next-session lever —
  NOT implemented here (grade code untouched).

## 4. Pristine failures (decision A documentation, not investigated)

3 `fem/tests` solver cases segfault under `ElmerSolver_mpi` on the clean
tree (no ports involved) and fail identically in every worktree build:

- `ConstantBCTemperature`, `ProfileBCTemperature`,
  `ProfileBCTemperatureRobin` — SIGSEGV in `DefaultDirichletBCs`
  (`BoundaryConditionUtils.F90:1237`, via `HeatSolve`), `mpiexec` reports
  `exited on signal 11`.
- Repro: `cd <pristine-build> && ctest -R ConstantBCTemperature`
  (also fails single-test isolated, and with `ulimit -s unlimited`).
- Backtrace pointer: `fem/tests/ConstantBCTemperature/test-stderr1.log`
  in any full-suite build dir (rank-0 frames
  `__defutils_MOD_defaultdirichletbcs` ← `heat solver` ← `ElmerSolver`).
- Baseline (`ctest -N` count 482) never executed tests, so these predate S3;
  first full-suite execution on this host was this session (479/482).

## 5. Code changes in this PR

- `crates/rustsmith-adapters`: subdir-aware C object discovery
  (walk `CMakeFiles` for `.dir/<rel>.o` instead of assuming
  `<name>.dir/<stem>.o`) + layout regression tests.
- `crates/rustsmith-adapters`: column-tolerant `Test #N:` parsing
  (`ctest_test_rhs`; ctest aligns `#` by suite width) + regression test.
  Without it, 482-test suites count 61 and every Elmer grade tamper-halts.
- `crates/rustsmith-cli/data/repo-content.json`: `matc` porting rules +
  differential probes (1→4: `1+2`, `1:10`, `ones(2,2)`, `[1,2;3,4]`) and
  `fhutiter` entries (Elmer data entry, no code).
- `crates/rustsmith-cli/src/mirror.rs`: `matc_probe_carries_repl_stdin`
  (pinned the S2 single-probe count) replaced by
  `matc_probes_carry_repl_stdin`, which asserts the intent (every probe
  stdin-fed, empty argv, REPL-terminating) plus scanner-surface coverage.

## 6. Pilot bug found and fixed: matc scanner tables

Combining all 22 individually-green ports into one binary exposed a real
divergence single-splices missed: `x=1:10` evaluated to scalar `1`
(expected the 1..10 range), `ones(2,2)` mis-parsed, `[1,2;3,4]` rejected.
Root cause: the model worker garbled `matc.c`'s lexer tables when porting —
`csymbols` lost `{ } # ? ' < > & | ~ @ % $` (14 entries + zero padding),
`ssymbols` held guessed values (`:`→nullsym instead of `vector`), and
`rsymbols`/`reswords` were all-zero/misordered (no reserved word
recognized). Single-splice preflights passed because the corpus never
exercised `:`, `,`, `;` distinctively. Bisected oper→parser→optim→eval→…
down to the matc unit (pairwise splices), then fixed by mechanical
re-transcription from `elmer/matc.h` (verified 1:1, 27/27 mappings).
Single-splice + pair + full 22-splice all green after the fix.

Consequences landed in this PR:

- `matc` differential probes extended 1→4 (`1+2`, `1:10` range, `ones(2,2)`
  comma args, `[1,2;3,4]` semicolon rows): the mirror differential gate now
  covers the scanner-sensitive surface that hid this bug.
- Multi-unit linking note: 22 same-toolchain Rust staticlibs collide on
  std/alloc monomorphizations at final link (`multiple definition of
  …handle_alloc_error…`). Combined splices link with
  `-Wl,--allow-multiple-definition` (identical definitions, dedup-safe;
  verification strength unchanged — every symbol must still resolve and
  the corpus must still match). The real merge path will need the same
  treatment or a single-workspace port layout (future work, §3 lever).
