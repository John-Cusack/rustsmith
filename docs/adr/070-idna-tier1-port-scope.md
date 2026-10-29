# ADR-070: idna tier-1 measured, rewrite SURVIVES (port scoped)

Date: 2026-10-29. Lane: rs-port-idna (brief: rerank-4 §3 Lane 1, 1339.5M
dl/mo, headroom 0.79 rewrite-phenotype). Outcome: **port** — the first
survivor since networkx. Kill criteria evaluated below; neither fired.

## Frozen workload (lane `WORKLOAD.md`)

`idna.encode(d, uts46=True)` + `idna.decode(d, uts46=True)` per domain in
a plain loop. Representative `mixed-8000` (70% valid ASCII, 20% IDN incl.
punycode-in, 10% invalid); showcase `ascii-16000` and `idn-4000`;
overhead `x1-valid/idn/invalid`. Deterministic seed 20261029. Box: Ryzen 9
5950X, CPython 3.12.3, idna 3.20.

## Section 1.3 pre-lane check (measured)

- T0: 0 `.so` files; all hot frames pure Python. T1 rebuilds: 0.0%.
  T2 model construction: 0.0% (bytes/str in-out, no object model).
  T3 user code: none on the hot path. No K1/K2/K3.
- T4 engines: C `bisect` 3.5%, builtins ~11%; `re` only for label
  splitting (C-speed). `encodings/punycode.py` is **pure-Python stdlib**
  (10.9% of wall) — portable by reimplementation in-core.
- T5 diffuse: no (top1 0.15 `check_label`, top5 0.42, pipeline).
  T6 shared-dep: none (no K6 — unlike email-validator, idna IS the dep).
- Per-op cost ~30µs/domain-pair vs ~0.5µs crossing toll: P1 viable.

## Tier-1 (pure Python, upstream-plausible; suite-identical)

