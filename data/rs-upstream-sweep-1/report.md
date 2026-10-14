# rs-upstream-sweep-1 report — packaging (no-gain) upstream review

Target: [pypa/packaging](https://github.com/pypa/packaging) (packaging 26.4.dev0).
Fork: [John-Cusack/packaging](https://github.com/John-Cusack/packaging) (branch
`rs-upstream-perf` created locally, **never pushed** — no PR opened).
Status: NO-GAIN — honest record with kill-table below. No upstream PR:
~1% is not a win.

## 1. Target selection (registries checked, never assumed)

Prod-PyPI registry check 2026-10-14 (all ours, unyanked, `project_urls.Upstream`
pointing at the real originals):

| Distribution | Version | Uploaded | Original dl/mo |
|---|---|---|---|
| packaging-rust | 26.4.0 | 2026-10-13 | packaging 1.55B (dl-rank 2) |
| charset-normalizer-rust | 3.5.1 | 2026-10-13 | excluded by brief (kill record) |
| markdown-it-py-rust | 4.2.0 | 2026-10-13 | markdown-it-py 427M |
| strsimpy-rust | 0.2.1 | 2026-10-13 | strsimpy (small) |
| python-dateutil-rust | 2.9.0 | 2026-10-09 | python-dateutil 874M |
| crc-rust | 0.1.0 | 2026-10-09 | crc (small) |
| python-multipart-rust / pyparsing-rust / pygments-rust | 404 on prod PyPI | — | not prod-live |

The brief named only crc + dateutil as live, but ordered the 7-batch verified
by registry, not assumed. Four of the seven batch are prod-live since
2026-10-13. Excluding pyyaml (PR yaml/pyyaml#976 open) and charset-normalizer
(killed with record), the top prod-live package by downloads is **packaging**
(1.55B dl/mo, dl-rank 2 — above dateutil's 874M). Target selected by the
rule, not by gut feel.

## 2. Upstream process discovery (followed to the letter)

Upstream HEAD `ef837aa` (main). Read before changing anything:

- `CONTRIBUTING.rst` → `docs/development/`: new branch per work, **small
  patches**, larger changes discussed in an issue first, dual Apache-2.0/BSD
  license on all code, changelog batched from PRs by maintainers.
- Tests: `python -m pytest` (property tests opt-in via `-m property`);
  **100% code coverage gate** (branch coverage, combined matrix).
- Lint: ruff (`nox -s lint` / pre-commit). License-header boilerplate on
  every file.
- CI: `test.yml` matrix (3.10–3.15 incl. free-threaded + PyPy), `lint.yml`,
  and **`perf.yml`: ASV `continuous origin/main HEAD` runs on every PR** —
  perf is taken seriously here.
- No compiled-accelerator precedent anywhere: zero native files, zero
  build-backend beyond flit_core, zero dependencies, no Rust/Cython/maturin
  discussion in open issues or PRs (searched). Pure Python forever.

## 3. Acceptable implementation shape (no ADR: lane-spec default, restricted)

Default lane shape is small optional speed core + pure fallback, but this
repo's facts restrict it to **pure-Python only**:

- Zero-dependency flit-built package vendored into pip; a native toolchain
  would need build-backend surgery plus an issue-first discussion per their
  guide. Nothing to propose without a win, so none proposed.
- No new toolchain, no rewrite, no behavior change. Port only the
  proven-hot unit (`specifiers.py::contains` fast path, §4).

## 4. Environment reproduction + frozen workload

- venv, editable install of upstream HEAD, `hypothesis` added (property
  modules import it at collection time): **62493 passed, 427 deselected**
  (property) in ~27s. Baseline green before any change.
- Workload frozen on their own benchmark corpora (`benchmarks/` @ HEAD):
  `version_sample.txt` (1305) + `dist_sample.txt` (500 requirement strings)
  + `specs_sample.txt`, CPython 3.12.3: parse all versions, build all
  requirements, resolver loop (457 specifier sets × 1265 versions `in`
  checks = 578k `contains` calls, 70335 matches) + 243 marker evaluations.

## 5. Profiling (where the time is)

cProfile over the frozen workload: `SpecifierSet.contains` →
`Specifier._fast_match` 0.45s self of 2.0s profiled time; the rest is
`is_prerelease`/`local`/`coerce_version`/`_require_spec_version` dispatch.
The maintainers already shipped a range engine (`_ranges.py`, bound
closures, single-range hot path), `_fast_match`, one-element spec-version
caches, and a `Version` simple-path fast lane. Saturation expected; three
genuine candidates tried, all killed with numbers.

## 6. Kill-table (all behavior-identical by construction + oracle-proven)

| Candidate | Idea | Wall (best, CPU-pinned, interleaved) | Verdict |
|---|---|---|---|
| C1: restructured `contains` (resolve spec versions once per call) | hoist per-(spec,version) cache hits | base 0.254 vs C1 0.345 (4 rounds best-of-5) | 36% SLOWER — killed (alloc/closure overhead dwarfs savings) |
| C2: cached per-spec fast-match plan (lazy slot, real fork patch on `rs-upstream-perf`) | eliminate endswith + dict.get + 2 method calls + dead local recheck per (spec,version) | base 0.2528 vs patched 0.2497 (4 rounds best-of-7, fresh processes) = **+1.2%**; scratch variant ranged +1–3% across runs at ±2% run variance | NOISE-SCALE — killed (effect irreproducible above run variance; a ~1% PR would spend PyPA goodwill and likely vanish under their ASV CI) |
| C4: `canonicalize_name` already-normalized fast path | skip regex for plain names | base 0.0099 vs C4 0.0080 mins but ranges overlap (0.0099–0.0147 vs 0.0080–0.0104); ~40ns/call absolute | NOISE-SCALE, weak workload link — killed |

Differential oracle (base vs patched tree): corpus contains
(688 specs × 1265 versions) + edge matrix (locals, wildcards, `~=`,
`===`, epochs, pre/post/dev × prereleases True/False/None) + singular
`Specifier.contains` + pickle roundtrips + str stability — **40 sections,
0 divergences**. Memory axis (tracemalloc peak over workload): 6.3 KB both
sides — unmoved, as expected for a dispatch-only change.

Root cause of no-gain: the pure-Python hot path is saturated by prior
maintainer perf work (range engine, fast path, caches). Remaining per-check
cost (~400ns) is irreducible attribute/method dispatch; restructuring
saves single-digit nanoseconds per pair and drowns in run variance.
A native core is outside the acceptable shape (§3).

## 7. Upstream PR — NOT OPENED (no-gain)

Fork `John-Cusack/packaging` exists (public, via gh-axi); experimental
branch stayed local and unpushed. Per honesty rules (~1% is not a win),
no PR was opened.

## 8. Proof artifacts (scratch, outside worktree)

- `/home/john/firstmate/data/rs-upstream-sweep-1/scratch/packaging/`
  (fork clone @ `ef837aa`, branch `rs-upstream-perf` with the C2 patch).
- `bench_pkg.py` / `bench_resolver.py` (frozen workloads),
  `bench_compare.py`, `bench_c2_proof.py`, `bench_proof.py` (interleaved
  pinned best-of-N harnesses), `diff_oracle.py` (40-section oracle),
  `candidates.py`, `candidate_c2.py`, `base_tree/` (pristine HEAD export).
- Baseline: 62493 passed + 427 deselected; ruff check + format clean on
  the patched file.

## 9. Checklist deltas (new lessons for future sweep lanes)

Standing checklist lives in `data/rs-upstream-lane-1/report.md` §Standing
checklist; two additions from this lane:

1. **Registry verification must cover the whole pending batch, not just
   the named-live set.** The brief named crc + dateutil live, but four
   more prod releases landed the day before (packaging, charset,
   markdown-it, strsimpy). Target selection changed (dateutil → packaging).
   Never trust a cached live-list; re-query prod PyPI per lane.
2. **Vendored zero-dependency packages rule out the speed-core shape.**
   For pip-vendored, flit-built, dependency-free originals, the acceptable
   shape is pure-Python only — decide this in step 3 from build files +
   vendoring, before profiling, since it caps the ceiling a lane can claim.
