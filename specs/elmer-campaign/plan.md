# Implementation Plan: Elmer full-port campaign

**Branch**: `fm/rs-speckit-pilot` | **Date**: 2026-10-08 | **Spec**: `specs/elmer-campaign/spec.md`

**Input**: `specs/elmer-campaign/spec.md`; sources `docs/IMPLEMENTATION_ELMER.md`,
`docs/ELMER_S3_PILOT.md`, ADR-027, ADR-028, S5 outcome (§7).

## Summary

Port the ≈1184 in-scope Elmer units (3025 frozen − 61 `out_of_scope` −
1587 `mathlibs` − 193 `umfpack` link-targets) through the proven mirror
loop — worker → substitute → integrity → differential → scoped oracle
parity → heldout — in subsystem slices across ≤3 parallel lanes, with
the ADR-027 ABI shim unblocking non-`BIND(C)` Fortran only after its
fixture proof, and the full-suite grade retained as the release
backstop (479/482, K=3 known upstream segfaults).

## Technical Context

**Language/Version**: Rust (ports) / C + Fortran (orig, gfortran 13.3.0) / Python spine (frozen)

**Primary Dependencies**: CMake/CTest spine, `rustsmith-agent` (worker contract),
`rustsmith-adapters` (`CmakeBridge::substitute`/`scaffold`), `rustsmith-oracle` + `rustsmith-sandbox` (executors)

**Storage**: run stores under `/tmp` (`--fork`/`--work`/`--store`/`--run-id`); repo holds only code + specs

**Testing**: `cargo test`, `cargo clippy -- -D warnings`, `tests/m*_acceptance.sh`, per-slice mirror runs, fault-injection equivalence protocol

**Target Platform**: Linux, 32-core measurement host (cmake 3.28.3, OpenMPI); Elmer @ `9f6af2f85`

**Project Type**: full-tree port campaign (phased slices, not a single run)

**Performance Goals**: per-unit grade ≈270 s leaf / ≈480 s core (ADR-028); slice wall budgets in § wall-time math

**Constraints**: ≤3 concurrent runs; oracle build serial per grade; no `--stage full` before pilot-green; Python spine byte-identical

**Scale/Scope**: ≈1184 port units; 482-test `-L quick` oracle baseline; F77 `COMMON` refused (whole-program coupling)

## Constitution Check

*GATE: Must pass before execution. Re-check per slice.*

- I (oracle correctness): scoped grading keeps frozen baseline/tolerance untouched; empty affected set falls back to full (never vacuous). PASS.
- II (measured-only): wall-time math below shows inputs + host; slice PRs carry graded-run logs. PASS.
- III (no silent scope): vendored exclusion + `COMMON` refusal are explicit scope marks with ADRs; shim relaxation per-shape PRs. PASS.
- IV (green + in-scope): merge owns build edits same-commit; honest halts bypass nothing. PASS.
- V (gate/agent split): no worker/model code in grade path; worker failures never graded. PASS.
- Limits: ≤3 lanes, isolated run dirs, no pushes, no publishing. PASS.

## Project Structure

### Documentation (this campaign)

```text
specs/
├── constitution.md          # Draft constitution (pending captain review)
└── elmer-campaign/
    ├── spec.md              # This campaign's specification
    ├── plan.md              # This file
    ├── research.md          # Phase 0: decided levers + rejected alternatives
    ├── tasks.md             # Phase 2: session-sized slices (worker-consumable)
    └── checklists/
        └── requirements.md  # Requirements quality gate
```

### Source Code (repository root)

Campaign touches, per slice brief (disjoint ownership per lane):

```text
crates/rustsmith-cli/src/mirror.rs      # scheduler, grading, merge (tool PRs only)
crates/rustsmith-adapters/              # CTest selection, substitute gate (tool PRs only)
crates/rustsmith-core/                  # TestCommand shape (tool PRs only)
crates/rustsmith-cli/data/repo-content.json  # porting rules + probes (data entries)
tests/fixtures/fortran_shim/            # ADR-027 proof fixtures (new)
rust/<crate>/                           # port sources (port PRs only)
```

Tool-fix PRs and port PRs are separate (Constitution III); the port PR
names the tool PR it depends on.

## Unit ordering

1. **Vendored exclusion first** (FR-001): land coverage/vendored
   exclusion from frozen diagnostics (`mathlibs/` 1587 + `umfpack/`
   193 as link targets, 61 `out_of_scope`). Port scope ≈1184.
2. **Pilot replay** (`matc`-class, 26 files): re-run the S3-proven
   slice green on the campaign harness (scoped grading now on); freeze
   per-unit cost/time inputs for the budget.
3. **Leaf expansion** (`elmergrid` 131 → `meshgen2d` 60 →
   `fhutiter`-class → `matc`-class remainder): self-contained I/O
   boundaries, leaf-first over the slice DAG. `fhutiter` carries
   porting rules but no differential probes (no binary — honest halt,
   not a fabricated path).
