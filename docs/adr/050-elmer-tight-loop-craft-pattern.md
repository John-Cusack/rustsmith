# ADR-050: the tight-loop craft pattern is the port template for Elmer units

Date: 2026-10-20. Lane: rs-elmer-rustcraft. Target: `John-Cusack/elmer-rust`
(53 ported units: grid-io 8, gui-c 21, fem-a 18, ice 5, fem-b 1) and the
~1100 Elmer units still to be ported. Evidence PR:
https://github.com/John-Cusack/elmer-rust/pull/9

## Context

The craft that paid off upstream on crc (ADR-049: 16.8x CRC-32 5 MiB,
24.7x CRC-16, 4.9x CRC-64, 100% coverage) was one idea, not a technique
catalogue: **one tight loop over plain ints instead of boxed items, the
per-item dispatch gone, and the variant case hoisted out of the body** so
the loop itself has no branching. The same lane (ADR-048) had already
shown that scoping to the *measured* kernel beats a whole-library bid
(5.06x / 8.75x on sympy polys).

The Elmer ports are a different shape from the crc port: they are
C/Fortran→Rust FFI transcriptions — raw pointers, `c_int` counters, the C
index math spelled out at every access, no Rust containers in any hot
path. An audit of the 53 ported units for the four anti-patterns the crc
work exposes found all four, in the C-transcription dialect:

- **A (per-item boxing/allocation)** — per-item heap temporaries and
  per-item stores where one final store does (the `mtrinv` inverse
  product stored its partial sum on every k iteration: an n^3 store chain);
- **B (per-item property access / dispatch)** — the C index math
  `n*i + j` recomputed on every access, and loop-invariant state re-read
  per item (the pivot candidate);
- **C (per-item hoisted-able branching)** — mode/size tests that are
  invariant over their loop (`j != i` inside the elimination k-loop);
- **D (per-item lookup a bulk transform replaces)** — per-item scattered
  accesses where one sequential pass would do.

Two things were missing before any of that could be acted on:

1. **No harness.** The mirror bench drivers (`mirror/<pkg>/bench/bench.py`,
   ADR-039) are Python-venv shaped and measure a Python child; ADR-045
   keeps per-unit grades free of timing claims. "Re-measure the gain" had
   nothing to run on for a Rust port.
2. **No reason to believe the recipe transfers.** A compiled Rust loop
   already unswitches loop-invariant branches and strength-reduces index
   math — the two things the crc recipe removes by hand. The fleet's own
   measurement (below) says the recipe is *not* a formula: applied
   unchanged to the Jacobi eigensolver it measured 1.01x, and one of its
   steps measured 0.96x.

## Decision

1. **The craft ships as a checklist, not a formula** —
   `docs/PORT_CRAFT_CHECKLIST.md`. Six steps, each ending in a
   measurement: know the hot loop and the corpus → hunt the four
   anti-patterns → split the work into one-change variants → keep the
   arithmetic identical → measure A/B (wall **and** peak RSS) → decide
   (win, or revert and record a no-gain row) → record machine rows.
2. **The checklist reaches every future port.** The Elmer porting rules
   are the machine-rendered artifact a port worker actually receives
   (`crates/rustsmith-cli/src/recon.rs` seeds `PORTING.md` from
   `crates/rustsmith-cli/data/repo-content.json` →
   `packages.Elmer.porting_rules`; `crates/rustsmith-cli/src/mirror.rs`
   writes it into the worker bundle). The craft rules are added there, so
   the next ~1100 Elmer units start from the tight version without a
   human step in between.
3. **Every recraft gets a bench in the port repo.** `elmer-rust/bench/`
   (shared A/B driver + `ab.sh` + per-port bench targets): one corpus,
   rep-interleaved arms, a fresh child per sample, median + MAD, peak RSS
   through `wait4`, a result digest that must match on both arms, a 3%
   noise band, and machine rows (`results.json` + `rows.md`) next to the
   port. The digest is the equivalence proof: a mismatch aborts the run,
   because a speed number for different results is meaningless.
4. **A recraft with no measured win is reverted and reported as
   no-gain.** The row stays; the code does not. This is the ADR-049
   discipline (the ~20% slower alternative was measured and dropped, with
   the number in the record) applied to the port side.
5. **The evidence lives with the code.** The machine rows and the lane
   evidence are in `elmer-rust` (the repo that owns the port code); this
   ADR and the checklist carry the table and cite the source, so the
   rustsmith repo never becomes a second copy of a port's numbers.

