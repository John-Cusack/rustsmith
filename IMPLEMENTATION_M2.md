# IMPLEMENTATION M2 — Council + Decision Log

**Spec contract:** `SPEC.md` §6 (`decisions` table) + §10 (seats, protocol, escalation, halts) + §12 `[models]`/`[privacy]`.
**Goal:** adversarial review that preserves minority positions and records every decision with its reasoning. No human in the loop after this.
**Non-goals:** no real porting judgments, no prompt tuning for quality — acceptance uses stub seats with scripted positions to prove the *protocol*, not model wisdom. Real model identities come from `config/default.toml` only.

## 1. Crate surfaces

```
rustsmith-council/   # NEW
config/default.toml  # ADD: [models] all five seats + worker; [privacy] enforcement
tests/m2_acceptance.sh
```

```rust
// rustsmith-council
pub enum Seat { Architect, Verifier, Performance, Scope }
pub struct Position { pub seat: Seat, pub stance: Stance, pub reasoning: String }
pub enum Stance { Approve, Reject }
pub struct Proposal { pub question: String, pub artifact_ref: String, pub proposer: Seat, pub reasoning: String }
pub enum Resolution { Consensus { approved: bool }, ArchitectTiebreak { approved: bool, justification: String } }

pub trait SeatDriver {  // one impl per provider; stub impl for tests
  fn critique(&self, seat: Seat, proposal: &Proposal, artifact: &[u8]) -> Result<Position>;
}
impl Council {
  pub fn new(drivers: HashMap<Seat, Box<dyn SeatDriver>>, store: Store) -> Self;
  // Protocol: proposer records proposal -> two critics critique BLIND (artifact+proposal only,
  // never each other's positions) -> agreement carries, disagreement -> Architect decides + writes why.
  pub fn decide(&self, run_id: &str, proposal: Proposal, artifact: &[u8], critics: (Seat, Seat)) -> Result<Resolution>;
  pub fn escalate_replan(&self, run_id: &str, unit_id: &str, failures: &[GateResult]) -> Result<Replan>;
}
pub enum Replan { Repartition { units: Vec<UnitSpec> }, ChangeApproach { note: String }, BindInsteadOfPort { module: String }, MarkPortOnly { module: String, reason: String } }

// privacy (in core or council — enforce in CODE, not config comment):
pub fn assert_training_tier_allowed(model: &str, repo_visibility: Visibility, cfg: &Config) -> Result<()>;
```

Model routing: seat → `(provider, model)` from config only; workers/subagents use cheap high-volume model. Never hardcode a model name outside `config/default.toml`. `training_tier_models=["worker"]` + `allow_training_tier_on_private_repos=false` hard-fails at spawn time on private repos.

## 2. Build order

| # | Slice | Done when |
|---|-------|-----------|
| 1 | `decisions` table write path (append-only; `seat_positions_json` = all positions + reasoning verbatim) | insert + re-read round-trips; no UPDATE/DELETE path exists |
| 2 | Blind-critique plumbing (each critic gets artifact+proposal; harness asserts critics never receive each other's output — enforce by API shape, not discipline) | stub drivers prove isolation: critic B output identical whether critic A approved or rejected |
| 3 | Consensus vs tiebreak paths + minority preservation | agreement → `Consensus`; disagreement → Architect writes `justification`, row reads `architect_tiebreak` with losing reasoning intact |
| 4 | `escalate_replan` (gate-fail ×3 → replan enum; unportable module → bind/shim/out-of-scope with logged reason) | all four replan variants produce a `decisions` row + updated `units` rows |
| 5 | Halt wiring (§10.3 triggers 1–5 → `runs.halt_reason`, report-as-stands, stop; tamper/divergence/token/resource/worktree-escape) | each trigger tested with a planted cause; tamper halts are never resumable |
| 6 | `tests/m2_acceptance.sh` | green (section 3) |

## 3. M2 acceptance (quoted from SPEC §15)

> "present the council a decision with a deliberately wrong majority position; confirm the minority reasoning is preserved in the log and the Architect's tiebreak is recorded with justification."

```sh
tests/m2_acceptance.sh
# 1. seed stub drivers: Architect=approve("X because ..."), Verifier=reject("X is wrong because ..."), Performance=reject("agrees with Verifier: ...")
# 2. council->decide(question="adopt X", critics=(Verifier, Performance))
# 3. assert: resolution == architect_tiebreak, row contains ALL THREE reasoning strings verbatim
#    (grep the minority's distinctive sentence in seat_positions_json)
# 4. assert: a second run with all-approve yields consensus with no tiebreak field
# 5. assert: privacy gate — training-tier worker model + private repo visibility => spawn refused with named error
```

## 4. Traps

* Correlated-model error (why voting is forbidden): majority-wrong must still resolve correctly via tiebreak + leave the minority recoverable. Test with the *majority* wrong, not the minority — the easy test proves nothing.
* Critique leakage: passing full transcript to critics lets them anchor on each other. The `critique()` signature takes only `(proposal, artifact)` — leakage is a type error, not a review finding.
* Decision log as audit trail (§16 item 10 depends on this): `seat_positions_json` stores reasoning text, not stance codes. A row with stances but no reasoning fails review.

## 5. Exit criteria

`m2_acceptance.sh` green + M0/M1 still green. Then M3.
