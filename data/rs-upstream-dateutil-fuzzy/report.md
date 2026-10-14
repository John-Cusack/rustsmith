# rs-upstream-dateutil-fuzzy report — python-dateutil fuzzy-parse upstream PR

Target: [dateutil/dateutil](https://github.com/dateutil/dateutil) @ `2642afa`.
Fork: [John-Cusack/dateutil](https://github.com/John-Cusack/dateutil),
branch `fuzzy-parse-speedup`.
Status: UPSTREAM PR OPENED — https://github.com/dateutil/dateutil/pull/1613
(open, `draft: no`, verified via `gh-axi pr view`).
Stacks with #1609 (same author, tokenizer fast path, still open): that PR owns
`StringIO.read(1)`-per-character; this one owns the per-token dispatch that
dominates once tokenizing is cheap. Based on upstream master `2642afa`,
no overlap with #1609's hunks.

## 1. Upstream process (followed to the letter)

- `CONTRIBUTING.md`: PEP-8 (no added line > 79 cols), tests required,
  towncrier fragment `changelog.d/<PR#>.misc.rst` added after PR creation
  for the number, dual-license (Apache-2.0 + BSD-3-clause) accepted,
  AUTHORS.md entry added.
- Acceptable shape per ADR-039: pure-Python, Python-2.7-compatible syntax,
  zero behavior change, full suite green.

## 2. Profiling (Ryzen 9 5950X, Python 3.12.3, cProfile, 20-string fuzzy corpus)

`_parse` ~48% of wall excluding the tokenizer (owned by #1609). Fuzzy-only
costs: `float()` ValueError per word token (already fixed by #1609),
`weekday`/`month`/`ampm`/`hms` misses each paying `str.lower()` +
raise-and-catch `KeyError`, `parserinfo.jump()` consulted for every junk
token even with `fuzzy=True` (outcome forced to skip), `_find_hms_idx`
called twice per numeric token reaching that branch, `all(x in
string.ascii_uppercase ...)` generator per tzname probe.

## 3. Candidates — win table (measured-only)

All figures: best-of-15 interleaved subprocess runs (order reversed each
rep), `time.process_time`, A/A control (pristine tree vs identical copy)
0.96x–1.02x on every workload, so only deltas well outside ±4% claimed.
Workloads: strict (10 clean, `fuzzy=False`), fuzzy (20 noisy,
`fuzzy=True`), tokens (same 20, `fuzzy_with_tokens=True`), large (2003
noisy log lines, fuzzy), big (3500 mixed lines, fuzzy).

| Candidate | Idea | Measured | Verdict |
|---|---|---|---|
| A: exception-free `parserinfo` misses | `weekday`/`month`/`hms`/`ampm`: `dict.get()` instead of `try/except KeyError`; identical returns, overrides unaffected | part of stacked win below | SHIPPED |
| B: fuzzy jump fast path | `elif fuzzy: skip` before `info.jump()`; strict path unchanged | part of stacked win below | SHIPPED |
| C: bound-method hoisting in `_parse` loop | 8 per-token callables to locals; overrides still honored | part of stacked win below | SHIPPED |
| D: C-level tzname check | `not token.strip(string.ascii_uppercase)` ≡ `all(...)` (parity-proven incl. empty/non-ASCII) | +2–3% incremental on fuzzy workloads | SHIPPED |
| A+B+C+D stacked | all four in `_parser.py`, +45/−32 | fuzzy **1.167x** best / 1.195x median; tokens 1.171x/1.164x; large 1.168x/1.155x; big 1.172x/1.180x; strict 1.077x/1.105x | SHIPPED |
| E: single `_find_hms_idx` call | elif condition + body each call it; cache result | 0.99x–1.00x (attribute store/load cancels the saved call) | KILLED — measured no-gain |
| F: `token in self.info.UTCZONE` set-ification | list scan → set | UTCZONE has 4 entries; sub-µs, unmeasurable | KILLED — not built |

Memory (same harness + `tracemalloc` pass + peak RSS): no regression.
tracemalloc peak +414–536 B (transient bound-method locals alive during
the token loop; disclosed in the PR); peak RSS flat-to-down on every
workload (e.g. big 13444 → 12928 KiB); snapshot trace counts identical.

## 4. Correctness

- Differential oracle, 496 outputs (60+ edge strings — nan/inf/infinity
  grammar edges, NUL bytes, unicode months, ampm/tz/hms/jump edges —
  × fuzzy/strict/tokens/dayfirst/yearfirst/default configs + custom
  `parserinfo` subclasses): **0 mismatches** base vs patched.
- Full suite `pytest tests/`: **1981 passed**, identical counts both
  sides; 49 `test_tz` failures + 1 property-test collection error are
  pre-existing environment issues (missing `dateutil-zoneinfo` data
  file), byte-identical with and without the patch. Parser/isoparser
  files: 825 passed.
- New regression tests (3): `testFuzzySkipsUnknownTokens`,
  `testFuzzyUppercaseTokenAsTzname`, `testFuzzyWithTokensSkippedRuns`.

## 5. Fork diff

`src/dateutil/parser/_parser.py` +45/−32 (4 ideas above);
`tests/test_parser.py` +25 (3 fuzzy tests);
`AUTHORS.md` +1; `changelog.d/1613.misc.rst` (added post-open for the
number, per CONTRIBUTING). No Rust, no packaging change, no tokens or
internal paths in the PR.

## 6. Checklist deltas (new lessons)

1. **Differential harnesses must assert which code they imported.**
   A venv editable-install (`.pth` path) silently shadowed a
   mis-laid-out candidate tree and the oracle compared base-vs-base
   (0 mismatches, vacuous) while the bench compared base-vs-base and
   still read 1.09–1.14x under load. Now: every harness asserts
   `module.__file__.startswith(expected_src)` and every tree ships a
   layout self-check.
2. **On a shared box, wall-clock best-of-N needs an A/A control gate.**
   Same-code-vs-same-code read 0.95x–1.03x at load 25 (32 threads).
   Fixed protocol: `time.process_time` with tracemalloc off, in-worker
   repeats sized to ≥0.2 s CPU per rep, 11–15 reps, A/A must read
   ~1.00x before any A/B number is trusted; claim only deltas well
   outside the A/A spread with best≈median agreement.
