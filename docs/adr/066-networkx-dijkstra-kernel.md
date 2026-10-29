# ADR-066: networkx lane — tier-1 measured, drop-in Dijkstra kernel ported (rewrite proceeds)

Date: 2026-10-29. Lane: rs-port-networkx-01 (networkx 3.7, 199M dl/mo).
Port: https://github.com/John-Cusack/networkx-rust @ `81fce79`
(graded tree `6f0e4e0`; the delta is a README-only fix, code-identical).

## Context

Rerank-3 §3 Lane 2 (novelty slice: first graph-compute family). Headroom
1.00 rewrite, pure 99.9%, tight_loop, top1 0.48
(`_dijkstra_multisource` over the dict-of-dict graph, 6 hot modules).
PageRank out of scope (rides scipy). Kill criteria: <2x on
multi-source Dijkstra over 1–5K-node graphs, or callable-weight
prevalence forcing the fallback on the representative path.

Guide consult (rustsmith `f8c81e4` + RUST_PATTERNS.md): expected P1
(whole hot unit crosses once — adjacency exported once per call), P2
(one tight loop over primitives — heap + dist arrays), P6
(export/resolve once — currently once per call; cached-handle variant
noted below but out of scope), P7 (narrow fast path + delegation guard —
stock numeric weights take core, callables/multigraph take the
pure-Python path by construction). Losers to avoid: markdown-it 0.20x
(no per-token crossings), pygments 1.00x (don't port where a C engine
dominates), arrow 1.99x-tier-1/1.78x-ceiling (measure tier-1 first).

## Tier-1 (pure Python, frozen WORKLOAD.md v1, wall + child-ru_maxrss both arms)

Arms on [`bench_tier1.py`](https://github.com/John-Cusack/networkx-rust/tree/main/bench) in networkx-rust (relocated out of `data/rs-port-networkx/`; medians, fresh subprocesses):

- T1 per-call adjacency export (float-convert every edge per query):
  **0.4–0.6x** — measured loser. Exporting O(E) Python objects per call
  costs more than the `dict.get` + weight-lambda it removes.
- T3 per-call export without float conversion: **0.4–0.9x** — loser.
  Confirms no transparent per-call export shape wins.
- T2 cached export (export once, reuse across queries): **1.39–1.77x**.
  Real but NOT drop-in: needs a caching-handle API on a mutable graph
  (staleness), i.e. an API change for both arms equally.
- heappushpop fusion: N/A by inspection — pushes and pops are separated
  by settled-check/relaxation logic across iterations; fusing would
  change semantics. Neighbor-list materialization is the T1/T3 shape
  (measured above).

No transparent pure-Python fix beats stock: the shippable baseline
stays stock networkx. (Upstream patch note records the losers so
upstream need not re-run the experiment; no PR opened.)

## Ceiling probe → rewrite decision

A throwaway PyO3 prototype (borrowed-API dict walk → CSR → binary heap,
distances only) measured end-to-end per call, medians of 7, checksums
exact: **2.41–3.35x vs stock** on all four workload graphs
(R1/R2/R5/GRID), 1.23–1.57x vs the non-shippable T2 baseline on ring
graphs. Callable-weight prevalence: upstream `test_weighted.py` uses
string/constant weights almost exclusively (2 lambda lines in the
file), so the fallback rarely triggers on the representative path —
kill criterion 2 does not fire. Kill criterion 1 (<2x vs the shippable
baseline) does not fire. Rewrite proceeds with the lane's drop-in
design; a cached-handle API (which would beat per-call crossing in
either language) is out of scope ("never mirror the package").

## Port boundary (seam shape i, whole-graph)

- `crates/nx-dijkstra-core`: `dijkstra(adj, sources, weight_key,
  cutoff, target, want_pred)` — one crossing in (`G._adj`), CSR + heap
  over flat arrays, fresh `{label: dist}` (+ optional `{label: [preds]}`)
  out. Push counter breaks ties exactly like upstream's
  `itertools.count`, so settlement order, first-predecessor choice,
  equal-cost multi-preds, cutoff/target, and the negative-weight
  `ValueError("Contradictory paths found:", "negative weights?")`
  tripwire all match `_dijkstra_multisource` arm for arm.
- `python/networkx_rust/weighted.py`: 12 public Dijkstra functions
  mirror upstream preambles verbatim, take the kernel on the fast path,
  delegate to captured-stock functions otherwise. A\* deliberately stays
  Python: the heuristic is a per-node user callback (P1 violation to
  cross per item). Bellman–Ford/Johnson/bidirectional/dense/unweighted/
  generic untouched.
- Errors are identical by construction: `NodeNotFound` (incl. each
  function's message variant), `NetworkXNoPath`, empty-sources
  `ValueError`; kernel refusal (non-numeric weight) falls back and lets
  stock produce the error.

## Gates (all measured, Ryzen 9 5950X, CPython 3.12.3, networkx 3.7)

- New: `tests/test_weighted_dispatch.py` 22/22 (incl. 30-graph fuzz over
  the full surface + error identity).
- Upstream `shortest_paths/tests/` through dispatch: **654 passed**.
- Full upstream suite backstop with dispatch active: **8412 passed,
  63 skipped, 741 xfailed**.
- Equiv corpus: 28/28 rows identical (distances in order, paths, preds).
- `cargo clippy -- -D warnings` clean. Peak RSS +1–2 MiB (extension
  pages; no alloc regression — CSR freed per call).

## Bench (two-arm, wall medians of fresh-subprocess runs + peak RSS)

| track  | base (ms) | port (ms) | speedup | RSS base → port |
|--------|-----------|-----------|---------|-----------------|
| rep    | 220.5     | 61.3      | 3.60x   | 47.1 → 49.5 MiB |
| path   | 97.6      | 43.6      | 2.24x   | 47.8 → 49.0 MiB |
| multi  | 35.9      | 13.0      | 2.77x   | 47.1 → 48.7 MiB |
| target | 77.4      | 37.1      | 2.09x   | 47.2 → 48.4 MiB |
| x1     | 2.9       | 1.3       | 2.29x   | 46.6 → 47.4 MiB |

Evidence (stays in rustsmith): `data/rs-port-networkx/{WORKLOAD.md,bench_evidence.json,
bench_rows.json,equiv_pure.jsonl,equiv_rust.jsonl,verification.json}`; lane tooling (bench scripts, tier-1 note) in [networkx-rust `bench/` + `tier1/`](https://github.com/John-Cusack/networkx-rust/tree/main/bench).

## Consequences

- Pattern-ledger input in `review-networkx-rust.md` (post-lane review):
  P1/P2/P7 confirm; P6 extend (cached-handle ceiling note); T1/T3 losers
  (per-call Python export cannot amortize — export floor ≥ loop it
  replaces); push-counter fidelity as a new portability note (tie-break
  order is observable through paths/preds).
- Lane store `data/rs-port-networkx/store.db` run rs-port-networkx-01:
  unit `python:.../weighted.py` passed, 4 gates (equiv 28/28, bench 5/5
  min 2.09x, bench-checksums 5/5). Note: the `verification` gate row was
  recorded from verification.json v1 (passed, no stage counts); the file
  on disk is v2 (stage shape). The v1 row drops loudly at export by
  design; the other three gates carry the totals.
- Publish: release prep → TestPyPI → prod PyPI + crates.io; registries
  need credentials (see evidence PR body for blocked-or-done state).
