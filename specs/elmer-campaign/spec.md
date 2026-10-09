# Feature Specification: Elmer full-port campaign

**Feature Branch**: `fm/rs-speckit-pilot`

**Created**: 2026-10-08

**Status**: Draft

**Input**: `docs/IMPLEMENTATION_ELMER.md`, `docs/ELMER_S3_PILOT.md` §3
(3025 units, scoped-grading lever now landed), ADR-027 (ABI shim
design), ADR-028 (scoped per-unit grading).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Pilot slice green with measured costs (Priority: P1)

A port author runs the mirror loop over a leaf slice (`matc`-class, 26
files) and reaches the first real unit grade with every gate honest:
worker → substitute → integrity → differential → scoped oracle parity →
heldout. The session ends with measured per-unit wall time and a
full-port budget extrapolation.

**Why this priority**: No full-tree run is credible before a pilot
slice reports green costs (IMPLEMENTATION_ELMER.md §6: never
`--stage full` until then). S3 already proved this for `matc`
(preflight + splice22 + mirror green3); the campaign replays the
pattern per slice.

**Independent Test**: `RUSTSMITH_SCOPE=matc` mirror run grades each
scheduled unit; `splice<N>` combined-splice corpus is byte-identical;
run log shows per-unit wall seconds and the extrapolation inputs.

**Acceptance Scenarios**:

1. **Given** a clean worktree at the pinned Elmer checkout, **When**
   the pilot slice runs, **Then** every scheduled unit is graded (no
   scope halt) and the run reports per-unit wall time.
2. **Given** all slice ports individually green, **When** they are
   combined-spliced into one binary, **Then** the extended corpus
   (including scanner-sensitive cases) is byte-identical to pristine.

---

### User Story 2 - Leaf subsystems ported lane-parallel (Priority: P2)

Port authors work subsystem slices (`elmergrid` → `meshgen2d` →
`fhutiter`-class leaves, COMMON-free) in up to 3 parallel lanes, one
worktree + branch per lane, disjoint file ownership, each lane ending
green with measured costs.

**Why this priority**: Leaf C and BIND(C)-clean units are the proven
shape; they fund the budget numbers for the core while carrying the
lowest ABI risk.

**Independent Test**: Each lane's mirror run is green on its slice and
`cargo test` + `clippy -D warnings` + acceptance scripts pass on its
branch.

**Acceptance Scenarios**:

1. **Given** two lanes on disjoint subsystems, **When** both run
   concurrently, **Then** neither touches the other's files and both
   report green slices.
2. **Given** a helper/header unit with no compilable claim,
   **When** the scheduler meets it, **Then** it records `skipped`
   (`outside_scope`), never a grade halt.

---

### User Story 3 - Scoped per-unit grading holds the budget (Priority: P3)

Every per-unit grade runs only the affected-test set (ADR-028
selection + startup-smoke gate), falling back to the full suite when
the affected set is empty or the smoke trips. Whole-repo full-suite
grade is retained as the backstop before anything is releasable.

**Why this priority**: Whole-suite-per-unit grading costs ~480 s/unit
(≈17 days single-lane for 3025 units, S3 §3). Scoped grading is the
landed lever: ~270 s/unit on `matc`-class leaves (~1.8× end-to-end).

**Independent Test**: Fault-injection equivalence protocol (ADR-028
Proof): per-unit `(scoped_failures − K) == ((full_failures ∩ affected)
− K)` and `(full_failures − K) ⊆ affected`, with K the known upstream
failures.

**Acceptance Scenarios**:

1. **Given** a behavioral fault in a leaf unit, **When** graded both
   ways, **Then** scoped and full verdicts agree and no sensitive test
   is missed by selection.
2. **Given** a startup-breaking fault (definition parsing, shared list
   chaining), **When** graded, **Then** the smoke gate trips and the
   grade honestly pays full-suite price.

---

### User Story 4 - Non-BIND(C) Fortran ports behind the ABI shim (Priority: P3)

