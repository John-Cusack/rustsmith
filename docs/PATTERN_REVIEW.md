# Pattern review — the repeatable process

How [`RUST_PATTERNS.md`](RUST_PATTERNS.md) stays living: after every
port or recraft, an LLM review pass reads the before/after code plus
the measured results and proposes guide updates. Every new port
brief consults the guide before building. This file is procedure;
the guide is the artifact. No LLM calls in gates — the review pass
proposes, the normal PR process disposes (AGENTS.md).

## When it runs

1. **After every port lane closes** (graded run + retro exist), and
   after every recraft/optimization round that measured a win *or*
   a no-gain. A no-gain round still produces a review: losers are
   guide updates too (anti-pattern ledger, "does NOT help" lines).
2. **Before every new port brief is written**: the brief author (or
   the scouting pass) consults the guide and the brief names which
   patterns it expects to apply and which losers it must avoid.
   This is a checklist item in the brief template, below.
3. **On pattern retirement pressure**: when a measurement
   contradicts a guide claim, the review that noticed it must
   either amend the claim's scope or open the retirement path —
   never leave a falsified claim standing.

## The review bundle (inputs)

One directory per review, assembled by
[`scripts/gather_pattern_review.sh`](../scripts/gather_pattern_review.sh)
so the reviewer (LLM or human) reads one bundle, not eleven repos:

```sh
scripts/gather_pattern_review.sh <port> <original-dir> <rust-dir> <out-dir> [--hot <symbol>]...
# e.g. scripts/gather_pattern_review.sh sqlparse ~/src/sqlparse-pure ~/src/sqlparse-rust /tmp/review-sqlparse
```

The bundle contains (script is the source of truth for the layout):

- `MANIFEST.md` — port, both repos' HEAD commits, date, machine,
  every file included and why.
- `before/` + `after/` — the hot-path files from the original and
  the port (same relative role: lexer↔lexer, engine↔core), plus
  the seam/binding files that decide crossings.
- `evidence/` — bench rows (`bench/rows.md`, `benches/rows.md`,
  two-track output), headroom row (`data/headroom/<port>.json`),
  retro, scope/boundary ADRs, upstream-review report. Whatever
  exists; missing items are listed in the manifest as **missing**,
  never silently absent.
- `numbers.md` — every speedup/RSS figure the script could extract
  from the evidence, verbatim with its workload label and file:line
  provenance. The reviewer cites from here; it does not re-measure
  or re-type numbers from prose.

If evidence is missing (no bench rows, no RSS column, no X1
overhead row), the review's first output is that gap — the port is
not pattern-eligible until measured. "Not benchmarked" is a valid
bundle state; an unlabeled number is not.

## The LLM review pass (steps)

Run as a normal lane task (scout/report shape — deliverable is a
proposed diff + report, never a direct edit to `main`):

1. **Read before/after side by side.** For each hot unit: what did
   the original do per item (allocation, dispatch, branching,
   crossings), what does the Rust do per item? Name the mechanism,
   not the language ("one loop over u64" — not "Rust is fast").
2. **Attach numbers or downgrade.** Every mechanism claim must cite
   a row from `numbers.md` (workload-labeled speedup + peak RSS).
   A mechanism with no row goes to Hypotheses wording in the
   proposal, clearly labeled — or is dropped.
3. **Classify against the guide**, one of four outcomes per finding:
   - **Confirm** — matches an existing pattern: add this port to its
     evidence list; bump confidence if it crosses a threshold
     (Single→Medium at 2 ports, Medium→High at 3).
   - **Extend** — same pattern, new scope limit or new loser:
     amend "does NOT help" with the losing row.
   - **New pattern** — mechanism not in the guide, measured:
     draft a full entry (problem shape, technique, before/after
     excerpt ≤ ~15 lines each side, measured rows, loser or
     "no loser recorded yet", confidence Single).
   - **Retire / narrow** — a guide claim this port falsified or
     failed to reproduce: propose the narrower claim with both the
     old and new rows kept. Retirement deletes the *claim*, never
     the evidence; the anti-pattern ledger gains the receipt.
4. **Update the per-port index + Appendix A** in the same diff:
   patterns found, representative figure, loser, source. A port
   with no index row is invisible to the next brief — that is the
   failure this process exists to prevent.
