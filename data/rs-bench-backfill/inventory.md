# Bench backfill inventory (Phase 1 — report, never code)

Source of truth checked: `README.md` POC board + Ported-packages section (HEAD `b43fa1c`),
every `mirror/<pkg>/` tree, `docs/adr/`, `data/rs-upstream-lane-1/report.md`.
No bench code was written or run for this file.

## Headline: measured/total = 3/10 packages with any measured figure; 0/10 with machine-written rows in-mirror

| Package | Board figure | Provenance | Harness in `mirror/<pkg>/` | Results rows in-mirror | Verdict |
|---|---|---|---|---|---|
| python-dateutil | 5.6x fuzzy-noisy / 3.8x strict-parse / 1.7x iso (§5); 1.6–4.2x rrule (ADR-021) | lane reports (`rs-dateutil-fuzzy` §5, re-audit V2), other machines | YES parser/iso only (`bench_parser/bench.py` + 5 corpora + `WORKLOAD.md` + `MANIFEST.sha256`) | none | measured-elsewhere, stale in-mirror; re-measure owed; rrule harness missing |
| pyyaml | 1.9x safe_load / 1.9x scan, 0.93x dump (§5) | `data/rs-upstream-lane-1/report.md` §8 + upstream PR body (other machine) | none | none | measured-elsewhere; full backfill owed (harness + re-measure + rows) |
| pygments | 9.8x regex_opt / 1.05x highlight / 1.00x lex-only | `mirror/pygments/bench/highlight.py`, measured 2026-10-13, figures hand-copied into `mirror/pygments/README.md` prose table | YES (`bench/highlight.py` + 2 corpora) | none (README table is hand-edited prose, no JSON) | measured, freshest; harden to spec (track labels, crossing overhead row, machine rows) |
| crc | not benchmarked (explicit) | — | none | none | missing; backfill owed |
| packaging | not benchmarked (explicit) | — | none | none | missing; backfill owed |
| charset-normalizer | not benchmarked (explicit) | — | none | none | missing; backfill owed |
| strsimpy | not benchmarked (explicit) | — | none | none | missing; backfill owed |
| markdown-it-py | not benchmarked (explicit) | — | none | none | missing; backfill owed |
| python-multipart | not benchmarked (explicit) | — | none | none | missing; backfill owed |
| pyparsing | not benchmarked (explicit) | — | none | none | missing; backfill owed |

Excluded by spec: Elmer (method work, not a package), `mirror/Mini` (test fixture),
ledger/candidates/publish.

## Detail per package

- **dateutil.** `bench_parser/` is the spec model: two-track (A representative log-ingest
  mix + control, B showcase adversarial fuzzy ceiling, C edge, D 2000-line bulk),
  median+MAD, Tukey flags, 3% noise floor, rep-interleaved, release-profile gate,
  per-row child-process RSS + tracemalloc with the native-alloc blind spot disclosed.
  But: no results file was ever written beside it, the §5 board figures come from the
  lane run on another box, and the ADR-021 rrule queries (count ~4x, after/before ~4x,
  between ~2x, sets ~2.5–4x, iteration unchanged) have no harness at all.
- **pyyaml.** Nothing in-mirror. Figures live in `data/rs-upstream-lane-1/report.md` §8:
  three-way `safe_load` pure 90.5ms / Rust 45.9ms / libyaml C 7.4ms (hence scan-only
  scope), corpus sweep pure 7.95s / 0.186MB vs Rust 2.11s / 0.350MB = 3.8x wall,
  tiny-doc 0.85x regression disclosed. Repro commands + versions are in the upstream
  PR body ([yaml/pyyaml#976](https://github.com/yaml/pyyaml/pull/976)), not in this repo.
- **pygments.** `bench/highlight.py` (seam-toggle A/B, 15 interleaved reps, medians,
  RSS both arms) + `corpus_py.txt` (110KB) + `corpus_escape.txt` (70KB); README table
  cites CPython 3.12/Linux x86_64 2026-10-13 with RSS + tracemalloc rows and an
  explicit unmeasured list. Gaps vs the backfill bar: workloads not labeled
  representative/showcase, no per-crossing overhead row (only an import-cost note),
  corpus distribution undescribed, figures hand-copied (no `results.json`).
- **The seven `not benchmarked` rows** (crc, packaging, charset-normalizer, strsimpy,
  markdown-it-py, python-multipart, pyparsing) are honest, not guesses — confirmed:
  no `bench/` dir, no `[[bench]]`/criterion target, no `*bench*.py` in any of them;
  the only `*bench*` hits repo-wide are the dateutil/pygments trees above.
  Board hot paths exist for the five KEEP rows (packaging, charset-normalizer,
  markdown-it-py, python-multipart, pyparsing); crc/strsimpy hot paths come from
  their ported surface (Phase 2 to pin down per package).

## Cross-cutting gaps (all packages)

1. **No machine-written rows anywhere.** Every board figure is hand-edited prose.
   No `results.json`/CSV beside any harness; no render script.
2. **No retro-batch update path in-repo.** The spec's "same update path as the
   retro-batch lane" has no artifact here (no script, no `retro-batch` reference
   anywhere). Phase 2 defines it: `mirror/<pkg>/bench/results.json` written by the
   driver + board rows copied 1:1 from those rows with provenance
   (host/commit/command) recorded in the JSON and `report.md`.
3. **Memory first-class only in 2/10.** dateutil harness + pygments driver report
   RSS both sides; tracemalloc-with-blind-spot disclosed in both. The rest: nothing.
4. **Per-crossing overhead disclosed in 0/10 as a measured row.** (pyyaml lane prose
   discloses +0.16MB transient and the tiny-doc regression; not a bench row.)
5. **One-machine re-measurement owed for all cited figures** before they can be
   called current: none of the 3 measured packages has in-mirror results from this box.

## Phase 2 work list (derived, not yet started)

Per package (representative + showcase, workload-labeled, RSS + allocs both sides,
crossing-overhead row, `results.json` in `mirror/<pkg>/bench/`, board row from rows):
dateutil (re-run parser/iso harness here + add rrule-query harness), pyyaml
(new harness: scan representative + safe_load showcase + dump no-regression row),
pygments (label tracks, add crossing row + rows file), crc, packaging,
charset-normalizer, strsimpy, markdown-it-py, python-multipart, pyparsing
(new harness each). Anything unmeasurable becomes an explicit unmeasured-with-reason
row. Hygiene: ≤3 bench procs, interleaved medians, one box + versions recorded.
