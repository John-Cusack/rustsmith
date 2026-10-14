# ADR-046: lane-ice landing repo + bench-sample choice

Date: 2026-10-14. Lane-ice close-out (5 graded / 179 recorded of 184 units).

## Context

The lane brief directs ports to land ONLY in `John-Cusack/elmer-rust` on
branch `elmer/lane-ice`, never rustsmith main. The landed precedent says
otherwise: every merged Elmer port so far (`matc` #58, `elmergrid` #67–69)
lives under `rust/*` in rustsmith, reviewed and merged there; `elmer-rust`
holds zero PRs and no lane branches. The delivery contract for this lane
(mode=direct-PR, ship `fm/rs-elmer-lane-ice`) likewise operates in rustsmith.

## Decision

Lane-ice follows the landed precedent: ports under `rust/elmerice/` shipped
as a rustsmith PR from `fm/rs-elmer-lane-ice`, owning only lane-ice paths
(`rust/elmerice/**`, this ADR). No rustsmith main push, no merge performed
here. If the campaign still wants the file set in `elmer-rust`, that move is
a maintainer/merge-lane copy of the identical tree — recorded here so the
conflict is explicit rather than silent.

`bench_sample_unit` is null in `lanes.json` (runbook: filled at dispatch;
never filled for this lane), so the lane chose its sample: `ExtrudeMesh`
serial extrusion of a 10201-node footprint at levels=5 (largest unit, only
C binary in the lane). Unquiesced triplicate, no timing claims; numbers in
`firstmate-data rs-elmer-lane-ice/timing.txt`.

## Consequences

- Graded-run evidence lives in the PR body + machine records in
  `firstmate-data rs-elmer-lane-ice/` (corpora, fuzz, gate log, records).
- UB-original divergences (info.dat uninit trio, DEM free-of-uninit exit,
  missing-file create-then-EOF) stay documented in-code and corpus-masked;
  none touch valid-input parity (34/34 corpus cases byte-identical).
