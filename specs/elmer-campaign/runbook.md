# Elmer campaign runbook: 1.9% → 100%

**Date**: 2026-10-14 | **Scope**: ≈1184 port units (3025 frozen − 61
`out_of_scope` − 1781 vendored `mathlibs/` + `umfpack/` per landed
`lanes.json` inventory; plan.md § wall-time math uses 1587 + 193 = 1780 —
see §1 resolved note) | **Done so far**: 22 `matc/` units graded + 4
skipped-as-helpers (T005–T007), i.e. 22/1184 ≈ 1.9%. Remaining: **1157 units**.

**Authority order**: panel-ruling.md OVERRIDES the spike report where they
conflict. This runbook sequences execution only — it grades no units,
changes no merge verdict, spawns no lanes. Normative details live in the
sources; this file only routes: scope/splits/halts → `plan.md`;
requirements → `spec.md`; levers → `research.md`; slices → `tasks.md`;
merge procedure → spike report §6 as corrected by the panel ruling;
baseline → `baseline.md`.

**Standing pins** (re-stated so lanes never re-derive them): Elmer @
`9f6af2f85` (NOT `a19504a`); oracle `test_count: 482`, backstop exactly
479/482 with failures == K=3 (`ConstantBCTemperature`,
`ProfileBCTemperature`, `ProfileBCTemperatureRobin`); one pinned nightly
`1.100.0 feaadeeac 2026-09-19` for ALL lanes (stable 1.89.0 cannot build
the ports — `c_variadic` E0658; mixed toolchains void the byte-identity
argument); linker flag `-Wl,--allow-multiple-definition` on
**both** `CMAKE_EXE_LINKER_FLAGS` and `CMAKE_SHARED_LINKER_FLAGS`
(load-bearing at static exe links; 5 alloc-shim symbols); ≤3 concurrent
runs (`[run].max_parallel_runs`); port code lands in
`https://github.com/John-Cusack/elmer-rust` (public, GPL-2.0-or-later +
`NOTICE` attributing CSC, upstream pinned — ADR-045, landed as PR84),
never rustsmith main; tool-fix and port PRs separate, port PR naming its
tool PR. Shared pristine-build cache: ccache + `CCACHE_BASEDIR` +
sloppiness (69/69 TU hits cross-dir, 9.93 s; sccache rejected — ignores
gfortran; recipe in `data/elmer-lanes/ccache.env`, ADR-045).

## 1. Lane roster (8 grade + 1 merge)

Source of truth: `data/elmer-lanes/lanes.json` (landed PR84, generator
`data/elmer-lanes/generate.py` rerunnable against frozen `dag.json`;
inventory 3025 frozen − 61 `out_of_scope` − 1781 vendored − 26 done
`matc/` = **1157 units, 0 duplicates, 8 lanes**). Lane briefs cite
`lanes.json` unit IDs; the table below is a routing summary, verified
equal to the landed file this session (counts, shapes, estimates, and
per-lane language mixes all re-checked against `lanes.json`).

**Resolved at landing**: the partition counts vendored as 1781
(`mathlibs/` + `umfpack/`), one more than plan.md's 1780 (1587 + 193) —
the frozen data governs; plan.md arithmetic is the stale figure.

| lane | shape | units | langs | grade est |
|---|---|---|---|---|
| lane-leaf (`meshgen2d/` ×60) | matc-like | 60 | c 35, cxx 25 | 300 min |
| lane-grid-io (`elmergrid/` 126 + `fhutiter/` 17 + misc 19 + `ElmerWorkflows/` 4) | io-boundary | 166 | c 133, cxx 8, fortran 23, python 2 | 899 min |
| lane-fem-a (`fem/` excl. `modules/`) | foreign-shape | 168 | c 42, cxx 2, fortran 123, python 1 | 1209 min |
| lane-fem-b (`fem/src/modules/`) | foreign-shape | 184 | fortran 184 | 1472 min |
| lane-ice (`elmerice/`) | foreign-shape | 184 | c 2, fortran 175, python 7 | 1445 min |
| lane-gui-a (`ElmerGUI/netgen/`, sorted-id half 1) | interactive | 124 | c 4, cxx 120 | 1240 min |
| lane-gui-b (`ElmerGUI/netgen/`, sorted-id half 2) | interactive | 123 | cxx 123 | 1230 min |
| lane-gui-c (`ElmerGUI/` 142 + `ElmerGUIlogger/` 3 + `ElmerGUItester/` 3) | interactive | 148 | c 107, cxx 41 | 1480 min |
| **merge lane** | merge only | 0 (owns no port files) | — | gate time only |

Per-unit estimate basis (`lanes.json` cost_model — estimates, NOT
measured): C/C++/Python 5 min (anchored on measured matc-class ~270 s
leaf); Fortran 8 min (core-class ~480 s analog, ADR-028); interactive
10 min flat (unmeasured). Total ≈9275 min ≈ 6.4 d single-lane grade
time, model porting effort excluded (as in plan.md).

