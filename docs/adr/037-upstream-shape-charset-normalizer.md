# ADR-037: upstream shape for charset-normalizer is pure-Python (+ Cython mirror), not PyO3

Date: 2026-10-13. Lane: rs-upstream-lane-1. Target: Ousret/charset_normalizer @ 4b33559.

## Context

The lane spec's default implementation shape is a small optional speed core
(e.g. PyO3/maturin) behind a pure-Python fallback. The target repo's history
overrides the default:

- Maintainers migrated the compiled path from mypyc to Cython (`6dc5f11`)
  and ship `md.pyx`/`cd.pyx` as the accepted speed core
  (`CHARSET_NORMALIZER_USE_CYTHON=1`).
- They reverted/removed compiled-Python artifacts after user-facing breakage:
  segfault on `requests` import (#659), missing-libm linkage (#692),
  `__mypyc`-module-not-found (#714). A new Rust toolchain in their
  cibuildwheel matrix replays exactly that failure class.
- `CONTRIBUTING.md` accepts "mostly anything" that keeps backward
  compatibility with green CI — a pure-Python change mirrored into the
  maintained `.pyx` sources fits; a new build toolchain does not.

## Decision

Carry the win as a pure-Python optimization of the proven-hot unit
(`md.py::mess_ratio`, plus `cd.py` helpers if measured), mirrored into
`md.pyx`/`cd.pyx`, with zero behavior change proven by differential oracle.
No PyO3/maturin extension, no new dependency, no rewrite.

## Consequences

- The PR stays inside what maintainers already build, test (pure + Cython
  CI matrices), and accept. Merge probability is higher; maintenance burden
  on them is nil.
- Absolute speedup ceiling is lower than a native core. Accepted: honesty
  rules require measured proof, and a small real win in acceptable shape
  beats a large win they will not take.