4. **`fem/` core** (508): near-full affected sets by construction;
   honestly pays ~full-suite price per unit (ADR-028 Consequences).
5. **F77 `COMMON`-block files last**: refused until a follow-up design
   lands (whole-program byte-layout coupling — ADR-027). Never forced.

## Lane parallelism + wall-time math

Lanes: ≤3 concurrent sessions, split by subsystem, one worktree +
branch per lane, disjoint file ownership (Constitution Limits).
Measured inputs (S3/S6, 32-core host):

| quantity | value |
|---|---|
| full quick oracle | ~390 s (482 tests) |
| scoped oracle, `matc`-class | ~180 s (~175/482 tests) |
| harness overhead / grade (substitute rebuild + probes + heldout) | ~60–120 s (use 90 s) |
| per-unit leaf (scoped) | ~270 s |
| per-unit core (near-full sets) | ~480 s |
| full build (configure ~8 s + build ~46 s, S5) | ~60 s |

Budget: assume ≈60% leaf-class (710 × 270 s ≈ 191,700 s) + ≈40%
core-class (474 × 480 s ≈ 227,500 s) ≈ 419,000 s ≈ **4.9 days
single-lane grade time**. Slice backstops add ~10 × 390 s ≈ 1 h.
Over 3 lanes with the per-grade oracle build as serial bottleneck:
**≈1.5–2.5 days wall for grading**. Excluded: model porting effort
(S3 interactive: 19/22 ≤2 iterations; solver-Fortran long tail
dominates and is unmeasured — each slice re-freezes it) and any
`COMMON` follow-up.

For comparison, the S3 pre-lever bound was 3025 × 480 s ≈ 17 days
single-lane (whole-suite grading, unscoped unit count). The two landed
levers — vendored exclusion (3025 → ≈1184 port units) and scoped
grading (~480 s → ~270 s leaf) — compound to roughly a 3.5× cut in
single-lane grade time before parallelism.

## Gates per phase

| phase | gate | fail means |
|---|---|---|
| worker | exit 0 + `{"tokens_in","tokens_out"}` JSON | worker failure — never graded |
| substitute | archive at `build/rust/lib<crate>.a`; `check_substitutable` allows shape | honest halt (missing archive / ABI refusal) |
| integrity | `test_count: 482`, hash/path checks | halt, report (ordering regression if `lib*.a` rule trips) |
| differential | recorded probes byte-identical (stdin-fed where recorded) | halt; corpus gap (cf. scanner-tables bug → extend probes) |
| oracle parity | scoped affected-set verdict == full verdict on faults | halt; selection bug (fix mapping, never shrink honestly) |
| heldout | divergence within tolerance | halt |
| release backstop | full-suite grade 479/482, failures exactly K=3 | any other failure fails the slice |

## Shim-build and grading sequencing

1. Build `tests/fixtures/fortran_shim/` proof first (no gate change):
   explicit-shape add + assumed-shape sum + F77 assumed-size scale
   (divergence 0), raw-pointer-into-assumed-shape (must diverge),
   zeroed-dtype descriptor (must fail fast), COMMON unit (pins
   refusal message). All four observed on the grade image's gfortran
   before any relaxation.
2. Relax `check_substitutable` per unit-shape, each as its own PR with
   graded-run numbers (Constitution III): file-granularity,
   COMMON-free, descriptor kinds pinned.
3. Grade shim ports through the unchanged chain (`ld -r` composition:
   Rust staticlib must define every removed `.o` global or the final
   link fails fast — honest, never silent). No link-line surgery, no
   `CMakeLists.txt` edits by workers.
4. Fix the known `fhutiter` example mangling (module procedure takes
   `__mod_MOD_proc`, not trailing-underscore F77 form) in the
   follow-up session, not here.

## Honest-halt handling

| halt | meaning | fix |
|---|---|---|
| `substitute: rust lib … does not exist` | worker produced no archive | write the port; never bypass |
| `exports non-BIND(C) Fortran …` | ABI gate fired correctly | whole-file shim port (§ sequencing); never force per-procedure |
| `no differential probes` | no built binary (e.g. `fhutiter`) | porting-rules only; never invent a probe path |
| `substitute: no built object` | no compilable claim (helpers/headers) | scheduler skip, not a port failure |
| `oracle_tamper … count_mismatch`, empty baseline | stale pre-discovery recon | re-run recon; never relax the gate |
| `matc` argv-only run hangs (exit 124) | reads past EOF | stdin-capable probe (S2 shape); never grade `matc` without it |
| `No rule to make target '…/build/rust/lib*.a'` | merged-archive ordering regression | report; `ensure_merged_archives` owns it |

## Done definition

The campaign is done when: every in-scope unit is graded green or
explicitly skipped with a recorded reason; every slice with a built
binary has a byte-identical combined-splice corpus; the full-suite
backstop passes at exactly 479/482 (K=3, any other failure fails);
all repo gates green (`cargo build --release`, `cargo test`,
`clippy -D warnings`, acceptance scripts); POC-board rows carry
measured numbers; each slice PR carries its graded-run evidence.
