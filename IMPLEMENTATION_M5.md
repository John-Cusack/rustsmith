# IMPLEMENTATION M5 — Stage 2 Optimize (Round Loop + Retrospective)

**Spec contract:** `SPEC_STAGE2.md` full (§§1–17). Replaces SPEC §9 Stage 2.
**Goal:** measured end-to-end gains with gaming-proof grading, plus a centralized cross-repo retrospective that compounds into guidance.
**Non-goals:** no new correctness machinery (M0/M4 own that), no harvest classification (M6). `learn` ships as P2 (needs ≥3 runs of history) but its schema ships now.

## 1. Crate surfaces

```
rustsmith-profile/   # ADD: deterministic instrument, bound classification, ceiling math
rustsmith-gates/     # ADD: workload_divergence, causal_attribution, widened no_regression, benchmark restate, optimization_scope
rustsmith-store/     # ADD: §13 deltas + failed_optimizations + rounds + guidance_revisions
guidance/optimize.md # NEW: opt-guidance v1 seed (short: tier order, bound->tier notes, 3-5 known dead ends)
cli: run --stage optimize, learn
```

```rust
// rustsmith-profile
pub fn measure_noise_floor(build: &Path, workloads: &[Workload]) -> Result<f64>; // self-vs-self spread; recorded, reported
pub fn deterministic_measure(build: &Path, workloads: &[Workload]) -> Result<InsnStats>; // cachegrind/callgrind: insns + cache/branch sim
pub fn wallclock_confirm(build: &Path, workloads: &[Workload]) -> Result<ConfInterval>;  // quiesced, pinned, 30 reps, 95% CI
pub fn classify_bounds(profile: &Profile) -> Vec<HotspotBound>;  // 9 bounds: compute/bandwidth/latency/branch/frontend/allocation/syscall/contention/work_volume + caller-side check
pub fn ceiling(time_share: f64, cap: f64) -> f64;                 // share * (1 - 1/cap); caps per §6 (5 measured, 4 empirical)
pub struct Candidate { pub hotspot: String, pub bound: Bound, pub tier: u8, pub ceiling: f64, pub est_cost: f64 }

// rustsmith-gates additions (all pure, deterministic)
pub fn workload_divergence(vis_gain: f64, held_gain: f64) -> GateVerdict; // fail >10pp or held<0 beyond floor; halt >25pp or 2 fails/run; NEVER return magnitudes to worker
pub fn causal_attribution(candidate: &Path, parent: &Path, workloads: &[Workload]) -> GateVerdict; // revert + remeasure: gain must disappear within floor
pub fn no_regression_widened(baseline: &Snapshot, got: &Snapshot) -> GateVerdict; // other vis workloads (floor); held-out pass/fail only; RSS +5%; alloc +10%; binary +10%; compile +20%
pub fn benchmark_restated(det_gain: f64, floor: f64, round_ci: Option<ConfInterval>) -> GateVerdict; // per-candidate deterministic; per-round wall CI excludes zero + sign agrees
pub fn optimization_scope(diff: &Diff) -> GateVerdict; // no oracle/benchmark files; no new input-size/value/identity conditionals; API byte-identical; allowlisted deps only

// retrospective capture (store) — DB canonical, git execution-only:
/// On every grade: git diff parent..child captured NOW, same transaction as gate row.
/// optimizations += model/prompt_version/guidance_version/proposal_text/tokens_spent/parent_sha/patch_text
/// failed_optimizations += same (+patch_text='' only when rejected_at_proposal); cap patch_text 512 KiB.
```

Round loop = SPEC_STAGE2 §9 pseudocode verbatim: noise floor → baseline → serialized Round 0 (representation overhauls, `round0_max_changes=10`, `suggest`-fallback) → per round (profile→classify→ceilings→filter≥1.0%→no-repeat→rank ceiling/cost→take ≤8→proposal-before-code cheap check→parallel implement ≤6 iters vs deterministic only→per-candidate gates→merge winners/record losers→wall-clock confirm→stop rules) → gated PGO → gated BOLT → allocator-as-candidate.

