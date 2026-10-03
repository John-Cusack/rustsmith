# IMPLEMENTATION — Full Spec Build Plan

**Contracts:** `SPEC.md` v1 (all §§) + `SPEC_STAGE2.md` (replaces SPEC §9 Stage 2).
**Spec commit:** `035b947` (M0 build base; `SPEC.md` + `SPEC_STAGE2.md` first committed in `47f9aec`, unchanged since). Every milestone quotes its acceptance verbatim.
**Order:** M0 → M1 → M2 → M3 → M4 → M5 → M6 (SPEC §15) → M7 → M8 → M9 (post-spec, same discipline) → polyglot spine (ADR-008) → Elmer track. No skipping; each milestone's acceptance must pass before the next starts. Detail docs (build depth — signatures, slices, scripts, traps): `IMPLEMENTATION_M0.md` → … → `IMPLEMENTATION_M7.md` → `IMPLEMENTATION_M8M9.md` → `docs/IMPLEMENTATION_POLYGLOT.md` → `docs/IMPLEMENTATION_ELMER.md`. This file is the map; those files are the build orders. Hand an LLM one build order plus the two spec files and it has everything for that milestone. Paste-in session prompts that drove the builds: `BUILD_PROMPT.md` (M0–M6), `BUILD_M7.md`, `BUILD_M8M9.md`.
**Status (re-verified 2026-10-03 on this branch, merged with main `e816848` / REWORK):** `m0`–`m9` and `release_acceptance.sh` green; `m0`/`m3` scripts updated per ADR-009; main regressions fixed in `ee91e40` (held-out exit 1 counts as a measurement per ADR-010). Polyglot spine landed `ff19517`; Elmer track in progress (CTest mirror loop proven on a `BIND(C)` fixture; pilot blocked on brief S1; no acceptance script yet).

**Fixtures (pinned in `config/fixture.toml`):** primary `Nicoretti/crc` @ `4e65ac4` — pure-Python CRC, BSD-2-Clause, ~750 lines (`src/crc/_crc.py` 715), 80 tests + 28 subtests, 0 skipped/0 xfailed, pytest-only, split invocation (`pytest test/unit` + `pytest test/integration`; `test/bench/` excluded from oracle; src-layout needs `pip install -e .` or `PYTHONPATH=src`). Known M6 win: slice-by-8/16 (ships byte-at-a-time). Secondary `luozhouyang/python-string-similarity` @ `115acaa` (MIT, ~19 modules) — M3 DAG check ONLY, since crc is single-module and its DAG is vacuous; M7 promotes it to a second oracle fixture (full SHA `115acaacf926b41a15664bd34e763d074682bda3`, flat setuptools layout, `pytest -q` → 18 passed). First non-Python subject (not a pinned fixture): Elmer FEM `release-26.2.1 @ a19504a` (Fortran+C+C++, CMake+CTest; ADR-007/008).

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

## M7 — Unattended end-to-end run (SPEC §16)