## Measured evidence

The rows are machine-written in the port repo (source of truth) and copied
to `data/elmer-craft/` here; the ADR-039 convention is that a board cell
copies a row 1:1. 11–15 reps, arms interleaved, one corpus per workload,
peak RSS through `wait4`, digest identical on both arms.

| unit | workload | before | after | speedup | verdict | peak RSS before -> after |
|---|---|---|---|---|---|---|
| `ludecomp` (fem-a) | LU decompose n=256 | 255.947 ms | 227.187 ms | 1.13x | faster | 2760 KiB -> 2772 KiB (+0.4%) |
| `ludecomp` (fem-a) | matrix inverse n=256 | 187.181 ms | 179.213 ms | 1.04x | faster | 2760 KiB -> 2748 KiB (-0.4%) |
| `ludecomp` (fem-a) | LU decompose n=512 | 214.458 ms | 203.146 ms | 1.06x | faster | 5840 KiB -> 5832 KiB (-0.1%) |
| `ludecomp` (fem-a) | matrix inverse n=512 | 467.345 ms | 421.481 ms | 1.11x | faster | 5816 KiB -> 5824 KiB (+0.1%) |
| `matrix` (gui-c) | column sums n=256 | 2.079 ms | 0.437 ms | 4.76x | faster | 2268 KiB -> 2264 KiB (-0.2%) |
| `matrix` (gui-c) | column sums n=1024 | 75.509 ms | 3.830 ms | 19.72x | faster | 9908 KiB -> 9928 KiB (+0.2%) |
| `matrix` (gui-c) | column sums n=4096 | 200.674 ms | 16.126 ms | 12.44x | faster | 132164 KiB -> 132240 KiB (+0.1%) |
| `jacobi` (gui-c) | generalized Jacobi eigen n=128 | 210.291 ms | 208.028 ms | 1.01x | **no-change** | 3096 KiB -> 3080 KiB (-0.5%) |

The `matrix` row is the anti-pattern D case (one strided pass per column
replaced by one sequential pass): the same sums, the same order, the same
bytes — 4.8x to 19.7x. The `jacobi` row is the no-gain: the recraft was
reverted and the port source is unchanged.

## Consequences

- The fleet's craft knowledge is now a checklist plus a worked example
  with numbers, and it is *wired to the worker prompt* through the Elmer
  porting rules — the ~1100 remaining units inherit it.
- A port without a bench is now visibly incomplete: the checklist starts
  at "build the bench first".
- The pattern is explicitly allowed to produce **no gain**. One of the two
  units tried in this lane (the Jacobi eigensolver, lane-gui-c) measured
  1.01x (no-change) and 0.96x (slower) for the two variants tried; both
  were reverted, the lane keeps the bench, and the no-gain row is
  recorded. A pattern that can only say "yes" is not a measurement.
- The recrafted unit keeps byte-identical behaviour: the same operations
  in the same order, proven by the digest on both arms.
- The bench adds `rlib` to two port crates' `crate-type` (a bench inside
  a package links the package's own lib). The `staticlib` archive — the
  artifact the campaign splices — is unchanged.

## Alternatives

- **Apply the crc recipe to all 53 units without measuring.** Rejected:
  the measured evidence says two of the recipe's steps are losses in a
  compiled Rust loop, and the port contract is byte-identical behaviour.
- **Land the checklist as prose only** (playbook section, no machine
  path). Rejected: `prompts/` is read by no code, and the worker's
  prompt is built from `PORTING.md` + the unit bundle, so a prose-only
  rule would not reach the next port.
- **Bump `guidance/optimize.md` to v3.** Rejected for now: that file
  evolves only through `rustsmith learn` plus human approval, and the
  version is pinned per attempt row. The craft rules go to the Elmer
  porting rules (package data, no version pin) and the checklist; the
  guidance bump is a human decision that can cite this ADR.
- **Reuse the Python `mirror/<pkg>/bench/bench.py` driver.** Rejected: it
  toggles a Python extension import and reads a Python child's RSS; a
  Rust port has neither. The protocol is copied (interleaving, median +
  MAD, wait4 RSS, digest equality, noise band), the implementation is
  Rust.
- **Measure with `perf`.** Unavailable on this host
  (`perf_event_paranoid=4`, ADR-003): the wall/RSS harness is the
  fallback the control plane already uses.
