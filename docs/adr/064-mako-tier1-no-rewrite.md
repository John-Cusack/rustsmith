# ADR-064: mako tier-1 measured, no Rust rewrite (no_gain)

Date: 2026-10-29. Lane: rs-port-mako (brief: rerank-3 §2 rank 3, 181M dl/mo,
headroom 0.83 rewrite). Outcome: **no_gain with numbers** — same disposition
class as arrow (ADR-059) and pyasn1 (ADR-060).

## Frozen workload (lane `WORKLOAD.md`)

Server steady-state: templates compiled once; measured unit is `render()`
only. Representative: inheritance page (base + child, `row` def x15 rows,
filters `|h`, `% for`, `% if`) + mail template (loop + nested def), 200
contexts x100 repeats = 40000 renders, ~1.1 s on Ryzen 9 5950X / CPython 3.12.3.
Mako 1.4.3, markupsafe with C speedups active.

## Tier-1 (pure Python, upstream-plausible; suite-identical)

- P1 `runtime._kwargs_for_callable`: argspec is a pure function of the
  immutable generated callable but was recomputed from `co_varnames` on
  every render → memoize per callable (same return values, verified).
- P2 `util.FastEncodingBuffer`: `deque` → `list` backing (`append`/`join`
  only; no `popleft` anywhere in the codebase).
- Measured: baseline 1.1406 s → tier-1 1.1238 s = **1.015x** (min-of-7,
  checksums identical 30756000). Patch: lane `tier1-runtime.patch`
  (upstream-ready note, no upstream PR opened).
- Upstream suite: **562 passed + 53 skipped on both arms** (stock sdist
  1.4.3 test dir; tier-1 run on patched copy, only `runtime.py`+`util.py`
  differ).
- Analyzed but declined P3: skip `_push_frame`/`_pop_frame` for defs with
  no `<%caller%>` use (~15% wall in a caller-breaking no-op probe). Correct
  implementation needs parsetree caller-use analysis in codegen; not cheap,
  not attempted. Recorded as bound, not as gain.

## Ceiling recomputed against tier-1 (kill math)

| arm | wall (min-of-7) | vs baseline | note |
|---|---|---|---|
| baseline | 1.1406 s | 1.00x | RSS 24272 KB |
| tier-1 (P1+P2) | 1.1238 s | 1.015x | RSS 25212 KB, suite-identical |
| machinery-deleted probe (tier-1 + no-op frames; breaks `<%caller%>`) | 0.9572 s | 1.19x | upper bound of ANY machinery port |
| semantic floor (hand-written Python, byte-identical output) | 0.3925 s | 2.91x | unreachable with a Python seam |

- Realizable-port bound: the port must keep generated code + user
  expressions (`${...}`, filters, `%` formatting, `str()`, subscripts) in
  Python behind the seam — that is the entire remaining 0.96 s. Best
  realizable ≈ machinery-deleted arm = 1.1238/0.9572 = **1.17x over
  tier-1 at zero FFI toll** → **< 2x: KILL** (brief kill criterion met).
- The 2.44x "generous" bound (machinery-deleted → hand floor) requires Rust
  to match hand-optimized Python while evaluating user code per node —
  physically impossible; documented only to show even the
  physics-violating bound barely clears 2x.
- FFI toll inverts the rest: a Rust `Context`/buffer turns every
  `__M_writer` (currently one C `list.append`, ~50 ns) into a PyO3 call
  (~70–100+ ns). ~220 writes per render-pair × 40000 pairs makes the toll
  larger than the entire portable share — expected port outcome ≤1.0x
  (markdown-it 0.20x bulk receipt, same P1 violation shape).

## Why the hot path is not portable (mechanism)

Per render-pair (~28.5 µs at bench scale): ~220 buffer writes (already C),
user expressions per node (dict subscripts, `"%.2f" %`, `str()`,
`markupsafe.escape` already C via `_speedups`), frame push/pop + context
gets + closure creation (needs codegen change, P3 declined). Nothing remains
that is both (a) executed in Python and (b) independent of user code.
Whole-template crossing is impossible: nodes ARE user Python (P1/P10 floor
rule: per-call work is ns-scale appends, far below the batching floor, and
cannot be batched because each node runs arbitrary Python).

## Standing lesson (extends arrow ADR-059)

Template-render pipelines where the generated code interleaves host-language
expression evaluation per node are a **no-rewrite family**: the seam keeps
100% of the per-node work, the portable share is dispatch-only (~20% here),
and the crossing toll is per-node. Headroom "pure 98%" misleads when the
pure-Python time is user-expression evaluation rather than portable
machinery. Recommended headroom-rule refinement: discount pipeline rows
whose top frames are generated-code `render_*` + expression helpers unless
a seam analysis shows batchable compute.

## Disposition

- No port code. `John-Cusack/mako-rust` repo holds a README recording the
  no_gain outcome (no Rust, no bindings).
- Publish state: n/a (nothing to release). Registry credentials never needed.
- Pattern review: adds the template-render receipt to RUST_PATTERNS.md
  ("does NOT help" for P1/P10) — proposed in the evidence PR.
