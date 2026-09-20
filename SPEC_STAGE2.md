# Stage 2 — Optimization Process (v2, merged)

**Status:** replaces `SPEC.md` §9 Stage 2. Everything else in `SPEC.md` v1 stands.
**Audience:** implementer + reviewers. **Crates:** `rustsmith-profile`, `rustsmith-gates`, `rustsmith-oracle`, `rustsmith-council`.

v1 loop — `profile → rank(time_share × headroom) → fan out N=8 → gate(parity+delta+no_regression) → merge → stop on gain/rounds` — is the right skeleton. It has three structural defects that this revision fixes. Adoption is phased (§13); P0 is soundness, P1 is efficiency.

## 1. Defects in v1

**D1 — Performance can be gamed, nothing catches it.** Correctness has freeze + held-out suite + divergence halt. Performance has no equivalent. A worker that sees the benchmark can key a cache on fixture inputs, tune a constant to the fixture size distribution, or short-circuit an unexercised path. All pass `oracle_parity + benchmark + no_regression` and evaporate on real input.

**D2 — Largest win is unreachable.** Stage 1 preserves module boundaries and Python data idioms (`HashMap<String,V>` for fixed keys, per-call `String`/`Vec`, `Rc<RefCell<T>>` graphs, AoS traversal, `Box<dyn Trait>` for closed dispatch, defensive copies). In documented Python→Rust ports representation change is the bulk of speedup. It is cross-cutting; no single-hotspot worker can make it; v1 only dispatches single hotspots.

**D3 — Loop optimizes symptoms.** `time_share × estimated_headroom` dispatches at a function without establishing the bound. Memory-bandwidth loop + SIMD proposal = wasted cycle. `estimated_headroom` is undefined — an LLM guess in a ranking formula.

Fixes: held-out workload suite + divergence gate (D1), Architect-owned Round 0 (D2), bound classification + computed ceilings as dispatch preconditions (D3). Plus: real measurement (§3), causal attribution, widened regression, failure memory.

## 2. Invariants (extend SPEC §4 core invariant)

- **I1 — No agent sees the graded workload.** Workers iterate against visible workloads; grading uses held-out ones. Same structure as correctness.
- **I2 — Speedup is end-to-end under the declared workload.** Microbenchmark-only wins are not optimizations, not merged.
- **I3 — Credit requires revert-attribution.** Removing the change must give the time back (§9.2). Round-boundary correlation is not attribution.
- **I4 — No trading unmeasured resources for measured ones.** Wall time, peak RSS, alloc count, binary size, compile time tracked. 6% faster + 3× RSS = regression unless contract says otherwise.
- **Zero merged optimizations is a valid outcome.** A well-mirrored repo terminates at round 1 with nothing merged and reports it honestly (§11.1).

## 3. Measurement instrument

v1 "beats CI noise floor with statistical confidence" is unimplementable on the specced host (`max_parallel_runs=3 × 16` workers ⇒ wall-clock noise routinely >10% vs 3% round threshold). Two instruments, in sequence:

**3.1 Primary: deterministic cost.** Cachegrind/Callgrind instruction counts + simulated cache/branch stats. Deterministic under contention; resolves sub-1% deltas. Used for all candidate screening, iteration feedback, ranking.

Limitation (accepted): mis-ranks MLP, real-branch-predictor effects, syscalls/IO/contention. Covered partially by simulated stats; remainder is why §3.2 exists.

**3.2 Confirmation: wall clock.** Once per round, on merged build, in dedicated grading container: quiesce other runs (or queue until idle), CPU pin, governor `performance` where permitted, ASLR disabled, env size normalized, bootstrap CI over sufficient repetitions (default 30, 95%). Report interval, not point estimate. Round gain accepted only if CI excludes zero **and** sign agrees with deterministic instrument. Surviving disagreement ⇒ log, park round, break (§11.2). Re-measure once on quiesced host before halting.

**3.3 Noise floor is measured, not configured.** At Stage 2 entry, run unmodified build against itself N times; spread = floor for the run; written to report; re-measured if contention profile changes. `benchmark_noise_floor_pct` becomes fallback only.

## 4. Stage 0 additions (frozen before any agent runs)