File-granularity Fortran ports substitute through the ADR-027 shim
(mangled `export_name`, constructed descriptors for assumed-shape
dummies) only for unit shapes already green in the fixture proof.
`COMMON`-coupled units stay refused; each gate relaxation is its own
PR with graded-run numbers.

**Why this priority**: ~99% of Elmer Fortran is non-`BIND(C)`; the
gate correctly refuses it today. Scale without the shim is capped at
the C/BIND(C) leaves.

**Independent Test**: `tests/fixtures/fortran_shim/` proof: explicit-
shape add + assumed-shape sum + F77 assumed-size scale grade to
divergence 0; raw-pointer-into-assumed-shape diverges (documents the
old refusal); zeroed-dtype descriptor fails fast; COMMON unit pins the
refusal message.

**Acceptance Scenarios**:

1. **Given** a COMMON-free file-granularity unit of a proven shape,
   **When** substituted, **Then** the final link resolves every
   removed `.o` global or fails fast on an undefined reference.
2. **Given** a `COMMON`-block unit, **When** graded, **Then**
   `check_substitutable` refuses with the pinned message — never a
   silent wrong-science link.

---

### Edge Cases

- Pristine-tree failures (S3 decision A: 3 `fem/tests` solver
  segfaults, SIGSEGV in `DefaultDirichletBCs`, predate the campaign):
  ports must introduce zero regressions beyond K=3; K is documented,
  never investigated inside a port session.
- `matc`-class binaries that read past EOF hang to the 600 s timeout
  (exit 124) by design — that halt stays honest, never silent.
- Merged-archive ordering regression (`No rule to make target
  '…/build/rust/lib*.a'`): report, don't work around
  (`ensure_merged_archives` owns it).
- Multi-unit Rust staticlibs collide on std/alloc monomorphizations at
  final link: combined splices link with
  `-Wl,--allow-multiple-definition` (dedup-safe); the merge path needs
  the same treatment or a single-workspace port layout.
- `CMAKE_RUNTIME_OUTPUT_DIRECTORY` changes relocate the `matc` probe
  binary: re-verify probe paths after any such change.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Campaign MUST exclude vendored numerics (`mathlibs/`,
  `umfpack/`) as link targets, not port scope, before porting any unit.
- **FR-002**: Scheduler MUST skip `out_of_scope` / `outside_scope`
  units with recorded reasons (no grade, no merge) and treat `skipped`
  deps as satisfied.
- **FR-003**: Every per-unit grade MUST run the ADR-028 scoped oracle
  (affected tests + startup-smoke gate) with honest fallback to the
  full suite; full-suite grade remains the release backstop.
- **FR-004**: Worker interface MUST stay `RUSTSMITH_WORKER_CMD`
  (stdin prompt, stdout `{"tokens_in","tokens_out"}` JSON, exit 0);
  worker failures MUST never be graded.
- **FR-005**: Ports MUST build `build/rust/lib<crate>.a` at the
  worker-contract path; a missing archive halts honestly pre-merge.
- **FR-006**: Non-`BIND(C)` Fortran MUST substitute only behind the
  ADR-027 shim for fixture-proven shapes; `COMMON`, `allocatable` /
  `pointer` dummies, character-length and `optional` hidden params stay
  refused.
- **FR-007**: Each campaign session MUST run in one worktree + branch
  with declared file ownership, isolated run dirs, measured costs, and
  no pushes.
- **FR-008**: Every session MUST end green on `cargo test` and
  `cargo clippy -- -D warnings` plus its brief's acceptance scripts,
  with graded-run evidence for any claimed number.
- **FR-009**: Every spec deviation MUST land a short ADR in
  `docs/adr/`; tool-fix and port changes MUST ship as separate PRs.

### Key Entities

- **Unit**: file-granularity port atom (3025 frozen: `fortran` 2094,
  `c` 600, `cxx` 320, `python` 11); carries frozen `exports`,
  `out_of_scope`, `diagnostics`.
