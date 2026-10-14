# rs-upstream-sweep-3 report — markdown-it-py upstream PR

Target: [executablebooks/markdown-it-py](https://github.com/executablebooks/markdown-it-py) @ `6f58654`.
Fork: [John-Cusack/markdown-it-py](https://github.com/John-Cusack/markdown-it-py),
branch `rs-upstream-perf`.
Status: UPSTREAM PR OPENED — https://github.com/executablebooks/markdown-it-py/pull/449
(open, `draft: no`, verified via `gh-axi pr view`).

## 1. Target selection (registry re-verified, not cached)

Prod PyPI re-queried per lane (sweep-1 lesson). Prod-live `-rust` set
2026-10-14: crc-rust 0.1.0, strsimpy-rust 0.2.1, python-dateutil-rust
2.9.0, markdown-it-py-rust 4.2.0, packaging-rust 26.4.0,
charset-normalizer-rust 3.5.1. TestPyPI-only (404 on prod PyPI):
pyyaml-rust, pyparsing-rust, python-multipart-rust, pygments-rust.
Exclusions hold (pyyaml PR yaml/pyyaml#976 open; charset
saturated/killed; packaging no-gain PR74; dateutil PR
dateutil/dateutil#1609 open). Highest-download prod-live original not
yet reviewed: **markdown-it-py** (427M dl/mo; KEEP #5). Hot units:
`parser_block.py`, `parser_inline.py`, `emphasis.py`; our side measured
2.36x render-16KB / 0.20x bulk-280KB (Rust mirror; bulk loss is
per-token GIL crossings — not shippable upstream, see ADR-040).

## 2. Upstream process (followed to the letter)

Upstream HEAD `6f58654` (master). Read before changing anything:

- `docs/contributing.md` (+EBP guide): pytest, tox matrix, ruff +
  black via pre-commit, type annotations + docstrings required. No PR
  template (issue templates only). CHANGELOG has an `Unreleased`
  section with per-PR entries (added post-open with the PR number).
- Gates (`tests.yml`): pre-commit, pytest on 3.10–3.13 + **pypy-3.10**,
  CommonMark spec freshness check, coverage, mdit-py-plugins downstream
  tests, `bench-core` artifact; `benchmark.yml` tracks package means.
- Build: flit_core, zero native files, one runtime dep (`mdurl`).
- Maintainer stance: pure-Python speedups with proof are merged (#389
  fixed quadratic `text_join`; #383 CLI fix). A 2022 mypyc suggestion
  (#198) was never adopted. The maintainer-owned Rust path lives
  *outside* this repo (`markdown-it-pyrs`, in `docs/performance.md` +
  `compare` extra) — so a vendored native core here would duplicate it.
  Acceptable shape: pure-Python only (ADR-040).

## 3. Baseline (their commands, before any change)

venv, editable install of upstream HEAD + `testing,linkify` extras
(CI installs `.[testing,linkify]`): **1031 passed, 1 skipped** —
matches the board exactly. Baseline green before any change. Frozen
workload for proof: mirror `bench/` corpora (16.7KB mixed doc, 280KB
bulk doc) + render differential oracle (`diff_oracle.py`: 2 corpora +
30 edge docs × default/commonmark/zero presets).

## 4. Profiling

cProfile over the frozen workload (per 16.7KB render, ~7ms real):
`StateBlock.__init__` line-marker precompute ~1.1ms (~15%) — a
per-character Python loop (`enumerate` + `isStrSpace` call per char)
over the whole source on every render. Rest is genuine port-faithful
parsing (block/inline rule dispatch tracked against the JS port in
`port.yaml`) — out of bounds. Killed without building: `src` property
→ plain attr (microbenched: 12ns/access × 5.5K/render ≈ 1% — API churn
not worth it), `LOGGER.debug` guards (0.25ms/render ≈ 3.5% — 10-file
churn maintainers would reject), `Token` double-dict alloc (≈1%).

## 5. The change — 1.20x, per-line marker precompute

`markdown_it/rules_block/state_block.py` +29/-29: build the identical
markers once per line — `str.find` locates each `\n` at C speed, only
leading whitespace scanned, same space/tab counting body (same
`isStrSpace` predicate, same tab-stop expansion).

| Workload (best-of-6 interleaved fresh processes, CPython 3.12.3) | Base | Patched |
|---|---|---|
| Public repro: 15 renders ~13KB synthetic doc, median | 79.7 ms | 65.7 ms (**1.21x**) |
| Same, best single round | 76.3 ms | 60.2 ms (1.27x) |
| Frozen 16.7KB corpus, per render | 4.55 ms | 3.75 ms (1.20x) |
| Frozen 280KB bulk, per render | 76.4 ms | 66.9 ms (1.14x) |
| Single 100KB line (old-loop pathological) | 5.42 ms | 0.93 ms (5.8x) |
| 20K short lines / empty input | — | faster / flat, no regressions |

Memory (tracemalloc peak): `1200469 B` **both sides, every run** —
neutral (same lists, same lengths).

Correctness: fork suite 1031 passed + 1 skipped (counts identical to
base); render oracle **0 divergences**; ruff + black clean.

## 6. Proof (public-materials-only repro in the PR body)

Self-contained script (stdlib + installed `markdown-it-py`, seed-fixed
synthetic doc — no internal corpora) pasted verbatim in the PR body;
numbers above are that script's own output. No tokens, no internal
paths in the PR.

## 7. Fork diff

- `markdown_it/rules_block/state_block.py` +29/-29 (per-line marker
  loop + NOTE comment).
- `CHANGELOG.md` +1 (`Unreleased` entry with PR link, 👌 marker per
  house style). No Rust, no packaging change.

## 8. Checklist deltas (new lessons; standing list unchanged otherwise)

1. **Screen candidates with wall-clock measured WITHOUT tracemalloc;
   measure peak memory in a separate run.** Tracemalloc inflates Python
   dispatch cost ~2–3x, and unevenly: under tracing, base looked 2.4x
   slower than the patch (33ms vs 14ms); without tracing the real gap
   is 1.2x (7.6ms vs 5.7ms medians). A combined harness would have
   overstated the claim in the PR body. Memory peaks are unaffected,
   so split the axes.
