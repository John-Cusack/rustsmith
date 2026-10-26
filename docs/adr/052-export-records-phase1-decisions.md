# ADR-052: export-records Phase 1 build decisions

Date: 2026-10-26
Status: accepted

## Context

The training-loop design report (`data/rs-training-loop/report.md`, §§2-3)
specifies `rustsmith export-records` emitting schema v1 JSONL with four
record kinds (`mirror_unit | optimization | failed_optimization |
harvest_item`), per-record validation, a write-time scrubber, `QUALITY.md`
regeneration, and a verify-sample script. Three points in the report do
not map 1:1 onto what the store actually records, and needed a decision
before code.

## Decision

1. **`harvest_item` deferred to Phase 3 (retro wiring).** The §3 tap list
   (`optimizations`, `failed_optimizations`, `units`, `gate_results`,
   `hunks`, `runs`, `rounds`, `decisions`, `guidance_revisions`, events,
   manifest, bench outputs, license inventory) contains no harvest-item
   source: suggestions are written at report time into the fork, and
   `provenance.upstream_review` is filled by the post-publish retro (§5),
   which is Phase 3 work. The exporter emits the three DB-backed kinds;
   `harvest_item` arrives with the retro backfill records (append-only,
   never mutating published JSONL).
2. **Mirror rows validate on `commit_sha`, not `parent_sha`/`patch_text`.**
   The store records no mirror parent or diff (mirror rows are tree
   states: `commit_sha` + gate verdicts; refs come from the `hunks`
   ledger when present). Requiring a patch would drop exactly the
   highest-value correctness records, so `mirror_unit` requires
   non-empty `commit_sha` and `oracle.total > 0`; the
   parent/patch rule applies to optimization rows (waived only for
   `rejected_at_proposal`, which never produced a diff).
3. **Mirror `provenance.model` is `"template"`.** The `units` table
   records no model or proposal text, and default-path mirrors are
   reference-template copies (report §7.4: label them honestly). All
   mirror records ship `model: "template"` and an empty proposal so
   trainers can weight or exclude them; model-authored ports will carry
   real provenance when the live-worker path writes it.
4. **`--since` filters on the run window.** Attempt rows carry no
   timestamps; runs do (`started_at`, `ended_at`). Incremental export
   keeps runs with `COALESCE(ended_at, started_at) >= since`. Exporting
   a mid-run store is caller discipline (finished runs only); the
   exporter does not filter on `runs.status`, whose values are
   lane-controlled and unreliable as a finished-marker.

## Verification

- `cargo test -p rustsmith-report` (scrubber secret/PII fixtures redact,
  clean fixtures byte-identical; validation unit tests for every rule
  above) — 26 green at time of writing.
- Fail-under proven: neutering one scrub pattern (`AKIA` → `AKIB`)
  turns `aws_key_and_token_prefixes_redact` red; restored after.
- Acceptance on three historical runs (see PR body): `export-records`
  runs, schema re-validated independently by `scripts/verify-sample.sh`,
  sampled patches re-apply, gate verdicts match `store.db`.

## Consequences

- Consumers must expect zero `harvest_item` rows in Phase 1 windows and
  must treat `model: "template"` mirror rows as output demonstrations,
  not reasoning traces.
- Any future mirror parent/diff capture (worktree commits at grade
  time) can tighten rule 2 without a schema change: the fields exist,
  they are empty for pre-Phase-1 rows.
- `--since` is run-granular: a long-running run re-exports all its rows
  when its window matches. Record IDs are content hashes, so re-export
  is idempotent downstream (dedupe on `record_id`).
