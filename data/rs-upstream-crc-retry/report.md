# rs-upstream-crc-retry report — crc `TableBasedRegister` fast-loop upstream PR

Target: [Nicoretti/crc](https://github.com/Nicoretti/crc) @ `4e65ac4`.
Fork: [John-Cusack/crc](https://github.com/John-Cusack/crc),
branch `table-register-fast-loop`.
Status: UPSTREAM PR OPENED — https://github.com/Nicoretti/crc/pull/211
(open, `draft: no`, verified via `gh-axi pr view`).
Resubmits closed #210 (maintainer: "AI PR with failing CI") with the red
gate genuinely fixed. Based on upstream master `4e65ac4` (identical to
#210's base); #210 itself untouched. Upstream CI (`.github/workflows/ci.yml`,
`on: pull_request` → format + typing + lint + `invoke test.coverage`) had not
reported checks yet at open time; all four gates were run locally instead
(see §4).

## 1. Upstream process (followed to the letter)

- No `CONTRIBUTING.md` in the repo; the gates are the process:
  `invoke check.format` (black + isort, 88 cols), `invoke check.typing
  --files src/` (mypy strict), `invoke check.lint --files src/` (pylint),
  `invoke test.coverage` (unit + integration under coverage,
  `--fail-under=100 --show-missing`, Python 3.11–3.14 × ubuntu/macos/windows).
- Patch shape: pure-Python, stdlib-only, zero behavior change, full suite
  green, no packaging or public-API change.

## 2. Patch review (correctness first, not just coverage)

`TableBasedRegister.update` override + `_REVERSED_BYTE` table, hunk-identical
to #210's `src/` change. Reviewed against `BasicRegister.update` +
`TableBasedRegister._process_byte` semantics; no real bug found:

- Masking: fast loop carries `register = self._register & bitmask` and
  re-masks after every byte — exactly what the `register` property
  getter/setter do in `_process_byte`. Final store + return value equal the
  base's masked `self.register`.
- `reverse_input`: `_REVERSED_BYTE[v] == int(Byte(v).reversed())` asserted
  for all 256 values; reflected configs (KERMIT, CRC-32, …) covered by
  parity tests.
- Error behavior: non-`bytes` input (`bytearray`, `memoryview`, `int`,
  `float`, `str`, `None`) delegates to `super().update`, so iteration and
  `TypeError`s are byte-identical to base (asserted for `int`).
- Subclass seam: `type(self)._process_byte is not
  TableBasedRegister._process_byte` delegates, preserving the documented
  override point ("assuming a user … will provide an overwrite for
  `_process_byte`").
- Edge parity: empty input (loop no-op, returns register), multi-`update()`
  accumulation, and widths < 8 (same `ValueError` from negative shift on
  both paths) all behave identically — the shift/index expressions are the
  same arithmetic, just hoisted to locals.

## 3. Why #210 failed, and what changed

Root cause proven from job 117763998564: all 80 tests passed; only
`coverage report --fail-under=100` failed at total 98 with 5 uncovered
lines in `src/crc/_crc.py` — the two `super().update` delegation-guard
returns plus the 3-line `_process_byte` body (bypassed on the fast path,
since `Calculator` always feeds `bytes` chunks). Same optimization here,
plus 8 new test cases in `test/unit/test_crc.py` (plain/reflected
8/16/32-bit configs):

- `test_table_based_update_with_non_bytes_input_matches_bytes`
  (`bytearray`/`memoryview` digest parity + chunked-`update()` accumulation);
- `test_table_based_update_with_unsupported_input_type_raises` (`int`
  still raises `TypeError`);
- `test_table_based_update_delegates_when_process_byte_is_overridden`
  (same digest, per-byte calls observed);
- `test_table_based_process_byte_matches_update` (direct `_process_byte`
  agrees with `update()`).

## 4. Correctness proof (measured-only, local box, Python 3.12.3)

| Gate | Result |
|---|---|
| Full suite `pytest test/unit test/integration` | **88 passed** (80 baseline + 8 new), 28 subtests passed |
| Coverage `report --fail-under=100 --show-missing` | **100%** (`src/crc/_crc.py` 256/256 stmts, 0 missed) |
| `black --check src/ test/` | clean (7 files unchanged) |
| `isort --check` on touched files | clean |
| `mypy --strict src/crc/_crc.py` | no issues |
| `pylint src/crc/_crc.py` | 9.50/10, exit 0 — identical standing to pristine master (9.46/10, exit 0; remaining C0116s pre-existing) |
| Differential fast-loop vs bit-by-bit `Register` | **0 mismatches** over all 25 bundled configurations (8/16/32/64-bit, plain + reflected) × full / chunked / empty / single-byte updates × `bytes`/`bytearray`/`memoryview` |

## 5. Performance (same harness, best-of-N, local box)

- New table fast loop vs old table path (`Byte`-object churn): **~20x**
  on 16 KiB CRC-32 (351.04 ms → 17.94 ms, best-of-9 × 10).
- New table fast loop vs bit-by-bit `Register`: **~46x** on the same
  input (429.19 ms → 9.26 ms, best-of-7 × 5).
- Memory: no new allocations beyond the 256-byte `_REVERSED_BYTE` table
  (one loop-local int set); no behavior change, so no memory delta to report.

## 6. Fork diff

`src/crc/_crc.py` +31 (fast-loop `update` + `_REVERSED_BYTE`);
`test/unit/test_crc.py` +60 (8 cases + `Crc32` import). No Rust, no
packaging change, no tokens or internal paths in the upstream PR.
