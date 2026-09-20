# ADR 003: Deterministic instrument without Cachegrind/perf
- Context: SPEC_STAGE2 §3.1 wants Cachegrind primary. Environment lacks it:
  no valgrind binary, no sudo to install; `perf` blocked (perf_event_paranoid=4,
  no CAP_PERFMON). No new tool can be installed.
- Decision: primary = process-CPU-time median over repetitions (per-process CPU
  clock; immune to contention, sensitive to frequency scaling like all clocks).
  Wall-clock CI confirms merged rounds (§3.2 unchanged). Noise floor measured
  self-vs-self (§3.3 unchanged). Bound classification uses proxy signals
  (CPU/wall ratio, tracemalloc, size-scaling, RSS) with the same 9-bound schema.
- Consequences: sub-1% resolution not claimed; floor is measured, so small
  deltas honestly fail screening. Slice-by-N gains (2-4x) resolve clearly.
- Alternatives: wall-only (rejected: contention-noisy, defeats §3 purpose).
- Spec: §3 structure (screen-then-confirm, disagreement parks) preserved.
