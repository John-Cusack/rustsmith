# rs-candidate-finder research: how the 10 ports were selected

Date: 2026-10-13. Sources: `git log -S`, README board history, SPEC.md §16,
`crates/rustsmith-cli/data/repo-content.json`, `docs/adr/`.

## What the old process was

Single manual triage, landed as part of commit `ff19517` (ADR-008, 2026-09-21).
No script, no tool, no re-runnable query — one engineer ranked packages by hand
and pasted the result into the README "POC board" as 7 KEEP rows. Ports then
landed roughly in KEEP order:

| KEEP # | Package | Port PR |
|---|---|---|
| 1 | packaging | #17 (oracle 62483/62483) |
| 2 | charset-normalizer | #9 (oracle 297/297) |
| 3 | python-dateutil (engine, then fuzzy/iso parse path per ADR-015) | #18, #19 |
| 4 | pyyaml | #56 (oracle 2608/2608) |
| 5 | markdown-it-py | #23 (ADR-017 data pack) |
| 6 | python-multipart | #24 |
| 7 | pyparsing | #33 |

Plus `crc` and `strsimpy`, which were never triaged — they are the SPEC.md §16
end-to-end fixtures ("small pure-Python package <5,000 lines, comprehensive
test suite, CPU-bound hot paths, no existing Rust core"), chosen before the
triage to prove the pipeline.

The selection rule, quoted verbatim from the original board text:

> Method: PyPI dl/mo (pypistats, mirrors included) × CPU headroom × text-gap;
> dependents via ecosyste.ms (libraries.io blocked); nearest-crate search on
> crates.io/lib.rs. KEEP = CPU-bound with no mature full-feature Rust equivalent.

In practice three filters, applied in order:

1. **Downloads.** Rank by monthly downloads, mirrors included.
2. **CPU-hotness.** Manual read of the source; each KEEP row names the hot
   path (`version.py::parse`, `md.py::mess_ratio`, `scanner.py`, …).
   Data-only bundles (certifi, pytz), trivial packages (iniconfig), thin
   dispatch (typing-inspection), and already-Cython hot paths (pandas) were
   cut here.
3. **Nearest-crate gap.** Manual crates.io/lib.rs search; each row names the
   nearest crate(s) plus why each falls short (versions-only, subset, stale,
   async-only, code-first vs runtime grammar…). Mature full-feature
   equivalents (jinja2→minijinja, jsonschema→`jsonschema` crate,
   beautifulsoup4→scraper/html5ever) were cut here — i.e. old path (a),
   adopt/wrap, was a *rejection* filter, never recorded as its own output.

The "Deliberately excluded" list carrying these rejections was deleted in
`5c71563` (#41, Ported-packages showcase); current README keeps only KEEP
rows plus the ported/parity-graded tables. Exclusion rationale now lives
nowhere but git history.

## What data it used

- Download counts: pypistats (BigQuery `pypi-public-data`, mirrors included).
  Numbers frozen in the README table, **no query date or snapshot pinned**.
- Dependent counts: ecosyste.ms API (libraries.io was blocked).
- Alternative verdicts: hand-searched crates.io/lib.rs links with prose gaps.
- Hot paths: hand-read function names, recorded per row, never measured
  (no profiling numbers anywhere in the triage).

## Where it lived

- `README.md` POC board: the only selection artifact (method one-liner +
  ranked table with dl/mo, dependents, hot path, nearest-crate+gap, thesis).
- `crates/rustsmith-cli/data/repo-content.json`: per-package *execution*
  data (probes, workloads, porting rules, held-out descriptors) — keyed by
  package name, data-not-code per `repo_content.rs`. This is what "adding a
  target" means today (cf. ADR-017: markdown-it-py needed only a data entry).
- `mirror/<package>/`: reference-port templates consumed by the mirror worker.
- `docs/adr/`: per-port deviations (e.g. ADR-015 dateutil full-parse scope).
- `SPEC.md` §16: fixture-selection rule (<5k lines, suite, CPU-bound, no
  Rust core) — the only written selection criterion besides the board line.
- Candidate scripts: **none exist**. `grep` for pypistats/hugovk/BigQuery
  hits only the README line; `scripts/` holds just `rustsmith-seat.sh`.

## Implications for the finder (phase 2)

1. **Reproducible download source.** Old numbers are unpinned pypistats.
   Finder input: [hugovk/top-pypi-packages](https://hugovk.github.io/top-pypi-packages/top-pypi-packages-30-days.json)
   (daily pinned JSON, offline-reproducible) as primary, with the
   [pypistats API](https://pypistats.org/api/) as the refresh path. Same
   PyPI-download lineage the old triage read (hugovk now serves ClickHouse,
   pypistats BigQuery; mirror-traffic inclusion may differ — ranking is what
   the tool consumes, never absolutes), now pinned and dated via
   `last_update`. Dependents stay on ecosyste.ms (live lookup, labeled
   as-point-in-time; note the API moved to `packages.ecosyste.ms`, the old
   `ecosyste.ms/api/v1` package paths 404).
2. **No `tools/` layout exists.** Repo convention is a library crate in
   `crates/` + subcommand module in `crates/rustsmith-cli/src/` (hand-rolled
   dispatch in `main.rs`, `flag()`/`has_flag()` helpers) + shell acceptance
   in `tests/m*_acceptance.sh`. The finder follows that: new
   `crates/rustsmith-candidate-finder/` library, `candidates` subcommand
   module, `tests/candidates_acceptance.sh`. (Brief's "tools/" = this.)
3. **Path (a) becomes output, not filter.** Old process discarded
   has-alternative packages; the finder records them with crate link + why.
4. **Path (c) has a precedent.** Stage-3 harvest already classifies accepted
   optimizations as `language_independent` / `module_local` (PyO3
   accelerator) / `port_only` — the upstream-contribution fit signal reuses
   that vocabulary: a package whose win is one hot module with a stable API
   boundary fits (c); a win spread across the whole interpreter-facing
   surface fits (b).
5. **Heuristic labeling.** Old hot-path claims were unmeasured reads. Per
   the brief, every finder signal is either measured (downloads, dependents,
   file layout, C-extension presence, pure-Python LOC) or labeled ESTIMATE.
6. New-behavior surface gets a short ADR per AGENTS.md (`docs/adr/`).
