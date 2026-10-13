# rs-upstream-lane-1 report — charset-normalizer upstream PR

Target: [Ousret/charset_normalizer](https://github.com/Ousret/charset_normalizer) (charset-normalizer 3.5.2).
Fork: [John-Cusack/charset_normalizer](https://github.com/John-Cusack/charset_normalizer).
Status: NO-GAIN — no genuine win found after measured attempts (see §5); upstream PR not opened, awaiting firstmate decision.

## 1. Target selection (from rs-candidate-finder)

`data/rs-candidate-finder/candidates.md` names seven `(c) upstream` packages:
charset-normalizer (dl-rank 8), cryptography (9), cffi (11), pyyaml (13),
pydantic (18), pydantic-core (21), numpy (23). Top-ranked by dl-rank is
**charset-normalizer** (1.19B dl/mo, 2,462 dependents). It is also POC-board
KEEP #2 with a full mirror at oracle parity (297/297 + heldout 20/20), and its
proven-hot units (`api.py::from_bytes`, `md.py::mess_ratio`,
`cd.py::coherence_ratio`) are identified and parity-measured on our side.
No gut-feel pick: the list existed and named an upstream fit.

## 2. Upstream process discovery (followed to the letter)

Read before changing anything (upstream HEAD `4b33559`, master):

- `CONTRIBUTING.md`: no sign-off/Changelog/issue-first gates. Hard gates are
  `pip install nox` then `nox -s test`, `nox -s lint`, `nox -s coverage`.
  PR bar: anything that keeps backward compatibility, with description +
  green CI ("What PR may be doomed": breaking compat, red CI, no description).
- No pull-request template (`.github/` has issue templates, CODEOWNERS,
  dependabot, workflows — no `PULL_REQUEST_TEMPLATE.md`).
- `CODE_OF_CONDUCT.md`: Contributor Covenant (pledge/standards; no extra process).
- Dev setup: `noxfile.py` sessions; CI (`.github/workflows/ci.yml`) runs
  lint, matrix tests py3.7–3.15 + pypy, detection-coverage gates,
  downstream niquests/requests integration, chardet backward-compat,
  Cython matrix (ubuntu/macos/windows incl. free-threaded), combined coverage
  `--fail-under=92`, then `nox -s performance` (their benchmark:
  `bin/performance.py` over `char-dataset`, CYTHON=1, vs chardet).
- Build system (`setup.py`): the maintainers' accepted speed-core shape is
  **Cython** (`md.pyx`, `cd.pyx`, `CHARSET_NORMALIZER_USE_CYTHON=1`;
  migrated from mypyc in `6dc5f11`). Prior compiled-Python paths were
  *removed* after crash/linkage incidents (#659 segfault, #692 libm linkage,
  #714 mypyc module-not-found). A new Rust/PyO3 toolchain would replay that
  history, so the acceptable shape for this repo is: **pure-Python
  improvement mirrored in the `.pyx` sources**, zero behavior change —
  never a rewrite, never a new toolchain. See `docs/adr/037-*.md`.

## 3. Environment reproduction (their commands, before any change)

- `nox -s test-3.12`: **299 passed, 16 subtests passed** (2.12s).
- `nox -s lint` (pre-commit: pyupgrade, ruff, ruff-format, cython-lint,
  mypy, zizmor, codespell): **all passed**.
- Baseline workload frozen for proof: `char-dataset @ f9e293f`
  (478 files with extensions, 25.452 MB input), `detect()` per file,
  CPython 3.12.3, pure-Python build (`CHARSET_NORMALIZER_USE_CYTHON=0`).

## 4. Profiling (where the time is)

cProfile over the frozen workload (6.2s total): `mess_ratio` 73% cumulative
(5830 calls, 2.40s self = per-char dispatch of ~12M `feed_info` calls),
`coherence_ratio` 22% (2711 calls; `characters_popularity_compare` 0.39s self,
`alpha_unicode_split` 0.32s self). Internal `lru_cache` hit rates measured
per `from_bytes` call: mess 21.1%, coherence 28.7% (dedup is load-bearing,
kept as-is).

## 5. The change — NO-GAIN verdict (all candidates killed with data)

Five behavior-identical candidates, each proven 0-divergence by differential
oracle (478-file corpus fingerprint + 150 step-boundary edge cases × 3
thresholds + debug=True path), each benchmarked best-of-N CPU-pinned
interleaved on the frozen workload:

| Candidate | Idea | Wall (best, s) | Verdict |
|---|---|---|---|
| A1: index loop in `mess_ratio` | replace per-block slice with `str[pos]` indexing | base 2.02–2.04 vs A1 2.06–2.11 | SLOWER — killed |
| C: single-pass countdown loop | remove slices, keep iterator protocol | base 5.98–6.09 vs C 6.04–6.07 | NEUTRAL — killed |
| D: C + `sorted(keys, key=get)` + fused `filter_alt_coherence_matches` | C-level sort key, single-pass filter | base 5.98–6.05 vs D 6.04–6.11 | ~1% SLOWER — killed |
| Fenwick/bisect for `characters_popularity_compare` | O(k²)→O(k log k) inner scan | k median 26, 86% ≤ 32, max 207 — pure-Python constant loses at this size | NOT BUILT — killed by measurement |
| cross-lg chunk-split cache | reuse `alpha_unicode_split` across `lg_inclusion` variants | only 1.5% of distinct chunks evaluated under >1 lg | NOT BUILT — killed by measurement |

Structural findings: inter-file chunk duplication 36% exists but a
cross-call cache contradicts deliberate maintainer design (local caches
"garbage collected at the end", removal of `payload_result_cache` in
`94876b3`) and the #732 memory-leak sensitivity — not proposed. `.py`/`.pyx`
are maintained in lockstep (no sync gap). Memory axis (tracemalloc):
baseline peak 89.16 MB over the workload; none of the candidates moved it.

Root cause of no-gain: the pure-Python hot path is saturated — maintainer
shipped mypyc→Cython migration, CharInfo pre-computation, unrolled detector
loop with bound-method locals, definitive-match/SB-cap/MB-skip pruning in
`from_bytes`, and a prior "speed+rss micro-optimizations" batch (`96a9fdd`).
Remaining wall is ~12M irreducible per-char Python dispatches plus small-k
O(k²) scans with C-speed constants. No genuine, provable, behavior-preserving
win exists in the acceptable shape (pure-Python + `.pyx` mirror, ADR-037).
A ~0% PR would violate honesty rules and spend maintainer goodwill, so none
was opened.

## 6. Proof — retained artifacts (scratch, outside worktree)

- `/home/john/firstmate/data/rs-upstream-lane-1/scratch/bench_detect.py`
  (frozen workload: char-dataset @ `f9e293f`, 478 files, 25.452 MB,
  CPython 3.12.3, pure-Python build) and `diff_oracle.py` + `baseline_fp.json`.
- `nox -s test-3.12` baseline: 299 passed + 16 subtests; `nox -s lint`: all
  hooks passed — reproduced before any change.
- Fork prepared (no PR opened): `John-Cusack/charset_normalizer`,
  branch `rs-upstream-perf` at upstream HEAD `4b33559`.

## 7. Upstream PR — NOT OPENED (no-gain; needs-decision below)

---

## Standing upstream-lane checklist (process artifact)

Generalized from the exact steps followed for charset-normalizer:

1. **Select target from the candidate list only.** Read
   `data/rs-candidate-finder/candidates.md`; take the top-ranked
   `(c) upstream` row by dl-rank. List missing or no `(c)` row →
   stop at `needs-decision`, never pick by gut feel. Record dl/mo,
   dependents, hot units, and what our side already measured (parity and/or
   speedup) for the chosen package.
2. **Discover the contribution process to the letter.** Read CONTRIBUTING,
   pull-request template (or note its absence), code of conduct, dev-setup
   docs, CI workflow. Record every hard gate (sign-offs, changelog, issues,
   coverage thresholds) and the exact dev/test/bench commands. Check open
   issues for maintainer stance on compiled accelerators (crashes/reverts
   tell you the acceptable shape).
3. **Determine the acceptable implementation shape.** Default per lane spec:
   small optional speed core behind a pure-Python fallback. But the repo's
   precedent wins: if maintainers already ship a compiled path (Cython here)
   or reverted one (mypyc here), match it — pure-Python improvement mirrored
   into their compiled sources, never a new toolchain, never a rewrite,
   never a behavior change. Port only the proven-hot unit. Any shape
   deviation from the lane spec gets an ADR.
4. **Reproduce env + baseline before changing anything.** Run their own
   commands exactly (`nox -s test…`, `lint`, …) and record green output.
   Freeze the workload (corpus pin/commit, file count, input bytes,
   interpreter + package versions, build flags). Capture a differential
   oracle (per-input results fingerprint) for later equivalence proof.
5. **Profile, then change minimally.** cProfile (or their profiler) to find
   the cost center; try candidates as scratch variants outside the worktree;
   kill losers with numbers (record rejections). Each candidate must be
   behavior-identical by construction AND proven by the differential oracle
   (full corpus + boundary edge cases + debug/logging paths).
6. **Bench both axes, honestly.** Before/after wall time (best-of-N,
   CPU-pinned, interleaved) AND peak memory (tracemalloc peak where RSS is
   environment-dominated) on the same frozen workload, both sides,
   workload-labeled, overhead disclosed. No figure without a reproduction
   (commands + versions in the PR body). If no candidate beats baseline:
   record the no-gain verdict with the kill-table instead of a PR.
7. **Open the PR from a public fork via gh-axi.** Their template first (or
   plain description where none exists), then evidence + reproduction.
   No tokens, no internal paths. Never merge, never push to their branches,
   never rebase without being asked. Lane ends at PR opened with proof —
   or at an honest no-gain needs-decision that names the gap.
8. **Answer reviews as needs-decision** with maintainer words quoted.
