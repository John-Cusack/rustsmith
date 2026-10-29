# Upstream patch note — mdurl tier-1 (measured, NOT offered)

Against executablebooks/mdurl @ 0.1.2 (PyPI sdist; clone HEAD matches).
No upstream PR opened: the combined gain is ~1.03x (box-noise-adjacent),
below any offer bar. Recorded so upstream need not re-run the experiment.

## Arms (frozen WORKLOAD.md v1: 2000-URL mixed corpus x 300 reps = 600,000
## parse+format ops, Ryzen 9 5950X, CPython 3.12.3, fresh-subprocess walls)

Baseline (pristine 0.1.2): 2.781 / 2.781 / 2.777 s (median 2.781).

| arm | change | walls (s) | median | delta |
|---|---|---|---|---|
| A | `len(url.split("#")) == 1` -> `"#" not in url` (_parse.py:125) | 2.792 / 2.772 / 2.751 | 2.772 | ~1.00x |
| B | precompile inline `re.search(r"^//[^@/]+@[^@/]+", rest)` to module `AUTH_HOST_PATTERN` | 2.770 / 2.758 / 2.837 (+ one 3.001 box-noise outlier) | 2.770 | ~1.00x |
| C | fuse the two min-find loops into `min(...)` generator | 3.502 / 3.413 / 3.387 | 3.413 | **0.82x (loser)** |
| D | `format`: `+=` chain -> single `"".join` | 2.865 / 2.907 / 2.705 | 2.865 | ~0.97x (noise) |
| A+D | combined shippable shape (`tier1.patch`) | 2.718 / 2.712 / 2.676 | 2.712 | **1.03x** |

Upstream suite: 189/189 on baseline and on A+D. Workload checksum 85300
identical on all arms.

## Reading

- K1 (per-call rebuilds) is ~zero here: every hot regex is already
  module-precompiled; the one inline pattern hits CPython's `re` cache
  (arm B measures nothing). The lane spec's named tier-1 thesis did not
  reproduce — priced cheap, as designed.
- Arm C is the instructive loser: the explicit `for` + `str.find` loop is
  already the fast shape; generator-expression fusion adds two nested
  generator frames per host parse and measures 0.82x. Do not "clean up"
  those loops.
- Arm A is strictly-less-work with identical semantics (`in` vs 1-list
  `split`); arm D amortizes 9 intermediate strings into one `"".join`.
  Combined they are a real but trivial ~2.5% — kept as `tier1.patch` for
  the lane record, not offered upstream.

## Tier-1 verdict

No shippable pure-Python win. Ceiling recomputed against the A+D baseline
(2.712 s): portable share ~0.63 (pure 0.73 minus ~0.10
object-construction floor that any faithful seam keeps) -> ~2.7x, above
the 2x lane bar; C-`find` + C-`re` = 27.5% of the improved profile, not
dominant. Lane proceeds to the Rust port (whole-URL crossing).
