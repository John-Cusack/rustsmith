# ADR-031: elmergrid scope — metis is a vendored link target; repriced merge-path budget

Date: 2026-10-11. Lane: `fm/rs-elmer-lane2` (T008, elmergrid slices 1–2).
Status: accepted (slice evidence in the lane PR body).

## Context

The frozen recon counts 128 `elmergrid` units, but only 19 are Elmer's own
sources (8 `.c` + 10 `.h` + the ElmerGUI plugin shim). The other 109 are
`metis-5.1.0/` — a bundled third-party partitioner with its own
`LICENSE.txt`, `Changelog`, and upstream. T001's vendored exclusion
(`VENDORED_LINK_PREFIXES = ["mathlibs", "umfpack"]`, FR-001) does not name
it, so a scheduler reading prefixes literally would queue 109 LAPACK-class
wins before any Elmer code.

## Decision

1. **metis-5.1.0 is a link target, not port scope** — same standing as
   `mathlibs/`/`umfpack` (FR-001 intent). The lane ports the 8 own C units
   slice by slice; metis units are recorded `outside_scope` (vendored
   third-party) if ever scheduled. Extending `VENDORED_LINK_PREFIXES` is a
   one-line tool change and therefore a separate tool PR per repo rules —
   this ADR is the spec cover for it, not the change itself.
2. **ElmerGUI plugin shim** (`ElmerGUI/Application/plugins/elmergrid_api.*`)
   is out of scope: ElmerGUI builds with `WITH_ELMERGUI*=FALSE` in the
   campaign config (no objects, no tests), and the `.cpp` half needs the
   meshgen2d-style C++ surface decision first.
3. **Headers** (10 own `.h`) carry no compilable claim: scheduler skip
   (`substitute: no built object`), never ports.
4. Lane port scope is therefore **8 C units**, sliced small-first:
   slice 1 `egutils+egexport+fempre`, slice 2 `egextra`. The remaining four
   (`egparallel`, `egnative`, `egconvert`, `egmesh`) are bigger (4.9–10.3
   kLOC each) and three touch `USE_METIS` (dual-config, see §5).

## Merge-path confirmation (measured, non-matc data)

The spike verdict holds verbatim on real units: keep
`-Wl,--allow-multiple-definition` on EXE+SHARED flags; workspace-of-crates
(prelude rlib + per-unit staticlibs, `#[inline]` prelude fns); minimal
splice (unit + alloc-shim members) + one whole-archive provider (`egutils`,
config-clean and dual-target); `test -f` object enumeration. Two
refinements from lane experience:

- The prelude member defines **zero text symbols** (all helpers inline
  away), so the minimal splice is unit + alloc-shim only — no prelude
  member to place.
- A minimal splice **never links standalone**: without the provider in the
  tree the final link fails on `core::panicking::*` undefined refs
  (observed, honest, never silent). Per-unit singles therefore carry the
  provider; the nm gate's U-subset assertion is load-bearing, the link
  failure is its backstop.

## Repriced budget inputs (this host: 32-core, rustc 1.99.0, gfortran 13.3.0)

Machine costs per unit-batch (measured, Debug):

| step | measured |
|---|---|
| `cargo build`, whole workspace cold (5 crates, ~6 kLOC Rust) | ~0.3 s, one invocation |
| splice incl. nm gate (4 archives) | ~0.9 s |
| relink `ElmerGrid` (+ `elmergrid_core`) | ~0.4–0.9 s |
| binary corpus (9 cases: usage, creation, conversion, bigerror exit-1, 4 export formats, roundtrip) | ~2.4 s |
| oracle `ctest -R ElmerGrid` (10 tests, 8 quick) | ~5.8 s |
| full-tree build (all 351 targets) | ~41 s |
| archive disk | ~23 MB/unit whole vs 0.2–0.5 MB minimal |

What this reprices (and what it does not):

- **Merge-path disk bound: 27 GB → ~0.5 GB** for the campaign (minimal
  splice proven: 1184 × ~0.3 MB + providers). The verdict's ~12 MB estimate
  was Debug-info-free thinking; measured minimal objects are 0.2–0.5 MB in
  Debug, still a ~60× cut over whole-archive.
- **Machine grade-time is oracle-bound, not merge-bound.** Splice + relink
  + differential run in seconds per unit; the plan's 270 s/unit leaf figure
  stands until the mirror loop (scoped oracle + heldout + probes) is
  measured on an elmergrid unit — which is currently blocked on the honest
  halt below, not on the merge path.
- **Authoring is now measured for C-leaf units**: ~4.3 kLOC C → ~6.2 kLOC
  Rust across 4 units in one lane session (variadics, gotos-as-loops,
  file I/O, exit paths, unguarded-index OOB mirroring all included). The
  remaining elmergrid four total ~28.6 kLOC with `USE_METIS` config
  coupling → project 5–7 lane sessions, re-freezable per slice.
- **Blocked (honest halt, not worked around)**: `repo-content.json` has no
  `elmergrid` package entry, so mirror-loop grading halts `no differential
  probes` (fhutiter class). The follow-up tool-data PR adds argv-driven
  ElmerGrid probes shaped by this lane's `corpus.sh`; it is separate per
  repo rules and owned by a future session.

## Per-target config rule (checklist item 2, audited)

- `egutils`: no `USE_*` refs; one archive serves both objects (symbol sets
  verified identical across configs).
- `egexport`, `fempre`, `egextra`: exe-only; single config (`USE_METIS=1`).
- `egnative`, `egmesh`, `egconvert` (future slices): touch `USE_METIS`;
  default to **exe-only splice with the core object left pristine** (the
  core lib has no in-tree consumer while ElmerGUI is off), or build a
  second `--cfg`-gated archive per config. `egparallel`, `fempre`-class
  `USE_METIS` blocks are exe-only by target membership.
- `HAVE_UNISTD_H` is undefined tree-wide; `USE_MATC` is 0 in both default
  configs (`DISABLE_MATC` exe, `EG_PLUGIN` core), so MATC branches are dead
  everywhere and transcribed as omitted.

## Consequences

- Slice PRs carry per-unit costs + the table above; the campaign wall-time
  math keeps its oracle-bound figures with merge costs now measured.
- No gate, scheduler, or recon change in this lane (ports + ADR + task
  claim only). The `VENDORED_LINK_PREFIXES` metis extension and the
  `elmergrid` probe entry are named follow-ups, each its own PR.
