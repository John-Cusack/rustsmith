# Port craft checklist: the tight loop

The template every port and every recraft follows. It is the Rust-shaped
version of the craft that paid off upstream on crc (ADR-049: one tight
loop over plain ints instead of boxed items, the per-item dispatch gone,
the variant case hoisted out of the body) — and it is the checklist, not
a formula: **every step ends in a measurement, and a change that does not
measure a win is reverted.**

Worked example with real numbers: "The worked example" at the bottom
(`ludecomp`, lane-fem-a, elmer-rust PR #9).

## 0. Know the hot loop before touching it

- [ ] Name the loop, its asymptotic cost, and what `n` is (nodes, elements,
      matrix dimension, sweeps, string length).
- [ ] Name the corpus: fixed seed, fixed size, one shape.
- [ ] Name the digest: what bytes prove the result is unchanged. If the
      unit has no bench yet, **build the bench first** — an unmeasured
      recraft is a guess. (`elmer-rust`: `bench/README.md`.)

## 1. Hunt the four anti-patterns

| # | anti-pattern | what it looks like in a C/Fortran→Rust port |
|---|---|---|
| A | per-item boxing / allocation | a heap allocation or owned object per loop item; a per-item temporary that could be a primitive |
| B | per-item property access / virtual dispatch | a loop-invariant field re-read per item; an index recomputed per access (`n*i + j`); an indirect call where a direct one suffices |
| C | per-item branching that could be hoisted | a condition inside the body that depends only on outer-loop or mode state |
| D | per-item table lookup where a bulk transform exists | a per-item lookup/scattered access that one sequential pass (or one precomputed copy) replaces |

- [ ] Record the hits with `file:line` **and** the anti-pattern letter.
      A "no" is a result too — it is what keeps the next port from
      re-auditing the same unit.

## 2. Split the work into variants, one change each

- [ ] One change per variant. "Hoist the branch" and "walk a row pointer"
      are **different** variants, not one recraft: on the worked example
      the row pointers were a 1.15x win in one phase and a 0.93x loss in
      another.
- [ ] Never assume the compiler does not already do it. A compiled Rust
      loop already unswitches loop-invariant branches and strength-reduces
      index math; a hand-rolled shape can *block* the unroller the
      original enabled. That is a measured fact on the worked example
      (0.81x for a "branch-free" split).
- [ ] Reject on measurement, not on taste. Keep the variant table — the
      rejected ones are the expensive part of the knowledge.

## 3. Keep the arithmetic identical

The port's contract is byte-identical behaviour; a faster port that
changes a digit is a different port.

- [ ] Same operations, same order. A reciprocal times a multiply is not a
      division: `x / a` stays `x / a`.
- [ ] A cached value may only replace a read that **cannot** change
      between the two reads. Prove it (which writes touch the location?)
      or do not cache it.
- [ ] A store moved out of a loop is only equivalent if no read inside the
      loop depends on the intermediate value. Prove it the same way.
- [ ] Watch the sign of zero: `0.0 + (-0.0)` is `+0.0`, so a leading
      `0.0` accumulator is part of the contract.

## 4. Measure: A/B, one corpus, wall **and** peak RSS

- [ ] Build the *before* and the *after* arm of the **same** bench
      binary; only the port source differs between the trees.
- [ ] Rep-interleave the arms and flip the order every rep.
- [ ] Fresh child process per sample; peak RSS from the parent's `wait4`
      view (a child's own `getrusage(RUSAGE_SELF)` inherits the parent's
      high-water mark and is not the child's footprint).
- [ ] Report median **and** MAD; flag Tukey outliers, never drop them.
- [ ] Compare against a noise band (3%): inside the band the verdict is
      `no-change`, not a win.
- [ ] Report the harness overhead next to the workload number. Never
      subtract it.
- [ ] Report peak RSS both sides. A faster arm that trades memory is a
      regression (`SPEC_STAGE2.md` §10.3: RSS +5% fails).

## 5. Decide: win or no-gain

- [ ] Win: keep the change, with its numbers.
- [ ] No win (inside the noise band) or a loss: **revert the port change**
      and keep the measurement as a no-gain row. A no-gain row is a
      result, not a failure — it is what stops the next attempt from
      repeating the same guess.
- [ ] A digest mismatch between the arms aborts the run. Never report a
      speed for different results.

## 6. Record

- [ ] Machine rows (`results.json` + `rows.md`) next to the port, board
      cells copied 1:1 (ADR-039).
- [ ] The lane evidence file carries the table, the rejected variants, and
      the reproduction commands.
- [ ] The verdict is in the report: `faster` / `no-change` / `slower`,
      labelled with the workload that produced it.

---

## The worked example: `ludecomp` (lane-fem-a, elmer-rust)

Unit `c:fem/src/view3d/LUDecomp.c` (crate `ludecomp`, symbols `ludecomp`
+ `mtrinv`), dense LU and matrix inverse. Source:
https://github.com/John-Cusack/elmer-rust/pull/9

**Anti-patterns found.** B: the C index math `n*i + j` was recomputed on
every access; the pivot candidate was re-read from the matrix on every
candidate of the pivot search. C: the `j != i` test sat inside the
elimination k-loop although it is invariant over it. A/D: the inverse
product stored its partial sum back to `a(i,j)` on every k iteration (an
n^3 store chain) although the value read back from `a(i,j)` is only read
on the first iteration.

**What was kept** (bit-exact, digest identical on both arms):

| change | phase effect |
|---|---|
| pivot candidate in a register | part of the 1.13x LU win |
| `j != i` hoisted out of the elimination k-loop | part of the 1.13x LU win |
| elimination inner loop walks row pointers | part of the 1.13x LU win |
| inverse product: one final store instead of n^3 | 1.09x on that phase |

**What was measured and thrown away:**

| variant | phase median | verdict |
|---|---|---|
| INV(U) original | 3.58 ms | kept |
| INV(U) row base for row i | 3.80 ms | 0.94x, rejected |
| INV(U) row bases for both rows | 3.86 ms | 0.93x, rejected |
| INV(AP) original | 7.06 ms | — |
| INV(AP) hoisted store | 6.49 ms | **kept (1.09x)** |
| INV(AP) split k-range into two branch-free bodies | 8.72 ms | 0.81x, rejected |
| INV(AP) hoisted store + split k-range | 7.31 ms | 0.90x, rejected |

**End to end** (15 reps, one corpus, peak RSS via `wait4`):

| workload | before | after | speedup | verdict | peak RSS before -> after |
|---|---|---|---|---|---|
| LU decompose n=256 | 255.947 ms | 227.187 ms | 1.13x | faster | 2760 KiB -> 2772 KiB (+0.4%) |
| matrix inverse n=256 | 187.181 ms | 179.213 ms | 1.04x | faster | 2760 KiB -> 2748 KiB (-0.4%) |
| LU decompose n=512 | 214.458 ms | 203.146 ms | 1.06x | faster | 5840 KiB -> 5832 KiB (-0.1%) |
| matrix inverse n=512 | 467.345 ms | 421.481 ms | 1.11x | faster | 5816 KiB -> 5824 KiB (+0.1%) |

**And the no-gain.** The same pattern applied to the Jacobi eigensolver
(`c:ElmerGUI/matc/src/jacobi.c`, lane-gui-c) measured **1.01x** (no
change) with the value-caching variant and **0.96x** (slower) with the
variant that also walked row pointers. Both were reverted; the lane keeps
the bench and the no-gain row
(`crates/elmer-gui-c/bench/jacobi-nogain/`). The port source is unchanged.