**4.1 `WORKLOAD.md` — the performance analogue of `PORTING.md`.** Architect produces, council reviews, control plane freezes. Contents: objective metric (one of `wall_time_p50 | wall_time_p99 | throughput | peak_rss | cold_start`; one primary + optional secondary), input distribution (sizes, cardinalities, value dists, degenerate cases, share of real calls; derived from fixtures, docs, existing benches, public API shapes), out-of-scope inputs (later regressions there are known trades), resource budgets (RSS/binary/compile ceilings from original measurements). `require_workload_contract=true` — Stage 0 fails without it. Wrong contract ⇒ all later numbers rigorous-looking and meaningless.

Gaming note: Architect-authored contract is circular (author grades to own distribution). Mitigations: derive inputs deterministically from repo artifacts; freeze alongside oracle; held-out suite drawn from same distribution but disjoint inputs (§4.3); divergence gate catches fitting to visible subset anyway.

**4.2 Benchmark oracle freeze.** Visible benchmark workloads hashed into `oracle/manifest.json` exactly like tests. Editing a benchmark ⇒ `oracle_integrity` tamper halt, same class as editing a test. Baseline captured on original per workload: insn count, wall time + CI, peak RSS, alloc count, profile. Denominator for every claim.

**4.3 Held-out workload suite.** Control-plane-generated, host-only, never in run container, never shown to any agent incl. council. Same declared distribution, zero shared concrete inputs: distribution-shifted sizes/values (catches tuned constants), adversarial shapes (empty/singleton/max/all-identical/all-distinct/pathological order), coverage-complement paths (visible suite doesn't reach ⇒ can't win by sacrificing them), identity-sensitive repeats (equal-but-not-identical ⇒ catches identity-keyed caches). Generated once, immutable for the run. `heldout_gain`/`divergence` columns live in host-only DB, never mounted into containers, never returned to workers even as failure magnitude (§9.1).

## 5. Bound classification (dispatch precondition, not advice)

Deterministic analysis at top of every round, before dispatch. `perf stat` top-down + IPC, L1/L2/L3 miss, mispredict, DTLB; counting-allocator stats (count + bytes per workload); `strace -c` syscall counts; futex/lock wait where threaded. One primary + ≤2 secondary bounds per hotspot; written into unit spec; constrains permitted techniques (§8) — bound-mismatched proposal rejected at review before costing a graded run.

| Bound | Signature | What can help |
|---|---|---|
| `compute` | high IPC, retiring-dominated | algorithm, SIMD, branchless, strength reduction |
| `memory_bandwidth` | backend-bound, high L3 miss, IPC falls with input | smaller working set, SoA, compression, streaming |
| `memory_latency` | backend-bound, low IPC, high miss, low BW use | layout, index structures, prefetch, blocking |
| `branch` | high bad-speculation share | branchless, sort inputs, table dispatch |
| `frontend` | frontend-bound, iTLB/icache miss | reduce inline, PGO, BOLT, outline cold |
| `allocation` | high alloc count, time in malloc/free | arenas, SmallVec, reserve, borrow vs own |
| `syscall_io` | time in kernel, high syscall count | buffering, batching, writev, mmap |
| `contention` | futex/lock wait, poor thread scaling | granularity, sharding, lock-free |
| `work_volume` | call count ≫ necessary lower bound | cache/hoist/batch/incrementalize; fix is in caller |

**Caller-side check (`work_volume`).** Flat profile points at callee; fix is usually caller. Compare each hot function's call count against lower bound derived from workload contract. Order-of-magnitude gap ⇒ scope unit to caller.

## 6. Ceiling arithmetic (dispatch gated on arithmetic, not ranking heuristic)

```
ceiling = time_share × (1 − 1 / plausible_speedup)
```

`time_share` = measured inclusive share of objective metric. `plausible_speedup` capped by bound, not guessed:

| Bound | Cap | Basis |
|---|---|---|
| `work_volume` | measured call-count ratio vs necessary bound | measured |
| `allocation` | measured allocator time share | measured |
| `syscall_io` | measured kernel share | measured |
| `contention` | measured lock-wait share | measured |
| `memory_bandwidth` | achieved/peak bandwidth ratio | roofline |
| `compute` | 4× (8× if SIMD-eligible + width supports) | roofline/empirical |
| `memory_latency` | 3× | empirical |
| `branch` | 2× | empirical |
| `frontend` | 1.3× | empirical (PGO/BOLT class) |

