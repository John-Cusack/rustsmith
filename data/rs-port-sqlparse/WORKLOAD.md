# WORKLOAD.md — rs-port-sqlparse (frozen 2026-10-28)

Probe: `rustsmith headroom probe` → `data/headroom/sqlparse.json`
(score 0.75 → rewrite; pure 88%, pipeline × 0.85, wall 0.87 s).

## Workload file

`crates/rustsmith-headroom/workloads/sqlparse_split_parse_format.py`
(`setup()` builds once, `run()` executes; doc-synthetic — upstream
`benchmarks/` vectors are DoS growth-regression shapes (GHSA-prg7-hcfm-mfcr,
GHSA-f2ff-p2ww-7p4p, GHSA-cfqr-cjx5-5jcm), not representative throughput,
so they are not the probe/bench basis).

## Payload (representative DDL/DML mix, ~13 KB, 121 statements)

- 1× `CREATE TABLE` (typed columns, defaults)
- 60× `INSERT … VALUES` (numeric + string literals)
- 40× `SELECT … JOIN … WHERE … ORDER BY … LIMIT` (aliases, comparison, sort)
- 20× commented `UPDATE` (`-- refund batch` trailing comments)

## Entry points per `run()` iteration (whole SQL string crosses once each)

1. `sqlparse.split(SCRIPT)` — statement splitter
2. `sqlparse.parse(SCRIPT)` — lexer + grouping engine
3. `sqlparse.format(SCRIPT, reindent=True, keyword_case="upper")` — full filter stack
4. `sqlparse.format(SCRIPT, keyword_case="upper", strip_comments=True)` — light path

## Probe settings

- package `sqlparse==0.6.0`, iters `4`, tool `cprofile`
- python `3.12.3`, machine `AMD Ryzen 9 5950X` (calibration box)
- per-pass ~0.41 s (split 0.04 / parse 0.12 / reindent 0.14 / light 0.11)

## Two-track bench workloads (same payload, labeled per figure)

Gate figures use this payload at 1× and 8× statement counts plus one
single-statement micro shape; every figure labeled `split` / `parse` /
`format-reindent` / `format-light`; wall + peak RSS both sides (PyPy-free
CPython 3.12 vs release extension).

## Kill criteria (spec)

<2× on representative split+format workloads, or per-token FFI toll
dominating under the caller-side check → stop, report numbers as `no_gain`.
