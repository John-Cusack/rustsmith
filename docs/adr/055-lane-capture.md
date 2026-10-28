# ADR-055: lane-capture training-record path (gaps 1, 2, 4–9, 11)

Date: 2026-10-28. Implements the rustsmith-side fixes from the
rs-data-audit gap table (§8) and recommendations (§9).

## Context

Port lanes (`rs-port-*`, `rs-elmer-*`) never invoke `run`/`grade`, so
`store.db` holds zero rows for all current work; no `sessions.csv` is ever
written, so `session_id` is null in 100% of records; evidence JSONs have no
exporter readers; cost/tokens live only in transcripts; `exporter_version`
is `+unknown`; `wall_seconds` is always null.

## Decisions

1. **No store schema changes.** No `session*` columns, no new tables.
   The session join stays file-level at export (`--session-map` CSV, new
   `--transcripts` dir index). Stores stay append-only and old readers keep
   working.
2. **New `lane-capture` CLI command** (not `grade --store`): `grade` stays a
   read-only probe. `lane-capture init | grade-unit | record-failed |
   session | finish` writes scoped `run`/`unit`/`gate_results`/`hunks` rows
   into a lane-local `store.db`, appends `(run,unit,session_id,omp,phoenix)`
   to `sessions.csv`, and refuses `--scratch` under `/tmp`.
3. **Evidence readers live in `rustsmith-report/src/evidence.rs`** as pure
   deterministic functions (equiv pairs, verification.json, probe.json,
   scope.json, gate-verdicts.jsonl, bench JSONs, scoped-grade JSON,
   OMP transcript index). The CLI reuses them; the exporter does not read
   evidence directly (store rows stay authoritative).
4. **Schema stays v1.** The only record change is additive-optional
   `provenance.cost_usd: Option<f64>`. Consumers that require v1 keys are
   unaffected.
5. **Scope verdicts travel in `units.spec_json`** (`scope_verdict`,
   `scope_evidence`) into `review.scope_verdict`. No new gate variant, no
   decision-row pollution (decisions attach run-wide to every unit).
6. **`wall_seconds`** = max `wall_secs` across the record's gate
   `detail_json`s (the lane helper records measured grade/bench wall time
   there), else the run span when `ended_at` is set and > 0, else null.
7. **Rights:** `record_license` defaults to `MIT` unless the upstream
   license mentions GPL (then the upstream value), overridable via
   `lane.toml`/`release.toml`. The exporter offers an opt-in
   `--license-split` sink for GPL rows; by default they ship inline.
   Amended 2026-10-28 per captain: Elmer (GPL-2.0-or-later upstream) is
   permitted training data and ships in the main dataset, with
   `rights.attribution_text` carrying a `rights_note` recording the
   training-use permission grant.
8. **Non-passed units export** (`failed` → `gate_failed`, `parked` /
   `abandoned` keep their status as outcome) when they carry gate rows;
   rows with `oracle.total == 0` still drop loudly. Queued/running/gated
   units never export (incomplete work, not training signal).
9. **Transcript provenance override:** when a session join hits, the OMP
   transcript's dominant model and summed usage replace the store's
   `model`/`tokens_spent` (mirror `"template"` included) and fill
   `cost_usd`. Without a transcript the store values stand.

## Follow-ups (not this change)

FIM/diffusion export shape (§6b note only); transcript backup + Phoenix
config (separate task); `--heldout` dead usage text on `grade` (untouched).
