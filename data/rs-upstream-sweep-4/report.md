# rs-upstream-sweep-4 report — crc upstream PR

Target: [Nicoretti/crc](https://github.com/Nicoretti/crc) @ `4e65ac4` (v8.0.0).
Fork: [John-Cusack/crc](https://github.com/John-Cusack/crc),
branch `rs-upstream-perf`.
Status: UPSTREAM PR OPENED — https://github.com/Nicoretti/crc/pull/210
(open, `draft: no`, verified via `gh-axi pr view`).

## 1. Target selection (registry re-verified, not cached)

Prod PyPI re-queried per lane (sweep-1 lesson): prod-live `-rust` set is
crc-rust 0.1.0, strsimpy-rust 0.2.1, python-dateutil-rust 2.9.0,
markdown-it-py-rust 4.2.0, packaging-rust 26.4.0, charset-normalizer-rust
3.5.1. TestPyPI-only (404 on prod PyPI): pyyaml-rust, pyparsing-rust,
python-multipart-rust, pygments-rust. Exclusions hold (pyyaml PR
yaml/pyyaml#976 open; charset saturated/killed; packaging no-gain;
dateutil PR dateutil/dateutil#1609 open; markdown-it-py PR
executablebooks/markdown-it-py#449 open). Remaining prod-live originals not
yet reviewed: crc and strsimpy. pypistats 2026-10-14: **crc 752K dl/mo vs
strsimpy 334K** — target is **crc**. Hot units: `BasicRegister.update`
per-byte path, `Byte.reversed`, `register` property; our side measured
2045x crc32-64KB (Rust mirror — not shippable upstream, see ADR-041).

## 2. Upstream process (followed to the letter)

Upstream HEAD `4e65ac4`. Read before changing anything:

- No CONTRIBUTING file, no PR template (issue templates only:
  bug/refactoring/documentation/feature/security/blank). Contributors page
  is a thanks-list, no process gates.
- Gates (`checks.yml`): format (black/isort via `invoke check.format`),
  `mypy --strict` on `src/`, pylint (`fail-under = 9.0`), pytest with
  coverage on 3.11–3.14 × ubuntu/macos/windows.
- Build: hatchling, description "Pure Python CRC library", zero runtime
  deps, zero native files in history; `requires-python = >=3.11`.
- Acceptable shape: pure-Python only (ADR-041).

## 3. Baseline (their commands, before any change)

venv + editable install of upstream HEAD + pytest: **80 passed, 28 subtests
passed**. Baseline green. Frozen workload for proof: mirror `bench/`
corpora (`corpus_1m.bin`, `corpus_64k.bin`) + render differential oracle
(`diff_oracle.py`: all 26 configs × registers/Calculator + edges + chunk
splits + input-type matrix + subclass seam + error paths).

## 4. Profiling

cProfile over 1 MB CRC32 + 16×64 KB frames + CRC16 (per §5 workload, ~6 s
profiled): `Byte.reversed` 1.1M calls / 1.63 s self, `Byte.__getitem__`
8.9M calls / 1.36 s self, `Byte.value` 10.1M calls / 0.60 s self,
`register` get+set 4.7M calls / 0.38 s self — ~31M tiny calls for ~1.2M
input bytes. The table lookup is cheap; the per-byte boxing around it is
the cost. Killed without building: speeding up bit-by-bit `Register`
(out of bounds — documented slow path), C-level `bytes.translate` reverse
(no gain over a 256-entry index: translate builds a second buffer).

## 5. The change — ~18.5x, int-level `TableBasedRegister.update`

`src/crc/_crc.py` +27/-0: override looping over plain ints with hoisted
width/shift/table/mask locals and a 256-entry `_REVERSED_BYTE` table;
non-`bytes` input and `_process_byte`-overriding subclasses delegate to
`super().update` (exact behavior including error messages, oracle-pinned).

| Workload (best-of-7 interleaved fresh processes, CPython 3.12.3) | Base | Patched |
|---|---|---|
| Public repro: seed-fixed synthetic ~5 MB mixed, best-of-5 | 11.348 s | 0.612 s (**18.5x**) |
| Frozen crc32-1MB ×5, best | 10.581 s | 0.574 s (18.5x) |
| Frozen crc16-modbus-64k ×20, best | 2.603 s | 0.107 s (24.3x) |
| Frozen crc64-1MB ×5, best | 3.231 s | 0.490 s (6.6x) |

Memory (tracemalloc peak, separate runs per sweep-3 lesson): public repro
`2170637 B` **both sides, byte-identical**; frozen `75494 B` vs `75498 B`
(noise). Overhead disclosed in PR body: none measured; tiny inputs
dominated by the two `isinstance`/seam guard checks (ns-scale per call).

Correctness: fork suite 80 passed + 28 subtests (counts identical to base);
differential oracle **106 sections / 1365 comparisons / 0 divergences**;
black + isort + mypy strict clean; pylint 9.50/10 (base 9.46 — patch is
cleaner than the file it touches).

## 6. Proof (public-materials-only repro in the PR body)

Self-contained script (stdlib + installed `crc`, seed-fixed synthetic data
— no internal corpora) pasted verbatim in the PR body; numbers above are
that script's own output. No tokens, no internal paths in the PR.

## 7. Fork diff

- `src/crc/_crc.py` +27/-0 (`TableBasedRegister.update` override +
  `_REVERSED_BYTE` table). No packaging change, no Rust.
- Two commits: the fast loop, then the bytes-only restriction (found by
  the oracle, §8.2).

## 8. Checklist deltas (new lessons; standing list unchanged otherwise)

1. **Oracle error-path messages, not just happy paths.** The first
   candidate passed 1300+ value comparisons but raised `TypeError` with a
   different message than base for non-`bytes` input (`str`, `float`).
   Both sides raise — but a message-matching downstream test would break.
   Fix pattern: restrict the fast override to the documented input type
   (`isinstance(data, bytes)`) and delegate everything else to `super()`,
   making off-contract behavior identical *by construction*, then pin it
   in the oracle (values + messages + exotic inputs: bytearray, memoryview,
   int-list, generator, str, float).
2. **Point `sys.path` at the package parent, never the package dir, in
   tree-comparison harnesses.** A smoke test inserted `.../src/crc` (the
   package directory itself) instead of `.../src`; `import crc` then
   silently resolved to the installed editable copy on both sides and the
   "speedup" measured base-vs-base (1.06x). Cross-check: assert
   `module.__file__` starts with the intended tree before trusting any
   number.
