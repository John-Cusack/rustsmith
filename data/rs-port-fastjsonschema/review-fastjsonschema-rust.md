# Pattern review — fastjsonschema-rust (post-lane, reviewer pass)

Date: 2026-10-29. Port: John-Cusack/fastjsonschema-rust @ HEAD.
Oracle: horejsek/python-fastjsonschema @ 738da70.
Evidence: `benches/rows.md` (port repo), lane `bench_port_results.json`,
`bench_tier1_results.json`, upstream suite 631/631 + JSON-Schema-Test-Suite
3908 passed / 435 xfailed / 950 xpassed (identical to stock run),
equiv corpus 195 differential cases green.

Guide consulted at rustsmith `f8c81e4` (pre-lane; expected P1, P6, P10,
P7-opaque, P12; losers: markdown-it 0.20x per-token crossings, pygments
1.00x C-engine dominance, arrow tier-1-first).

## Mechanism (before/after, per hot unit)

Before: per-value interpreter dispatch over generated `validate`
bytecode — `isinstance` chains (34/doc on the workload), two set
constructions + N removes per object, per-option enum chains, plus
exception building on the invalid half. Profile (tier-1 arm E, rep):
62% validate-frame bytecode, 23% isinstance, ~10% exception/message.

After: one PyO3 crossing per `validate(doc)` (one per `validate_many`
batch); the Rust kernel walks the same PyObjects via the C API —
type checks as flag tests, comparisons via `RichCompare`, numerics via
the same `Decimal(repr())` dance, sets via real `PySet` construction
in upstream order, regexes via the same precompiled `re.Pattern`
objects, custom formats via the user's own callables. Message texts
are precomputed static parts + runtime-built dynamic parts through the
same `str()`/`sorted()` calls. Only interpreter dispatch is removed;
no semantic is reimplemented.

## Findings (classified)

1. **Confirm P6** (compile once, run many). Plan compiled once per
   schema (regexes compiled once, thresholds/defs held); 3000-doc
   batches reuse it. Representative 2.44x. No new limit found.
2. **Confirm P1** (whole hot unit crosses once). Per-doc `validate`
   = one crossing (~0.3 µs vs ~2.4 µs valid-path work — above the
   ~1 µs floor, so per-doc crossing pays). x1 track (0.37 µs work)
   shows the floor honestly: 1.58x port vs 1.77x tier-1 — at tiny
   sizes codegen shapes beat the kernel. Batch entry (`validate_many`,
   one crossing per 3000 docs) adds ~6% over the loop on the valid
   track.
3. **Confirm P10** (batch the query). Same as (2): batch geometry
   available, per-doc API preserved; modest gain because per-doc work
   dominates. Confirms with a small number — batching is not always
   the big win; it is the enabler.
4. **Confirm P7** (opaque nodes). `pattern`/`format` regexes run the
   *same* `re` engine via opaque nodes (identical semantics incl.
   look-around the Rust `regex` crate cannot do); custom-format
   callables invoked per value at same cost as upstream. Full speed
   elsewhere. Direct transfer of the ADR-053 marshmallow geometry.
5. **Confirm P12** (engine in Rust, object layer stays Python). Docs
   cross as `PyObject`s, mutated in place for `default`s; exceptions
   are the upstream classes with identical attrs. 4539-test parity is
   the receipt.
6. **NEW (propose, Single): keep semantics in the host — identical
   ops through the C ABI.** Wherever Python semantics are subtle
   (int/float/Decimal/bool tower, `multipleOf` Decimal dance +
   `inifinity` typo, `uniqueItems` normalization, `sorted()`/`str(set)`
   message parts, set-order-dependent key iteration), the kernel
   performs the *same* Python operations on the live objects instead
   of reimplementing them. Cost: a few C-API calls per check; win:
   the port carries 4539-test parity with no semantic drift, and the
   measured 2.44x comes purely from dispatch removal. Loser/limit
   recorded: none yet — no loser observed; confidence Single.
7. **Extend P3 loser-line (in-Rust micro-shape).** N/A — no in-Rust
   recraft attempted. Instead: tier-1 upstream-shape wins (required
   list-comp 1.21x, no-dead-keys 1.14x, combined 1.63x rep) confirm
   P3's "shape first" direction in pure Python. No guide change
   beyond noting the combined 1.63x as another P3-shape receipt.
8. **Pit receipts (propose for anti-pattern ledger):**
   - pyo3 extracts `False` as `usize` 0: check `bool` before `usize`
     when parsing plans (caught by `additionalItems: false` case —
     would have validated index 2 against the root level).
   - Tuple `FromPyObject` rejects lists: parse templates as `Vec`.
   - Transcription traps: `re.sub(r'\Z')` vs upstream `r'\\Z'`;
     source-escape helpers (`e()`/safe-pattern) must NOT leak into
     runtime messages (runtime parses escapes back).
   - Scope-dedup assumption was wrong: upstream `_keys` is built
     once per object level and shared (verified against generated
     code); read the codegen, don't assume sibling-scope rebuilds.

## Checklist answers

- Bundle: port repo HEAD + oracle commit above; excerpts from those
  commits. Figures cited from `benches/rows.md` + lane JSONs with
  workload labels + track. Peak RSS both arms present. No X1 row
  missing (x1 present). Missing: nothing material.
- Mechanism named per hot unit (dispatch removal; crossings counted:
  1 per doc, 1 per batch). PyO3 shapes: borrowed values in, same
  object out; both-trees-alive RSS checked flat.
- Classification above; losers recorded (x1 floor, P10 small win);
  per-port index update + Top-10 proposal below.

## Proposed guide updates (for the guide PR, not applied here)

- Per-port index row: fastjsonschema | P6, P1, P10, P7, P12, NEW
  host-semantics | representative 2.44x rep (1.69x over tier-1) |
  loser: x1 1.58x (crossing floor; tier-1 shape wins tiny sizes) |
  source: fastjsonschema-rust README + benches/rows.md, ADR (lane).
- New pattern entry (Single): "Keep host semantics with identical
  host ops" (mechanism + before/after ≤15 lines each + 2.44x row +
  no-loser-recorded).
- Anti-pattern ledger: bool-first extraction order; Vec-not-tuple
  plan parsing; source-escape vs runtime-message separation.
- Top 10: no change (representative 2.44x does not displace P2/P5;
  NEW pattern enters Hypotheses-adjacent Single list, not Top 10).
