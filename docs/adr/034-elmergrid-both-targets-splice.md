# ADR-034: both-targets splice for config-conditional-free dual-target units

Date: 2026-10-11. Lane: `fm/rs-elmer-lane2` (T008, elmergrid final slice: egmesh).
Status: accepted (slice evidence in the lane PR body).

## Context

ADR-031's per-target config rule defaulted `egnative`/`egmesh`/`egconvert`
to **exe-only splice with the core object left pristine**, on the grounds
that the three touch `USE_METIS`. Slice-3 (`egparallel`+`egnative`, PR
#67) instead spliced `egnative` into **both** `ElmerGrid.dir` and
`elmergrid_core.dir` objects, and the final slice does the same for
`egconvert` and `egmesh`.

## Finding (verified by grep, not assumed)

- `egmesh.c`: no `USE_MATC`/`USE_METIS`/`EG_PLUGIN`/`DISABLE_MATC` refs.
  Only `#if DEBUG` / `#if 0`, both dead.
- `egconvert.c`: no `USE_*`/`EG_PLUGIN` refs. Single `#if 0` (dead).
- `egnative.c` (slice-3 precedent): MATC/METIS arms exist but agree under
  both default configs (`DISABLE_MATC` exe, `EG_PLUGIN` core).

So for `egmesh`/`egconvert` there is no configuration divergence at all:
one archive is bit-identical in behavior under both targets.

## Decision

A dual-target unit whose sources carry **zero live config conditionals**
is spliced into both target objects from the same archive (minimal splice
+ provider, same nm gate). The exe-only default remains for units with
live per-config divergence. This keeps the tree consistent (no stale-C
core object beside a Rust exe object) at zero extra archive cost.

## Consequences

- `splice3.sh` splices `egmesh`/`egconvert` into both objects; nm gate
  unchanged (zero port-level dups across all eight lane archives).
- No gate, scheduler, or recon change (ports + ADR + task claim only).
