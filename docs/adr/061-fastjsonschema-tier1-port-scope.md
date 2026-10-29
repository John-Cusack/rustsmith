# ADR-061: fastjsonschema lane — tier-1 measured (1.63x), rewrite GO (2.44x), oracle and parity decisions

Date: 2026-10-29. Lane: rs-port-fastjsonschema-01 (fastjsonschema, 108M dl/mo).

## Context

Rerank-3 §3 Lane 3 prescribed the marshmallow seam (whole-batch
crossing + opaque nodes, ADR-053) with tier-1-first sequencing and a
<1.5x kill bar over the tier-1 baseline. Standing lessons: arrow
(ADR-059, tier-1 1.99x vs 1.78x ceiling) and pyasn1 (ADR-060, tier-1
1.17x vs 1.83x ceiling) both ended no_gain after measuring tier-1
first.

## Measurements (Ryzen 9 5950X, CPython 3.12.3, frozen WORKLOAD.md v1)

- Tier-1 (pure-Python codegen shapes, [`bench_tier1.py`](https://github.com/John-Cusack/fastjsonschema-rust/tree/main/bench) in fastjsonschema-rust — relocated out of `data/rs-port-fastjsonschema/`, min-of-7):
  rep **1.63x**, valid 1.76x, x1 1.98x (combined arm E; B/C/D arms
  record the split). Upstream-ready patch note ([`upstream-patch-note.md`](https://github.com/John-Cusack/fastjsonschema-rust/blob/main/tier1/upstream-patch-note.md), patch [`tier1.patch`](https://github.com/John-Cusack/fastjsonschema-rust/blob/main/tier1/tier1.patch); no PR opened).
- Rewrite ceiling vs tier-1 baseline: floor (C-API traversal +
  identical-exception costs, conservative) 0.0571 s vs E rep
  0.2192 s → **3.84x**. Kill bar (<1.5x) cleared with margin → GO.
- Port (release build, `benches/rows.md`): rep **2.44x vs stock,
  1.69x vs tier-1**; valid 2.50x; x1 1.58x; batch showcase
  `validate_many` ~6% over the per-doc loop. RSS flat all arms.

## Decisions

1. **Oracle is clone HEAD 738da70, not the 2.22.2 sdist.** The test
   suite runs against the clone, which is newer and behaves
   differently: numeric guards exclude bools, property names double
   braces, `propertyNames: false` is dict-nonempty, FORMAT_REGEXS
   (date-time/ipv6/time) updated. The port matches the clone; the
   rerank's 2.22.2-based headroom numbers transfer (shapes unchanged
   on the workload path).
2. **Per-doc crossing, not batch-only.** The representative use is
   per-request `validate(doc)`; per-doc work (~2.4–3.5 µs) sits well
   above the ~0.3 µs crossing floor, so one crossing per doc pays
   (2.44x). `validate_many` (one crossing per batch) is offered as a
   showcase entry (+~6%). x1 honestly shows the floor (1.58x port vs
   1.77x tier-1 at tiny sizes).
3. **Opaque nodes for `pattern`/`format`/custom formats** (ADR-053
   geometry verbatim): the same `re` engine (look-around the Rust
   `regex` crate cannot do) and the user's own callables run per
   value; full speed elsewhere.
4. **Upstream-bug parity.** `const` + `detailed_exceptions=False`
   crashes upstream at compile with `KeyError('definition_rule')`;
   the port replicates it. The `contentEncoding` dead
   `if variable == ""` check (bytes vs str, never fires) is omitted.
   Brace-containing `patternProperties`/`additionalProperties` keys
   (upstream `KeyError` crash) render literally in the port.
5. **Depth guard 4000.** Upstream recurses per `$ref` without a
   guard; the port caps nesting depth (no test approaches it) and
   would raise `RecursionError` instead of growing the native stack.

## Consequences

- Gates: full original suite 631/631 via import alias + the
  JSON-Schema-Test-Suite the project runs (3908 passed / 435 xfailed
  / 950 xpassed, identical to the stock run) + seeded equivalence
  corpus (832 differential rows incl. messages/paths, plus the
  compile-time KeyError parity) green.
- Pattern-ledger input: `review-fastjsonschema-rust.md` (proposes a
  NEW Single pattern — identical host ops through the C ABI — plus
  three pit receipts).
- Publish: TestPyPI → prod PyPI + crates.io pending registry
  credentials (reported blocked with the exact missing item if
  absent at that step).
