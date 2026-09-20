# IMPLEMENTATION — Full Spec Build Plan

**Contracts:** `SPEC.md` v1 (all §§) + `SPEC_STAGE2.md` (replaces SPEC §9 Stage 2).
**Spec commit:** _fill in `git rev-parse --short HEAD` before starting; every milestone quotes its acceptance verbatim._
**Order:** M0 → M1 → M2 → M3 → M4 → M5 → M6. No skipping; each milestone's acceptance must pass before the next starts. Detail docs (build depth — signatures, slices, scripts, traps): `IMPLEMENTATION_M0.md` → M1 → M2 → M3 → M4 → M5 → M6. This file is the map; the `IMPLEMENTATION_MN.md` files are the build orders. Hand an LLM one `IMPLEMENTATION_MN.md` plus the two spec files and it has everything for that milestone.

**Fixtures (pinned in `config/fixture.toml`):** primary `Nicoretti/crc` @ `4e65ac4` — pure-Python CRC, BSD-2-Clause, ~750 lines (`src/crc/_crc.py` 715), 80 tests + 28 subtests, 0 skipped/0 xfailed, pytest-only, split invocation (`pytest test/unit` + `pytest test/integration`; `test/bench/` excluded from oracle; src-layout needs `pip install -e .` or `PYTHONPATH=src`). Known M6 win: slice-by-8/16 (ships byte-at-a-time). Secondary `luozhouyang/python-string-similarity` @ `115acaa` (MIT, ~19 modules) — M3 DAG check ONLY, since crc is single-module and its DAG is vacuous.

## M0 — Oracle, gates, store, sandbox (load-bearing; no agents)

**Crates:** `core` (types) → `store` (SQLite + JSONL) → `oracle` (freeze/hash/graded run) → `gates` (pure) → `sandbox` (images) → `cli` skeleton (`run --stage recon`, `audit`).
**Key interfaces:** `Oracle::freeze → Manifest`, `verify_hashes`, `graded_run → (GradedResult, divergence)`; `gates::oracle_integrity/parity/heldout_divergence/differential/unsafe_budget/miri/clippy`; `Sandbox::grading_run` (ephemeral, `--network=none`, read-only oracle mount); `Store::open/append_event/record_gate` with SPEC §6 tables verbatim.
**Hard rules:** `gates` + `oracle` MUST NOT depend on `agent`/`council` (CI dep-check day 1); `store.db` lives on host, never mounted; held-out suite host-only, never in any container.
**Acceptance (SPEC §15, quoted):** "freeze a real Python repo's oracle; run a graded pass in a clean container; hand-modify a test file and confirm `oracle_integrity` fails; hand-add `@pytest.mark.skip` and confirm the skip-count check fails; confirm the held-out suite runs on the host only."
**Detail:** `IMPLEMENTATION_M0.md` §§1–6 (signatures, 8 slices, `tests/m0_acceptance.sh`, traps: `collect-only` baseline counts, hash the invocation + config, pin image digests).

## M1 — Sandbox confinement + worker driver

**Crates:** `sandbox` (worktrees, cgroups, build-mutex leases, `PATH` wrapper) + new `agent` (one OMP CLI subprocess per unit, structured-output parse, no interactive sessions).
**Key interfaces:** `Sandbox::alloc_worktree(run) → Path`, `BuildLease::acquire/release` (one at a time per run container), `Agent::spawn(unit_spec, worktree) → StructuredResult` (prompt version + tokens in/out recorded); `PATH` wrapper rejecting `git`/`cargo` outside own worktree (deny + log, not prompt text).
**Config:** `[run] max_parallel_workers=16`, cgroup CPU/mem caps per worker, `unit_token_ceiling`.
**Acceptance (quoted):** "spawn 8 concurrent OMP subprocesses in one container; confirm each is confined to its worktree; confirm an attempt to run `git checkout` on the run branch is blocked and logged as a halt trigger."
**Trap SPEC §17 warns about:** prevention at filesystem/process level, never by prompt instruction.

## M2 — Council + decision log

