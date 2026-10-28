# rs-port-sqlparse bench record (measured 2026-10-28)

Release wheel (`maturin build --release`), Ryzen 9 5950X, CPython 3.12.
Runner: `benches/bench_two_track.py` in
https://github.com/John-Cusack/sqlparse-rust (same installed wheel both
sides; `SQLPARSE_PURE=1` selects pure). Wall = per-call best-of-3.
RSS = subprocess peak `ru_maxrss` per workload.

## Wall (per call)

| workload | pure | rust | speedup |
|---|---|---|---|
| split-1x (~13 KB, 121 stmts) | 31.0 ms | 4.2 ms | 7.4x |
| split-8x (~106 KB) | 255 ms | 32 ms | 8.1x |
| parse-1x | 91.1 ms | 15.9 ms | 5.7x |
| parse-8x | 839 ms | 157 ms | 5.3x |
| format-reindent-1x | 105 ms | 9.0 ms | 11.7x |
| format-reindent-8x | 895 ms | 76 ms | 11.8x |
| format-light-1x | 106 ms | 8.5 ms | 12.4x |
| format-light-8x | 784 ms | 69 ms | 11.4x |
| parse-single | 0.35 ms | 0.082 ms | 4.3x |
| format-single | 0.47 ms | 0.044 ms | 10.6x |

Kill criteria (<2x on split+format): passed on every workload (min 4.3x).

## Peak RSS (both sides, KiB)

| workload | pure | rust |
|---|---|---|
| baseline (import only) | 13244 | 13612 |
| split-1x | 12364 | 13724 |
| split-8x | 13464 | 16276 |
| parse-1x | 20508 | 23996 |
| parse-8x | 31412 | 34252 |
| format-reindent-1x | 14192 | 14240 |
| format-reindent-8x | 14476 | 15576 |
| format-light-1x | 13640 | 14020 |
| format-light-8x | 13200 | 15352 |
| parse-single | 13720 | 13464 |
| format-single | 12932 | 12972 |

RSS runs ~0-20% above pure on bulk workloads (batch token tables vs the
pure generator pipeline; ±1 MB run-to-run noise on this box). parse-8x
improved from +33% to +9% by constructing the tree directly in the
binding (no intermediate tuple tree); the lexer drops its char/offset
tables before returning.

## Parity counts (all against the installed wheel)

- Upstream suite (0.6.0, verbatim): 506 passed, 2 xfailed, 1 xpassed
  (the xpass also xpasses on pure upstream — stale xfail, identical).
- `tests/test_lex_parity.py`: 2 passed (60+ edge + 520 generated inputs).
- `tests/test_diff_fuzz.py`: 7 passed (parse shape + parent links +
  get_type over 60 edge + 870 generated; format matrix over 29 option
  sets; split matrix; DoS + right_margin error parity).
- `tests/test_seams.py`: 10 passed (pure fallback, missing extension,
  custom lexer/filters, coercions, TypeError).
- Release-prep battery (`rustsmith release-prep`): all green (core
  package, smoke exprs, release profile, rust consumer, core tests).
