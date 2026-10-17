# ADR-048: sympy headroom split verdict — port polys domains only

Date: 2026-10-17. Status: accepted.

## Context

Rerank §3 #3 nominates sympy (129.6M dl/mo, 497K LOC, 0 native) with a
kernel-scoped thesis: probe polys/simplify first, never an 857-file
mirror bid. `data/headroom/sympy.json` now grades both kernels (sympy
1.14.0, cProfile, AMD Ryzen 9 5950X):

- polys-domains: score 0.85 → rewrite. 99.8% pure-Python, 4 hot modules;
  top frames are `dup_mul` + `pythonmpq` QQ ops — one clean seam.
- simplify-core: score 0.49 → upstream_only. 98.9% pure-Python but
  dispatch across 9 core modules (cache/sympify/count_ops) — seam-hostile.

## Decision

Port the proven kernel only: dense polys arithmetic over QQ in
`John-Cusack/sympy-rust`, starting with `dup_*`/`dmp_mul` + QQ ops.
Simplify stays out of mirror scope (upstream-only: a speed core
contributed upstream, not a port); expand only with measured wins.

## Consequences

No 497K-LOC mirror; no simplify port work. Publish steps stay
human-gated per repo Limits (release-prep locally, no registry pushes).