**Ownership**: each grade lane owns its elmer-rust subsystem path(s)
exclusively — same-file edits never merge; disjoint ownership is the
conflict avoidance mechanism (§3). lane-leaf doubles as
pipeline-validation lane. The merge lane owns merge tooling, the gate
runs, and the merged tree — never port files.

**Dispatch consumption of the partition (before any lane starts)**:
rerun `generate.py` against frozen `dag.json` and diff against the landed
`lanes.json` — any drift re-freezes the §4 budget before lanes start;
fill per-lane `bench_sample_unit` (null until dispatch, ADR-045 §4);
lane briefs cite `lanes.json` unit IDs, never the §1 summary table.

**Lane risks pre-registered**: meshgen2d is C++ (27 TUs, `Mesh2D` links
`c++` under the same flag) — Rust cannot provide C++-mangled symbols;
lane-leaf MUST scope the C-linkable surface first (spike §7.8). F77
`COMMON`-block files are refused everywhere until a follow-up design
lands (ADR-027) — recorded skips, never forced. `fhutiter`-class units
keep the honest `no differential probes` halt (no binary exists; value is
porting-rule coverage, spec clarification 3). First lane proves a real
multi-unit minimal batch incl. `libmatc.so` + exe with
missing-provider/member negative controls (panel correction 2), not just
the single-unit sample. `USE_METIS`-touching units need per-config
archives (ADR-031 rule); config-clean units may share one provider.
`test -f` every splice target — DependInfo-listed-but-missing objects
(e.g. phantom `elmergrid_core` `egutils.c.o`) are warnings, never errors.
No campaign delay for no_std / runtime redesign (panel correction 5).

## 2. Per-lane done-gate (all four, every lane)

A lane is done only when all four hold; anything else is an honest halt
(plan.md halt table — fix at source or scope explicitly, never bypass):

1. **Equivalence corpus green**: per-unit chain worker → substitute →
   integrity (`test_count: 482`) → differential → scoped oracle parity
   (fault protocol `(scoped_failures − K) == ((full_failures ∩
   affected) − K)`, `(full_failures − K) ⊆ affected`) → heldout; plus the
   lane combined-splice corpus byte-identical where a binary exists
   (`SPLICE<N>_GREEN`), covering I/O + error/exit paths with stderr
   compare, not just REPL values. EOF-overread hang parity (exit 124) is
   kept — nobody "fixes" it mid-lane. No-binary slices keep the honest
   `no differential probes` halt.
2. **Refined merge gate** (panel corrections 1–3, supersedes T-only audit):
   assert zero STRONG port-level dups across full replacement+native
   link inputs (`nm -g --defined-only`, all types, `NF==3`, toolchain
   classes filtered — Fortran `__<mod>_MOD_<proc>` exports are real code;
   weak libm dups are permanent, never asserted away); weak/toolchain
   dups need the byte-identity argument (same-toolchain member `cmp`
   identical); selective member extraction only (named `ar x` of unit +
   alloc-shim members — never materialize ~230 runtime members/unit);
   reuse unchanged symbol inventories across merges; minimal splice +
   exactly one `--whole-archive` provider object per final binary
   (minimal-only tree does not link — `core::panicking::*` + hidden
   `__rdl_alloc`, reproduced); disk quoted qualified — ~0.5 GB =
   assembled objects, NOT peak/retained (archives retained ≈ 27–34 GB).
3. **Machine rows to store.db**: attribution/bench/retro writes belong in
   the grading transaction (step-back ruling); lane boards render from
   machine-written rows via `combine-stores` rollup, never
   human-updated markdown (Ported-gate rule: grade-green rows missing
   install/bench figures list under honest second status, never inflated,
   never deleted).
4. **Timing sample rule**: per-unit wall seconds in every run log (budget
   re-freeze input); showcase benches run quiesced (≤3 concurrent) and
   sample ONCE per lane (8/wave), never per unit (ADR-045 §4, landed;
   `bench_sample_unit` filled at dispatch); bench gate
   advisory-with-coverage-target per step-back ruling.

Repo gates per lane: `cargo test` + `cargo clippy -- -D warnings` +
brief's acceptance scripts green; graded-run evidence in the PR body;
POC-board rows carry measured numbers only.

## 3. Merge cadence

**Rhythm**: merge per completed lane — when a grade lane passes §2, the
merge lane splices its archives into the merged tree and runs the gate
before accepting. No batching across incomplete lanes; no `--stage full`
before pilot-green costs gate scale-up (tasks.md dependencies).