Dispatch: only if `ceiling ≥ min_candidate_ceiling_pct` (default 1.0%); below-line logged with reason — largest token saving in Stage 2 (v1 dispatches 8 regardless). Rank survivors by `ceiling / estimated_cost` (files touched, callsites, cross-module?); tie ⇒ cheaper first. Round dispatches up to `hotspots_per_round` (ceiling, not target); **zero survivors is a stopping rule** (§11.1). Caps are conservative by design; `predicted ceiling vs realised gain` is logged per row (§12.1) to calibrate across runs — the only way caps improve.

## 7. Round 0 — structural pass (fixes D2)

Architect-owned, whole-repo, serialized, before per-hotspot fan-out.

**7.1 Targets.** Representation report from control plane enumerates every site + attributable alloc/time: fixed-key `HashMap<String,V>` ⇒ struct/enum/interned index; per-call `String`/`Vec` ⇒ borrow/`&str`/`SmallVec`; `Rc<RefCell<T>>` Python-semantics graphs ⇒ arena indices; AoS traversed field-wise ⇒ SoA; `Box<dyn Trait>` over closed set ⇒ enum monomorphization; defensive copies ⇒ borrowck already prevents; hot-path `Box<dyn Error>` + `format!` ⇒ concrete error types. Invisible in flat profile (cost spread thin), cross-cutting.

**7.2 Process.** (1) Representation report. (2) Architect proposes ranked change set with sites, mechanism, blast radius. (3) Adversarial council review; Verifier's charge = boundary-semantics risk (aliasing, mutation visibility, iteration order). (4) Approved items dispatched **one at a time**, each own unit + graded run (blast radius ⇒ parallel = conflicting + unattributable). (5) Each gated on full parity + both divergences + attribution + no_regression.

**7.3 Termination/risk.** Ends at change-set exhaustion or `round0_max_changes` (default 10). No iteration. Zero gain ⇒ recorded, proceed to Round 1. Risk acknowledged: largest cross-cutting agent change right after Stage-1 victory, protected only by same gates. Fallback via config: `round0_enabled=false` or `round0_mode="suggest"` ⇒ report findings without applying. Default `true` for Python ports (representation overhead proven); reconsider per-adapter.

## 8. Technique hierarchy (proposal must name tier + bound it addresses; top-down)

1. **Do less work** (`work_volume`): redundant passes, hoisting, batching, memoize, incremental, lazy, short-circuit. In call graph, not flat profile. Highest yield, smallest diff, often in caller.
2. **Complexity** (`work_volume|compute`): accidental quadratic, scan-in-loop, repeated sort, wrong container. Verify by scaling across sizes, not code reading.
3. **Representation** (`latency|bandwidth|allocation`): SoA, interning, index graphs, flattening, right-sized ints, bitsets. Mostly Round 0; remainder is hotspot-local.
4. **Allocation** (`allocation`): arenas/bump, SmallVec, reserve, `&str`/`Cow`, kill hot `format!`/`.clone()`, buffer reuse. Dominant cost in naive ports.
5. **Memory access** (`latency|bandwidth`): field order/padding, blocking, prefetcher-friendly patterns, false-sharing removal, working-set fit.
6. **Syscalls/IO** (`syscall_io`): buffering, batching, writev, mmap, fewer round trips.
7. **Parallelism** (`compute|syscall_io`): only after serial is tight. Watch for new `contention` bound + bandwidth saturation.
8. **Micro/SIMD** (`compute|branch`): branchless, explicit SIMD, strength reduction, LUTs. Worst review-burden/gain ratio; unconstrained agents reach here first — bound gate exists to stop that.
9. **Build-level** (`frontend|+`): LTO, `codegen-units=1`, `target-cpu`, PGO, BOLT, allocator swap. Gated independently at end (§10), never a substitute.

**Forbidden at proposal review regardless of measured gain:** shape-dependent behaviour change absent from original; caches keyed to distinguish visible from same-distribution workloads; relaxed tolerance/precision/error guarantees; `unsafe` outside FFI (restates `unsafe_budget` — Tier 8 is where workers reach); deleting paths the benchmark doesn't exercise.

