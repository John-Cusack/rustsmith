# ADR-044: pre-port headroom probe (`rustsmith headroom`)

Date: 2026-10-14. Status: accepted.

## Context

Download counts cannot tell whether a Rust rewrite will make a package
faster. Two lanes proved the gap after paying full-lane cost:

- charset burned a lane to an honest no-gain: the hot path was already
  saturated by the maintainer's compiled core (rs-upstream-lane-1
  kill-table, 5 behavior-identical candidates, all neutral-or-slower).
- pygments tied its own engine: the hot path already runs in the shared C
  `re` engine (bench W2 lex-only 1.00x, ADR-038).

Both should have been flagged before any port work started. The finder
ranks by downloads × estimated surface (ADR-036) minus curated filters
(ADR-043) — still no measured CPU signal. This ADR adds one: a probe that
scores predicted Rust speedup per candidate workload.

Repo layout note: the brief said `tool/`; the repo convention (ADR-036)
is `crates/` + CLI module, so the probe is `rustsmith-headroom` +
`rustsmith headroom probe`, with workloads under
`crates/rustsmith-headroom/workloads/` and machine-written rows under
`data/headroom/`.

## Decision

New library crate `rustsmith-headroom` + `rustsmith headroom probe`
subcommand (hand-rolled dispatch, `flag()` helpers — the `release.rs`
pattern). Per package:

1. The candidate installs from PyPI into a scratch venv (caller-owned;
   the tool never installs anything itself).
2. A representative workload file — package benchmarks if present
   (`--workload-source package-bench`), else a doc-driven synthetic
   (`doc-synthetic`) defining `run()` (plus optional `setup()`) — runs
   under cProfile (deterministic; py-spy is the preferred sampler and
   py-spy cross-checks are recorded, but bucketing consumes the exact
   per-function attribution cProfile gives).
3. Hot-spot time classifies into five buckets (see rules below) and the
   row scores `pure_python × loop_factor` with three learned caps.

### Bucket rules

- `pure_python` — CPython eval-loop time **including** its C micro-ops
  (`len`, `dict.get`, `list.append`, `re.Match.group`): the headroom a
  Rust rewrite recovers. O(1) fabric vanishes in Rust.
- `native` — O(n) data-crunching already in C (`bytes.find`,
  `_codecs` decoders, `zlib`, …) **plus** frames from the package's own
  compiled modules. mypyc/Cython `.so` files keep their `.py` filenames
  under cProfile, so the driver records the `sys.modules` `__file__` map
  and any at-or-under the probed import root backed by `.so`/`.pyd`
  counts as native (measured, not guessed).
- `io_wait` — socket/file reads, writes, sleeps, locks, unlink: a mirror
  ties on wait.
- `regex` — the shared C `re` engine (`re.py`/`sre_*` frames,
  `re.Pattern`/`re.Match` methods): a port ties with itself.
- `overhead` — import machinery, `codecs` lookup, `abc`/`typing`.

### Loop nature (playbook §7 port boundaries)

The hot path must sit on one side of the port seam: tight buffer loops
port cleanly, dispatch-heavy plugin seams do not. From concentration
(top-1/top-5 self-time share, distinct hot modules in the top-15):
- `tight_loop` ×1.0 — top1 ≥ 25%, or top5 ≥ 55% over ≤3 modules.
- `pipeline` ×0.85 — a small stage pipeline (scanner/reader/parser).
- `dispatch` ×0.5 — ≥8 hot modules or top5 < 25%.
- `thin_native` ×0.15 — thin profile (wall < 2s **and** self-time < 0.5s)
  over a shipped compiled cover: no Python loop left to port.

### Score and verdict

`score = pure_python × loop_factor`, then caps:

| # | Condition (all measured) | Verdict |
|---|---|---|
| 7a | thin profile + package-owned `.so` cover (saturated native core; the residual pure fraction is dispatch skin) | `exclude`, score capped 0.10 |
| 7b | `regex` ≥ 0.35 (shared-engine tie) | at best `upstream_only` |
| 7c | `io_wait` ≥ 0.50 (I/O-bound mainline) | at best `upstream_only` |
| — | score ≥ 0.50 | `rewrite` |
| — | score ≥ 0.25 | `upstream_only` |
| — | otherwise | `exclude` |

Rule 7 (finder): `candidates classify --headroom <rows>` applies one row
per package after the ADR-036/043 rules. The row always rides into
evidence with its measured fractions; it moves the standing **only** when
no curated filter already dispositions the package (curated lane data
outranks one probe workload): `exclude` flags a mirror EXCLUDED (renders
with the non-mirror block, Filter `headroom/exclude`), `upstream_only`
re-paths a mirror to (c) (Filter `headroom/upstream_only`), `rewrite`
never re-paths — it is measured support for the nomination. Non-mirror
rows keep their path. No row = unmeasured = no claim, never a guess.

### Row schema (`data/headroom/<package>.json`, written by the tool)

`package`, `version` (importlib.metadata), `workload{name, source,
iters, detail}`, `tool` (`cprofile`), `machine`, `python`,
`wall_secs`, `tottime_secs`, `fractions{pure_python, native, io_wait,
regex, overhead}`, `concentration{top1, top5, hot_modules}`,
`top[]` (top-8 `file:func`, ms, calls), `loop_nature`, `loop_factor`,
`native_cover[]`, `score`, `verdict`, `why`. Boards render from these
rows; no hand-written markdown carries a headroom claim.

## Validation (all measured on this box, Ryzen 9 5950X, Python 3.12.3)

| Package | Workload (doc-synthetic, cProfile) | wall | pure / nat / io / rx | loop | Score → verdict |
|---|---|---|---|---|---|
| charset-normalizer 3.5.2 | detect-28-blobs, 28× `from_bytes().best()` | 0.02s | 92% / 8% / 0% / 0% | thin_native (cover: `md`+`cd` `.so`) | 0.10 → **exclude** (saturated) |
| pygments 2.21.0 | highlight-synth-120k, 3× `highlight()` 120KB | 2.23s | 46% / 0% / 0% / 54% | tight_loop | 0.46 → **upstream_only** (shared-engine tie) |
| pyyaml 6.0.3 | safeload-4000svc, 3× `safe_load()` pure-Python path | 10.69s | 99% / 0% / 0% / <1% | pipeline | 0.84 → **rewrite** |
| python-multipart 0.0.32 | form-event-heavy, 10× chunked event-heavy form | 0.09s | 87% / 11% / 1% / 0% | pipeline | 0.74 → **rewrite** |

Required outcomes hold: charset rewrite-excluded, pygments capped,
pyyaml and multipart high. py-spy cross-check on the pyyaml workload
(1010 samples, 0 errors): 100% Python frames topped by
`scanner.stale_possible_simple_keys`, `reader.peek`, `parser.parse_node`
— the sampler agrees with the bucketing. The charset residual (92%
"pure") is the dispatch skin over the compiled `md`/`cd` core plus C
codec builtins in 16ms of total self-time: ~0% is not a win, matching
the lane kill-table. The multipart wall (0.09s) is small in absolute
terms and labeled as such — the score is relative headroom, and the
lane's 1.27x on this exact shape corroborates the direction.

## Consequences

- Every new port nomination gets a probe row before the lane launches;
  a row older than the pinned version re-runs (rows carry versions).
- Thresholds are tuned on four workloads: retune only with new measured
  rows in the table, never by editing fractions to fit a verdict.
- The finder gains a seventh signal without touching rules 1–6 or the
  curated tables: measured rows are data, curated rows stay curated.
