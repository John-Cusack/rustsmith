# Pattern review: markdown → markdown-rust (John-Cusack/markdown-rust @ d856b21)

Port: whole-document Rust core (`markdown-core`), PyO3 binding, vendored
upstream 3.11.2 + `_bridge.py` seam. Compliance target: Python-Markdown's
own behavior (not CommonMark).

## Measured results (workload: frozen WORKLOAD.md, probevenv, no Pygments)

| workload | pure wall | rust wall | speedup | bytes equal |
|---|---|---|---|---|
| small-readme (~4KB, extra) | 0.2ms | 0.3ms | 0.67x (noise floor) | yes |
| docs-page (~34KB, full extension set) | 35.3ms | 16.3ms | **2.17x** | yes |
| large-doc (~270KB bulk, extra+toc) | 152.8ms | 107.2ms | **1.43x** | yes |

Peak RSS: identical both arms (696492 KB, environment baseline;
per-render delta 0 KB both arms — no allocation regression).
Parity: upstream suite 809 passed + 64 skipped (pygments-env-gated) +
210 md_in_html via unittest; equivalence corpus 400/400 byte-identical;
`cargo clippy -- -D warnings` clean.

## Findings (mechanism + classification)

1. **P1 confirmed (whole document crosses once).** One `render()`
   call per `convert` (string in, dict out). The alternative —
   per-pattern or per-block callbacks — would re-cross per node and
   lose (cf. markdown-it 0.20x bulk receipt). No per-item crossing
   anywhere; third-party extensions decline to pure Python.
   Classification: Confirm.
2. **P6 confirmed + extended (compile once).** Initial port compiled
   ~600 `fancy_regex` patterns per render (11x slower than pure on
   docs-page). Fix: `consts::compile` caches by pattern string
   (covers dynamic delimiter boundaries too). Lesson for the guide:
   the cache must cover *dynamically built* patterns, not just dumped
   consts — the hot ones were built per call. Classification: Extend.
3. **NEW: trigger-byte prefilter before regex dispatch.** Each inline
   and block pattern declares required literal bytes (e.g. smarty
   dashes need `-`, links need `[`); a per-text byte-mask skips patterns
   whose triggers are absent, and skips `handle_inline` entirely when
   the mask is disjoint from the enabled union. Measured: docs-page
   70.6ms → 26.4ms (single change); block prefilter saved another
   ~5ms. fancy-regex failing searches cost ~10µs vs C `re` ~1µs, so
   avoiding searches beats accelerating them. Soundness: triggers are
   necessary literals of each pattern (verified against dumped sources);
   admonition continuations key off parent state, not just `!`.
   Classification: New (propose as P-refilter alongside P8).
4. **NEW: regex-dialect trap (`\<`).** Python `re'\<'` is literal `<`;
   fancy-regex reads it as a word boundary. The hand-written smarty
   angled patterns compiled to zero-width matches → unbounded
   placeholder growth (hang). Fixed by spelling literal `<<`; dumped
   patterns grepped clean. Propose guide rule: dump patterns from live
   compiled instances with machine diffing; never hand-transcribe;
   grep for dialect divergences. Classification: New.
5. **Delimiter stack: faithful port required.** Three independent `if`s
   (branch1/branch2/open-push) must not nest; `is_start` clears on
   branch 2; token-local (not global) reset; `get_state`-style
   boundaries verified by unit tests. A single structural deviation
   (nesting the push inside branch 1) broke 8 nesting tests with no
   performance signal — correctness-only, caught by the suite.
   Classification: Confirm (P-fidelity: mirror control flow exactly).
6. **Losers / non-applicable:** P8 (hand-matchers) not attempted —
   prefilter recovered the win without rewriting matchers; pygments
   path declined (no Rust equivalent possible, ADR-056); md_in_html
   ported (not declined) because `extra` bundles it and the frozen
   workload uses `extra`.

## Proposed guide updates

- Extend P6: "compile once" explicitly includes dynamically built
  patterns (delimiter boundaries, per-link regexes) — cache by string.
- New pattern (propose P-refilter): per-pattern trigger-byte masks +
  whole-text early-out before regex dispatch; mandatory soundness
  argument per pattern (necessary literal); mind cross-call state
  (`cur_mask` save/restore across recursive `handle_inline`).
- New pitfall: regex-dialect mismatches between dump source and
  compile engine; machine-dump + diff harness required.
- Checklist addition for ports with vendored upstream: run the
  upstream suite *through* the fast path (not just fallback) and the
  equivalence corpus before any bench; a green suite via fallback
  proves nothing about the port.

## Checklist answers

- Evidence integrity: bench figures above are medians-of-10 from
  `bench_lane.py` runs in this lane (evidence file
  `scratch/bench_evidence.json` with sha256 checksums both arms);
  bundle `numbers.md` does not yet contain them (gap noted).
  RSS both arms measured, identical baseline-dominated; no alloc
  figures cited as wall.
- Mechanism: per-item mechanisms named above with file pointers
  (`markdown-core/src/{inline,block,consts,ext/*}.rs`,
  `markdown/_bridge.py`).
- Crossings: exactly one per `convert` (whole document in, response
  dict out); PyO3 conversion is scalars + flat spec lists (no object
  graph; no both-trees-alive risk).
- Algorithm class: same algorithms, faster (prefilter skips work;
  no algorithmic change) — except compile caching (same regexes).
- No oracle/gate/threshold touched: suite, corpus, and kill
  thresholds unchanged; the 2x bar was cleared (2.17x) without
  re-freezing or weakening tests.
