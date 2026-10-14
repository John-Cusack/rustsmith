# rs-upstream-sweep-5 report — python-multipart upstream PR

Target: [Kludex/python-multipart](https://github.com/Kludex/python-multipart) @ `51ff1a4`.
Fork: [John-Cusack/python-multipart](https://github.com/John-Cusack/python-multipart),
branch `rs-upstream-perf`.
Status: UPSTREAM PR OPENED — https://github.com/Kludex/python-multipart/pull/337
(open, `draft: no`, verified via `gh-axi pr view`).

## 1. Target selection (registry re-verified, not cached)

Exclusions hold (pyyaml PR yaml/pyyaml#976 open; charset-normalizer
saturated/killed; packaging no-gain; dateutil PR dateutil/dateutil#1609
open; markdown-it-py PR executablebooks/markdown-it-py#449 open; crc PR
Nicoretti/crc#210 open). Prod PyPI re-queried per lane (sweep-1 lesson):
**python-multipart 0.0.32** (`Development Status :: 5 -
Production/Stable`, live, not yanked) and **pyparsing 3.3.3** (same)
both prod-live. POC-board dl/mo: **python-multipart 313M (#6) >
pyparsing 298M (#7)** — target is **python-multipart**. Hot units
(POC board + this lane's profile): `multipart.py::_internal_write`
(byte state machine), `parse_options_header`. Our side measured
1.86x parse / 2.72x headers (Rust mirror — not shippable upstream, see
ADR-042).

## 2. Upstream process (followed to the letter)

Upstream HEAD `51ff1a4`. Read before changing anything:

- No CONTRIBUTING file, no PR template. No contributor process gates.
- Gates (CI `main.yml` + CodSpeed `benchmark.yml`): `scripts/check`
  (`ruff format --check`, `ruff check`, `mypy`, `check-sdist`, `uv lock`),
  `scripts/test` (coverage pytest), `scripts/rename` (nox: `multipart/`
  legacy-shim parity), pytest benchmarks on 3.11–3.15.
- Build: hatchling, zero runtime deps, zero native files in history;
  `requires-python = >=3.11`. Maintainers merge pure-Python speedups
  with proof (#295, #300, #316, #328, #330, #336).
- Acceptable shape: pure-Python only (ADR-042). The #316
  QuerystringParser dispatch-hoisting is the direct precedent for this
  patch's shape.

## 3. Baseline (their commands, before any change)

`uv sync --frozen` + pytest: **158 passed, 5 deselected
(benchmark-marked)**. Baseline green. Frozen workloads for proof: the
builders in `tests/test_benchmarks.py` (`SIMPLE_FORM`, `LARGE_FORM`
100-part, 8 MiB `FILE_UPLOAD`, 1 MiB CRLF `WORSTCASE_BCHAR`,
`URLENCODED_LARGE`) + differential event oracle (every callback name +
every data payload, whole/multi-chunk/1-byte-chunked).

## 4. Profiling

cProfile over benchmark forms + a 4000-call `parse_options_header`
batch: `MultipartParser._internal_write` dominates event-heavy parses;
inside, `BaseParser.callback` costs a `"on_" + name` concat + dict
lookup + `typing.cast` per emitted event (~8 events/part). Killed
without building: a `parse_options_header` rewrite (hot only in my
synthetic batch, not in their benchmark mix — no workload link);
touching `OctetStreamParser.write` dispatch (3 calls per `write`, not
per byte — negligible).

## 5. The change — ~1.27x on the event-heavy form, memory-neutral

`python_multipart/multipart.py` (+~60/-~15 in `_internal_write`):
hoist all nine multipart callback lookups to once per `write()` with
`_noop_event`/`_noop_data` fallbacks; `data_callback(name, end_i, func,
...)` takes the resolved function. Notification sites call `on_x()`
directly. `OctetStreamParser` untouched. Tests: new
`TestMultipartParser` (`test_no_callbacks`, `test_none_callbacks`
mirroring #316, `test_events_match_across_chunkings` pinning joined
payloads per channel + notification sequence across 1/3/7/64-byte
chunkings). CHANGELOG Unreleased entry with PR link, per convention.

| Workload (interleaved best-of-9, CPython 3.14.2, 2 runs) | Base | Patched |
|---|---|---|
| 100-part form | 432.2 / 426.0 µs | 334.4 / 336.9 µs (**1.26–1.29x**) |
| 2-part form | 13.9 / 13.5 µs | 11.4 / 11.4 µs (1.18–1.22x) |
| 8 MiB upload, 64 KiB chunks | 779.8 / 722.9 µs | 675.0 / 680.0 µs (1.06–1.16x, noisy) |
| 1 MiB CRLF worst case | 108.3 / 101.2 µs | 105.3 / 99.1 µs (~1.02x, unchanged) |
| 8 MiB single `write()` | — | ~1.00x (data-copy bound, unchanged) |

Memory (tracemalloc peak): 3.5 KiB both sides on 100-part and 8 MiB
workloads — identical. One disclosed edge (PR body): callbacks resolve
once per `write()`, so mid-chunk `set_callback` takes effect on the
next `write()` — the semantics #316 established for `QuerystringParser`.

Correctness: fork suite **161 passed** (158 + 3 new), 5 deselected;
differential oracle **0 divergences** (all forms × 1/2/3/7/64-chunk +
1-byte adversarial); `ruff format --check`, `ruff check`, `mypy` clean.
A scratch-prototype sed overreach into `OctetStreamParser.finalize`
was caught by 7 suite failures and reverted before the fork patch
(final diff verified `self.callback` remains only there).

## 6. Proof (benchmark-derived repro in the PR body)

Figures use the repo's own `tests/test_benchmarks.py` builders with a
stated interpreter + interleaved best-of-9 method — reproducible with
stdlib + the package. No tokens, no internal paths in the PR.

## 7. Fork diff

- `python_multipart/multipart.py`: callback hoist + `data_callback`
  rewire (2 commits: code, then CHANGELOG entry with the PR number).
- `tests/test_multipart.py`: +95 (`TestMultipartParser`, 3 tests).
- Branch `rs-upstream-perf` pushed; PR opened from the public fork via
  `gh-axi` (one mistargeted fork-PR #1 closed immediately; correct
  upstream PR is #337 — see §8).

## 8. Checklist deltas (new lessons; standing list unchanged otherwise)

1. **Target upstream explicitly with `gh-axi pr -R OWNER/NAME`.**
   A bare `gh-axi pr create` from the fork clone filed the PR on the
   fork itself (John-Cusack #1, closed unmerged) instead of upstream.
   Worse, adding an `upstream` remote flips gh-axi's repo resolution
   (standard gh behavior), so the fork PR became unreachable by number
   from that clone — it had to be closed with `-R
   John-Cusack/python-multipart`. Future lanes: create with `-R
   <upstream-owner>/<repo> --base main --head
   <fork-owner>:rs-upstream-perf`, and verify the returned URL's owner
   before reporting done.
