# Optimization playbook (for port authors)

How to spend optimization effort on a port without wasting it. This is
procedure, not machinery: the normative definitions live in `SPEC.md`
and `SPEC_STAGE2.md` (gates, instruments, tables, thresholds). Where
this page names a mechanism, the spec section it cites governs.

A port that ends with nothing merged is a complete result, not a
failure — report it as `no_gain` with the numbers that closed each
direction (`SPEC_STAGE2.md` §2, invariant I4; §12.1).

## 1. Frame tight

One hot spot, one measured ceiling, profile map first — kill criteria
up front.

- Profile the current build under the `WORKLOAD.md` workloads before
  proposing anything (`SPEC_STAGE2.md` §4–§5).
- Compute the ceiling — `time_share × (1 − 1 / plausible_speedup)` —
  with the per-bound speedup caps, and dispatch only above
  `min_candidate_ceiling_pct` (`SPEC_STAGE2.md` §6; default 1.0%).
- Set the kill line before measuring and honor it: below-line
  candidates are logged with the reason and walked away from. That
  log entry is the largest token saving in the stage.

## 2. Analyze, then review

The first deliverable is a report with evidence, never code.

- The analysis pass is deterministic: profile, bound classification
  (one primary + ≤2 secondary bounds per hotspot), caller-side
  `work_volume` check, ceiling arithmetic (`SPEC_STAGE2.md` §5–§6).
- Each proposal names the technique tier and the bound it addresses,
  top-down from the hierarchy (`SPEC_STAGE2.md` §8). A
  bound-mismatched proposal is rejected at review before it costs a
  graded run (`SPEC_STAGE2.md` §9; `require_technique_proposal`).
- Forbidden regardless of measured gain: shape-dependent behavior
  change, caches keyed to distinguish visible from same-distribution
  inputs, relaxed tolerances, `unsafe` outside FFI, deleting paths
  the visible suite does not exercise (`SPEC_STAGE2.md` §8, §10.5).

## 3. Decide before building

Explicit go/no-go with the options on the table. No silent scope
growth.

- Record the decision and the rejected options with reasoning
  (`SPEC.md` §10.2, `decisions` table). A scope change is a new
  decision, not an edit mid-flight.
- The fence is structural: no frozen-oracle/benchmark touches, no new
  input-conditional branches absent from the original control flow,
  public API surface byte-identical (`SPEC_STAGE2.md` §10.5,
  `optimization_scope`).

## 4. Compare variants side by side

Never crown a technique on its own numbers. Run the top candidates
against the same frozen bench plus the held-out workload, through the
same gates.

- Same frozen bench: benchmark workloads hashed into
  `oracle/manifest.json`; deterministic instrument screens,
  wall-clock confirms (`SPEC_STAGE2.md` §3–§4).
- Same gates for every candidate: `oracle_integrity`,
  `oracle_parity`, correctness and workload divergence, causal
  attribution (the gain must disappear on revert), `benchmark`,
  widened `no_regression`, `optimization_scope`
  (`SPEC_STAGE2.md` §9–§10).
- The winner keeps its merge only with correctness green and both
  instruments agreeing: visible and held-out gains within the
  divergence threshold with matching sign, otherwise park the round
  and stop (`SPEC_STAGE2.md` §10.1, §12.2).

## 5. Publish the ledger

Winners AND losers, with numbers, into the report and the store.

- Every attempt lands in `optimizations` or `failed_optimizations`
  with bound, tier, technique, ceiling, measured gains, gate/outcome,
  and cost (`SPEC_STAGE2.md` §11, §13, §17.2).
- The report carries a negative-results section — attempted,
  why-failed, cost. On low-gain runs that section *is* the value
  (`SPEC_STAGE2.md` §11).
- Later rounds exclude `(hotspot, tier, technique)` losers unless the
  bound was reclassified — the only honest reason to expect a
  different outcome (`SPEC_STAGE2.md` §9 loop filter).

## 6. Two-track benches

Gate on the representative workloads; demonstrate the ceiling
separately. Honesty rules on both tracks, always.

- Gate numbers come from the representative workloads declared in
  `WORKLOAD.md` (one primary metric, input distribution, resource
  budgets). Ceiling figures (§6) are labeled as potential, never
  presented as measured gain (`SPEC_STAGE2.md` §4.1, §6; §15 item 13).
- Measured-only: every accepted gain beats the measured noise floor;
  wall-clock figures carry confidence intervals, and all three report
  formats agree on every number (`SPEC_STAGE2.md` §3, §15 item 17).
- Disclose crossing and companion costs: binding overhead, peak RSS,
  alloc count, binary size, compile time — against the contract
  budgets (`SPEC_STAGE2.md` §10.3). Label every figure with the
  workload that produced it.

## 7. Port boundaries

The minimum viable boundary is the whole hot unit: slicing smaller
pays a per-crossing toll on every call.

- Keep the hot path on one side of the seam. Representation wins
  (borrow vs own, arenas, SoA, enum dispatch) are cross-cutting and
  get a serialized whole-repo pass before per-hotspot fan-out
  (`SPEC_STAGE2.md` §7).
- The destination shape is a reusable core crate plus thin bindings
  preserving the supported interface (`README.md`, "Product vision";
  `SPEC.md` §9 Stage 1).
- Dynamic behavior (C extensions, heavy metaprogramming) stays behind
  a documented seam: classify each dependency `port | bind | keep`,
  and for what cannot be ported choose per case — bind via FFI, shim
  the original module in place, or mark out of scope — with the
  reasoning recorded (`SPEC.md` §9 Stage 0 item 4, §10.3).

## 8. Long runs, stated upfront

Full benches, PGO/BOLT passes, and Miri runs are long. State the
expected wall time before starting one.

- A run that goes silent past its stated time gets one status check
  before you kill or retry it — never orphan runs, never stack
  retries on an unknown state (`rustsmith status` / `audit`;
  `halt` / `resume` for non-tamper stops: `SPEC.md` §11, §13).
- Contended hosts lie about time: quiesce, pin, and normalize for
  confirmation runs (`SPEC_STAGE2.md` §3.2); do not run concurrent
  builds against each other (`SPEC.md` §11, build mutex).
- Retries are bounded by the spend guards — per-unit ceilings and the
  marginal gain/token floor — not by patience (`SPEC.md` §10.3,
  §12; `SPEC_STAGE2.md` §12.1).