Patch: [`tier1/tier1.patch`](https://github.com/John-Cusack/idna-rust/tree/main/tier1)
(applies to installed 3.20; `bench/bench_tier1.py` applies it to a temp
copy itself). Upstream-ready note in
[`tier1/upstream-patch-note.md`](https://github.com/John-Cusack/idna-rust/tree/main/tier1)
(no upstream PR opened, lane policy).

- **T-ascii** (P3+P7): lowercase-ASCII-valid label fast path
  (`_ascii_label_ok_re`: 1–63 chars, alnum ends, no `--` at 3–4 — which
  also keeps every `xn--` A-label on the slow path) in `check_label`,
  `alabel`, `ulabel`. Match implies NFC/combiner/PVALID/bidi all pass
  (verified: A–Z not PVALID, ASCII never category-M, ASCII NFC-stable);
  anything else falls through with identical errors. Uppercase ASCII
  keeps slow-path behavior (e.g. `encode("ABC.com")` → `b"ABC.com"`).
- **T-import** (P6): hoist `from .uts46data import ...` out of
  `uts46_remap` to module top (no cycle: `uts46data` imports only `array`).

Measured (`bench/bench_tier1.py` in idna-rust, min-of-7 × 3 sessions,
per-cell best-of-21, worker self-RSS):

| Track | Stock | Tier-1 | Speedup |
|---|---|---|---|
| mixed-8000 (representative) | 0.2228 s | 0.0856 s | **2.60x** |
| ascii-16000 | 0.4264 s | 0.0567 s | **7.52x** |
| idn-4000 | 0.1398 s | 0.1248 s | **1.12x** |
| x1-valid / x1-idn / x1-invalid | 24 / 31 / 13 µs | 4 / 27 / 6 µs | — |

Peak RSS: stock ~14.8–15.3 MB, tier-1 ~15.1–16.0 MB (delta is
worker-harness temp-copy import, not a library regression).

Equivalence: upstream suite (`tests/` from the 3.20 sdist, incl.
hypothesis property tests) **6441 passed + 56 subtests both arms**;
differential corpus 41 adversarial labels × 4 flag sets × 5 functions =
**900/900 identical** incl. exact exception types/codes/messages.

## Ceiling recomputed against tier-1 (kill math)

Warmed cProfile attribution of 3× representative on the tier-1 arm:

| Bucket | mixed | idn |
|---|---|---|
| idna-own Python (`core`+`intranges`) | 56.9% | 49.4% |
| stdlib punycode (pure Python, reimplemented in-core by any port) | 10.6% | 23.1% |
| C (`bisect` 2.5%, `unicodedata`, `re`, str/bytes methods) | 14.1% | 16.3% |
| **portable total → infinite-speedup ceiling** | **67.5% → 3.08x** | **72.5% → 3.64x** |
| own-Python-only ceiling (punycode excluded) | 2.32x | 1.98x |

Top remainder frames are all Python loops: `check_label` per-char
classify, `intranges_contain` wrapper (`_encode/_decode_range`
bit-twiddling around one C `bisect`), `uts46_remap` per-char loop,
`check_bidi` per-char loop, stdlib punycode algorithm.

## Kill-criteria evaluation

- Clause 1 (ceiling <2x over tier-1): 3.08x mixed / 3.64x idn with the
  standard punycode-in-core scope → **not fired**. (Own-only 2.32x/1.98x
  is noted; punycode reimplementation is routine — the Rust `idna`
  ecosystem carries its own — so excluding it would be false modesty.)
- Clause 2 (label-cache + ASCII-fast-path removes the majority, remainder
  is C-`bisect` + table lookups): tier-1 ASCII path removed 62%
  mixed / 86% ascii, but the remainder is Python loops + Python punycode,
  **not** C (`bisect` is 2.5% of wall) → **not fired**.
- Analyzed but declined: per-label `lru_cache` arm. Unbounded validation
  caching is a harder upstream sell, helps only repeated labels, and
  cannot move the IDN-heavy decision track (diverse labels). Recorded so
  a later lane need not re-run the experiment without new evidence.

## Disposition: PORT

Port boundary (whole-address crossing, P1): UTS-46 map/normalize/validate/
encode kernel to core, **one crossing per address**; public API
(`encode/decode/alabel/ulabel/check_label/...`, `codec.py`) preserved via
shims; failures surface **byte-exact `IDNAError` type/code/message**
raised from Python (P12 — same trap class as email-validator, but here
the messages are this package's own, so the port owns them outright).
Importable pure-Python fallback required.

Pattern consult (PATTERN_REVIEW.md checklist, RUST_PATTERNS.md @ `bd5227f`):
P2 (per-char classify → tight loop over u32 tables), P6 (UTS-46/IDNA
tables compiled in once), P1 (whole-address crossing), P7 (tier-1 ASCII
guard shape reused as the port's delegation guard), P12 (errors in
Python). Losers to avoid: markdown-it 0.20x (no per-label crossings),
charset 0.33x (no fighting C fast paths — kept as guards), mako (no user
code on path — none here). Bench plan: representative + showcase + X1,
wall + peak RSS both arms.

Risks: (1) Unicode-version skew — idna 3.20 tables are Unicode 18.0 while
CPython 3.12 `unicodedata` (bidi/category/NFC) is 15.1; the port vendors
tables generated from idna 3.20's own data files (zero skew by
construction) and differential-tests the `unicodedata`-derived pieces
over TR46 vectors. (2) Realistic yield is a fraction of the 3.08x ceiling
(fastjsonschema realized 64%: 3.84x → 2.44x); ~2x expected — worth it at
1339.5M dl/mo. (3) `uts46data`+`idnadata` are large; tables embed as Rust
statics via codegen script (vendored output, documented regeneration).

Port repo: `John-Cusack/idna-rust` (created public, BSD-3-Clause + NOTICE).
No registry publishes (`blocked: no registry credentials` recorded in PR).
