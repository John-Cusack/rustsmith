# ADR-056: capture backfill for the parked lanes (gaps 1, 2)

Date: 2026-10-28. Follow-up to ADR-055: rustsmith#121 backfilled 31
complete joined records but dropped 24 lane rows with `oracle.total == 0`
(verification/probe evidence carries no parity counts). This records how
the missing parity evidence was recovered. No store schema, exporter, or
record-schema changes; no new evidence readers.

## Method

Per package (pycparser, tabulate, marshmallow, sqlparse, docutils, sympy):
re-ran the upstream suite and the port's own parity/bench suites against
the published `John-Cusack/<pkg>-rust` mirror head (builds and venvs in
`/home/john/firstmate/data/<lane>/scratch`, never `/tmp`), wrote the
measured counts into formats the existing readers parse, re-graded the
parked units, joined the existing OMP sessions, re-exported. Nothing was
invented: every count below was measured on 2026-10-28 (raw logs in each
lane's `scratch/`, evidence in each lane's `capture/`).

## Evidence mapping (all existing readers)

- Upstream suite counts and port-test counts reuse the scoped-grade shape
  (`integrity.detail.test_count`, `parity.detail.failed`,
  `failed_tests: []`): it is exactly "suite ran green, N tests, 0 failed".
  `detail.reason` names the suite and any skips/xfails; raw pytest logs sit
  next to the evidence. `test_count` = number passed; skips/xfails are
  disclosed in `reason`, never counted as passes.
- Equivalence corpora use equiv-pair JSONL where the lane materializes both
  sides (tabulate: 400/400 kernel-vs-pure over the frozen seeded corpus).
  Where the lane's equivalence is in-process (port pytest suites), the
  scoped-grade file carries it.
- Bench rows use bench JSON re-measured with the lane's own two-track
  script against the release artifact (installed wheel / release example
  binary). Exception: a checkout debug `.so` measured ~1.3x on sqlparse;
  reran with the release wheel (7.3–13.4x, matching the committed table).
- sympy `bench/results.json` is reshaped 1:1 into bench JSON
  (`workload`->`track`, `sympy_wall_secs`->`pure_wall_s`, RSS kept); no
  reader change needed.
- Sub-second `--wall-secs` truncate to `wall_seconds: 0` in export (i64);
  that 0 is a measured sub-second run, not a zero-fill.

## Per-lane result (before -> after exportable records)

- pycparser 0 -> 3: upstream 451/451 (+500 subtests), port 14/14 (+20
  subtests), core 13/13, bench 16 tracks (parse 2.2–6.9x).
- tabulate 0 -> 3: upstream 318/318 (+1 intentional skip), equiv 400/400,
  bench 5 tracks (7.9–9.2x on a loaded box; committed 10.1–11.9x quiet).
- marshmallow 0 -> 3: upstream gate 1188/1188, port 8/8 (incl. seeded
  200-schema fuzz), bench 1 track (4.25x; committed 4.53x).
- sqlparse 0 -> 3: upstream 506 + 2 xfailed + 1 xpassed (stale, as
  upstream), port 19/19, bench 10 tracks (3.9–13.4x).
- sympy 0 -> 4: equiv 9/9, core 14/14, upstream kernel-subset baseline
  105/105 (run on unmodified upstream as the oracle baseline, labeled as
  such), bench 3 tracks (4.93x/8.65x/0.18x slower row honest).
- docutils (new lane) 0 -> 3: equiv corpus 7/7 at published head
  `8e53457`, core 17/17, bench 8 tracks (bulk 65.8x, ~14x RSS).

## Stays parked

- sympy `headroom:simplify` (no port exists: the probe verdict was
  `upstream_only`, 0.49 dispatch; only the polys kernel was ported). Zero
  gate rows, still drops with `oracle.total <= 0`. Reason recorded here
  and in the PR body; the unit is untouched.

## Replay

`scripts/capture-backfill.sh` replays the re-grade (delete parked rows,
grade, session, export) against the backfill stores. Run-once:
`grade-unit` is single-shot per unit by design.
