# ADR 015: port the dateutil fuzzy parser + isoparser to Rust

- Context: ADR-013 scoped the fuzzy parser out (assumed I/O-ish string
  work with no stub-crate gap). The rs-dateutil-fuzzy report (§4) shows
  strict and fuzzy share one `_parse` path, so engine-port users' remaining
  `parse()` calls pay it too; whole-path port arithmetic gives 3-5x
  fuzzy-noisy / 2-3x strict-parse behind a single FFI crossing.
- Decision: port `_timelex` + `parser._parse` + `_parse_numeric_token` +
  `_ymd` + default `parserinfo` tables + tz builders (delegating to the
  ported tz engine) + `isoparser.py` as one unit in the established
  `dateutil-core`/binding split. Custom `parserinfo` subclasses and
  `tzinfos` callables stay dynamic via the `Info`-trait/Python-callback
  seam; `_tzparser` stays verbatim Python (tzstr-path, not fuzzy path).
  This supersedes the ADR-013 scope exclusion for `parser`/`isoparser`.
- Consequences: oracle parity exact (2031 passed incl. parser/isoparser
  suites, zero accept/reject divergence on the frozen corpora); bench
  corpora + harness live in `mirror/python-dateutil/bench_parser/` with
  manifest hashes frozen before measurement.
- Alternatives: hot-function slice (rejected: per-token FFI eats the exact
  overhead being removed; report §4).
- Spec: POC-board rank 3 scope note now covers the full parse path.
