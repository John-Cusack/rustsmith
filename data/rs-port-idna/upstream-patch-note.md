# Upstream patch note — idna pure-Python speedups (not sent upstream)

Suggested recipient: kjd/idna. No upstream PR opened (lane policy).
Patch: `data/rs-port-idna/tier1.patch` against 3.20. All changes preserve
exact error types, codes, messages and return values (verified: upstream
suite 6441 passed + 56 subtests both arms; 900-row differential incl.
uppercase, `xn--` edge labels, hyphen positions, non-ASCII, all flag
combos — 0 mismatches).

Measured (Ryzen 9 5950X, CPython 3.12.3, `bench_tier1.py`, min-of-7 × 3
sessions, per-cell best-of-21):

| Track | Stock | Patched | Speedup |
|---|---|---|---|
| mixed-8000 (representative) | 0.2228 s | 0.0856 s | 2.60x |
| ascii-16000 | 0.4264 s | 0.0567 s | 7.52x |
| idn-4000 | 0.1398 s | 0.1248 s | 1.12x |

What the patch does (`idna/core.py` only):

1. **Lowercase-ASCII-valid label fast path** (`check_label`, `alabel`,
   `ulabel`) via one module-level compiled regex
   (`(?!..--)[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\Z`). A match implies
   every check below passes: NFC-stable (ASCII invariant), no leading
   combiner (ASCII never category-M), all-PVALID (verified exhaustively;
   note uppercase A–Z is *not* PVALID, hence the lowercase-only class),
   hyphen rules (alnum ends + no `--` at 3–4, which also keeps every
   `xn--` A-label on the slow path so non-canonical forms still raise),
   and bidi (no RTL characters). Anything else falls through to the
   unchanged slow path with identical errors.
2. **Hoist the `uts46data` import to module top** in `uts46_remap`
   (no import cycle: that module imports only stdlib `array`). Isolated
   effect (T-import-only arm): 1.01x mixed / 1.07x idn — i.e. the T-ascii
   fast path carries essentially the whole win. Suggested upstream shape:
   (1) could be narrowed to `check_label` only or rewritten as an explicit
   length + bytes-table check if the maintainers prefer to avoid a regex;
   (2) is an uncontroversial mechanical hoist.
