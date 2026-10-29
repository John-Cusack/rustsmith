# Pattern review — networkx-rust (post-lane)

Date: 2026-10-29. Port: https://github.com/John-Cusack/networkx-rust @
`6f0e4e0`. Evidence: `data/rs-port-networkx/` (WORKLOAD.md, tier-1
benches, prototype ceiling, bench_evidence.json, equiv jsonl 28/28,
verification.json), ADR-066, port README bench table.

## Review checklist

**Evidence integrity**

- [x] Bundle manifest: port commit `6f0e4e0`, upstream networkx 3.7
  (venv sdist), excerpts from those commits, machine Ryzen 9 5950X.
- [x] Figures workload-labeled + track: rep/path/multi/target/x1,
  representative (rep/multi) + overhead (x1) rows present.
- [x] Peak RSS both arms; RSS deltas reported, never cited as wall.
- [x] No missing evidence: bench rows, RSS, X1 all present.

**Mechanism, not vibes**

- [x] Per-item mechanism: per-relaxation `dict.get` + weight-lambda +
  `_heapq` push/pop + `reportviews` iteration → one CSR walk (borrowed
  refs, no incref) + binary heap over flat `f64`/`usize` + fresh dicts.
- [x] Crossings counted: one in (`G._adj`), one out (dist/pred dicts).
  No per-item callbacks (weight callables route to the slow path).
- [x] PyO3 shape: borrowed-dict walk → owned flat buffers → fresh
  owned dicts; CSR freed per call; no both-trees-alive (RSS +1–2 MiB =
  extension pages only).
- [x] Algorithm class: same Dijkstra, same order (push counter mirrors
  `itertools.count`); no algorithm substitution.

**Guide bookkeeping**

- [x] Each finding classified below.
- [x] Losers recorded with numbers (T1/T3).
- [x] Confidence: no threshold crossings (P1/P2/P7 already High).
- [x] Index + Appendix A drafted; Top 10 unchanged (representative
  3.60x confirms P1 without promoting it).
- [x] No oracle/gate/threshold touched (upstream suite run verbatim
  through dispatch; 654 + backstop 8412/63/741).

## Findings

1. **Confirm P1** — whole-graph crossing once per call; rep 3.60x,
   multi 2.77x, target 2.09x, path 2.24x, x1 2.29x. Adds graph-compute
   (dict-graph + heap) as a new confirmed family for P1 (prior
   confirmations were parsers/codecs/validators/renderers).
2. **Confirm P2** — heap/dist arrays over primitives; per-item boxing
   (`dict.get`, weight lambda, tuple entries) eliminated on the fast
   path.
3. **Confirm P3 (light)** — weight-key lookup hoisted: key object fixed
   per call, attr fetched once per edge in the CSR walk; loop body is
   branch-free arithmetic + heap ops. No separate P3 figure (folds
   into the kernel win).
4. **Confirm P7** — guard (`not multigraph and not callable(weight)`)
   + kernel refusal (non-numeric → TypeError → captured-stock replay).
   Fallback hit-rate: upstream weighted suite passes THROUGH dispatch
   (654 green), incl. multigraph/callable/error tests that all take the
   slow path — parity by construction, measured.
5. **Extend P6** — new loser line: per-call Python-side export cannot
   amortize (T1 0.4–0.6x, T3 0.4–0.9x — the export costs more than the
   loop it accelerates); cross-call caching (T2 1.39–1.77x) needs a
   caching-handle API on a mutable graph, i.e. an API change, not a
   drop-in. Ceiling note: a cached handle would beat per-call crossing
   in *either* language — the choice is scope, not speed.
6. **New note (Single): tie-break fidelity** — Dijkstra's
   tie-breaking is observable (paths, multi-preds), so the Rust heap
   must replicate upstream's `(dist, count)` ordering, not `(dist,
   node)`. A `(dist, node)` prototype passed all distance checks and
   would have shipped wrong paths on zero-weight ties; caught by the
   equiv corpus shape (order-sensitive dist lists + tie graphs).
   Measured: 28/28 equiv rows incl. zero-weight tie-breaking graphs;
   zero-weight upstream-suite tests green through dispatch.
7. **A\* negative boundary** — heuristic is a per-node user callback;
   porting it means per-item crossings (P1 violation shape). Stays
   Python by design; recorded so the next graph lane doesn't re-scope
   it without a batched-heuristic design.

## Proposed guide updates (for the guide PR; port PR carries this file only)

A. Per-port index row:

```
| networkx | P1, P2, P7 | 3.60x rep (multi 2.77x) | T1/T3 per-call export 0.4–0.9x losers; A* stays Python (callback seam) | networkx-rust `6f0e4e0` README; `data/rs-port-networkx/bench_evidence.json`, ADR-066 |
```

B. Appendix A paragraph:

```
- **networkx** — Graph-compute novelty slice (P1/P2 over a dict-graph
  + heap, not a parser): borrowed-API CSR walk once per call, binary
  heap over flat arrays, fresh result dicts. 2.09–3.60x across
  rep/path/multi/target/x1 with RSS +1–2 MiB. Exactness bought the
  hard way: push-counter fidelity for tie-break order, verbatim
  preambles, captured-stock fallbacks (upstream weighted suite runs
  THROUGH dispatch). Transparent tier-1 attempts all lost (per-call
  export 0.4–0.9x); only a caching-handle API wins in pure Python —
  out of drop-in scope in both languages.
```

C. P6 "Does NOT help" append:

```
  Per-call export of a Python object graph for a kernel that consumes
  it once (networkx T1 0.4–0.6x, T3 0.4–0.9x): the O(E) export costs
  more than the per-item overhead it removes. Either cache across
  calls (needs a handle API — networkx T2 1.39–1.77x) or walk the
  objects once inside the kernel (borrowed refs, this port).
```

D. Anti-pattern ledger row:

```
| Exporting the graph per call to feed one kernel run | networkx T1 0.4–0.6x, T3 0.4–0.9x | Cache across calls (handle API) or walk inside the kernel |
```

E. P2 entry, tie-break fidelity note (append to "Does NOT help" /
   mechanism paragraph):

```
  Ordering fidelity: where the algorithm's tie-breaking is observable
  (Dijkstra paths/multi-preds), the heap ordering must replicate the
  original exactly — networkx needed `(dist, push_count)`, not
  `(dist, node)`. A `(dist, node)` variant passed every distance check
  and failed path identity on zero-weight ties.
```

F. No Top-10 movement (P1's 17–52x representative best stands; this
   port confirms without promoting). No retirements.
