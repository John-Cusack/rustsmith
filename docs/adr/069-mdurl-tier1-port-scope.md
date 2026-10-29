# ADR-069: mdurl tier-1 no-rewrite, port scope (lane rs-port-mdurl)

Date: 2026-10-29. Lane: rs-port-mdurl (rerank-4 lane 2: 427M dl/mo,
headroom 0.73). Verdict: **tier-1 no_gain (1.03x), port proceeds —
3.12x representative / 3.69x x1 over stock, 2.93x over the tier-1 baseline.**

## 0. Pre-lane check (§1.3) on the frozen workload

Corpus: 2000 mixed URLs x 300 reps = 600,000 parse+format ops, ~2.8 s on
Ryzen 9 5950X / CPython 3.12.3 (`data/rs-port-mdurl/WORKLOAD.md`).

- **T0 native audit:** 6 shipped `.py`, no `.so`; profiled frames are `.py`
  + C `str.find` + C `re`. Pure.
- **T1 rebuild census:** every hot regex is already module-precompiled;
  the single inline pattern (`re.search(r"^//[^@/]+@[^@/]+", rest)`)
  hits CPython's `re` cache. K1 ~= 0.
- **T2 model census:** one `MutableURL` + one `URL` NamedTuple per parse
  (~0.10 share) — the unportable floor behind any faithful seam.
- **T3 user code:** none (plain strings in/out).
- **T4 engine census:** native `str.find` 19% + C `re` 8.5% = 27.5%;
  per-URL work ~4.8 us >> ~0.5 us crossing toll.
- **T5 kill arithmetic:** portable ~= 0.73 - 0.10 = 0.63;
  ceiling_est = 1/(1-0.63) ~= **2.7x**.

## 1. Tier-1 measured: 1.03x, nothing shippable

| arm | delta | verdict |
|---|---|---|
| A `"#" not in url` for `len(url.split("#"))==1` | 1.00x | no-op |
| B precompiled `AUTH_HOST_PATTERN` | 1.00x | `re` cache already absorbs it |
| C `min(...)`-generator find fusion | **0.82x** | loser: generator frames cost more than the explicit loop |
| D `format` `+=` chain -> `"".join` | ~0.97x | noise (format is 3% of profile) |
| A+D combined (`data/rs-port-mdurl/tier1.patch`) | **1.03x** | real but trivial; not offered |

Upstream suite 189/189 on baseline and A+D; checksum 85300 identical.
`data/rs-port-mdurl/upstream-patch-note.md` records the arms as
measured losers + micro-win (networkx class: nothing to offer).
The lane spec's named tier-1 thesis (hoist per-parse regex) did not
reproduce — priced cheap, as designed.

## 2. Kill criteria (lane spec, applied exactly)

- Ceiling < 2x over tier-1? **No:** 2.7x estimated, 2.93x measured
  (tier-1 median 2.712 s / port median 0.925 s).
- C-`find` + C-`re` dominates the improved profile? **No:** 27.5%,
  and the Rust byte-scan replaces them as part of the win.
- **Neither fires. Proceed to port.**

## 3. Port boundary (whole-URL crossing)

Parse+format kernel to core (`mdurl-core`, no PyO3), one crossing per URL:
`parse(str) -> 8-tuple`, `format(8 fields) -> str`. The Python wrapper
rebuilds the upstream `URL` NamedTuple, so the return type is identical;
`decode`/`encode` re-exported from upstream `mdurl`; importable
pure-Python fallback. The markdown-it call path stays Python.
No user callbacks exist on the hot path (T3 clean), so no opaque-node
machinery is needed.

Error behavior: mdurl raises no parse exceptions (total function, `None`
fields) — byte-exactness = field equality + round-trip equality, checked
over 4176 corpus pairs (fixtures + workload, both `slashes_denote_host`
settings) + 20,000 adversarial fuzz inputs, 0 mismatches.

## 4. Measured result (release wheel, fresh-subprocess `/usr/bin/time -v`)

| track | stock | port | speedup | RSS |
|---|---|---|---|---|
| representative 600k URLs | 2.886 s | 0.925 s | **3.12x** | 11944 -> 12524 KiB |
| x1 single parse (200k) | 4.877 us | 1.321 us | **3.69x** | 11636 -> 12416 KiB |

Port suite: 190/190 (189 upstream-adapted + workload-corpus gate).
`cargo test --workspace` green, `cargo clippy --workspace --all-targets`
clean. Publish: **blocked: no registry credentials** (no
TWINE_*/CARGO_REGISTRY_TOKEN in environment); release-prep (wheel builds,
`sdist` not yet) — install pending like the sibling lanes.

## 5. Pattern receipts (for the PATTERN_REVIEW pass)

P1 (whole-URL crossing), P2 (byte loop, no per-item dispatch), P4
(borrowed `&str` scan; `String` only for fields), P8 (4 regexes +
find-loops hand-rolled into one pass). Transcription traps fixed by the
fuzz (see review): `//?` mandates the slash; empty-proto truthiness in the
slash-strip guard; empty-auth falsy in `format`; Python `str` whitespace
(U+001C-001F) vs Rust `White_Space`. Tier-1 arm C extends P8's "does NOT
help" (generator fusion 0.82x).
