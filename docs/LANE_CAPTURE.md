# Lane capture: recording port lanes as training data

Contract for every agent-driven port lane (ADR-055). Today's lanes bypass
`run`/`grade`, so `store.db` holds zero rows for all current work and
`session_id` is null in 100% of records. This document is the fix: run the
commands below and the lane becomes exportable, joinable training records
via the existing `export-records` path.

Rule zero: **nothing here publishes.** Export + scrub review stays manual
and human-gated. Keep transcripts and raw data local.

## 1. Lane lifecycle

One lane = one directory with `store.db`, `lane.toml`, `sessions.csv`,
`events.jsonl` (next to the store) and a durable `scratch/` dir. Run, in
order:

```sh
LANE=/home/john/firstmate/data/rs-port-<name>
DB=$LANE/store.db

# 1. init: run row + lane.toml + sessions.csv + durable scratch (never /tmp)
rustsmith lane-capture init --store $DB --run rs-port-<name>-01 \
  --repo https://github.com/<org>/<port>-rust --lang python \
  --lane rs-port-<name> --package <name> \
  --upstream-license <MIT|BSD-3-Clause|...> \
  [--upstream-repo https://github.com/<org>/<upstream>] \
  [--upstream-commit <sha>] [--record-license <id>] \
  [--rights-note "<permission grant>"] [--scratch $LANE/scratch]

# 2. per graded unit: one row per unit, from already-measured evidence
rustsmith lane-capture grade-unit --store $DB --run rs-port-<name>-01 \
  --unit python:<module/path> --status passed --commit-sha <port-sha> \
  --evidence <equiv_rust.jsonl> --evidence-kind equiv-pair \
  --expected <equiv_pure.jsonl> [--wall-secs <s>] \
  [--hunk-before <upstream-ref>] [--package <name>]

# 3. after every grade: join key for transcripts + traces
rustsmith lane-capture session --sessions $LANE/sessions.csv \
  --run rs-port-<name>-01 --unit python:<module/path> \
  --session-id <omp-session-id> [--omp <transcript.jsonl>] [--phoenix <trace-id>]

# 4. failed / no_gain attempts are rows, not prose
rustsmith lane-capture record-failed --store $DB --run rs-port-<name>-01 \
  --hotspot <module> --outcome no_gain --detail-json '{...}' \
  [--patch-file <diff> --parent-sha <sha> --measured-delta <pct>]

# 5. close the run (sets ended_at: the run wall-span fallback)
rustsmith lane-capture finish --store $DB --run rs-port-<name>-01 --status passed
```

Then export (scrub runs at export, fail-loud):

```sh
rustsmith export-records --store $DB --out $LANE/records.jsonl \
  --session-map $LANE/sessions.csv --transcripts ~/.omp/agent/sessions \
  [--license-split $LANE/gpl.jsonl]
```

`--evidence` is repeatable (one `--evidence-kind` for all files, or one per
file positionally): a unit can combine evidence, e.g. a full grade plus its
scoped-grade twin, with `--wall-secs` per file.

Before the lane merges: evidence committed **or** copied to
`/home/john/firstmate/data/<lane>/` — worktree teardown drops uncommitted
proof (witnessed live). Builds/venvs live under the lane `scratch/`, never
`/tmp` (`init` refuses `/tmp` outright).

## 2. Files

`sessions.csv` — header `run,unit,session_id,omp,phoenix,tripwire`; one row
per graded unit. Empty fields are empty strings. Values must not contain
commas (the exporter splits naively, by design).

`lane.toml` — highest-priority export source (above `release.toml` and
recon `facts.json`):

```toml
lane = "rs-port-babel"
package = "babel"
upstream_repo = "https://github.com/python-babel/babel"
upstream_commit = ""
upstream_license = "BSD-3-Clause"
# record_license: explicit wins; else GPL upstream forces the upstream
# value; else MIT. rights_note appends to rights.attribution_text, e.g. a
# training-use permission grant (Elmer: "training-use permission obtained
# 2026-10-28 (captain)").
record_license = "MIT"
rights_note = ""
scratch = "/home/john/firstmate/data/rs-port-babel/scratch"
```

## 3. Evidence → gate mapping (`grade-unit --evidence-kind`)