**Crates:** new `council` (4 seats, swappable models via `config/default.toml` only — never hardcoded).
**Seats:** Architect (partitioning, `PORTING.md`, tiebreak; strongest reasoning model), Verifier (behavior vs oracle, cannot modify tests; different provider), Performance (hotspots/techniques/benchmarks; third provider), Scope (token ceilings, loop detection; cheap fast model). Workers/subagents run cheap high-volume at high concurrency.
**Protocol (adversarial, never majority vote):** one seat proposes + reasoning → two others critique blind to each other → agreement carries, disagreement goes to Architect tiebreak with written why. Every decision → `decisions` table (`question, seat_positions_json, resolution, resolved_by, decided_at`); minority positions recoverable.
**Escalation:** unit fails gate N=3× → council re-plans (re-partition, change approach, bind-not-port, `port_only`). Unportable module (C ext, dynamic behavior, metaprogramming) → FFI bind / shim / out-of-scope, logged + reported. Halts (§10.3: tamper, divergence, token ceiling, resource exhaustion, out-of-worktree action) → `runs.halt_reason`, emit report, stop, loud + permanent.
**Acceptance (quoted):** "present the council a decision with a deliberately wrong majority position; confirm the minority reasoning is preserved in the log and the Architect's tiebreak is recorded with justification."

## M3 — Stage 0 recon + Python adapter

**Crates:** `adapters/python` (only implemented adapter; trait must admit future Fortran/C++), recon pipeline in `cli run --stage recon`.
**Pipeline (deterministic first, Architect last):** detect language/build → module + call graph (tree-sitter + imports) → test inventory + baseline run (count + skip list) → dependency classify (`port|bind|keep`) → license/attribution record → `py-spy` hotspot baseline → held-out suite generation (property/differential/fuzz from public API) → Architect `PORTING.md` (few-hundred concrete rules: type/error/naming mapping, crate layout, ownership/lifetimes, cross-language traps; Bun precedent ~300 rules) → leaf-first unit DAG for parallel execution → council review/approve.
**Stage-2 prerequisites (SPEC_STAGE2 §§3–4, same freeze discipline):** `WORKLOAD.md` (primary metric, input distribution, out-of-scope, resource budgets; `require_workload_contract=true`), benchmark workload freeze into `oracle/manifest.json`, held-out *workload* suite (distribution-shifted, adversarial shapes, coverage-complement, identity-sensitive), per-workload baseline (insns, wall+CI, RSS, allocs, profile).
**Acceptance (quoted):** "on a small pure-Python package, produce a `PORTING.md` of at least 100 concrete rules and a unit DAG whose leaf-first order is verifiably correct." (Use the fixture repo.)

## M4 — Stage 1 mirror, end to end (no redesign)

**Rule:** behavior-identical Rust, same module boundaries. "No redesign. No cleverness. No 'while I was in there.'"
**Loop:** workers take units in dependency order (independent in parallel ≤ max): unit spec + `PORTING.md` + original source + dependency contracts + read-only oracle → graded gates on completion → merge passing units (delete original-language module in same commit so conflicts surface) → adversarial review (implementer never reviews own diff; two reviewers, different providers, diff-only).
**Complete when:** every unit passed + whole-repo graded run at 100% parity, divergence under threshold.
**Acceptance:** SPEC §16 items 1–6 + 8 + 10–12 on the fixture repo (100% oracle, <5pp divergence, hashes/counts/skips match, `unsafe` FFI-only + `// SAFETY:`, miri + clippy clean, reports agree, event log self-sufficient, both adversarial plants halt correctly).

## M5 — Stage 2 optimize (SPEC_STAGE2 full)