**Goal:** `rustsmith run <repo>` with no human input: clone → recon → mirror → optimize → report, halt-safe, on crc; mirror parity on strsimpy (a second fixture proves the pipeline is not crc-shaped).
**Build:** full-pipeline `cmd_run` (no `--stage` or `--stage full`; `--stage recon` keeps the exact M0 path so `m0_acceptance.sh` stays green) + `run_report` extracted from `cmd_report`; `mirror/<pkg>/template.json` (`files`, `delete_on_merge`, `orig_source`) replaces hardcoded crc paths; `mirror/strsimpy` PyO3 reference port (single ext + `sys.modules` submodule aliases, 0 `unsafe`); per-fixture recon (invocation, host-only held-out, `WORKLOAD.md`, strsimpy `PORTING.md` ≥40 rules).
**New halts:** whole-repo `!integrity.passed` → `oracle_tamper <files>` + tamper event; whole-repo divergence over threshold → `heldout_divergence`. Per-unit divergence still fails only the unit.
**Live plants (`run --plant-live`):** `test-edit` (frozen test edited in a scratch worktree → real `oracle_integrity` fail → halt before mirror); `hardcode` (visible-input-only answers patched in after mirror → parity passes, held-out fails → halt before optimize). Graded through the real path; worker self-report is never evidence.
**Acceptance (`tests/m7_acceptance.sh`):** SPEC §16 items 1–12 on crc (unattended exit 0, 80/80 in fork venv, divergence <5pp recomputed not trusted, manifest verify green, 0 `unsafe`, clippy + miri clean, rounds ≥2 with a stopping-rule stop and ≥1 merge above floor, md/json/html agree, ≥1 classified suggestion, audit replayable, both live plants halt with the right `halt_reason`) + item 13: same `run` on strsimpy → 18/18, divergence <5pp, clippy + miri clean, zero merges OK, any halt = FAIL.
**Superseded since:** fixture dispatch (`FixtureKind`, `recon/fixture.json`, ADR-004) was replaced by the polyglot spine (ADR-008); behavior now reads `recon/facts.json`. The `template.json` mechanism survives.
**Detail:** `IMPLEMENTATION_M7.md` (slices 1–8, signatures, traps); session prompt `BUILD_M7.md`. Landed `602e840` … `e6fe1a3`.

## M8 — Live agents (worker + seat commands)

**Goal:** replace the M1/M2 stubs with a shell-command interface so a model (or a deterministic script) can back workers and seats. No provider literals outside `config/default.toml` + tests, no network calls from the harness, no credentials.
**Worker:** `RUSTSMITH_WORKER_CMD` — `sh -c` with prompt on stdin, cwd = worktree, env scrubbed; stdout `{"tokens_in":N,"tokens_out":M}` (+ optional `note`); non-JSON or nonzero exit = worker failure, never graded. `Agent::spawn_worker`, `build_worker_prompt` (stable prefix + `PORTING.md` + unit bundle), `WORKER_PROMPT_VERSION = "worker-cmd-v1"`, `Store::set_unit_tokens`. Unset = existing stub, offline green.
**Seats:** `RUSTSMITH_SEAT_CMD_{ARCHITECT,VERIFIER,PERFORMANCE,SCOPE}` → `WorkerSeatDriver`; the seat sees question + artifact, never other seats' positions; stdout `{"stance":"approve"|"reject","reasoning":"..."}`. Identities come from env or config only.
**Rule:** usage JSON feeds token accounting only; pass/fail comes from graded gates.
**Acceptance (`tests/m8_acceptance.sh`):** crc baseline 80+28 else STOP; worker-probe with a fake script records tokens > 0, prompt contains the unit id, gate detail `prompt_version == worker-cmd-v1` (never `m1-stub-v1`); seat-probe with two disagreeing fake seats preserves minority reasoning verbatim and records `architect_tiebreak` with justification; gates/oracle still free of agent/council refs.
**Detail:** `IMPLEMENTATION_M8M9.md` §§0–3 (env surface, slices 3–4); session prompt `BUILD_M8M9.md`.

## M9 — Deferred slices

