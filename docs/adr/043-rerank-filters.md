# ADR-043: learned exclusion/demotion filters for the candidate finder

Date: 2026-10-14. Status: accepted.

## Context

The first finder run (ADR-036, top 30, 2026-10-13) ranked `botocore` payoff #1
and kept `urllib3`/`requests`/`pygments` as clean mirrors. Since then five
measured lessons plus one standing rule say those rankings mislead:

1. **I/O-bound mainline** — yt-dlp lesson: network dominates, a mirror would
   tie. Upstream-probe only with measured CPU headroom.
2. **High-churn knowledge surface** — thousands of site extractors with
   nightly breakage: a mirror rots on contact (yt-dlp).
3. **AWS-owned** — boto3/botocore excluded standing.
4. **Mature native cores** — libyaml, numpy/cffi-style: upstream win unlikely
   without proven headroom.
5. **Shared-engine tie** — pygments lesson (ADR-038): the hot path already
   runs in C via `re` (bench W2 lex-only 1.00x); a full-engine port ties
   with itself.
6. **Saturated hot paths** — charset lesson: killed after 5 measured
   behavior-identical attempts, all neutral-or-slower (rs-upstream-lane-1
   kill-table, 2026-10-13). ~0% is not a win; do not renominate.

## Decision

- New curated data file
  `crates/rustsmith-candidate-finder/data/exclusion-filters.json`:
  `{package: [{filter, disposition, reason, provenance}]}`. `filter` is one
  of the six above; `disposition` is `exclude` | `demote` | `upstream_only`.
  Each row carries provenance: a measured artifact (kill-table, bench,
  sdist layout) or the captain-directed lesson, labeled in the open.
- `classify()` looks the table up and applies rule 6 after the ADR-036
  rules: `exclude` on a mirror flags it EXCLUDED (renders with the
  non-mirror block, never nominated); `demote` on a mirror ranks it below
  clean mirrors; `upstream_only` on a mirror re-paths to (c) until measured
  CPU headroom says otherwise. Non-mirror rows keep their path. The filter
  rides into evidence as a curated-2026-10-14 MEASURED line plus a short
  `why` marker.
- `render_markdown()` gains a Filter column (`filter/disposition`, else
  `—`) and sections clean mirrors → demoted mirrors → rest (adopt,
  upstream, excluded mirrors). The JSON sidecar gains `filter`,
  `filter_reason`, `filter_provenance`.
- The curated alternatives table grows the verdicts the re-rank judged:
  `full` for idna, tomli, xlsxwriter, rsa, feedparser; `partial` for
  sqlglot, openpyxl, rich, sympy, docutils, babel, markdown, mistune,
  sqlparse, marshmallow, dateparser, arrow, soupsieve, sortedcontainers,
  tomlkit. Notably idna flips a top-30 row (mirror #18) to adopt.
- Acceptance fixture grows boto3/pygments/idna rows; the script asserts
  the exclude/demote/adopt behavior offline. Eleven new unit tests cover
  one disposition per filter kind plus render ordering.

## Consequences

- The next run applies the filters automatically — rerank judgment is data,
  not prose. Removing a filter is a data-row deletion, reviewed like code.
- `upstream_only` is deliberately conservative: hypothesis (ships a Rust
  core via maturin), orjson, pendulum, regex, markupsafe, pyyaml can only
  come back as mirrors with a measured headroom number attached.
- Honesty boundary: filter rows sourced from the captain's brief (not from
  a measurement) say so in `provenance`; only the kill-table/bench/sdist
  rows claim measured grounding.