## 9. Round loop

```
stage_2:
    measure_noise_floor()
    baseline = graded_measure(current_build)   # all instruments, all workloads
    round_0_structural_pass()                  # §7, serialized

    for round in 1..=max_rounds:
        # analysis: deterministic, no agents
        profile    = profile_under_visible_workloads(current_build)
        bounds     = classify_bounds(profile)              # §5
        callers    = caller_side_check(profile)            # §5.3
        candidates = enumerate(profile, bounds, callers)
        for c in candidates:
            c.ceiling = time_share(c) * (1 - 1/speedup_cap(c.bound))  # §6
        candidates = candidates
            .filter(ceiling >= min_candidate_ceiling_pct)
            .filter(not previously_failed_with_same_technique)  # §10
            .sort_by(ceiling / estimated_cost)
            .take(hotspots_per_round)
        if candidates.is_empty():
            stop_reason = "no candidate above ceiling"; break

        # proposal review: technique only, no diff (cheap seat check)
        for c in candidates:
            proposal = worker_proposes_technique(c)
            if not technique_permitted_for_bound(proposal, c.bound): reject; log; continue
            council_review(proposal)

        # implementation: parallel isolated worktrees, iterate vs DETERMINISTIC
        # instrument only (fast, reproducible, leaks nothing about held-out)
        for c in approved, in parallel:
            worker implements (cap iterations_per_hotspot + token ceiling)

        # gating: per candidate, grading container
        for c in completed:
            gate oracle_integrity
            gate oracle_parity
            gate heldout_divergence      # correctness, SPEC §8
            gate workload_divergence     # NEW §9.1
            gate causal_attribution      # NEW §9.2
            gate benchmark               # deterministic, §9.4
            gate no_regression           # widened §9.3

        merge(winners); record(losers) in failed_optimizations  # §10
        round_result = graded_measure(merged_build)  # wall-clock §3.2
        round_gain   = CI-bounded delta vs previous round
        if not sign_agreement(deterministic_delta, wall_clock_delta):
            log divergence; park round; break
        if round_gain.lower_bound < round_gain_threshold: break

    final_pass:  # each measured + gated independently, never unconditional
        PGO, then BOLT, then allocator-swap-as-candidate
```

Deliberate deltas from v1: proposal-before-code (cheap reject saves implement+grade); deterministic-only iteration (no held-out leakage); post-merge remeasure (interactions: two solo wins can lose together via cache/inline pressure — only merged number counts to `round_gain`); gated finals.

## 10. Gates (all deterministic Rust in `rustsmith-gates`; hard rule unchanged)

**10.1 `workload_divergence` (new, most important).**
```
visible_gain = (baseline_vis - candidate_vis) / baseline_vis
heldout_gain = (baseline_held - candidate_held) / baseline_held
divergence   = visible_gain - heldout_gain
```
Fail unit if `divergence > workload_divergence_threshold` (default 10pp) or `heldout_gain` negative beyond floor (helps measured, harms rest = overfit signature). Halt run if `divergence > workload_divergence_halt` (25pp) or 2 units fail this gate in one run (pattern ⇒ incentive failure, not worker error). Held-out magnitudes **never returned to worker** in any form (incl. failure reason — worker hears only "rejected for workload divergence"); magnitude is a gradient to climb. Threshold default 10pp; calibrate per-repo from baseline cross-workload variance when available (legit caches genuinely help unevenly).

**10.2 `causal_attribution` (new).** Grading container reverts candidate commit, re-measures deterministic instrument. Pass iff gain disappears within floor. Catches phantom gains (credit stolen from same-round sibling), layout luck, drift. Cost: one extra deterministic run per candidate.

**10.3 `no_regression` (widened).** v1 checked other benches only. Now, vs baseline + contract budgets: any other visible workload objective (floor); any held-out workload objective pass/fail only, no magnitudes (floor); peak RSS +5%; alloc count +10%; binary size +10%; clean compile +20%; contract secondary (as declared). Breach ⇒ fail with dimension named (except held-out: name only).

**10.4 `benchmark` (restated).** Per-candidate: deterministic gain > measured floor. Per-round (merged): wall-clock CI excludes zero, same sign. No per-candidate wall-clock on contended host — not worth cost.

