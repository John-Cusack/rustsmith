# ADR-053: sqlparse port scope and architecture

Date: 2026-10-28. Status: accepted. Lane: rs-port-sqlparse (rerank §3 lane 4).

## Context

- Rerank: sqlparse 110M dl/mo, headroom 0.77. Re-probe frozen
  (`data/rs-port-sqlparse/WORKLOAD.md`, split+parse+format×2 over a 121-stmt
  mixed script): score **0.75 → rewrite** (pure 88%, pipeline × 0.85,
  `data/headroom/sqlparse.json`). Top frames: `re.Pattern.match` (546K
  calls, shared C `re`), `grouping._group`, `lexer.get_tokens`,
  `utils.imt`, `sql._token_matching` — lex dispatch + grouping dispatch.
- Upstream: andialbrecht/sqlparse 0.6.0, BSD-3-Clause, 21 shipped `.py`,
  12 test files (~3300 lines). Own `benchmarks/` are DoS growth-regression
  vectors (GHSA-prg7-hcfm-mfcr, GHSA-f2ff-p2ww-7p4p, GHSA-cfqr-cjx5-5jcm),
  not throughput shapes — not the bench basis.

## sqlparser-crate gap audit (2026-10-28, crates.io API)

`sqlparser` 0.63.0 (86.9M dl): "Extensible SQL Lexer and Parser with
support for ANSI SQL:2011", repo apache/datafusion-sqlparser-rs. Gap:

1. It is a **validating** ANSI parser producing a typed AST per dialect;
   sqlparse is a **non-validating** tokenizer that never rejects input and
   exposes the raw token stream, grouping classes, and statement splitting.
2. It has **no format/pretty-print path** (`reindent`, `keyword_case`,
   `strip_comments`, aligned indent, output wrapping) — the measured third
   of the workload.
3. Its `Display` impls emit canonical SQL, not sqlparse filter-stack output.

Adopt/partial-equivalent rejected: no full equivalent to adopt.

## Decision: kernel-in-Rust, tree-stays-Python

Port the whole hot unit — default-config lexer + delimited-span pre-scan +
statement splitter + grouping engine + format/split/tokenize string paths —
into `sqlparse-core` (no Python dependency). The **whole SQL string crosses
once in**; results cross once out:

- `parse` → nested flat node tuples; **one** Python assembly pass builds
  genuine `sql.py` objects (that module stays byte-identical, so the entire
  tree API — `match` identity semantics, `group_tokens` mutators,
  `get_type`, `Case.get_cases`, `Function.get_parameters` — is exact by
  construction).
- `format`/`split` → Rust end-to-end, returns `str`/`list[str]` (all filter
  logic replicated; `RightMarginFilter` stays a raising stub like upstream).
- `tokenize` → list of `(ttype, value)` wrapped in `iter()`.

`sqlparse/` facade ships all 21 upstream modules byte-identical except
`__init__.py` (route the 4 entry points to `_core` when pristine) and
`lexer.py` (route `get_tokens` through `_core` when pristine). Everything
else — `sql.py`, `engine/`, `filters/`, `formatter.py` (validation messages
free), `utils.py`, `tokens.py`, `keywords.py`, `cli.py` — is upstream code,
so the pure-Python fallback (extension missing, `SQLPARSE_PURE=1`, or
mutated `Lexer` singleton / custom user filters) is exact with no import
rewrites. `tokens.py`/`keywords.py` stay Python: `_TokenType` tuple
dynamics and 813-entry keyword order (3 first-wins conflicts) are data, not
CPU. Rust bakes the default config (SQL_REGEX order + merged keyword map,
generated from installed 0.6.0 with provenance header).

Seams (documented in mirror README): `Lexer.clear`/`set_SQL_REGEX`/
`add_keywords` mutations, custom FilterStack filters, direct use of
`grouping.group` on hand-built trees → pure path. Default entry points
never consult Python callbacks in the hot loop.

## Kill criteria (unchanged from WORKLOAD.md)

<2x on representative split+format workloads, or per-token FFI toll
dominating under the caller-side check → `no_gain` with numbers. The
12%-`re.Pattern.match` share already runs in C: the Rust scanner must beat
compiled `re` on the lex path or the ceiling fails — measured at the
two-track bench, not assumed.

## Consequences

- Equivalence is differential: upstream suite via facade (Rust kernel +
  Python assembly) plus fuzz corpus facade-vs-pure on generated SQL,
  including non-ASCII identifiers (Python `\w`/casefold vs Rust
  classifier), lookbehind edge (`(?<=\.)`, FIXME-atronah never-match),
  unclosed dollar/comment delimiters, and DoS caps (same messages;
  explicit depth counter, no unbounded Rust recursion).
- Publish shape: drop-in mirror template (dist `sqlparse-rust`, import
  `sqlparse`), BSD-3-Clause, repo `John-Cusack/sqlparse-rust` (live,
  seeded with LICENSE+NOTICE+README).