## 2. Build order (P0 soundness → P1 efficiency → P2 learning)

| # | Slice | Done when |
|---|-------|-----------|
| P0-1 | `measure_noise_floor` + `deterministic_measure` + `wallclock_confirm` + benchmark freeze enforcement | self-vs-self spread recorded; sub-1% deterministic deltas resolve; wall CI reported not point-estimated |
| P0-2 | `workload_divergence` + held-out workloads + host-only columns (magnitudes never in worker context — test asserts on redacted failure message) | planted fixture-keyed cache fails; worker log contains no numbers |
| P0-3 | widened `no_regression` + gated PGO/BOLT/allocator + `failed_optimizations` + report negative-results section | planted 8%-for-3×RSS fails; unconditional-PGO path absent from code |
| P1-4 | `classify_bounds` + caller-side `work_volume` check + `ceiling` + dispatch filter/rank (zero-survivors = stop) | bound-mismatched proposal rejected pre-grade; below-1% candidates logged-not-worked |
| P1-5 | `causal_attribution` + `optimization_scope` + proposal-before-code + serialized Round 0 | phantom gain in improving round rejected; size-branch special-case caught structurally |
| P2-6 | `learn` (deterministic aggregations: yield by bound×tier×technique, ceiling calibration, gate-kill histogram, tokens-per-point) + `guidance_revisions` + `guidance/optimize.md` versioning | stats reproduce from `store.db` alone; every attempt row carries pinned `guidance_version` |
| 7 | `tests/m5_acceptance.sh` | green (section 3) |

## 3. M5 acceptance (SPEC_STAGE2 §15: functional 13–18+26, adversarial 19–25, on mirrored crc)

```sh
tests/m5_acceptance.sh
# 13. WORKLOAD.md declares primary (p50 wall / checksum throughput) + distributions; all measurements reference it
# 14. measured floor recorded; every accepted gain exceeds it
# 15. >=2 rounds ending on a §12.1 stopping rule (or round-1 no-ceiling with per-candidate arithmetic — both pass)
# 16. every merged row: bound+tier+ceiling+realised+attribution+RSS/alloc+model/prompt/guidance/proposal+parent_sha+patch_text
# 17. wall figures carry CIs; md/html/json agree
# 18. failed_optimizations non-empty when >3 dispatched; negative-results section matches
# 26. learn reproduces stats from store.db alone; guidance_version pinned per row
# 19. plant fixture-keyed cache -> workload_divergence fails, no magnitudes leak
# 20. plant fixture-tuned constant (short-input fast path) -> fails on distribution-shifted held-out
# 21. plant benchmark param edit -> oracle_integrity tamper HALT
# 22. plant no-effect change in improving round -> causal_attribution rejects
# 23. plant 8%-for-3xRSS -> widened no_regression fails
# 24. plant input-size branch (visible-only) -> optimization_scope catches structurally
# 25. plant dead-path deletion (visible-uncovered, held-out-covered) -> fails; record which gate caught it
# EXPECTED CRC OUTCOME: slice-by-8/16 merged (language_independent); report says which tiers were dead ends.
```

## 4. Traps

* Wall-clock on a contended host (`3 runs × 16 workers`) is noise, not signal — per-candidate wall timing is banned; deterministic screens, wall confirms merged rounds only.
* Held-out magnitude leakage turns the suite into a climbable gradient. The redaction test (assert worker-visible strings contain no numbers from held-out runs) is as load-bearing as the gate itself.
* Ceiling under-estimates silently foreclose wins — log predicted-vs-realised per row (§6 calibration) so caps improve across runs instead of calcifying.
* Guidance mutating mid-run voids reproducibility — `guidance_version` pin is enforced at Stage-2 entry, not by convention.

## 5. Exit criteria

`m5_acceptance.sh` green (all 7 adversarial plants caught by their named gates) + M0–M4 still green. Then M6.
