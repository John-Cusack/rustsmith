# ADR-045: Elmer 8-lane production setup (repo, partition, shared cache, bench sampling)

Date: 2026-10-14. Setup task only: partitions the remaining campaign scope and
provisions shared infrastructure. Merge path (rs-elmer-spike) and lane dispatch
are explicitly out of scope — dispatch happens separately after this lands.

## Context

Elmer stands at 22/1184 units (~1.9%) after two weeks single-lane. The campaign
scope is ≈1184 units (`specs/elmer-campaign/plan.md` wall-time math: ~4.9 days
single-lane grade time); T001–T007 are green with Splice22 proven (AGREE 22/22
on the matc slice). Frozen inventory re-measured from `dag.json`
(elmer-work/recon): 3025 units, 61 `out_of_scope`, 1781 vendored
(`mathlibs/` + `umfpack/` — one more than the plan's 1587+193=1780; frozen
data rules), 26 done `matc/` slice units. Remaining: **1157 units**.

## Decision 1: port repo `John-Cusack/elmer-rust`

- URL: https://github.com/John-Cusack/elmer-rust (public, created 2026-10-14).
- License: GPL-2.0-or-later (`LICENSE` = upstream GPL-2.0 text). Upstream mixes
  GPL-2.0 and LGPL-2.x: the ElmerSolver main library (`fem/src/*.F90`), `matc`
  and `fhutiter` are LGPL; ElmerGUI, ElmerGrid and most solver modules are GPL
  (upstream `license_texts/ElmerLicensePolicy.md`). LGPL parts ported here are
  conveyed under GPL-2.0-or-later, which the LGPL permits; GPL parts stay under
  GPL terms. This matches the upstream requirement (modified code distributed
  under the same license as the original).
- `NOTICE` attributes CSC – IT Center for Science et al., pins upstream
  `9f6af2f85`, and cites the Elmer publication.
- Seed commit (LICENSE + NOTICE + README with per-lane crate layout) is on
  `main`. Port code never lands in rustsmith main (repo-early rule).

## Decision 2: 8-lane partition (`data/elmer-lanes/lanes.json`)

Generator `data/elmer-lanes/generate.py` (rerunnable against frozen `dag.json`);
one row per lane: lane id, shape, unit list (id + lang + est_min), totals,
`est_basis`, `bench_sample_unit` (null until dispatch).

| lane | shape | units | est total |
|---|---|---|---|
| lane-leaf | matc-like | 60 (`meshgen2d/`) | 300 min |
| lane-grid-io | io-boundary | 166 (`elmergrid/` + `fhutiter/` + `ElmerWorkflows/` + `misc/`) | 899 min |
| lane-fem-a | foreign-shape | 168 (`fem/` minus `src/modules/`) | 1209 min |
| lane-fem-b | foreign-shape | 184 (`fem/src/modules/`) | 1472 min |
| lane-ice | foreign-shape | 184 (`elmerice/`) | 1445 min |
| lane-gui-a | interactive | 124 (`ElmerGUI/netgen/`, sorted-id half 1) | 1240 min |
| lane-gui-b | interactive | 123 (`ElmerGUI/netgen/`, sorted-id half 2) | 1230 min |
| lane-gui-c | interactive | 148 (GUI shell: `Application/` + `PythonQt/` + GUI-`matc/` + logger + tester) | 1480 min |

Cost model: C/C++/Python 5 min, Fortran 8 min, interactive flat 10 min.
**Every per-unit number is an estimate** (`est_basis: estimate` on all rows):
the only measurement is matc-class ~270 s/unit (plan.md); Fortran 8 min is the
core-class ~480 s analog; interactive 10 min is unmeasured Qt weight. Recorded
as estimates per the brief; lanes re-freeze real costs at dispatch.

Balance: lanes are subsystem-pure (disjoint ownership trivially holds), so the
300–1480 min spread is structural — small subsystems make small lanes. Total
≈9275 min (~6.4 days single-lane grade time, model porting effort excluded as
in plan.md). lane-leaf doubles as the pipeline-validation lane (smallest,
proven shape); with the ≤3-concurrent-run cap, small lanes fill wave gaps.

Observations for dispatch (not decisions here): `ElmerGUI/netgen/` (247 units)
is bundled third-party mesher code — a link-instead-of-port re-scope candidate,
but T001 exclusion covers only `mathlibs/` + `umfpack/`, so it stays partitioned.
GUI lanes need a headless/Xvfb grading strategy at dispatch. `fhutiter` keeps its
honest `no differential probes` halt (spec clarification 3).

## Decision 3: shared build cache (measured, not guessed)

Probe 2026-10-14, 32-core host, cmake target `ElmerGrid` (C, leaf-size),
distinct source dirs simulating lanes, one shared cache:

| arm | wall | cache result |
|---|---|---|
| cold (sccache 0.13.0 / ccache 4.9.1) | ~11.0–11.7 s | 0/69 (populate) |
| lane-2 distinct dir, sccache warm | 11.28 s | 0/138 cumulative — no reuse |
| lane-2 distinct dir, ccache warm (default) | 11.46 s | 0 — no reuse |
| lane-B distinct dir, ccache + `CCACHE_BASEDIR=$LANE_ROOT` + sloppiness | 9.93 s | **69/69 new-call hits** |
| cross-root check (different BASEDIR per lane, same intra-lane layout) | — | 1 miss + 1 hit — sharing works |
| sccache + `gfortran` probe file | — | invocation ignored (0/0): no Fortran support |
| touch-one-file incremental / no-op rebuild | 0.42 s / 0.25 s | lower bound |

Standard: ccache with shared `CCACHE_DIR=/home/john/firstmate/data/elmer-lanes-cache`
(pre-created empty), per-lane `CCACHE_BASEDIR=$LANE_ROOT`,
`CCACHE_SLOPPINESS="file_macro include_file_mtime include_file_ctime time_macros pch_defines"`,
`cmake -DCMAKE_C_COMPILER_LAUNCHER=ccache -DCMAKE_CXX_COMPILER_LAUNCHER=ccache`.
Exact setup in `data/elmer-lanes/ccache.env` (source with `LANE_ROOT` set).
sccache rejected: zero cross-dir reuse (no path-independence knob in 0.13.0) and
it ignores Fortran — fatal for a 505-Fortran-unit campaign. Caveat: configure
time (~8–10 s) dominates small targets and is not cacheable; absolute savings
grow with target size (unmeasured for `elmersolver`-scale libs — lanes report).

## Decision 4: showcase-bench sampling rule

Timing-sensitive showcase benches (any workload behind a speedup claim) stay on
the 3-parallel measurement cap: they run only in a quiesced window with **at most
3 concurrent rustsmith runs** (`[run].max_parallel_runs`, `config/default.toml`),
and are **sampled once per lane, never per unit** — one representative unit per
lane, recorded in that lane's `bench_sample_unit` at dispatch, 8 samples per
campaign wave. Per-unit grades carry no timing claims. Rationale: bench timing
is host-noise-sensitive; 8 lane-samples bound measurement cost while covering
all three shapes; per-unit sampling (1157×) would serialize the campaign behind
measurement.

## Non-goals

Merge-path decision (spike runs separately), lane dispatch (separate task, not
spawned here), F77 `COMMON` design, K=3 investigation, any gate relaxation.