**10.5 `optimization_scope` (new, cheap structural).** Diff touches no frozen-oracle/benchmark file; introduces no new conditional on input size/value/identity absent from original control flow (structural special-case detector, complements §10.1 statistical); public API surface byte-identical to post-Stage-1; no dependency outside adapter allowlist.

Unchanged gates still apply per `SPEC.md` §8: `oracle_integrity`, `oracle_parity`, `heldout_divergence`, `differential`, `unsafe_budget`, `miri`, `clippy`, `loop_detector`, `provenance`.

## 11. Negative results (loop memory)

v1 discards losers ⇒ rediscovers same dead end up to 6× at full token cost, and report omits what was tried — the section humans want most on low-gain runs.

```sql
CREATE TABLE failed_optimizations (  -- §11 in-run memory, §17 cross-run memory
  id INTEGER PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(id),
  round INTEGER NOT NULL,
  hotspot TEXT NOT NULL,
  bound TEXT NOT NULL,
  tier INTEGER NOT NULL,
  technique TEXT NOT NULL,
  outcome TEXT NOT NULL,   -- rejected_at_proposal | gate_failed | no_gain | reverted
  gate TEXT,
  measured_delta_pct REAL,
  detail_json TEXT NOT NULL,
  tokens_spent INTEGER NOT NULL,
  model TEXT NOT NULL,              -- §17: which worker model attempted it
  prompt_version TEXT NOT NULL,     -- §17: prompt template version
  guidance_version TEXT NOT NULL,   -- §17: guidance/optimize.md version pinned at run
  proposal_text TEXT NOT NULL,      -- §17: worker's pre-impl rationale (why it should work)
  parent_sha TEXT NOT NULL,         -- §17.2: before-state pointer
  patch_text TEXT NOT NULL          -- §17.2: centralized diff, canonical (empty when rejected_at_proposal)
);
```

Uses: exclude `(hotspot, tier, technique)` in later rounds unless bound reclassified (only honest reason to expect different outcome; alternative — re-allow when hotspot absolute time shifts >factor — deferred, see §14 Q); inject prior failures into dispatched worker prompt (stable prefix, cache-friendly — cheapest quality/token in loop); `RUSTSMITH_REPORT.md` negative-results section (attempted/why-failed/cost; on low-gain runs this *is* the value: mirror is near ceiling, directions closed).

## 12. Termination and halting

**12.1 Stopping rules (normal, recorded with numbers, not errors).** (1) No candidate above ceiling (fires round 1 on good mirrors — pass). (2) Round-gain CI lower bound < `round_gain_threshold_pct`. (3) Max rounds. (4) All remaining candidates previously failed, bound unchanged. (5) Spend guard: cumulative Stage-2 tokens > `stage2_token_ceiling` or marginal gain/token < `min_gain_per_mtoken`.

**12.2 Halts (immediate, no override; extend SPEC §10.3).** (6) Benchmark tamper (hash/invocation mismatch). (7) Workload divergence over halt threshold or 2 failures/run. (8) Instrument disagreement surviving quiesced re-measure (measurement model wrong — further numbers untrustworthy). (9) Post-merge parity loss (per-candidate gating says impossible ⇒ gating assumption false ⇒ halt, not retry).

**12.3 On halt.** Stage 2 halts leave Stage-1 mirror intact, status `optimize_halted`. Stage 3 harvest still runs on merged+verified rows. Halted Stage 2 never invalidates a correct mirror; report states where history stopped.

## 13. Data model / config deltas

