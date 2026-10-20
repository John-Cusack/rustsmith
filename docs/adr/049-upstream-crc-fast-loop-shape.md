# ADR-049: crc upstream fast loop ships as one loop body with repo idiom tests

Date: 2026-10-20. Lane: rs-upstream-crc-dry. Target: Nicoretti/crc @ 4e65ac4.

## Context

The first two attempts to upstream the `TableBasedRegister.update` fast loop
were closed by the maintainer, both times for structural reasons:

- #210: the code was fine, the coverage gate was not — `invoke test.coverage`
  requires 100% and five lines of the new code were never executed.
- #211: "code is not dry, test do not use best practices and does not fit
  existing style" — the per byte loop body was duplicated for the
  `reverse_input` and the normal case, and the new tests were written in a
  pytest dialect that the test module does not use.

So the upstream shape is not only "pure Python" (ADR-041), it is also "one
implementation and one test dialect". The maintainer reviews the diff, not the
intent: a patch that reads like the surrounding code has a chance, a patch that
reads like a second dialect does not.

## Decision

- One loop body. The input reflection of `reverse_input` configurations is
  applied once before the loop with `bytes.translate` and a 256 byte table, so
  the fast loop has no per byte branching and no duplicated body. The
  alternative (index a 256 entry table per byte) was measured ~20% slower
  (crc32 1 MiB: 0.104 s vs 0.122 s) and was dropped.
- One delegation point. Input that is not `bytes` and registers with an own
  `_process_byte` share a single condition and go to `super().update(data)`,
  which makes the off contract behavior identical to the base implementation
  by construction (digests and error messages).
- The repo's own test dialect. `unittest.TestCase`, `subTest`, `namedtuple`
  fixtures and plain assertions in `test/unit/test_crc.py`; no pytest
  fixture, no `parametrize`, no new test file, no new framework.

## Consequences

- The upstream diff is a single implementation of one idea, so the "not dry"
  objection cannot recur; the tests read like the module they live in, so the
  "existing style" objection cannot recur.
- The coverage gate is green by construction: the delegation paths and the
  bypassed `_process_byte` body each have a test that reaches them, so
  `invoke test.coverage` reports 100% (the gate is `fail-under=100`).
- `bytes.translate` needs one extra buffer of the input size while the loop
  runs for `reverse_input` configurations. That trade off is measured,
  disclosed in the PR body and left for the maintainer to weigh.
- A wrong style guess costs a whole review round: the dialect is fixed by
  reading the target repository, not by preference, before the PR is opened.
