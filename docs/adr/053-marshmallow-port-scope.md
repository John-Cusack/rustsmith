# ADR-053: marshmallow port scope and seam shape

Date: 2026-10-28. Lane 3 of the rs-rerank-2 report (§3).

## Context

The rerank prescribes seam shapes i+iii for marshmallow: field codecs +
validation kernel to core with whole-batch crossing; custom `Field`
subclasses and validators fall back to the pure-Python schema path via a
documented seam; dynamic/user-extension paths stay Python behind a
documented seam; pure-Python fallback importable without the extension.

## Decision

- Whole-schema fallback triggers only for: decorated schema hooks,
  overridden dump/load/_do_load/_deserialize/_serialize entry points, or a
  missing extension. Everything else compiles, including schemas with
  custom `Field` subclasses, custom validators, field pre/post_load
  processors, callable defaults, and Method/Function fields.
- Custom `Field` subclasses compile to **opaque nodes** that invoke the
  field's own `serialize`/`deserialize` per value (same cost as upstream
  for that field, full speed elsewhere) — a per-field slow path by
  construction, rather than the whole-schema fallback the rerank text
  suggests. Same for user callables (validators, processors, defaults,
  Method/Function targets): invoked via C-API calls with identical
  observable behavior, including exception types.
- Rationale: a single `validate=lambda` on one field of thirty should not
  forfeit the kernel win on the other twenty-nine; exactness is preserved
  in both shapes because the same callables run in the same order.
- Error-message formatting happens Python-side from templates extracted at
  compile time with the live offending values, so messages (including
  custom `error_messages` with `{input}` interpolation) are identical by
  construction. Only accept/reject decisions and tree shapes are replayed
  in Rust.
- Exact-type fast paths (int/float/str/bool, exact-dict extraction and
  storage) eliminate per-value calls on the common path; every other type
  keeps the C-call path. Non-ASCII numerics and exotic objects always take
  the exact C-call path — never a hand-rolled approximation.

## Consequences

- Parity gate: full upstream suite 1188/1188 via import alias, plus a
  seeded differential fuzzer (200 randomized schemas). The fuzzer caught
  three real divergences during development (valid_data on validator
  failure, nested partial defaults, unbound `isoformat` on dump); all
  fixed, all covered by regression tests in the mirror.
- Measured 4.53x on the frozen nested-batch-500x4 workload (kill line was
  <1.5x). Port peak RSS runs ~2 MB above upstream (extension pages);
  disclosed, not gated.
