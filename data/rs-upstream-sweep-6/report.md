# rs-upstream-sweep-6 report — strsimpy upstream PR (retargeted from pyparsing)

Target: [luozhouyang/python-string-similarity](https://github.com/luozhouyang/python-string-similarity) @ `115acaa`.
Fork: [John-Cusack/python-string-similarity](https://github.com/John-Cusack/python-string-similarity),
branch `rs-upstream-perf`.
Status: UPSTREAM PR OPENED — https://github.com/luozhouyang/python-string-similarity/pull/39
(open, `draft: no`, verified via `gh-axi pr view`).

## 1. Target selection (registry re-verified, retargeted mid-lane)

Exclusions hold (pyyaml PR yaml/pyyaml#976 open; charset-normalizer
saturated/killed; packaging no-gain; dateutil PR dateutil/dateutil#1609
open; markdown-it-py PR executablebooks/markdown-it-py#449 open; crc PR
Nicoretti/crc#210 open; python-multipart PR Kludex/python-multipart#337
open). The brief named pyparsing (298M) next once prod-live. I confirmed
the **original** `pyparsing` 3.3.3 prod-live on prod PyPI (200,
`Development Status :: 5 - Production/Stable`, not yanked) and did the
full pyparsing lane (profiled, two ~3–7% micro-opts, suite green,
differential oracle clean — parked, artifacts retained under
`/home/john/firstmate/data/rs-upstream-sweep-6/`, nothing pushed
upstream). Firstmate then ruled (002/003): reviews follow our prod
ships, and `pyparsing-rust` is 404 on prod PyPI (TestPyPI-only), so it
is not shipped — retarget to the highest-download prod-live mirror
outside exclusions, strsimpy suggested.

Re-verified myself via curl against both registries (not from cache):

| Distribution | Prod PyPI | TestPyPI | Verdict |
|---|---|---|---|
| `strsimpy-rust` (mirror) | 200 (0.2.1, not yanked) | 200 | **prod-live — target** |
| `strsimpy` (original) | 200 (0.2.1, not yanked) | 200 | prod-live original |
| `pyparsing-rust` | 404 | 200 | not shipped — parked |
| `pyyaml-rust` | 404 | — | not shipped (excluded anyway) |
| `markdown-it-py-rust` | 200 | — | shipped but excluded |
| `python-dateutil-rust`, `crc-rust` | 200 | — | shipped but excluded |

Note for the record: firstmate's 001 404s were our **mirror** names,
mine was the **original** name — both checks were correct answers to
different questions. Lesson recorded in §8: verify original and mirror
names as separate registry queries and label which is which.

Hot units (POC board + this lane's profile): `levenshtein.py::distance`,
`longest_common_subsequence.py::length` (via `MetricLCS`). Our side
measured 65x levenshtein / 9.2x cosine / 471x metric-LCS (Rust mirror —
not shippable upstream, pure-Python original, see ADR-043).

## 2. Upstream process (followed to the letter)

Upstream HEAD `115acaa` (2021-10-11; quiet maintainer, 3 open PRs, 7
open issues, none perf-related). Read before changing anything:

- No CONTRIBUTING, no PR template, no CoC. No contributor process gates.
- Gates (CI `python-package.yml`): flake8 hard gate
  (`--select=E9,F63,F7,F82`), advisory flake8
  (`--max-complexity=10 --max-line-length=127`), pytest on 3.5–3.9.
- Build: setuptools, zero runtime deps, zero native files; version 0.2.1.
- Acceptable shape: pure-Python only (ADR-043, same class as ADR-041/042).

## 3. Baseline (their commands, before any change)

`pytest strsimpy`: **18 passed** (0.15s). Baseline green. Frozen
workloads for proof: 200 seeded short pairs (10–40 chars), 30 seeded doc
pairs (200–800 chars, `random.Random(7)`), 2 seeded 2048×2048 pairs +
the untouched-measure control (Damerau/Jaro-Winkler/SIFT4/Cosine).

## 4. Profiling

cProfile over all measures: `Levenshtein.distance` 2.27s cum (1.29s
self) and `LongestCommonSubsequence.length` (via MetricLCS) 1.97s cum
(1.12s self) dominate; inner loops are 8.35M `min` + 7.47M `max` calls.
Per-cell costs: re-indexed `s0[i]`, recomputed `len()`s, and — for LCS —
a full `(n+1)×(m+1)` matrix build when only the previous row is read.

## 5. The change — rolling-row LCS (2.3–2.6x, ~99.7% less memory) + Levenshtein hoist (1.02–1.05x)

`strsimpy/levenshtein.py` (+9/−10): hoist row char + both lengths out
of the inner loop, `list(range(...))` initial row, ternary cost. Same DP,
same `min()` order.
`strsimpy/longest_common_subsequence.py` (+12/−6): two rolling rows
instead of the full matrix; `max(a, b)` → conditional (call overhead
dominates at this size). Same recurrence, same return.

| Workload (best-of-7, CPython 3.12.3, taskset-pinned, A/B/A/B rounds) | Base | Patched |
|---|---|---|
| LCS, 30 doc pairs | 1.21–1.22 s / 4.5 MB peak | 0.47–0.53 s / 13 KiB peak (**2.3–2.6x**, **~350x less**) |
| LCS, 2×2 KiB pairs | 1.44–1.45 s / 35.5 MB peak | 0.62–0.63 s / 62 KiB peak (**2.3x**, **~570x less**) |
| Levenshtein, 30 doc pairs | 1.36–1.37 s / 63 KiB | 1.31–1.33 s / 63 KiB (**1.02–1.05x**, neutral) |
| Levenshtein, 200 short pairs | ~18 ms | ~19 ms (inside ±5% noise — no change claimed) |
| Damerau/JW/SIFT4/Cosine (untouched control) | ~53 ms | ~53 ms (unchanged, as expected) |

Memory = tracemalloc peak. Killed without building: Damerau
`str()`/`int()` index churn (smaller user base than Levenshtein/LCS,
kept PR to the two proven-hot units).

Correctness: fork suite **18 passed** on the patched tree (identical to
baseline); differential oracle base-vs-patched over all 17 measures × 8
edge-case inputs (empty/identical/unicode/long) + None-contract:
**187 comparisons, 0 divergences** (float outputs bit-identical);
flake8 hard gate 0 on changed files, advisory count unchanged (12
pre-existing elsewhere, identical on pristine tree).

## 6. Proof (harness + repro in the PR body)

Figures use frozen seeded workloads (`random.Random(7)`/`(99)`,
documented sizes) with stated interpreter + best-of-7 pinned method —
reproducible with stdlib + the package. No tokens, no internal paths in
the PR. Short-pair Levenshtein explicitly claimed as no-change; box load
(~10) disclosed as the reason for best-of-N + round repetition instead
of single means.

## 7. Fork diff

- `strsimpy/levenshtein.py`, `strsimpy/longest_common_subsequence.py`:
  the two opts above (1 commit).
- Branch `rs-upstream-perf` pushed; PR opened from the public fork via
  `gh-axi` with explicit `-R <upstream> --base master --head
  <fork>:rs-upstream-perf` (sweep-5 lesson); returned URL owner verified
  before reporting.

## 8. Checklist deltas (new lessons; standing list unchanged otherwise)

1. **Query original AND mirror names as separate labeled registry
   checks.** `pyparsing` → 200 and `pyparsing-rust` → 404 are both true;
   the 001 confusion came from unlabeled "prod-live" claims. Report
   which name each code belongs to (this report's §1 table).
2. **Assert the import path + print the package version in every
   benchmark log, and check it before trusting numbers.** A harness run
   with `PPPPATH` set but `PYTHONPATH` unset silently measured system
   pyparsing 3.1.1 on both sides (identical code still showed ±30%
   swings on a loaded box — noise mistaken for signal). The
   `__version__` column in the suite CSV is the canary.
3. **Never stash-dance the tree under test while a background benchmark
   runs** — one round's pop failed (`No stash entries found`) and the
   pairing had to be reconstructed from the memory signature. Freeze
   static base/patch tree copies for A/B instead.
4. **Black-stable drift is pre-existing — scope the format check to your
   hunks.** Pristine HEAD also fails the installed black; conforming
   only the touched lines keeps the diff reviewable.
