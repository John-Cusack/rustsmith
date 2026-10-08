# ADR-022: noop merges for whole-tree templates on flat layouts

Date: 2026-10-07
Status: accepted

## Context

The mirror worker task materializes the *whole* template tree for every
unit (`units::unit_task` copies `template.json` `files` unconditionally).
On src-layout ports (packaging, charset-normalizer, dateutil) every merge
still carries the per-unit `deletes` entry (the `src/<pkg>/...` original),
so the staged merge diff is never empty and review always has content.

python-multipart is the first *flat*-layout Python port: the template
overwrites the originals in place, so `delete_on_merge` is empty for every
unit. After the first unit merges, all later units stage an empty diff and
`merge_unit` failed closed with "empty diff, nothing to review" — halting a
fully-green run.

## Decision

`merge_unit` returns `false` (noop) when the staged merge diff is empty:
abort any `--no-commit` merge state and skip review+commit. The caller
records the unit pass with a `"noop": true` merge event. This is sound
because:

- grading (oracle parity, held-out divergence, differential) already passed
  in the worktree *before* the merge;
- every byte present in the fork entered through an earlier reviewed merge
  (a worker that produces nothing fails `maturin develop` before grading,
  since the extension manifest would be missing);
- `record_review_with_council` keeps its fail-closed empty-diff rejection;
  only `merge_unit` avoids calling it with nothing to review.

## Consequences

- Flat-layout ports merge unit 1 with review, units 2..N as audited noops.
- Src-layout behavior is unchanged (deletes keep every diff non-empty).
- Acceptance suites covering merge review (`m4`/`m5`) still exercise the
  non-empty path; the noop path is covered by the python-multipart
  graded run itself.
