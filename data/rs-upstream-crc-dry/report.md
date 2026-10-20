# rs-upstream-crc-dry report — crc upstream PR (DRY rewrite)

Target: [Nicoretti/crc](https://github.com/Nicoretti/crc) @ `4e65ac4` (v8.0.0).
Fork: [John-Cusack/crc](https://github.com/John-Cusack/crc), branch
`feature/table-register-fast-loop`, commit `b6b307b`.
Upstream PR: https://github.com/Nicoretti/crc/pull/212 (open, `draft: no`, verified
via `gh-axi pr view 212 -R Nicoretti/crc`).
Rustsmith PR: RUSTSMITH_PR_URL
Status: UPSTREAM PR OPENED. The fork CI run
(https://github.com/Nicoretti/crc/actions/runs/39498582264) is `action_required`,
i.e. waiting for the maintainer to approve the workflow of a fork PR — it is not
a failure. Every upstream gate was reproduced green locally (§5).

## 1. History: what the maintainer objected to

| PR | closed | maintainer comment | root cause (proven, not guessed) |
|---|---|---|---|
| https://github.com/Nicoretti/crc/pull/210 | 2026-10-18 | "AI PR with failing CI." | all 80 tests passed, but `invoke test.coverage` failed on `fail-under=100` — 5 uncovered lines in `src/crc/_crc.py` (the two `super().update` delegation guards + the now bypassed `_process_byte` body) |
| https://github.com/Nicoretti/crc/pull/211 | 2026-10-19 | "code is not dry, test do not use best practices and does not fit existing style" | the patch duplicated the whole per-byte loop body (one for `reverse_input`, one for the normal case), and the new tests were pytest style (`@pytest.mark.parametrize`, module level functions, bare asserts, a class defined inside a test) instead of the module's `unittest.TestCase` idiom |

Both objections are structural, so this lane rewrote the patch instead of
patching the patch. Nothing of #210/#211 was reopened or reused verbatim.

## 2. The rewrite — one loop body, repo idiom tests

### 2.1 DRY: a single loop body

`src/crc/_crc.py` +33/-0 (one commit, `b6b307b`):

- `TableBasedRegister.update` overrides the base implementation with **one**
  loop over plain ints. The byte reflection of `reverse_input` configurations is
  applied once *before* the loop with `bytes.translate` (256 byte table
  `_REVERSED_BYTE`), so the loop body is literally the same for both cases:

  ```python
  for byte in data:
      index = byte ^ (register >> shift)
      register = (lookup_table[index] ^ (register << 8)) & bitmask
  ```

  No duplicated body, no per byte branching, no `Byte` boxing, no `register`
  property traffic, no virtual `_process_byte` dispatch.
- The delegation guards of the earlier attempt are kept and merged into one
  condition: input that is not `bytes`, or a register whose `_process_byte` is
  overridden, goes to `super().update(data)` and therefore keeps the base
  behavior *by construction* — same digests, same `TypeError` messages.
- Semantics are identical to `BasicRegister.update`: masking to `_bitmask`,
  `reverse_input` reflection, `init_value` handling, empty input, and the
  `ValueError` of a width below 8 (raised inside the loop, exactly where the
  base raised it inside `_process_byte`).

### 2.2 House style tests

`test/unit/test_crc.py` +89/-0, new `TableBasedRegisterTest`:

- `unittest.TestCase` class with `subTest` loops, plain `self.assertEqual`
  assertions and a `namedtuple` fixture (`UpdateFixture`), exactly like the
  rest of the module. No pytest fixture, no `parametrize`, no new test
  framework, no new test file.
- Coverage of the change:
  - `test_update_produces_the_expected_checksum` — pinned digests
    (CRC-8/CCITT `0xF4`, CRC-16/KERMIT `0x2189`, CRC-32 `0xCBF43926`, the last
    one identical to `zlib.crc32`).
  - `test_update_produces_the_same_digest_as_the_bit_by_bit_register` — the fast
    loop against the bit by bit `Register` for all 28 bundled configurations
    (empty, `"123456789"`, all 256 byte values).
  - `test_update_can_be_called_with_chunks` — chunked `update()` accumulates
    like a single call.
  - `test_update_with_other_input_types_uses_the_base_implementation` —
    `bytearray` and `memoryview` delegate and produce the same digest.
  - `test_update_with_unsupported_input_raises_a_type_error` — `int` and `str`
    still raise `TypeError` with the base messages.
  - `test_update_uses_an_overridden_process_byte` — a subclass with its own
    `_process_byte` is honored (per byte calls observed, same digest); this
    also keeps `TableBasedRegister._process_byte` covered.

## 3. Review of the patch (what was checked before opening)

- Semantics of the fast loop were re-derived against `BasicRegister.update`
  line by line (masking, reflection, index computation, register width), not
  inherited from the earlier attempt.
- The seam check `type(self)._process_byte is not TableBasedRegister._process_byte`
  was tested in both directions: an unmodified subclass takes the fast path, an
  overriding subclass takes the base path.
- `bytes.translate` was benchmarked against the alternative "index a 256 entry
  table per byte" (the shape the brief suggested): translate is ~20% faster
  (crc32 1 MiB: 0.104 s vs 0.122 s) and keeps the loop free of any per byte
  lookup, at the price of one extra buffer of the input size. That trade off
  is disclosed in the upstream PR body.
- Nothing else in the package was touched: no packaging change, no dependency,
  no Rust, pure Python (ADR-041).

## 4. Upstream process followed to the letter

- Upstream HEAD `4e65ac4`, no CONTRIBUTING file, no PR template (issue
  templates only).
- Gates (`checks.yml`, run through `ci.yml` on `pull_request`): format
  (`invoke check.format` = isort + black), typing (`invoke check.typing
  --files src/` = mypy strict), lint (`invoke check.lint --files src/` =
  pylint, `fail-under = 9.0`), tests (`invoke test.coverage` = pytest with
  `coverage report --fail-under=100`).
- The PR was opened from a fresh branch on the fork; #210 and #211 were not
  touched, reopened or edited.

## 5. Gates reproduced locally (upstream commands, upstream tree + patch)

| command | base `4e65ac4` | patched |
|---|---|---|
| `uv run pytest test/unit` | 77 passed, 28 subtests | **83 passed, 111 subtests** |
| `uv run invoke test.coverage` | 100% | **100%** (`src/crc/_crc.py` 252 stmts, 0 missed) |
| `uv run invoke check.format` | clean | **clean** (9 files unchanged) |
| `uv run invoke check.typing --files src/` | clean | **clean** (mypy strict, 3 files) |
| `uv run invoke check.lint --files src/` | 9.42/10 | **9.47/10** (`fail-under = 9.0`) |

The coverage gate is the one that killed #210: `invoke test.coverage` reports
**100%** for `src/crc` with the new tests, so `fail-under=100` is green.

## 6. Differential correctness proof (base vs patched)

`digest_dump.py` (public materials only: the package under test + stdlib)
dumped the digests of both register types for **28 configurations** (all
bundled `Crc8`/`Crc16`/`Crc32`/`Crc64` members plus 4 custom configurations,
including reflected input and reflected output) × 6 input shapes (empty,
`"123456789"`, all 256 byte values, chunked, `bytearray`, `memoryview`) ×
3 unsupported input types (`int`, `str`, `float`, including the exception
messages).

**336 value comparisons + 168 error comparisons, 0 divergences** — the two JSON
dumps are byte identical.

## 7. Performance (measured, not estimated)

AMD Ryzen 9 5950X, Linux 6.14 x86_64, CPython 3.14.2, fresh process per
measurement, three interleaved rounds, best of 3/20/5 repetitions per workload
(same script as in the upstream PR body):

| workload | base | patched | speedup |
|---|---|---|---|
| crc32, 5 MiB | 8.757878 s | 0.519825 s | **16.8x** |
| crc16/modbus, 64 KiB × 20 | 0.100429 s | 0.004064 s | **24.7x** |
| crc64, 1 MiB × 5 | 0.490092 s | 0.100825 s | **4.9x** |

The measurement harness asserts `crc.__file__` starts with the tree under test
(the sweep-4 lesson: pointing `sys.path` at the package directory instead of
its parent silently measures base against base).

## 8. Fork diff

- `src/crc/_crc.py` +33/-0 (`TableBasedRegister.update` override +
  `_REVERSED_BYTE` table).
- `test/unit/test_crc.py` +89/-0 (`TableBasedRegisterTest`).
- One commit: `b6b307b` "Speed up TableBasedRegister.update with a single int
  level loop".
- No packaging change, no dependency, no Rust, pure Python (ADR-041).

## 9. Open items for the maintainer

- The fork CI run needs the usual approval for a first time contributor PR
  (`action_required`); the local reproduction of the same commands is green.
- The upstream PR body discloses the one extra buffer that `bytes.translate`
  needs for `reverse_input` configurations.
- The maintainer decides on merging; nothing was merged or pushed to the
  upstream repository.