**Slices:** `run-batch --repo A,B` (sequential, isolated fork/work dirs, one shared store, distinct run-ids; ADR-005 defers concurrency) · `run|run-batch --config PATH` (TOML over `config/default.toml`; every override echoed into `optimize-report.json` `"config"`) · HTML report gains inline-SVG `#round-gains` + `#unit-dag` table from `store.db` rows (ADR-006 defers M6's flame graphs: no stack-sample source) · Round 0 apply (`run_round0_apply`: one serialized worker per approved representation finding through `grade_candidate`, recorded merged or in `failed_optimizations`) · finals harness (`decide_final` pure accept/reject; missing `llvm-profdata`/`llvm-bolt` → honest refusal naming the tool) · `learn propose` / `learn apply --human NAME --proposal FILE` (writes `guidance_revisions`, pins `guidance_version` on later rows; lands the P2 deferred from M5) · generated held-outs (`heldout::generate_from_manifest`, seeded from manifest sha256, disjoint inputs: empty/singleton/max/unicode/shifted sizes; host-only).
**Acceptance (`tests/m9_acceptance.sh`):** baselines (crc 80+28, strsimpy 18) else STOP; run-batch over 2 local repos → 2 done runs in one store; `max_rounds=1` override changes the report and is echoed; HTML svg + table agree with JSON; a `round==0` row on crc; finals refusal names the tools + `cargo test decide_final` green; learn propose→apply writes a revision and pins the version; generated held-outs green on the pristine original and halt the M7 hardcode plant on `heldout_divergence`.
**Detail:** `IMPLEMENTATION_M8M9.md` (slices 5–12, signatures, traps). Landed `e7def7f`, `25f28ba`, `8133ab9`.

## Polyglot spine (ADR-008) — Elmer prerequisite

**Why:** every execution path spawned `python3 -m pytest` / maturin / py-spy directly. Elmer needs N languages per repo, and `src/` must carry zero repo names.
**Shape:** parse per language (`Frontend`s: Python, Fortran, C/C++), decide per repo (one spine: `TestRunner` + `BuildBridge` + `Profiler`), assemble per repo (`CompositeAdapter`); stages touch only the composite. Core contracts: `TestCommand{cwd,launcher,timeout,collect}`, `RunOutput`, `Outcome`, `ObservableSpec`, Manifest v2 (`runner, languages, prepare, invocation, config_hash, observables`) with a v1 reader, parameterized `ImageSpec`; `UnitId = <lang>:<repo-rel source>[#symbol]`.
**Landed:** tracks A–I in `ff19517` (`FixtureKind` deleted, flows read `recon/facts.json`, `CtestRunner` + `CmakeBridge` + Fortran/C++ frontends); probe-first recon for CMake repos in `27bd494`. Python-fixture output stays byte-identical.
**Detail:** `docs/adr/008-polyglot-composite-adapter.md` (accepted; supersedes 004 + 007), review `docs/adr/007-polyglot-composite-adapter.review.md`, build order `docs/IMPLEMENTATION_POLYGLOT.md` (tracks A→I + integration order).

## Elmer track — Fortran/C/C++ mirror at scale (in progress)

**Subject:** Elmer FEM `release-26.2.1 @ a19504a`, the first non-Python repo and the stress test for the polyglot spine. Claimed payoff is safety/maintainability only, no speedup claims.
**Ground truth (measured 2026-09-22):** 3025 file-granularity units (`fortran` 2094, `c` 600, `cxx` 320, `python` 11); frozen per unit: `exports` (3019), `out_of_scope` (61 vendored `contrib/`), `diagnostics` (2653). Baseline `test_count: 482` from `ctest -N -L quick` on a configure-only recon build (the label scoping is load-bearing, else every grade trips `count_mismatch`). 19 Fortran files (<1%) carry `BIND(C)`, so ~99% of ports are whole-file behind an ABI shim.
**Worker contract (normative):** interface per M8; read `.bundles/<unit>/`; write `rust/<crate>/` (staticlib, name from `scaffold_crate_name`); build `build/rust/lib<crate>.a` yourself; export original linkage names; only `BIND(C)`-clean units substitute per procedure; never edit `CMakeLists.txt`, tests, or other units (the merge owns build edits, in the same commit as the deletion).
**Landed:** CTest grade spine + `ld -r` object splice + build-aware merge + discovered baselines (`0b08995` … `c5f3a86`; loop proven on a throwaway `BIND(C)` fixture, `mirror: 1/1 passed divergence=0.0000`); File API target resolution; shared pristine build per run; frozen coarsened file-unit DAG; merge-safe tamper hashing (`CMakeLists.txt` hashes test-defining lines only).
**Scale-up order:** vendored exclusion first (`mathlibs/` 1587 + `umfpack/` 193 are link targets, not scope) → pilot `fhutiter` (17) or `matc` (26), measure per-unit cost → `elmergrid` → `meshgen2d` → `fem/` core, F77 `COMMON`-block files last.
**Open (session briefs; one worktree + branch each):**

| Brief | Goal | Needs |
|---|---|---|
| S1 | Scheduler scope-skip: `dag.json` `out_of_scope` + `RUSTSMITH_SCOPE` prefix allowlist → `skipped` (deps count as satisfied) | — (unblocks pilot) |
| S2 | Stdin-capable probes (`TestCommand` stdin; `matc` `1+2` → `         3`) | — |
| S3 | Real model behind `RUSTSMITH_WORKER_CMD`; pilot slice green; full-port budget extrapolated | S1 (+ S2 for `matc`) |
| S4 | ABI shim design ADR (mangled names, descriptors, assumed- vs explicit-shape; `COMMON` stays refused) | — |
| S5 | Full Elmer build on this host (compile DB, coverage, build time) | — |

**Hard rules:** never `--stage full` on Elmer before a pilot reports green costs; never relax the ABI gate (`CmakeBridge::check_substitutable`) without the S4 design + a fixture proof; isolate `--fork/--work/--store/--run-id` under `/tmp`; Python-spine output stays byte-identical (suite 132/0); never push.
**Exit (S3 acceptance, quoted):** "`fhutiter` slice (or stdin-ready `matc`) fully green in mirror with measured per-unit wall time; report extrapolates the full-port budget. Every gate honest; nothing forced."
**Detail:** `docs/IMPLEMENTATION_ELMER.md` (§2 worker contract, §4 landed/open, §5 honest-halt catalog, §7 briefs S1–S5).

## Cross-cutting (do once, use everywhere)

* **Store schema evolution:** SPEC §6 base (`runs, units, gate_results, decisions, optimizations`) → SPEC_STAGE2 §13 deltas (bound/tier/ceiling/visible/heldout/divergence/instrument/CI/attribution/RSS/alloc + model/prompt/guidance/proposal/tokens + `parent_sha/patch_text`) + `failed_optimizations` + `rounds` + `guidance_revisions` (written by M9 `learn apply`) + M8 per-unit `tokens_in/out`. `heldout_*` columns host-only (never mounted). One `store.db` across runs — it *is* the cross-repo retrospective.
* **Config** (`config/default.toml`, SPEC §12 + SPEC_STAGE2 §13): `[run]`, `[oracle]`, `[gates]`, `[optimize + .heldout/.regression/.final]`, `[measure]`, `[workload]`, `[models]` (swappable, never hardcoded), `[privacy]` (`allow_training_tier_on_private_repos=false` enforced in code). Per-run override: `--config PATH` (M9). Env: `RUSTSMITH_WORKER_CMD`, `RUSTSMITH_SEAT_CMD_*` (M8); `RUSTSMITH_SCOPE` planned (Elmer S1).
* **Containers** (`containers/`): run image (repo + toolchains) vs grading image (ephemeral, no net, clean oracle) — M0 proves the split, M1 adds worktrees/cgroups/mutex, the polyglot spine parameterizes images via `ImageSpec` (Python vs gcc + cmake toolchains).
* **Prompts** (`prompts/`, one file per role, versioned; version logged per turn) + `guidance/optimize.md` (`opt-guidance vN`, pinned per run, evolved only via `learn` + human approval).
* **Docs:** `SPEC*.md` = contract (frozen per milestone, quote acceptance verbatim); `IMPLEMENTATION_M0.md` … `IMPLEMENTATION_M8M9.md` + `docs/IMPLEMENTATION_{POLYGLOT,ELMER}.md` = build orders; `BUILD_*.md` = paste-in session prompts; this file = map; `docs/adr/` = every spec deviation in 5 lines (001–006, 009, 010; 008 is the full polyglot design and supersedes 004 + 007).
