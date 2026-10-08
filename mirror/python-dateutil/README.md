# python-dateutil-rust

Pure-Rust port of the `python-dateutil` engine (`rrule`, `relativedelta`,
`tz`) plus the whole `parser`/`isoparser` path, with Python bindings
preserving the `dateutil` interface.
rustsmith Stage-1 mirror of [dateutil/dateutil](https://github.com/dateutil/dateutil)
(Apache-2.0), covering the recurrence/calendar/timezone engine and the
fuzzy/strict/ISO parse path (ADR-015 supersedes the ADR-013 parser
exclusion).

Layout: reusable core crate (`dateutil-core`, no Python dependency) plus a
thin PyO3 binding (`dateutil._dateutil`); `src/dateutil/{rrule,relativedelta,tz/tz}.py`
and `src/dateutil/parser/{_parser,isoparser}.py` are re-export shims over
the extension (`_parser_py.py` alongside is the verbatim upstream module
kept for the `_tzparser` lineage). Custom `parserinfo` subclasses and
`tzinfos` callables stay dynamic via the `Info`-trait/Python-callback seam
documented in `src/parser_mod.rs`. Two-track bench corpora + harness live
in `bench_parser/` (manifest hashes frozen before measurement; workload
contract and repro command in `bench_parser/WORKLOAD.md`).
