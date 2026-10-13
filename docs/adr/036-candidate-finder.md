# ADR-036: rewrite-candidate finder tool (`rustsmith candidates`)

Date: 2026-10-13. Status: accepted.

## Context

Port targets were picked once, by hand, in the 2026-09-21 triage
(`ff19517`): pypistats downloads × CPU headroom × crates.io gap search,
pasted into the README POC board with unpinned numbers and no re-runnable
process (`data/rs-candidate-finder/research.md`). The captain now wants a
repeatable finder with a third path: upstream contribution (PyO3/maturin
speed core) alongside adopt/wrap and mirror-rewrite.

## Decision

- New library crate `rustsmith-candidate-finder` + `rustsmith candidates
  fetch|classify` subcommand (hand-rolled dispatch, `flag()` helpers — the
  `release.rs` pattern), shell acceptance `tests/candidates_acceptance.sh`
  over a pinned offline fixture. There is no `tools/` layout; `crates/` +
  CLI module is the repo convention.
- Download input: hugovk top-pypi-packages pinned snapshot (same download
  lineage as the old pypistats numbers, now dated via `last_update`);
  dependents from `packages.ecosyste.ms` (the old `ecosyste.ms/api/v1`
  paths 404 since the packages-service split).
- Transport is `curl` subprocesses, not a new HTTP crate: zero Cargo.lock
  churn, and every fetch failure degrades to documented absence (the
  package still classifies exactly once, payoff labeled downloads-only).
- Replacement verdicts come ONLY from curated
  `data/known-alternatives.json` (seeded from the README board + excluded
  list; verdicts `full|partial|upstream|noop`). Live crates.io hits are
  recorded per row as observed-but-UNJUDGED evidence — a human confirms
  before the table flips a path.
- Classification rules: table `full`/`noop` → (a), table `upstream` → (c);
  measured sdist with BUILT native code (shipped native files plus a build
  declaration: `ext_modules`, Cython, maturin…) + ≥2000 LOC Python → (c,
  fit ESTIMATE); measured trivial surface (<1000 LOC, no keywords, no
  native) → (a) noop; else (b) with payoff ESTIMATE = downloads ×
  (1 + keyword hits). Layout counts cover shipped paths only
  (test/example/doc/fixture trees excluded: pygments' lexer corpora must
  not read as "upstream ships native code"); C files without a build read
  as data (pycparser's fake-libc headers). Sdist layout is the only
  measured hotness signal; everything else is labeled ESTIMATE, never data.
