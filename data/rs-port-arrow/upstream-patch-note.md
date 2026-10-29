# Upstream-ready patch note: memoize the per-parse pattern build (arrow-py/arrow)

Status: patch note only — no upstream PR opened (lane rule).
Target: arrow 1.4.0, `arrow/parser.py` + `arrow/factory.py`.

## Problem

Every `arrow.get(s)` / `arrow.get(s, fmt)` constructs a fresh
`DateTimeParser(locale)` with `cache_size=0`, so each parse pays:

1. `DateTimeParser.__init__` — locale lookup + 6 `_generate_choice_re`
   → `re.compile` calls (≈28% of a fmt-parse call);
2. `_generate_pattern_re(fmt)` — full regex rebuild from the format
   string (≈42% of a fmt-parse call, top cProfile frame).

The existing `lru_cache` opt-in (`cache_size > 0`, default 0) cannot
help this path: it wraps the *bound method per instance*, and the
factory discards the instance after one call. Measured on 2000-item
batches (Ryzen 9 5950X, Python 3.12, min of 5):

| arm | fmt-parse 2000 | vs baseline |
|---|---|---|
| A baseline `arrow.get(s, FMT)` | 44.6 ms | 1.00x |
| B one shared parser, `cache_size=0` | 24.3 ms | 1.84x |
| C one shared parser, `cache_size=128` | 7.4 ms | 6.03x |
| D `arrow.get` + class-level `(locale, fmt)` memo prototype | 22.9 ms | 1.95x |
| end-to-end representative pipeline (parse→shift→format→humanize) | 74.4 → 37.4 ms | **1.99x** |

ISO-string parsing (`parse_iso` → `parse`) hits the same frame and
gains the same way (iso-parse 61.8 ms shares the rebuild).

## Proposed change (behavior-preserving)

- Memoize `_generate_pattern_re` at class level keyed by
  `(str(locale), fmt)` instead of (or in addition to) the per-instance
  `lru_cache`, so the factory path reuses compiled patterns across
  instances; and/or
- cache one `DateTimeParser` per locale in `ArrowFactory` (or default
  `cache_size` to a small nonzero value *combined* with instance
  reuse — either half alone leaves half the cost: D shows pattern-only
  memo is 1.95x, instance reuse alone is 1.84x).

Compiled patterns are immutable and `(locale, fmt)`-determined, so the
cache is semantically transparent: identical `re` objects, identical
matches and errors.

## Costs / risks to review

- Memory: one compiled pattern per distinct `(locale, fmt)` — bounded
  by caller variety; suggest `maxsize` (e.g. 128) via `lru_cache` on a
  classmethod taking `(locale_key, fmt)`.
- Threads: guard with the same discipline as the existing `lru_cache`
  (GIL-atomic `dict`/`lru_cache` access is sufficient for CPython).
- `cache_size=0` default stays valid: the class-level memo only
  replicates what a shared instance already computes.

## Why not a Rust port (lane verdict)

After this memoization, the representative pipeline is 37.4 ms/2000
items with parse 7.1 + format 9.3 = 16.4 ms addressable by a
parse/format kernel port. Even at infinite kernel speed the ceiling is
37.4/21.0 = **1.78x < 2x** kill bar (realistic ≈1.4–1.5x); the rest is
`Arrow.shift`/object-model dispatch across `arrow.py` + locale-aware
`formatter.py` + `locales.py` — dispatch-dominated, and widening scope
to cover it means reimplementing locale data in Rust (mirror-scale
price, parity risk). Honest `no_gain`; the pure-Python memo above is
the win worth taking.

## Validated candidate (lane-tested, not submitted)

`data/rs-port-arrow/tier1-memo.patch` implements the above against
arrow 1.4.0 (`parser.py`: class-level `(locale-class, fmt)` memo, 128
entries; `factory.py`: one parser per locale string). Verified:

- Equivalence: 20-case corpus (ISO, 14 format shapes incl. locale
  month/day names, week dates, meridiem, fractional seconds, plus 5
  error-path cases) byte-identical stock vs patched, incl. identical
  `ParserMatchError`/`ParserError` messages.
- Locale isolation: `MMMM D, YYYY` parsed correctly under en/fr/de with
  3 separate cache entries.
- End-to-end through `arrow.get` (patched venv, same box): fmt-parse
  44.6 → 10.9 ms (**4.09x**), iso-parse 61.8 → 26.0 ms (**2.38x**),
  representative pipeline 74.4 → 39.0 ms (**1.91x**).
