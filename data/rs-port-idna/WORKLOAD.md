# WORKLOAD.md — rs-port-idna frozen workload (2026-10-29)

All benches run `idna.encode(d, uts46=True)` + `idna.decode(d, uts46=True)`
per domain in a plain Python loop (the rerank-4 probe shape: per-label
UTS-46 remap + bidi/checks per address on signup/validation paths).
Corpus is deterministic (`seed=20261029`); sizes count domains per
`run()` call. Machine: Ryzen 9 5950X, CPython 3.12.3, idna 3.20.

## Tracks

| Track | Name | N | Mix |
|---|---|---|---|
| representative | `mixed-8000` | 8000 | 70% valid ASCII (`mail{i}.example{j}.com`), 20% valid IDN (`münchen`-style U-labels, `xn--` punycode-in, `日本語.jp`), 10% invalid (no-dot, `..`, leading/trailing hyphen, 64ch label, underscore) |
| showcase | `ascii-16000` | 16000 | 100% valid ASCII — tier-1 fast-path showcase |
| showcase | `idn-4000` | 4000 | 100% IDN/punycode — kernel ceiling probe (punycode + UTS-46 tables) |
| overhead | `x1-valid`, `x1-idn`, `x1-invalid` | 1 | single-domain encode+decode crossing floor |

## What is timed

`encode` + `decode` (each `uts46=True`) per domain. Wall =
`time.perf_counter` around the loop (min-of-7, fresh subprocess per
arm/track); peak RSS = child-process `ru_maxrss` both arms. Attribution
arm: `cProfile` over one `run()`, frames bucketed into `idna`-own
(`core`+`intranges`), stdlib-punycode (`encodings/punycode.py`, pure
Python — portable by reimplementation in-core), C (`bisect`,
`unicodedata`, `re`, codecs).

## Kill criteria (lane spec §3 Lane 1)

- Tier-1 gain recorded separately; ceiling recomputed vs tier-1 baseline.
- Kill at ceiling <2x over tier-1 on `mixed-8000`, or label-cache +
  ASCII-fast-path removes the majority (remainder is C-`bisect` + table
  lookups) → honest no_gain.