| kind | file | gate rows written |
|---|---|---|
| `equiv-pair` (+`--expected`) | two `equiv/*.jsonl` sides joined on `key` | `oracle_parity`: passed=matched,total(+mismatched/missing); pass iff no mismatch/missing |
| `verification` | `verification.json` | `oracle_integrity`: stage pass counts; pass = top-level `passed` |
| `probe` | `probe.json` | none (fingerprint; printed to stderr for the lane log) |
| `scope` / `gate-verdicts` | `scope.json` / `gate-verdicts.jsonl` | none; the unit's verdict fills `spec.scope_verdict` → `review.scope_verdict` (refuse + `--status passed` is an error) |
| `bench` | `bench*.json` | `benchmark` + `oracle_parity` from checksum agreement (pass iff all tracks match) |
| `scoped-grade` | T011 `scoped-<unit>.json` | `oracle_integrity` (test_count) + `oracle_parity` (failed/failed_tests) |

Repeatable `--gate NAME:0|1:DETAIL_JSON` adds explicit gate rows
(`oracle_parity|oracle_integrity|benchmark|heldout_divergence|differential`).
`--wall-secs` lands in each written gate detail as `wall_secs` and becomes
`provenance.wall_seconds` at export (max across the record's gates, else the
run span when `ended_at` is set, else null — never zero-filled).

`grade-unit` is single-shot per unit (existing gate rows refuse). Statuses:
`passed` needs `--commit-sha` (export requires it); `failed` exports as
`gate_failed` when it carries gate rows; `parked` (evidence recorded, grade
deferred) and `abandoned` (scope-refused) export with their status as
outcome when they carry gate rows. Queued/running/gated units never export.
Rows with `oracle.total == 0` drop loudly at export — honest missing signal,
not a silent gap. Backfill runs are left open (no `finish`): backfill
`ended_at` would measure the replay script's seconds, not the lane effort.

Hunks: `grade-unit` records one `hunks` row (`source = "lane-port"`) on
passed units when a real `before_ref` exists (`--hunk-before`, else the
lane's upstream commit). Absent means no hunk, never a fake ref.

## 4. Provenance joins at export

- `--session-map` joins `(run,unit)` → `session_id` + `trace_ids`
  (omp/phoenix/tripwire). Absent map: nulls, as before.
- `--transcripts <dir>` indexes OMP session JSONL (`message.usage` with
  input/output and `cost.total` on assistant messages; dominant model by
  output tokens). On a session hit, the transcript's model + summed tokens
  replace the store's `model`/`tokens_spent` (mirror `"template"` included)
  and fill `provenance.cost_usd`. Without a transcript the store values
  stand. Transcript spend is session-level: every record joined to the same
  session carries the session totals (effort lineage, not per-unit cost).
- `review.scope_verdict` comes from `spec.scope_verdict`; `review` is
  otherwise unchanged. `upstream_review` stays null until Phase-3 retro.

## 5. Rights

`record_license` rule (§2), GPL detection = case-insensitive `GPL`
substring. Without `--license-split`, GPL rows ship **inline** (Elmer is
permitted training data per the captain 2026-10-28; the grant lives in
`rights.attribution_text` via `rights_note`). With `--license-split PATH`,
GPL rows go to that file; QUALITY.md notes the split either way. License
inventory is unchanged: export still drops rows with an empty
`upstream_license`.

## 6. Backfill method (used 2026-10-28)

Lane stores live in `/home/john/firstmate/data/rs-lane-capture/backfill/`
(one dir per lane: `store.db`, `lane.toml`, `sessions.csv`,
`export/records.jsonl`). Per lane: `init` with upstream license + commit
where known; one `grade-unit` per ported unit from committed evidence (see
table); `session` rows from OMP transcript filenames (trailing component =
session id); `finish`; `export-records` with `--session-map` +
`--transcripts`. Session ids also cross-checked against Phoenix span
sessions where reachable. Counts reported in the PR body.

## 7. Follow-ups (not this contract)

FIM/diffusion export shape (render `(before,mask,after,verdict)` triples
offline from the mirror repos + equiv suites — no pipeline change needed);
transcript backup + Phoenix config (separate task).