5. **Re-rank the Top 10** only on representative-track rows.
   Showcase rows never promote a pattern in the table.
6. **Emit the proposal as a PR** (docs-only, or docs + the bundle
   script if it changed): PR body lists per-port findings and any
   Top-10 movement with the rows that caused it. Port PRs and
   tool/guide PRs stay separate (AGENTS.md); the guide PR names
   the port PRs it consumed.

## The pre-port consult (brief checklist)

Every new port or recraft brief must include, before build steps:

- [ ] Guide consulted on <date> at rustsmith commit <sha>.
- [ ] Patterns expected to apply: P__ (why this port's hot path
      matches the problem shape), P__ …
- [ ] Losers to design around: the "does NOT help" rows nearest
      this port's shape (name the receipt, e.g. "markdown-it 0.20x
      bulk → no per-token crossings; batch the token stream").
- [ ] Headroom probe row exists (`data/headroom/<port>.json`) or
      is step 1 of the lane (P11). Score/verdict: ____.
- [ ] Kill criteria written in guide terms: "below __x on
      representative __, or pattern P__ fails to reproduce →
      honest no_gain, walk away."
- [ ] Bench plan covers: representative + showcase + X1
      per-crossing rows, wall **and** peak RSS both arms, every
      figure workload-labeled (the rows the future review bundle
      will extract).
- [ ] Post-lane pattern review is in the done-gate (bundle +
      review PR), next to the retro.

## Review checklist (for the reviewer / LLM pass)

Copy into the review report; every box answered, "n/a — why" is an
answer, blank is not.

**Evidence integrity**
- [ ] Bundle manifest lists both HEAD commits; excerpts come from
      those commits, not a working tree.
- [ ] Every figure cited appears verbatim in `numbers.md` with
      workload label + track (representative/showcase/overhead).
- [ ] Peak RSS present both arms; alloc figures never cited as
      wall. Cross-box figures kept separate and labeled.
- [ ] Missing evidence named (no bench / no RSS / no X1) — port
      marked not-yet-eligible for the affected claim.

**Mechanism, not vibes**
- [ ] Each finding names the per-item mechanism changed
      (allocation, dispatch, branch, crossing, access order,
      algorithm, memory shape) with before/after excerpts.
- [ ] FFI crossings counted before/after (per item? per call?
      per batch?) — P1/P10 verdict explicit.
- [ ] PyO3 conversion shape named (scalars / flat descriptors /
      object graph; both-trees-alive RSS risk checked).
- [ ] Algorithm class compared (same algorithm faster vs
      different algorithm — sympy GCD receipt).

**Guide bookkeeping**
- [ ] Each finding classified Confirm / Extend / New / Retire.
- [ ] Losers recorded with numbers, including "pattern applied,
      measured no-change" (1.00x is a finding).
- [ ] Confidence changes justified by independent-port count.
- [ ] Per-port index + Appendix A updated; Top 10 re-ranked only
      on representative rows; hypotheses quarantined as unmeasured.
- [ ] No oracle/gate/threshold touched to make a number fit
      (AGENTS.md — a gate failure is feedback, not a workaround).

## Cadence and ownership

- The lane that produced the port owes the bundle; the review pass
  is a separate short lane (fresh eyes beat the port author's).
- Guide PRs are docs-only and merge on the same rigor as any PR:
  `cargo build --release`, `cargo test`,
  `cargo clippy -- -D warnings` where code (the bundle script)
  changed, and a smoke run of the script on a real port.
- Spec deviations discovered during review get an ADR in
  `docs/adr/` (AGENTS.md), cross-linked from the pattern entry.

## Worked example (this guide's own creation)

The 2026-10-28 creation pass ran this process retroactively over
18 ports: local mirrors (`mirror/*`) + cloned public mirrors
(John-Cusack/*-rust, elmer-rust at the commits in Appendix B of the
guide), bench-backfill rows, headroom rows, retros and ADRs as the
evidence set. Outcome: 12 patterns (7 High / 2 Medium / 1 Single /
2 process-load-bearing), 8 anti-pattern receipts, 5 hypotheses
quarantined. Every future pass should be *smaller* than this one —
one port, one bundle, one proposed diff.
