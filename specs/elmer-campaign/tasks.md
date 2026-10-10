---
description: "Task list for the Elmer full-port campaign"
---

# Tasks: Elmer full-port campaign

**Input**: `specs/elmer-campaign/spec.md`, `plan.md`, `research.md`

**Prerequisites**: spec.md, plan.md, research.md (all in this directory; no data-model/contracts — no new data model or endpoints)

**Tests**: mirror runs + fault-injection equivalence are the tests; each slice task names its exact gate commands.

**Organization**: grouped by campaign user story; each slice is independently runnable and independently green.

## How a worker consumes a task slice

Each task below is self-contained: goal, file ownership, acceptance,
non-goals. A worker needs only the repo at the campaign base commit
plus `specs/elmer-campaign/{spec,plan,research}.md`, then:

1. Open ONE worktree + branch per task; own only the listed files.
2. Isolate run dirs (`--fork`/`--work`/`--store`/`--run-id` under
   `/tmp`, never `~/runs/*`). Never push.
3. Run the task's acceptance exactly; report measured wall seconds.
4. End green on `cargo test` + `cargo clippy -- -D warnings` (+ the
   brief's acceptance scripts); tool-fix and port changes ship as
   separate PRs, port PR naming its tool PR.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallel-safe (disjoint files, no dependencies)
- **[Story]**: US1–US4 from spec.md

## Phase 1: Setup (exclusion + baseline)

**Purpose**: shrink port scope before any port; freeze budget inputs

- [ ] T001 [US1] [claimed fm/rs-elmer-c1] Land vendored exclusion from frozen diagnostics in `crates/rustsmith-cli/src/mirror.rs` (`mathlibs/` 1587 + `umfpack/` 193 as link targets, 61 `out_of_scope` skipped with reasons); acceptance: `RUSTSMITH_SCOPE=matc` static expectation 26 scheduled / 61 `out_of_scope` / 2938 `outside_scope`, 2-unit fixture proof green, suite green. Non-goal: touching recon frozen shape.
- [ ] T002 [P] [US1] [claimed fm/rs-elmer-c2] Pin campaign baseline: Elmer @ `9f6af2f85`, `ctest -N` count 482, full execution 479/482 with exactly K=3 (`ConstantBCTemperature`, `ProfileBCTemperature`, `ProfileBCTemperatureRobin` segfaults); record log under `/tmp`, reference from slice PRs. Non-goal: investigating K (decision A, upstream).

**Checkpoint**: port scope ≈1184 units; baseline frozen.

---

## Phase 2: Foundational (blocks US4)

**Purpose**: shim proof before any non-`BIND(C)` relaxation

- [ ] T003 [US4] [claimed fm/rs-elmer-c3] Build `tests/fixtures/fortran_shim/` proof (orig Fortran + reference outputs, Rust ports): explicit-shape add + assumed-shape sum + F77 assumed-size scale grade to divergence 0; raw-pointer-into-assumed-shape MUST diverge; zeroed-dtype descriptor MUST fail fast; COMMON unit MUST pin the `check_substitutable` refusal message. Acceptance: all four observed on the grade image's gfortran, `gfortran --version` pinned. Non-goal: any gate relaxation.
- [ ] T004 [P] [US4] [claimed fm/rs-elmer-c4] Fix the `fhutiter` porting-rule mangling example in `crates/rustsmith-cli/data/repo-content.json` (module procedure → `__mod_MOD_proc` form; trailing underscore is F77 globals only). Acceptance: rule text matches ADR-027 §1 table; no code touched.

**Checkpoint**: shim proven callable; gate still refuses (no relaxation in this phase).

---

## Phase 3: US1 pilot replay (Priority: P1) 🎯 MVP

**Goal**: `matc`-class slice green on the campaign harness with scoped grading; budget inputs frozen.

- [ ] T005 [US1] [claimed fm/rs-elmer-c7, successor of fm/rs-elmer-c6 — adopted 685bdd4, fixed eval.c gate failure] Mirror run `RUSTSMITH_SCOPE=matc` (26 files): all scheduled units graded, no scope halt; per-unit wall seconds reported. Acceptance: run log + extrapolation recompute vs plan § wall-time math.
- [ ] T006 [P] [US1] [claimed fm/rs-elmer-c8] Combined-splice all slice archives into one `matc` binary; extended corpus (12 cases + paired-`rand` + 6 scanner cases `:`, `,`, `;`, `while`, reverse range) byte-identical (`SPLICE<N>_GREEN`). Link with `-Wl,--allow-multiple-definition`; acceptance: corpus green, note kept for merge-path decision.
- [ ] T007 [US1] Scoped-grading equivalence replay for the slice (ADR-028 fault protocol, one realistic fault/unit, both-ways grading): AGREE on every unit, `(full_failures − K) ⊆ affected`. Acceptance: per-unit table in PR body.

**Checkpoint**: pilot replay green; per-unit cost/time frozen for the budget.

---

## Phase 4: US2 leaf lanes (Priority: P2)

**Goal**: leaf subsystems green in parallel lanes.

- [ ] T008 [P] [US2] Lane `elmergrid` (131 units): mirror slice green, combined-splice corpus green where a binary exists, costs reported. Files: slice ports under `rust/<crate>/` only. Non-goal: `fem/` core.
- [ ] T009 [P] [US2] Lane `meshgen2d` (60 units): same acceptance as T008. Files: slice ports only (disjoint from T008).
- [ ] T010 [P] [US2] Lane `fhutiter` porting-rules coverage (17 units, whole-file non-`BIND(C)` per ADR-027 sequencing only where shape-proven; else refused): ports build + substitute where allowed, honest `no differential probes` halt kept. Files: slice ports only. Non-goal: inventing probe paths.
- [ ] T011 [P] [US2] Per-lane scoped-grading equivalence (fault protocol) for T008–T010 slices. Non-goal: changing selection code to accommodate weak ports.

**Checkpoint**: leaves green; lane-parallelism validated (disjoint ownership held).

---

## Phase 5: US3 scoped grading as backstop discipline (Priority: P3)

**Goal**: every slice ships with equivalence evidence; full suite stays the release gate.

- [ ] T012 [US3] Per-slice equivalence tables (extends T007/T011 pattern to any new slice): identical verdicts, no sensitive test missed; smoke-fallback slices honestly pay full price. Non-goal: batching/`ctest -j`/probe-primary (future sessions).
- [ ] T013 [US3] Full-suite backstop run per merged slice: exactly 479/482 with failures == K=3; any other failure fails the slice. Record `LastTest.log` location + wall seconds.

---

## Phase 6: US4 shim-gated Fortran (Priority: P3)

**Goal**: scale beyond the C leaves without ever relaxing the gate on unproven shapes.

- [ ] T014 [US4] Gate relaxation PR #1 (one unit shape: COMMON-free, file-granularity, descriptor kinds pinned) in `crates/rustsmith-adapters` (`check_substitutable` allowlist) with graded-run numbers. Non-goal: any second shape in the same PR.
- [ ] T015 [P] [US4] `fem/` core lane A (sub-subsystem split, disjoint files): shim ports graded through the unchanged chain; near-full affected sets expected (~480 s/unit). Non-goal: `COMMON` units (refused, recorded).
- [ ] T016 [P] [US4] `fem/` core lane B (disjoint split from T015): same acceptance.
- [ ] T017 [US4] Per-shape relaxation PRs for each further proven shape (one PR per shape, each with graded-run numbers). Non-goal: `COMMON` strategy (follow-up design).

**Checkpoint**: core green except explicitly refused `COMMON` units.

---

## Phase 7: Release evidence

- [ ] T018 Update POC-board row in `README.md` with measured numbers only (graded-run walls, per-unit costs, backstop 479/482+K); PR body carries run logs + equivalence tables. Non-goal: publishing (human-gated).
- [ ] T019 [P] Re-verify probe paths after any `CMAKE_RUNTIME_OUTPUT_DIRECTORY` change; re-run `tests/m*_acceptance.sh` (+ `tests/release_acceptance.sh` for release work), `cargo build --release`, full `cargo test`, `clippy -D warnings` green on the merge commit.

---

## Dependencies & Execution Order

- Phase 1 → all (scope + baseline freeze first).
- Phase 2 → Phase 6 (no shim relaxation before proof).
- Phase 3 → Phases 4–6 (pilot-green costs gate scale-up; never `--stage full` before).
- T008/T009/T010/T011 mutually [P] (disjoint files); T015/T016 mutually [P]; T014 → T015/T016 (shape allowlist first).
- Phase 7 after all desired slices (backstop + evidence last).

## Implementation Strategy

- MVP: T001 → T002 → T005 → T006 → T007 (pilot replay, the S3-proven path with scoped grading).
- Incremental: add one leaf lane at a time (each independently green + evidenced), then core lanes behind per-shape relaxations.
- Parallel team: ≤3 lanes max; one worktree + branch per lane; same-file edits never merge — split by subsystem.