- **Slice**: subsystem-scoped unit set scheduled via `RUSTSMITH_SCOPE`
  (e.g. `matc` 26, `fhutiter` 17 scheduled / 61 `out_of_scope` /
  remainder `outside_scope`).
- **Grade**: worker → substitute → integrity → differential → scoped
  oracle parity → heldout verdict chain per unit; full-suite grade is
  the release backstop.
- **Lane**: one session (worktree + branch + file ownership) running a
  slice; at most 3 concurrent (`[run].max_parallel_runs`).
- **Honest halt**: refusal to grade/merge with a named cause
  (missing archive, ABI refusal, vacuous probes, scope halt) — always
  fixed at source or scoped explicitly, never bypassed.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Pilot slice replays green: every scheduled unit graded,
  combined-splice corpus byte-identical, per-unit wall time reported
  (baseline: `matc` 22/22 preflight-green, SPLICE22 green, mirror
  green3 523 s wall incl. ~390 s oracle).
- **SC-002**: Port scope (≈1184 units after vendored + `out_of_scope`
  exclusion) completes at the plan's wall-time budget (see plan.md §
  wall-time math) with zero regressions beyond K=3 known upstream
  failures.
- **SC-003**: Scoped-grading equivalence holds per slice: identical
  verdicts to full grading on the fault protocol, no sensitive test
  missed (AGREE on every unit, as in the 22/22 `matc` proof).
- **SC-004**: Shim proof fixtures all observed (2 green + 2 negative +
  COMMON refusal) before any non-`BIND(C)` gate relaxation merges.
- **SC-005**: Every merged slice ships `cargo test` + `clippy
  -D warnings` + acceptance scripts green with graded-run evidence in
  the PR body; POC-board rows carry measured numbers only.

## Assumptions

- Host for wall-time math: 32-core, cmake 3.28.3, gfortran 13.3.0 +
  OpenMPI (S5/S3 measurement host); numbers re-freeze per host.
- Elmer checkout `release-26.2-641-g9f6af2f85`
  (`9f6af2f85`), NOT `release-26.2.1 @ a19504a`.
- Baseline `test_count: 482` (`ctest -N`, `-L quick` scoping
  load-bearing); full execution 479/482 with K=3 upstream segfaults.
- `WITH_ElmerIce=FALSE`, `WITH_ELMERGUI*=FALSE`; system BLAS/LAPACK
  linked, bundled sources skipped.
- gfortran descriptor layout pinned to gfortran 13.3.0; fixture
  re-dumps on version mismatch; only `BT_REAL` dtype measured so far.

## Clarifications

Decided 2026-10-08 from source docs (no captain input needed; all
grounded in landed measurements):

1. **Port scope**: 3025 − 61 `out_of_scope` − 1587 `mathlibs` − 193
   `umfpack` ≈ 1184 units. Vendored exclusion lands before any port
   (FR-001); the scheduler's first win is never LAPACK.
2. **Ordering**: pilot replay → `elmergrid` → `meshgen2d` → `fem/`
   core → F77 `COMMON`-block files last (refused until a follow-up
   design; whole-program coupling, ADR-027).
3. **`fhutiter` differential**: none — no built binary exists, and
   inventing a probe path would be false data. `fhutiter` grades keep
   the honest `no differential probes` halt; its value is porting-rule
   coverage, not grading.
4. **Lane cap**: 3 concurrent runs max (standing rule). Lanes split by
   subsystem with disjoint file ownership; the per-grade oracle build
   is the serial bottleneck (plan.md wall-time math).
5. **Done per slice**: all scheduled units graded green or explicitly
   skipped with reasons; combined-splice corpus green where a binary
   exists; repo gates green; evidence in the PR body.
6. **K=3 pristine failures** are campaign-external (decision A): the
   backstop passes at 479/482, and any deviation from exactly those 3
   fails the slice.
