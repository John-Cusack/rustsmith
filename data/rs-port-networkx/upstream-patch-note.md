# Upstream patch note — networkx Dijkstra (no PR opened)

Date: 2026-10-29. Upstream: networkx 3.7
(`networkx/algorithms/shortest_paths/weighted.py`,
`_dijkstra_multisource`). Status: note only, no upstream PR.

## What was tried (pure Python, frozen WORKLOAD.md v1: 1–5K-node
ring/shortcut graphs + 50×50 grid, stock numeric weights; wall medians
of fresh-subprocess runs, checksums exact in all arms)

1. **Per-call adjacency export with float conversion** (materialize
   `{v: [(u, float_w)]}` per query, then a local-bound loop):
   **0.4–0.6x** — slower than stock on every track. The O(E) export
   (tuple/list/float churn over every edge) costs more than the
   `dict.get` + weight-lambda per relaxation it removes.
2. **Per-call export without conversion** (keep raw attr values):
   **0.4–0.9x** — still a loser on every track.
3. **Cached export** (export once per graph, reuse across queries):
   **1.39–1.77x**. Real, but not upstreamable as a transparent patch:
   it needs a caching handle on a mutable graph (staleness guard),
   i.e. an API change, not a faster loop.
4. **heappushpop fusion**: not applicable by inspection — heap pushes
   and pops are separated by settled-check and relaxation logic across
   loop iterations; fusing would change semantics.

## Recommendation to upstream

There is no transparent pure-Python win here; the per-relaxation
`data.get(weight, 1)` + `_heapq` loop is already near the floor for
dict-of-dict graphs, and any per-call precomputation costs more than
it saves. Do not merge an export-cache without a mutation-safe
invalidation story. (The Rust port that did clear the bar,
networkx-rust, walks the dict once inside the kernel per call:
2.09–3.60x. That shape is not expressible in pure Python.)
