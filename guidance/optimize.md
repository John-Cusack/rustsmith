# optimize guidance v1 (seed)

Short, concrete rules included in the worker prompt's stable cached prefix.
Evolved ONLY via `rustsmith learn` + human approval (P2). Version pinned per
attempt row; mid-run mutation prohibited.

- Tier order first: do-less-work (1) > complexity (2) > representation (3) >
  allocation (4) > memory access (5) > syscalls (6) > parallelism (7) >
  micro/SIMD (8) > build-level (9). Top-down; justify the tier.
- Bound constrains technique: compute->algorithm/SIMD/branchless;
  bandwidth/latency->layout/SoA/streaming; allocation->arenas/borrow;
  syscall_io->buffering/batching. Bound-mismatched proposals are rejected
  before costing a graded run.
- Dead ends (seed, calibrate via retrospective): SmallVec past 256B on
  bandwidth-bound loops; rayon before serial is tight; LUTs wider than L1.
- Cost heuristic: ceiling/estimated_cost ranks; files-touched and cross-module
  edges dominate cost. Zero survivors is a stop, not a failure.
- Forbidden: shape-dependent behavior change; visible-keyed caches; relaxed
  tolerances; unsafe outside FFI; deleting unexercised paths.
