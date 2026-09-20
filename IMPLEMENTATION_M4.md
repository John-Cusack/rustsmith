# IMPLEMENTATION M4 — Stage 1 Mirror (Behavior-Identical Rust)

**Spec contract:** `SPEC.md` §9 Stage 1 + §16 items 1–6, 8, 10–12.
**Goal:** whole-repo Rust mirror at 100% oracle parity with zero redesign, merged unit by unit with conflicts surfacing immediately.
**Non-goals:** no optimization (a faster unit that changes behavior fails review), no API reshaping, no dependency upgrades "while in there." Deviating here costs more than it saves — enforce by review checklist, not trust.

## 1. Surfaces (no new crates; orchestration in `cli run --stage mirror`)

Worker input bundle (exact, nothing else): unit spec + `PORTING.md` + relevant original source + dependency interface contracts + read-only oracle path. Worker output: worktree commit on `unit/<id>`.
Control-plane merge rule: passing unit merges to run branch AND deletes the mirrored original-language module in the same commit (conflicts surface now, not at the end).
Review rule: implementer never reviews own diff; two reviewers on different providers, diff-only.

```rust
// orchestration (cli/council-adjacent, sketch — fit to M1/M2 types)
pub fn mirror_round(dag: &UnitDag, store: &Store) -> Result<()>;
  // take ready units (deps passed) up to max_parallel_workers
  // Agent::spawn per unit (M1) -> graded_run (M0) -> gates -> record_gate
  // pass: merge + delete mirrored module, status=passed
  // fail: attempts+=1; 3 fails -> Council::escalate_replan (repartition / bind / port_only)
pub fn whole_repo_grade(run_id: &str) -> Result<(GradedResult, f64)>; // 100% parity + divergence
```

## 2. Build order

| # | Slice | Done when |
|---|-------|-----------|
| 1 | Dependency-order scheduler (ready = all deps `passed`; parallel cap; `units` status machine `queued→running→gated→passed/parked/abandoned`) | strsimpy DAG schedules bases before dependents; crc single-unit trivially schedules |
| 2 | Worker bundle assembler (spec + PORTING.md + source slice + contracts + read-only oracle mount; prompt version logged per turn) | bundle contents asserted in test (no held-out paths present, ever) |
| 3 | Grade→gate→merge loop (`oracle_integrity/parity/heldout_divergence/differential/unsafe_budget/miri/clippy` per unit; merge+delete-module commit) | planted behavior break fails `oracle_parity`; planted `unsafe` without `SAFETY` fails `unsafe_budget` |
| 4 | Adversarial review plumbing (reviewer assignment excludes implementer seat; two providers; verdicts in `decisions`) | self-review assignment impossible by query (test asserts) |
| 5 | Whole-repo grade + completion gate (100% + divergence < threshold) | green on mirrored crc |
| 6 | `tests/m4_acceptance.sh` | green (section 3) |

## 3. M4 acceptance (SPEC §16 on crc pin — items 1–6, 8, 10–12)

```sh
tests/m4_acceptance.sh <crc-pin>
# 1. fork contains Rust crates with same public API (Calculator.checksum/verify, Register/TableBasedRegister over bytes->int)
# 2. graded run: 100% frozen oracle passes (80/80 + subtests)
# 3. held-out divergence < 5pp
# 4. manifest hashes unchanged; count/skip lists match baseline
# 5. every unsafe at FFI boundary + // SAFETY: (cargo geiger cross-check)
# 6. miri clean (runnable subset) + clippy -D warnings clean
# 8. md/html/json reports exist and agree on every number
# 10. event log alone reconstructs every unit + council decision (audit replay test)
# 11. plant: worker instructed to edit a test -> run HALTS with tamper event + logged reason
# 12. plant: hardcoded visible-input special-case -> held-out catches it, divergence gate halts
```

## 4. Traps

* "Helpful" redesign is the #1 Stage-1 killer: faster-but-different, renamed APIs, restructured modules. Review checklist rejects all three even with green benches — benches don't gate Stage 1.
* Merge-and-delete in one commit is load-bearing: deferring module deletion hides interface drift until the end, when it's most expensive.
* Held-out blindness: worker bundle assembler must be tested with a canary path (assert build fails if a held-out path ever appears in a bundle). One leak voids the §16 item-12 guarantee.

## 5. Exit criteria

`m4_acceptance.sh` green (both halves: 1–10 functional + 11–12 adversarial) + M0–M3 still green. Then M5.
