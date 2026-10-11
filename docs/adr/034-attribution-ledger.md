# ADR-034: Attribution ledger as a store.db hunks table (narrowed)

Date: 2026-10-11. Supersedes the sidecar design first drafted for this PR
(per-mirror SQLite DBs beside the code): per scope ruling, the ledger lives
in the existing `store.db`, mirror-only, with the shootout loop and
fleet-index automation deferred.

## Decision

- New `hunks` table in `store.db`, added by a forward-only
  `migrate_hunks()` following the `migrate_stage2()` pattern (CREATE IF NOT
  EXISTS; later changes append ALTERs, never rewrites):
  `run_id, unit_id, package, source, before_ref, after_ref,
  wall_delta_pct NULL, workload NULL, verdict, recorded_at`.
- `source`, not `technique`: mirror hunks are reference-template copies, so
  the row names the hunk origin (`reference-template`), never an
  optimization technique that was never applied.
- `wall_delta_pct`/`workload` are NULL when unmeasured -- never fake zeros.
  The mirror lane measures no perf delta at merge time, so mirror rows carry
  NULLs; the columns exist for the deferred shootout to fill.
- One `record_hunk()` call in the grade-to-merge transaction (`mirror.rs`,
  next to `set_unit_commit`): fork HEAD before/after as refs, fire only on a
  real (non-noop) merge. Identity columns validated non-empty; store I/O
  errors fail the run like every other store write (no silent gaps).
- `combine_stores(sources, out)` in `rustsmith-store` + thin
  `rustsmith combine-stores --stores <a,b> --out <merged>` CLI: runs merge
  OR IGNORE, hunks copy verbatim scoped by run_id, missing sources are
  errors. This is the fleet rollup for hunks; full fleet-index automation is
  deferred with the shootout.

## Non-goals (deferred, not dropped)

Shootout-loop loser rows (same-schema kills arrive with the shootout work),
fleet-index automation beyond combine-stores, e2e remeasurement.

## Consequences

- `tests/hunks_acceptance.sh`: mirror-only pilot (crc) asserting one hunk
  row per merged unit with NULL wall/workload, plus a two-store combine.
- `GradeResult`/optimize untouched: no wall/alloc fields, no sidecar flags.