```sql
-- Per-attempt retrospective provenance. Every row in both tables is one
-- worker attempt with full context to answer: what was tried, why, by whom,
-- under which guidance, at what cost, with what measured result.
ALTER TABLE optimizations ADD COLUMN bound TEXT NOT NULL;
ALTER TABLE optimizations ADD COLUMN tier INTEGER NOT NULL;
ALTER TABLE optimizations ADD COLUMN ceiling_pct REAL NOT NULL;
ALTER TABLE optimizations ADD COLUMN visible_gain_pct REAL NOT NULL;
ALTER TABLE optimizations ADD COLUMN heldout_gain_pct REAL NOT NULL; -- host-only
ALTER TABLE optimizations ADD COLUMN divergence_pct REAL NOT NULL;   -- host-only
ALTER TABLE optimizations ADD COLUMN instrument TEXT NOT NULL;
ALTER TABLE optimizations ADD COLUMN ci_low REAL;
ALTER TABLE optimizations ADD COLUMN ci_high REAL;
ALTER TABLE optimizations ADD COLUMN attribution_verified INTEGER NOT NULL;
ALTER TABLE optimizations ADD COLUMN rss_delta_pct REAL NOT NULL;
ALTER TABLE optimizations ADD COLUMN alloc_delta_pct REAL NOT NULL;
ALTER TABLE optimizations ADD COLUMN model TEXT NOT NULL;            -- §17: who
ALTER TABLE optimizations ADD COLUMN prompt_version TEXT NOT NULL;   -- §17: instructions
ALTER TABLE optimizations ADD COLUMN guidance_version TEXT NOT NULL; -- §17: pinned guidance
ALTER TABLE optimizations ADD COLUMN proposal_text TEXT NOT NULL;    -- §17: why it should work (pre-impl)
ALTER TABLE optimizations ADD COLUMN tokens_spent INTEGER NOT NULL;  -- §17: cost
ALTER TABLE optimizations ADD COLUMN parent_sha TEXT NOT NULL;       -- §17.2: before-state pointer
ALTER TABLE optimizations ADD COLUMN patch_text TEXT NOT NULL;       -- §17.2: centralized diff, canonical
CREATE TABLE guidance_revisions (
  id INTEGER PRIMARY KEY,
  created_at INTEGER NOT NULL,
  guidance_version TEXT NOT NULL UNIQUE,  -- e.g. 'opt-guidance v7'
  change_summary TEXT NOT NULL,           -- what changed in guidance/optimize.md
  evidence_query TEXT NOT NULL,           -- SQL/views that justify it
  stats_json TEXT NOT NULL,               -- aggregation output at review time
  runs_included_json TEXT NOT NULL,       -- run ids in the review window
  decided_by TEXT NOT NULL,               -- 'human' | 'architect_tiebreak'
  prompt_diff TEXT NOT NULL               -- unified diff of guidance/optimize.md
);
```

```toml
[workload]
primary_metric = "wall_time_p50"
require_workload_contract = true

[measure]
primary_instrument = "cachegrind"
wallclock_confirmation = true
wallclock_repetitions = 30
wallclock_ci = 0.95
quiesce_host_for_wallclock = true
noise_floor_measured = true

[optimize]
round0_enabled = true
round0_mode = "apply"        # apply | suggest
round0_max_changes = 10
min_candidate_ceiling_pct = 1.0
hotspots_per_round = 8       # ceiling, not target
iterations_per_hotspot = 6
round_gain_threshold_pct = 3.0
max_rounds = 6
stage2_token_ceiling = 100_000_000
min_gain_per_mtoken = 0.01
require_technique_proposal = true

[optimize.heldout]
workload_divergence_threshold = 0.10
workload_divergence_halt = 0.25
max_divergence_failures_per_run = 2

[optimize.regression]
rss_tolerance_pct = 5.0
alloc_tolerance_pct = 10.0
binary_size_tolerance_pct = 10.0
compile_time_tolerance_pct = 20.0

[optimize.final]
pgo = "gated"   # gated | off, never unconditional
bolt = "gated"
allocator_swap = "candidate"
```

## 14. Adoption phasing (do not big-bang)

- **P0 — soundness (M5 minimal):** `WORKLOAD.md` + benchmark freeze + held-out suite/divergence gate + measured floor + widened `no_regression` + gated finals + `failed_optimizations`. Without these numbers are untrustworthy.
- **P1 — efficiency (M5 full):** bound classification + ceiling filter + causal attribution + scope gate + proposal review + Round 0. These cut tokens and find bigger wins once numbers are sound.
- **P2 — learning (post-M5):** retrospective columns + `guidance_revisions` + `rustsmith learn` (§17). No new gates; strictly offline over `store.db`. Ships after ≥3 runs of history exist to review.

## 15. Acceptance (extend SPEC §16 item 7; items 13–18 replace it)

