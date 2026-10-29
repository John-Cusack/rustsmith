# Pattern review — mdurl-rust (lane rs-port-mdurl, 2026-10-29)

Guide consulted 2026-10-29 at rustsmith commit `bd5227f`
(`docs/RUST_PATTERNS.md`, `docs/PATTERN_REVIEW.md`).
Port: https://github.com/John-Cusack/mdurl-rust @ `c1fced0`
(upstream executablebooks/mdurl 0.1.2). Method: side-by-side read of
`_parse.py`/`_format.py` vs `mdurl-core/src/lib.rs` + measured rows below.

## Pre-port consult (retroactive — lane brief carried the same content)

- [x] Guide consulted 2026-10-29 at `bd5227f`.
- [x] Expected patterns: P1 (whole-URL crossing — plain strings in/out,
      one call per URL), P8 (hand lexer over 4 regexes + find-loops),
      P4 (borrowed `&str` scan), P2 (byte loop, no per-item dispatch).
- [x] Losers to avoid: markdown-it 0.20x bulk (per-token crossings —
      batched whole URL instead); mako 1.17x (user code per node —
      n/a, T3 clean); C-`find`/C-`re` already fast (K4 — measured 27.5%,
      ported as part of the win, not assumed).
- [x] Headroom row: rerank-4 probe mdurl.json (pure 0.73, tight_loop,
      top1 0.43 `parse`, top5 0.74).
- [x] Kill criteria in guide terms: below 2x over tier-1 on the frozen
      representative workload, or K4 dominating the improved profile
      -> honest no_gain.
- [x] Bench plan: representative (600k URLs) + x1 (200k single parses),
      wall (fresh-subprocess `/usr/bin/time -v`) + peak RSS both arms.
- [x] Post-lane review: this file.

## Evidence integrity

- [x] Bundle = port repo @ `c1fced0` + upstream 0.1.2 sdist; excerpts are
      the committed files, not a working tree.
- [x] Every figure below is a fresh-subprocess median-of-3 with workload
      label + track; cross-box figures: none (single box).
- [x] Peak RSS present both arms, both tracks (flat ~12 MB).
- [x] Missing evidence: none. Box-noise disclosure: this box runs up to 3
      parallel lanes; arm B showed one 3.001 s outlier vs a 2.77 s median
      and arm D a 2.71–2.91 s spread — medians of 3 reported throughout.

## Findings

**Confirm P1** — one `parse(&str)` / one `format_url(&Parsed)` per URL,
8-scalar tuple across (no object graph, no callbacks). 3.12x rep /
3.69x x1; RSS flat. The anti-receipt held in mind throughout: markdown-it
0.20x bulk came from per-token crossings, so the seam was drawn at the
whole URL from the start.

**Confirm P2** — one byte-loop over ASCII delimiters per stage (protocol
scan, host-ending scan, non-host scan, tail splits); no per-item boxing,
property access, or dispatch. Field `String`s are the only per-URL
allocation, one per present field.

**Confirm P4** — kernel borrows `&str` end to end (`py_strip`,
`match_simple_path` return slices; owned `String` only where the
algorithm rebinds `rest` or emits fields). Second independent confirm
after dateutil; stays Medium pending a third port.

**Confirm P8** — 4 module regexes + 2 find-loops + 2 `rfind`s hand-rolled
into one pass. Contributes to the 3.12x; the C-`re` it displaces was
8.5% of the profile, the C-`find`s 19%.

**Extend P8 "does NOT help"** — tier-1 arm C: fusing the two min-find
loops into `min(...)` generator expressions measured **0.82x**
(3.413 s vs 2.781 s baseline). Generator frames cost more than the
explicit `for` + C-`find` loop they replace. The explicit loop is already
the fast shape; do not "clean it up".

**New pattern (proposed, Single): transcribe the host's character
classes exactly, fuzz the edges.** Python `str.strip`/`re \s` treat
U+001C–U+001F as whitespace; Rust `char::is_whitespace` (White_Space)
does not. The port's `py_is_space` exists only because the 20k-input
fuzz caught `\x1c`-padded URLs diverging. Same class as the `//?`
mandatory-slash trap and the empty-proto truthiness inversion in the
slash-strip guard — all three found by differential fuzz, none by
reading. Proposed entry: problem = host stdlib char classes differ at
the edges; technique = re-implement the host's class (not the obvious
native one) and fuzz controls/unicode/empty; mdurl receipt
(24176/24176 after fix, 1827 mismatches before).

**No-update verdicts:** P6 n/a (nothing to compile once — tables are
`const` by construction); P3 n/a in Rust (single pass, no invariant
branch); P7 n/a (no fast path needed — whole input is the fast path);
P10 n/a (per-URL work 4.8 us >> toll, no batching needed); Top 10
unchanged (3.12x representative displaces nothing).

## Mechanism notes (crossings / conversions / algorithm)

- Crossings: 1 in + 1 out per URL (P1 by construction). X1 measures
  1.32 us vs 4.88 us stock — per-item work stays >> toll.
- PyO3 conversion: 8 scalars (7x `Option<String>` + bool); both-trees
  alive never happens (tuple in, tuple out; RSS flat).
- Algorithm class: same algorithm, faster substrate + no per-item
  interpreter dispatch (not a different algorithm — no sympy-GCD
  subtlety applies).

## Proposed guide diff

1. Per-port index + Appendix A: mdurl row (P1/P2/P4/P8; 3.12x rep,
   3.69x x1; loser = tier-1 arm C 0.82x + B/D no-ops; source = lane
   `data/rs-port-mdurl/bench.json` + port README).
2. P4 evidence: add mdurl (stays Medium).
3. P8 "does NOT help": add arm-C 0.82x receipt.
4. New Single pattern (character-class transcription) with mdurl receipt.
5. No Top-10 movement.
