# ADR-054: babel hot-unit port (number/currency/date kernel + pattern parsing)

Date: 2026-10-28. Status: accepted.

## Context

Rerank lane 4 (rs-rerank-2 §2 rank 4): `babel` 2.18.0 (104M dl/mo),
headroom 0.79. Frozen-`WORKLOAD.md` re-probe
(`crates/rustsmith-headroom/workloads/babel_number_date.py`, 6 locales ×
9 ops, setup warms pickle load) scored **0.79 → rewrite**
(`data/headroom/babel.json`: pure 93%, pipeline, loop_factor 0.85,
io_wait 0.0095, no native cover; top frames `localedata.__getitem__`
+ `numbers.apply` + `core.parse_locale`/`parse`). Pickle-I/O kill line
does NOT trigger: cold load of all 6 locales is 27.5 ms one-time vs
201 ms per formatting run; steady-state top frames are lookup/format CPU.
ICU4X audit (2026-10-28, `icu` 2.4.0 = `icu@2.4.0` tag source):
`icu_decimal` formats basic decimals only — "Support for currencies,
measurement units, and compact notation is planned ... icu4x#275" (no
`format_currency`/compact equivalent); `icu_datetime` neo
`DateTimeFormatter` takes typed fieldsets, and the only pattern-string
API (`DateTimePatternFormatter`) "forgoes most internationalization
functionality ... assumes that the pattern is already localized" (no
babel-equivalent localized custom-pattern formatting); `icu_locale` is
BCP47 canonicalization, not babel `en_US`+modifier+likely-subtag
identifiers. No drop-in equivalent — adopt rejected, port stands.
CLDR bulk (10 MB sdist) is the priced lane cost, not a kill line.

## Decision

Port the formatting kernels + number/date pattern parsing as one hot
unit (`babel-core` in `John-Cusack/babel-rust`, repo-early rule) with
batch-friendly entry points (whole formatted value crosses once; locale
data resolved once per locale, never per-call FFI marshaling of CLDR
dicts). User-supplied patterns and plural rules stay Python behind a
documented seam (host-callback shape, slow path by construction);
pure-Python fallback stays importable without the extension. Do NOT
vendor a second copy of CLDR into the Rust crate unless a bench proves
the pickle-load path is the bottleneck (cold load is 27.5 ms today).
This repo carries only the workload, `data/headroom/babel.json`, bench
evidence, board row, and this record.

## Consequences

Equivalence is per-locale output equality (differential corpus across
many locales + full upstream suite against the installed wheel, Rust
and fallback paths identical). Two-track benches (wall + peak RSS both
sides, every figure labeled by workload); kill criteria re-applied
post-port (<2× on representative formatting workloads → honest
`no_gain`). Registry publishes stay human-gated (no credentials in-lane).