Functional: (13) `WORKLOAD.md` exists, declares primary metric + distribution, all measurements reference it. (14) Measured floor recorded; every accepted gain exceeds it. (15) ≥2 rounds ending on §12.1 rule — or round-1 `no candidate above ceiling` with per-candidate ceiling arithmetic recorded (both pass). (16) Every merged row has bound, tier, predicted ceiling, realised gain, attribution pass, RSS + alloc deltas, plus model/prompt/guidance versions and proposal text (§17). (17) All wall-clock figures carry CIs; all three report formats agree. (18) `failed_optimizations` non-empty when >3 dispatched; report negative-results section matches. (26) `rustsmith learn` reproduces its stats from `store.db` alone and every attempt row carries a `guidance_version` pinned at Stage-2 entry (§17).

Adversarial (claim = agents can't game measurement; all must pass): (19) fixture-keyed cache ⇒ `workload_divergence` fails, magnitudes never in worker context. (20) Fixture-tuned constant ⇒ fails on distribution-shifted held-out. (21) Benchmark param edit ⇒ `oracle_integrity` tamper halt. (22) No-effect change in improving round ⇒ `causal_attribution` rejects. (23) 8% time for 3× RSS ⇒ widened `no_regression` fails. (24) Input-size branch present only in visible workload ⇒ `optimization_scope` catches structurally. (25) Dead-path deletion uncovered by visible but covered by held-out ⇒ fails (via `oracle_parity` and/or `workload_divergence`; record which).

## 16. Open questions (reviewer prompts retained; implementer guidance given)

- **Q1 Round 0 risk?** Kept, default apply-with-`suggest`-fallback. Serialized + Verifier boundary-risk charge + full gates make it as safe as Stage 1 mechanics allow. Demote to suggest-only per-repo if oracle coverage thin.
- **Q2 Deterministic-primary sound?** Yes for CPU-bound library ports (expected majority). Workload contract may flip primary to wall-clock for IO-bound repos; per-round wall-clock confirmation catches mis-ranks before they accumulate across rounds (within-round accumulation accepted as cost tradeoff).
- **Q3 Caps defensible?** Conservative constants; 5/9 measured. Under-estimate forecloses wins silently — mitigated by cross-run predicted-vs-realised calibration (§6). Better basis without running the work: none found; keep constants, improve empirically.
- **Q4 10pp threshold discriminate?** No data; default stands, per-repo variance calibration preferred where baseline supports it.
- **Q5 Proposal-before-impl pay?** Kept but cheap-seat check (Performance/Scope), not full adversarial — prevents wasted grade cycles at thousands not hundreds of thousands of tokens. Plausible-but-failing proposals still possible; failure memory (§11) bounds repeat cost.
- **Q6 Merge granularity?** Per-round merge + per-change revert-attribution is cheapest design keeping both interaction visibility and per-change credit. Finer (bisect merges) costs more grades than it saves.
- **Q7 Missing?** Known gaps handled: `WORKLOAD.md` circularity (§4.1), `unsafe`/Tier-8 interaction (still `unsafe_budget`-gated), harvest classification (tier+bound recorded per row to aid Stage 3). Retrospective itself (§17) is the remaining answer: unknown failure modes surface in aggregation, not in pre-speculation.

## 17. Self-improving loop (retrospective → guidance)

No new database. `store.db` is already cross-run (one `runs` row per repo); §11 + §13 make it the retrospective. Git is execution only — the DB is canonical for review. A fork repo may be deleted, rebased, or never shipped; a `store.db` row must still answer what changed and why without touching git.

**17.1 Guidance artifact.** `guidance/optimize.md` — short, concrete rules the worker prompt includes in its stable cached prefix (technique preferences, bound→tier mapping notes, known dead ends, cost heuristics). Versioned in git (`opt-guidance vN`). Stage-2 entry records `guidance_version` on every attempt row in both tables; mid-run guidance change is prohibited (reproducibility). A run is explainable from `store.db` + guidance version alone.

**17.2 What each attempt records, centrally.** Every attempt writes one row with the full before/after **in the row**, not behind a git pointer. `parent_sha` + `commit_sha` identify the code states; `patch_text` (unified `git diff parent..child`, required, `''` only when `outcome=rejected_at_proposal` — no code existed) **is** the change. Alongside it: `model` (who), `prompt_version` + `guidance_version` (under which instructions), `proposal_text` (worker's pre-impl why-it-should-work, verbatim), `technique/bound/tier/ceiling` (what was claimed), measured `visible_gain/heldout_gain/divergence/CI/attribution/rss/alloc` or `outcome/gate/measured_delta` on failure (what happened), `tokens_spent` + `files_touched_json` (at what cost, where). `detail_json` holds diff stats + gate details. Capture rule: control plane runs `git diff` at grade time and writes the row in the same transaction as the gate result — never reconstructed later from a branch that may have moved. Cap: `patch_text` hard-capped at 512 KiB (larger ⇒ store truncated flag in `detail_json` + full patch as artifact file referenced there; the row stays queryable). Cross-repo review works because every row carries `run_id → runs.repo_url + source_lang`: filter by technique/bound/tier across repos, then read `proposal_text` + `patch_text` side by side with no checkout.

**17.3 Review cadence.** Manual, offline, after ≥3 runs or ≥50 attempts — never automatic, never in-loop. Command:

```
rustsmith learn [--since DATE] [--run-ids A,B] [--min-attempts 50]
```

Steps: (1) control plane runs deterministic aggregations over `store.db` (queries below) and emits stats JSON; (2) Performance seat drafts a guidance diff from the stats; (3) human approves (or Architect tiebreak logged); (4) merged as new guidance version + one `guidance_revisions` row (evidence query, stats, runs included, prompt diff). Rejected proposals are logged too — a review that changes nothing still writes a row saying why.

**17.4 Standard aggregations (deterministic; the evidence behind every guidance change).**

```sql
-- Yield by (bound, tier, technique): what actually works, where
SELECT bound, tier, technique,
  COUNT(*) AS n,
  SUM(CASE WHEN outcome IS NULL THEN 1 ELSE 0 END) AS merged,  -- optimizations vs failed
  AVG(visible_gain_pct) AS avg_gain, AVG(tokens_spent) AS avg_cost
FROM (
  SELECT bound, tier, technique, NULL AS outcome, visible_gain_pct, tokens_spent FROM optimizations
  UNION ALL
  SELECT bound, tier, technique, outcome, measured_delta_pct, tokens_spent FROM failed_optimizations
) GROUP BY bound, tier, technique ORDER BY merged DESC, avg_gain DESC;

-- Ceiling calibration: are §6 caps grounded? (systematic over/under ⇒ adjust cap)
SELECT bound, AVG(ceiling_pct - visible_gain_pct) AS avg_overpredict
FROM optimizations GROUP BY bound;

-- Failure modes: which gate kills what (proposal reject rate ⇒ guidance unclear)
SELECT gate, COUNT(*) FROM failed_optimizations WHERE outcome='gate_failed' GROUP BY gate;

-- Cost per point of gain, by model (routing signal, not a leaderboard)
SELECT model, SUM(tokens_spent) / NULLIF(SUM(visible_gain_pct),0) AS tokens_per_point
FROM optimizations GROUP BY model;
```

Guidance changes must cite which query + window produced them (`evidence_query` + `runs_included_json`). A rule without a query is an opinion, not a retrospective.

**17.5 What guidance updates look like.** Prefer small, reversible diffs: demote a technique with <10% merge rate over ≥20 attempts to "attempt last"; tighten/loosen one §6 cap where `avg_overpredict` is systematically signed; add one dead-end note ("SmallVec past 256B regresses on `memory_bandwidth` — 0/14 merged"); adjust `ceiling/cost` heuristic weights. One claim per line, each traceable to a row set. Never rewrite technique hierarchy (§8) on one review — that needs convergent evidence across ≥2 windows.

**17.6 Guardrails.** Guidance affects future grading incentives, so: reviews never see held-out magnitudes beyond what gates already expose (aggregations use per-run `divergence_pct` pass/fail + visible gains only where the worker could have seen them — no new leakage channel); guidance diffs go through the same adversarial council review as code; `guidance_version` pin means old runs stay reproducible; `prompt_diff` + stats are the audit trail. If a guidance version degrades merge rate over its next window, revert it — the revisions table makes that a one-row lookup.
