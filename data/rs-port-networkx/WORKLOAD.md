# WORKLOAD.md — rs-port-networkx frozen workload v1

Frozen 2026-10-29. All benches in this lane run these tracks, same machine,
same venv (`scratch/venv`, networkx 3.7, numpy/scipy present but unused —
PageRank/scipy explicitly out of scope). Deterministic: every graph built
from a fixed seed; checksums verify equivalence across arms.

## Graphs (all undirected `nx.Graph`, stock numeric `weight` attr)

| id | shape | nodes | edges(~) | seed |
|----|-------|-------|----------|------|
| R1 | ring + 2 shortcuts/node + 30% random | 1,000 | ~3.3K | 20261028 |
| R2 | same family | 2,500 | ~8.3K | 20261029 |
| R5 | same family, sparser (no 30% random) | 5,000 | ~15K | 20261030 |
| GRID | 50×50 grid, uniform(1,10) weights | 2,500 | ~4.9K | 20261031 |

Spec range is 1–5K nodes: R1/R2/R5 cover it; GRID covers degree-4 lattice
shape (route-planning vs mesh diversity).

## Tracks

- **rep** (representative): `single_source_dijkstra_path_length` full
  expansion from 6 spread sources per graph (24 expansions). This is the
  rerank-3 probe path (`_dijkstra_multisource` top1 0.48).
- **path**: `dijkstra_path` + `dijkstra_path_length` for 4 pairs per graph
  (16 pairs; path reconstruction + pred overhead).
- **multi**: `multi_source_dijkstra` from 8 sources per graph (4 runs).
- **target**: source→target early-exit `dijkstra_path_length` 4 pairs per
  graph (16 runs; tests the `v == target: break` shape).
- **x1** (overhead floor): 1 pair on R1 — bounds per-call export cost.

## Checksums

Per track: `(round(sum_of_distances, 6), number_of_nodes_reached,
path_length_sum)`. Arms must agree exactly (float sums bitwise-identical:
same addition order is guaranteed only within the same algorithm shape —
tier-1 preserves iteration order, so exact equality is required; the Rust
kernel, if built, must match distances exactly and paths node-for-node).

## Success bar (lane kill criteria)

<2x on multi-source Dijkstra (rep+multi tracks) over these graphs vs the
tier-1 baseline, or callable-weight prevalence forcing the fallback on the
representative path → `no_gain`.
