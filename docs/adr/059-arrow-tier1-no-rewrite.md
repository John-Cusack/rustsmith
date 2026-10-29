# ADR-059: arrow lane — tier-1 memoization measured, Rust rewrite declined (no_gain)

Date: 2026-10-28. Lane: rs-port-arrow-01 (arrow 1.4.0, 60.2M dl/mo).

## Context

Rerank (§2 rank 7) scored arrow 0.71 rewrite with a tier-1 do-less-work
candidate: the per-parse `_generate_pattern_re` rebuild (top frame, 12%
stdlib `re`). Lane spec: measure tier-1 in pure Python first, re-probe a
frozen workload, kill at <2x over the memoized baseline or on a
dispatch-dominated hot path.

Guide consult (rustsmith `a8e97f1`): expected P6 (compile once, run
many — the pattern build is a per-call compile), P3 (hoist the
locale/choice-regex build out of the per-item path). Losers to avoid:
markdown-it 0.20x (no per-token crossings — batch the unit), pygments
1.00x (don't port where C-`re` dominates).

## Measurements (Ryzen 9 5950X, Python 3.12, min of 5, WORKLOAD.md v1)

Frozen workload: 2000-timestamp batches, parse (ISO + explicit format)
→ shift → format → humanize. Headroom probe on the representative
pipeline reproduces the rerank: **0.72 rewrite, pure 85%, pipeline**,
top frame `_generate_pattern_re` (`data/headroom/arrow.json`).

Tier-1 (pure Python, [`bench_tier1.py`](https://github.com/John-Cusack/arrow-rust/tree/main/bench) in arrow-rust — relocated out of `data/rs-port-arrow/`):

- fmt-parse 2000: baseline 44.6 ms → shared parser 24.3 ms (1.84x) →
  shared + `cache_size=128` **7.4 ms (6.03x)**; class-level memo through
  stock `arrow.get` 22.9 ms (1.95x).
- Representative end-to-end equivalent: **74.4 → 37.4 ms (1.99x)**.
- Mechanism split per call: pattern rebuild ≈42%, parser `__init__`
  (locale + 6 choice compiles) ≈28%, match/extract/build ≈30%.
  Upstream's `lru_cache` opt-in (default `cache_size=0`) cannot fire on
  the `arrow.get` path because `factory.py` builds a fresh parser per
  call — the cache dies with the instance. Upstream-ready patch note ([`upstream-patch-note.md`](https://github.com/John-Cusack/arrow-rust/blob/main/tier1/upstream-patch-note.md), patch [`tier1-memo.patch`](https://github.com/John-Cusack/arrow-rust/blob/main/tier1/tier1-memo.patch); no PR opened).

## Decision: no_gain, no Rust port

Kill-bar math against the memoized baseline (37.4 ms): a parse/format
kernel port addresses 16.4 ms (parse 7.1 + format 9.3). Infinite-speed
ceiling = 37.4/21.0 = **1.78x < 2x**; realistic ≈1.4–1.5x. The
remainder is `Arrow.shift`/object-model dispatch plus locale-aware
formatting (`formatter.py`, `locales.py`) — dispatch-dominated, and
folding it in scope means reimplementing locale data in Rust.
`no_gain` recorded in lane store; `John-Cusack/arrow-rust` created
per repo-early rule and seeded with license/notice/outcome only.

## Consequences

- Offer the memoization upstream (note ready); do not renominate arrow
  for a rewrite lane without a new workload showing ≥2x over the
  *memoized* baseline.
- Pattern-ledger input: per-instance `lru_cache` on a factory-built
  object is a no-op cache — P6 "compile once" must live at or above the
  constructing call site. Propose as P6 loser-line extension in the
  post-lane pattern review (thumbnail: arrow D-arm 1.95x vs C-arm
  6.03x — same cache, wrong level).
