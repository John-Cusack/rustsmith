# WORKLOAD.md — rs-port-fastjsonschema frozen workload (v1, 2026-10-29)

Per-request API validation against a compiled nested schema.
Frozen for the tier-1 pure-Python bench, any headroom re-probe,
and any later two-track bench. Compile-once cost is excluded from
`run()` by construction (P6 already satisfied upstream).

## Schema (nested order, representative of API validation)

- Top object, `required: [id, total, status, customer, items]`
  (5 required keys — exercises the `set(...) - data.keys()` shape).
- `status`: string enum of 4 (exercises the `isinstance-or` enum chain).
- `customer`: nested object, `required: [name, vip]`, `tags` string array.
- `items`: array of 3 objects, each `required: [sku, qty, price]`
  with `qty` integer ≥ 1, `price` number ≥ 0 (exercises per-item
  object validation; the rerank probe used the same geometry).
- No `additionalProperties`, no `patternProperties`, no custom
  `formats`: the common path where the `_keys` set is built,
  pruned, and never read.

## Corpus

- `NDOCS = 3000` deterministic docs (`ord-{i}`, `total = (i*37) % 5000`,
  status rotates over the 4 enum values, 3 line items each).
- `rep` / `x1`: even `i` valid; odd `i` invalid
  (`items[0].qty = "two"`, a type violation — same as the rerank probe).
- `valid`: all 3000 docs valid (exception-free path; isolates raise cost).
- Same shape as the rerank-3 probe workload
  (`data/rs-rerank-3/wl/fastjsonschema_wl.py`, 600 docs, ~14 ms/run);
  5x docs so the representative track runs ~0.1 s/iter; the harness
  repeats `run()` to size iterations to ~0.5 s.

## Tracks

- **representative** (`rep`): `compile(schema)` once, then validate
  every doc, counting passes (exceptions caught). Kill-criteria track.
- **valid-only** (`valid`): same with the all-valid corpus.
- **X1 overhead** (`x1`): single small schema
  (`{"type": "object", "properties": {"a": {"type": "string"}},
  "required": ["a"]}`) over one doc (crossing-floor reference;
  only meaningful if a Rust port is scoped).

## Arms (tier-1 round)

- A `baseline`: stock `fastjsonschema.compile` per process.
- B `required`: `generate_required` emits a list-comprehension
  missing-key check (`[k for k in [...] if k not in data]`) instead
  of `set([...]) - data.keys()`. Same exception type, message,
  `definition`, and `rule`; set construction moves off the valid path.
- C `keys`: `generate_properties` / `generate_pattern_properties`
  skip the `_keys` set (and per-key `in`/`remove` bookkeeping) when no
  consumer exists at that object level (`additionalProperties`
  absent/True/{} and no `propertyNames: false`); direct
  `if "key" in {variable}` guards instead.
- D `enum`: `generate_enum` emits tuple membership
  (`if x not in ('a', 'b', ...):`) when every enum value is a string,
  replacing the per-option `isinstance-and-==` or-chain.
  Same message, `definition`, `rule`.
- E `combined`: B + C + D.

All tier-1 arms are pure-Python codegen-shape changes; the bench
imports a patched package copy per arm (`scratch/fjs_<arm>`) and
ships the equivalent diff as `tier1.patch`.

## Metrics

Wall seconds (perf_counter, min of 7 runs, each run in a fresh
subprocess) + peak RSS (child `ru_maxrss`, both arms). Same box,
same venv (`scratch/venv`), labeled rows. Box: Ryzen 9 5950X,
CPython 3.12.3, fastjsonschema 2.22.2.