**Crates:** `profile` (measure/rank) + gates extensions + `store` schema extensions.
**Loop (rounds, not open loop):** `measure_noise_floor` (measured spread, not config) → baseline → Architect-owned serialized Round 0 (representation overhauls: fixed-key maps→structs, per-call `String`→borrows, `Rc<RefCell>`→arenas, AoS→SoA, `Box<dyn>`→enums; one at a time, `round0_max_changes=10`, `suggest`-only fallback) → per round: deterministic profile (Cachegrind primary) → bound classification (9 bounds: compute/bandwidth/latency/branch/frontend/allocation/syscall/contention/work_volume + caller-side check) → `ceiling = share×(1−1/cap)` → filter `≥min_candidate_ceiling_pct` + no-repeat + rank `ceiling/cost` + take ≤8 (zero survivors = stop) → proposal-before-code review (cheap Performance/Scope bound check) → parallel implement (cap 6 iters + token ceiling, iterate vs deterministic only) → per-candidate gates (`oracle_integrity, oracle_parity, heldout_divergence, workload_divergence[NEW], causal_attribution[NEW revert-remeasure], benchmark[deterministic], no_regression[widened: +RSS/alloc/binary/compile]`, `optimization_scope[NEW structural]`) → merge winners + record losers → wall-clock confirmation on merged build (quiesced, pinned, 30 reps, CI must exclude zero + sign-agree) → stop on gain < threshold / max 6 / no candidates / spend guard → gated PGO → gated BOLT → allocator-as-candidate.
**Retrospective capture (§17, centralized — DB canonical, git execution-only):** every attempt writes one row with `model, prompt_version, guidance_version` (pinned at Stage-2 entry), `proposal_text` (verbatim why), `technique/bound/tier/ceiling`, measured gains/divergence/CI/attribution/RSS/alloc or `outcome/gate/delta`, `tokens_spent`, **`parent_sha + patch_text`** (unified diff at grade time, same transaction as gate result; `''` only for `rejected_at_proposal`; 512 KiB cap, overflow → truncated flag + artifact ref). Cross-repo review = filter `(technique,bound,tier)` across `run_id → repo_url`, read `proposal_text + patch_text` with no checkout. `guidance_revisions` log + `guidance/optimize.md` versioning + offline `rustsmith learn` ship as P2 (needs ≥3 runs history).
**Acceptance:** SPEC_STAGE2 §15 items 13–18 + 26 (contract + measured floor + ≥2 rounds or round-1 no-ceiling with arithmetic + per-row bound/tier/ceiling/attribution/RSS/alloc/provenance + CIs agree + `failed_optimizations` non-empty) + adversarial 19–25 (fixture-keyed cache, tuned constant, benchmark tamper, phantom gain, RSS-for-speed, structural special-case, dead-path removal — each caught by its named gate).

## M6 — Stage 3 harvest + reporting

**Crates:** `harvest` (classification) + `report` (md/html/json) + `provenance` gate on every artifact.
**Classification per `optimizations` row:** `language_independent` → patch in original language + benchmark proving gain there; `module_local` → optional PyO3 accelerator (dispatch shim, pure-Python fallback, maturin + cibuildwheel config); `port_only` → documented reason to take the full port. Rank suggestions by expected gain / review burden.
**Output (per fork repo):** Rust crates + `RUSTSMITH_REPORT.md` (parity, divergence, unsafe locations, per-round deltas, end-to-end speedup, unported modules + why, consequential decisions, tokens + cache hit rate, ranked suggestions w/ paragraph each, negative-results section from `failed_optimizations`) + `rustsmith-report.html` (flame graphs before/after, round charts, unit DAG with status) + `rustsmith-report.json` (same numbers — all three must agree) + `suggestions/{README,patches/,accelerators/}`.
**CLI full surface:** `run [--stage recon|mirror|optimize|harvest]`, `status`, `report --run-id`, `halt` (kill switch), `resume` (non-tamper halts only), `audit` (replay JSONL), `learn` (Stage-2 retrospective → guidance diff).
**Acceptance (quoted):** "produce at least one `language_independent` patch that applies cleanly to the original repo and measurably improves it in the original language; confirm every artifact carries preserved attribution."

## Cross-cutting (do once, use everywhere)

* **Store schema evolution:** SPEC §6 base (`runs, units, gate_results, decisions, optimizations`) → SPEC_STAGE2 §13 deltas (bound/tier/ceiling/visible/heldout/divergence/instrument/CI/attribution/RSS/alloc + model/prompt/guidance/proposal/tokens + `parent_sha/patch_text`) + `failed_optimizations` + `rounds` + `guidance_revisions`. `heldout_*` columns host-only (never mounted). One `store.db` across runs — it *is* the cross-repo retrospective.
* **Config** (`config/default.toml`, SPEC §12 + SPEC_STAGE2 §13): `[run]`, `[oracle]`, `[gates]`, `[optimize + .heldout/.regression/.final]`, `[measure]`, `[workload]`, `[models]` (swappable, never hardcoded), `[privacy]` (`allow_training_tier_on_private_repos=false` enforced in code).
* **Containers** (`containers/`): run image (repo + toolchains) vs grading image (ephemeral, no net, clean oracle) — M0 proves the split, M1 adds worktrees/cgroups/mutex.
* **Prompts** (`prompts/`, one file per role, versioned; version logged per turn) + `guidance/optimize.md` (`opt-guidance vN`, pinned per run, evolved only via `learn` + human approval).
* **Docs:** `SPEC*.md` = contract (frozen per milestone, quote acceptance verbatim); `IMPLEMENTATION_M0.md` = M0 detail; this file = map; `docs/adr/` = every spec deviation in 5 lines.
