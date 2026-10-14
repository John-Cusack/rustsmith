# rs-upstream-sweep-2 report — python-dateutil upstream PR

Target: [dateutil/dateutil](https://github.com/dateutil/dateutil) @ `2642afa`.
Fork: [John-Cusack/dateutil](https://github.com/John-Cusack/dateutil),
branch `rs-upstream-perf`.
Status: UPSTREAM PR OPENED — https://github.com/dateutil/dateutil/pull/1609
(open, `draft: no`, verified via `gh-axi pr view`).

## 1. Target selection (registry re-verified, not cached)

Prod PyPI re-queried per lane (sweep-1 lesson, which also caught README
staleness: strsimpy-rust 0.2.1 and markdown-it-py-rust 4.2.0 are prod-live
though the board still says TestPyPI). Prod-live set:
python-dateutil-rust 2.9.0, crc-rust 0.1.0, strsimpy-rust 0.2.1,
markdown-it-py-rust 4.2.0; pyyaml-rust/pyparsing-rust TestPyPI-only.
Exclusions hold (pyyaml PR yaml/pyyaml#976 open; charset saturated/killed;
packaging no-gain). Highest-download prod-live original not yet reviewed:
**python-dateutil** (874M dl/mo, 6,070 dependents; KEEP #3). Hot units
`_parser.py::get_token`, `rrule.py::_iter`, `tz.py::_isdst`; our side
measured 5.6x fuzzy-noisy / 3.8x strict-parse / 1.7x iso (Rust, not
shippable upstream — see ADR-039).

## 2. Upstream process (followed to the letter)

- `CONTRIBUTING.md`: PEP-8-ish, tests required, towncrier fragment
  `changelog.d/<PR#>.<type>.rst` (easiest after PR creation).
- PR template: Summary + checklist (tests / AUTHORS.md / news fragment).
- Gates: pytest via tox (py27–py3.12 matrix + pypy, coverage); no lint
  gate in CI. `python_requires = >=2.7`, universal wheel.
- No maintainer discussion of compiled accelerators found; the build
  (pure setuptools, zero deps, py2.7 still supported) rules out any
  native shape. Acceptable shape: pure-Python, py2-compatible (ADR-039).

## 3. Baseline (their commands, before any change)

`pytest tests/ --ignore=tests/property`: **1977 passed, 47 skipped,
17 xfailed**; 49 `test_tz` failures all from the unbuilt
`dateutil-zoneinfo.tar.gz` data file (needs network via
`updatezinfo.py`) — 0 non-env failures. Target suites
(parser/isoparser/rrule/relativedelta): 1446 passed, 16 xfailed.
Frozen workload for proof: mirror `bench_parser` corpora
(10 clean strict + 10 noisy + 2000 bulk fuzzy + boundary/edge) +
5769-record differential fingerprint (`diff_oracle.py` + `baseline_fp.json`).

## 4. Profiling

cProfile over the frozen workload: `get_token` 38% cumulative
(542K `StringIO.read(1)` calls, 514K `isword` dispatches),
`_parse` float-probe exceptions on every word token, `_parse_numeric_token`
11%. `Decimal` per numeric token and per-parse `parserinfo` construction
are load-bearing by maintainer intent (GH 427/662, time-dependent
century window) — not touched.

## 5. The change — 1.26x, stacked A+B

| Candidate | Idea | Wall (best-of-7, pinned, interleaved) | Verdict |
|---|---|---|---|
| A: index tokenizer `_get_token_str` | str input tokenized by index, no `read(1)`/char; subclass overrides still honored; streams keep old path | 0.488 → 0.456 s (1.07x), peak flat | SHIPPED |
| B: `float()` guard | attempt `float()` only on leading-digit or inf/infinity/nan tokens (GH 662 path preserved, suite-pinned) | stacked 0.486 → 0.386 s (**1.26x**), peak 15907 → 15774 | SHIPPED |
| C: elif double-lookup hoisting | single-evaluate weekday/month/ampm/`_find_hms_idx` | ~2% est, needs invasive chain restructure + changes subclass call counts | NOT BUILT — killed |
| D: Decimal→float in `_to_decimal` | skip `Decimal()` per numeric token | GH 427/662 deliberate semantics | NOT BUILT — killed |
| E: cache default parserinfo | skip 7-dict build per `parse()` | time-dependent century window + thread-safety | NOT BUILT — killed |
| F: `__next__` bypass dispatch | save one frame per token | ~0.6% est, shadows subclass `get_token` overrides | NOT BUILT — killed |

Correctness: fork suite 1994 passed (16 new parity/NUL tests) + docs
doctests, same 49 env failures; 5769-record oracle **0 divergences**;
inf/nan/fuzzy edge pins identical (`'Nan'`, `'inf'`, `'nanny'`,
`'inferno 2024'` etc.).

## 6. Proof (public-materials-only repro in the PR body)

Self-contained script (stdlib + dateutil, seed-fixed, pasted in the PR):
100 strict + 50 fuzzy parses × 7 reps, interleaved, pinned:
master 0.0241 s / 15841 B peak vs branch 0.0206 s / 15739 B peak =
**1.17x wall, memory neutral**. Frozen-workload figure 1.26x disclosed
alongside. No tokens, no internal paths in the PR.

## 7. Fork diff

`src/dateutil/parser/_parser.py` +150/-3 (init fast-path state,
`get_token` dispatch, `_get_token_str`, float guard);
`tests/test_parser.py` +32 (str-vs-stream token/parse parity, NUL pin);
`AUTHORS.md` +1; `changelog.d/1609.misc.rst` (added post-open for the
number, per CONTRIBUTING). No Rust, no packaging change.

## 8. Checklist deltas (new lessons; standing list unchanged otherwise)

1. **Skipping exception-based probes requires enumerating the probe's
   full accepted grammar.** `float()` accepts `inf`/`infinity`/`nan`
   (any case) — a first-char-digit guard alone would reroute GH 662
   inputs off the numeric path. Pin every grammar edge with regression
   tests (`'Nan'` existed; add the rest) and verify fuzzy-mode
   divergence, not just strict.
2. **PR proof must be reproducible from public materials only.**
   Internal frozen corpora can freeze the workload for development, but
   the PR body needs a self-contained script (seed-fixed, stdlib-only)
   with its own measured numbers — never cite a figure whose inputs a
   maintainer cannot obtain.