**Who runs the gate**: the merge lane, on the merged tree, per merge:
strong-symbol gate re-run (toolchain drift is the only thing that breaks
the invariant) + relink of each affected binary (relink wall per binary,
not per unit; full-tree relink timing is lane-owned follow-up, currently
unmeasured — TBD) + corpus on affected binaries + backstop `ctest`
(T013: exactly 479/482, failures == K=3) + `cargo build --release`,
full `cargo test`, `clippy -D warnings`, `tests/m*_acceptance.sh` green
on the merge commit (T019).

**Conflict rule**: conflicts are errors, not resolutions. Disjoint
ownership (§1) means two lanes never touch the same file — a merge
conflict signals a partition violation: reject the merge, re-split
ownership, re-run the affected lane gate. Same-file edits never merge
(tasks.md). Merge owns build edits same-commit (Constitution IV); honest
halts bypass nothing.

## 4. Throughput budget

Two anchored estimates (inputs disagree — both cited, lanes re-freeze):

- plan.md wall-time math (measured S3/S6 inputs, 32-core host): ≈60%
  leaf-class (710 × ~270 s) + ≈40% core-class (474 × ~480 s) ≈ 419,000 s
  ≈ **4.9 d single-lane grade time**; slice backstops ~10 × 390 s ≈ 1 h;
  over 3 lanes with per-grade oracle build serial: **≈1.5–2.5 d wall**.
  Excludes model porting effort (solver-Fortran long tail unmeasured —
  each slice re-freezes) and any `COMMON` follow-up. Pre-lever bound was
  ~17 d (3025 × 480 s); landed levers (vendored exclusion + scoped
  grading) compound to ~3.5× before parallelism.
- `lanes.json` partition sum (estimate-basis per-unit rates, landed):
  ≈9275 min ≈ **6.4 d single-lane grade time**, model effort likewise excluded.

Merge overhead per unit is flat and tiny (`ld -r` <1 s/unit ≈ 20 min
serial worst case, parallel-safe; nm gate ~2 min linear scan; relink ~1 s
per binary; corpus ~1 s/case per binary; final link argv ~60–120 KB «
2 MB `ARG_MAX`, response files anyway). Full-tree relink wall: TBD,
lane-owned.

**Priced mortality line** (step-back ruling 3; lanes never block on
backend relaunch): observed evidence 1 explicit worker death
(rs-port-pyyaml) + 2 wedge-successions (Elmer c5/c6) over 2 days ≈
**1.5 incidents/day**. Budget carries
`expected_incidents × spin-up_cost` where spin-up is bounded by per-unit
grade costs (≈270 s leaf / ≈480 s core of lost grade time per incident,
plus re-dispatch overhead TBD per incident). At campaign scale (≈2–6 d
wall): ≈3–9 incidents expected → carry ≈0.5–1.5 h grade-time equivalent
plus re-dispatch — explicit line in each lane brief, re-frozen from
observed rate as the campaign proceeds.

## 5. Campaign complete (mechanical)

ALL of the following, verified by command output, not by reading:

- [ ] Every in-scope unit graded green or explicitly skipped with a
  recorded reason (26 done + 1157 remaining = 1183 units by landed
  inventory, ≈1184 scope per plan.md; `skipped` deps
  satisfy ready-check; `COMMON` refusals recorded, never forced).
- [ ] Merged tree links: every binary relinked with the flag +
  one-provider minimal splice; zero strong port-level dups on the final
  gate run.
- [ ] Every slice with a built binary holds a byte-identical
  combined-splice corpus; no-binary slices hold the recorded honest halt.
- [ ] Full-suite backstop exactly 479/482, failures == K=3, on the merged
  tree (`LastTest.log` location + wall seconds recorded).
- [ ] Equivalence: AGREE on every unit under the fault protocol, no
  sensitive test missed, smoke-fallback slices honestly paid full price.
- [ ] POC-board rows machine-rendered from the `combine-stores` rollup
  with measured numbers only (grade-green rows missing install/bench
  figures under honest second status per the Ported-gate rule).
- [ ] Repo gates green on the merge commit: `cargo build --release`,
  `cargo test`, `clippy -D warnings`, `tests/m*_acceptance.sh` (+
  `tests/release_acceptance.sh` for release work).
- [ ] Each slice PR carries graded-run evidence; every spec deviation
  carries a short ADR in `docs/adr/`.

## 6. Retro loop

Post-publish retro rule, adapted per lane: every grade lane closes with a
retro before teardown — what the lane taught (tool gaps, naming
surprises, boundary lessons, benchmark honesty notes). Firstmate curates
findings into (a) rustsmith SPEC/STAGE2 or tool PRs for mechanical fixes,
(b) the Optimization playbook for cross-lane lessons, (c)
`specs/constitution.md` amendments for standing-rule changes (captain
ratifies). Retro findings ship through the same PR + auto-merge rigor as
any other change; process improvements are never applied by hand to main.
Retro rows are written machine-side into store.db in the grading
transaction (step-back ruling 2), and each lane brief carries the retro
step in its done-gate.
