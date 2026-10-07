# optimize guidance v2 (seed + perf-P0)

Short, concrete rules included in the worker prompt's stable cached prefix.
Evolved ONLY via `rustsmith learn` + human approval (P2). Version pinned per
attempt row; mid-run mutation prohibited.
v2 adds: FFI per-call budget + boundary-selection (C12/D), tier-3/5 layout
moves (E22), vectorizer checklist + stable-SIMD rule (E20/E21).

- Tier order first: do-less-work (1) > complexity (2) > representation (3) >
  allocation (4) > memory access (5) > syscalls (6) > parallelism (7) >
  micro/SIMD (8) > build-level (9). Top-down; justify the tier.
  External check (pythonspeed FFI study): redefining the problem (tier 1)
  beat porting — an O(1) closed form in pure Python landed within 1.5x of
  the Rust port. Exhaust do-less-work before porting the loop.
- Bound constrains technique: compute->algorithm/SIMD/branchless;
  bandwidth/latency->layout/SoA/streaming; allocation->arenas/borrow;
  syscall_io->buffering/batching. Bound-mismatched proposals are rejected
  before costing a graded run.
- Layout by access pattern (tiers 3/5): iterate one field -> SoA +
  streaming; touch most fields together -> AoS; mixed -> chunked AoSoA
  sized to L1. Bandwidth bounds stream; latency bounds chunk/prefetch.
  Treat 10-100x layout multiples as uncalibrated; the MemoryBandwidth
  cap stays 2.0 until in-house Cachegrind calibration exists.
- Vectorizer-friendly loops first (tier 8): contiguous slices; no aliasing
  ambiguity (split borrows); no switches/calls in inner loops; float
  reductions need explicit fast-math acknowledgment (accuracy gate
  interaction — flag, never sneak). Remarks evidence before `std::arch`.
- Stable-SIMD rule: `portable_simd` is nightly-only — stable code MUST NOT
  assume it. SIMD order: vectorizer-friendly loops -> remarks evidence ->
  `std::arch` + runtime dispatch only with measured uplift, baseline wheels
  stay portable. `simd_eligible` stays conservative until remarks exist.
- FFI per-call budget: per-call Rust work < ~1us is boundary-dominated
  (tens of ns + conversion per crossing) -> batch, hoist the loop across
  the boundary, or don't port the unit. Time budget on top of the
  `caller_side_check` 10x count rule.
- Boundary selection: count crossings BEFORE choosing thin-hot-loop vs
  whole-component port. Thin loses when crossings dominate -> port the
  loop's caller or the whole component. Keep bulk data extension-side;
  bytes/str cross once and are consumed in Rust exactly once; no Python
  callbacks in Rust hot loops (GIL + conversion per item = bound
  mismatch, rejected pre-run).
- Dead ends (seed, calibrate via retrospective): SmallVec past 256B on
  bandwidth-bound loops; rayon before serial is tight; LUTs wider than L1.
- Cost heuristic: ceiling/estimated_cost ranks; files-touched and cross-module
  edges dominate cost. Zero survivors is a stop, not a failure.
- Forbidden: shape-dependent behavior change; visible-keyed caches; relaxed
  tolerances; unsafe outside FFI; nightly-only features (incl.
  portable_simd) without explicit exception; deleting unexercised paths.
