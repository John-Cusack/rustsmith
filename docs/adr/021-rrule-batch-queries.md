# ADR 021: rrule window queries batched in Rust; per-item path kept as slow fallback

- Context: audit V2 (`data/rs-dateutil-reaudit/report.md`): `between`/`after`/
  `before`/`count` routed through the per-item pull/merge machinery
  (`rrule_mod.rs` `EnginePull::__next__` + `pull_merge`), paying a Python
  datetime construction plus 1–3 rich compares per candidate while the
  engine step underneath is nanoseconds. Verified at fix time: `q_between`
  (and siblings) consumed `QueryPull` one item at a time — no batching.
- Decision: implement the four queries as single crossings that run the
  engine filter in Rust over wall triples (`Wall` ord/hh/mm/ss/us), with
  integer bound prefilters, Python confirmation inside an adaptive margin
  band (measured `utcoffset`s + slack, ≥3d), and one materialization per
  surviving item. Single-tz fold-0 sets merge by wall; anything else
  (mixed naive/aware, differing tz, folded date points, non-datetime
  members/bounds, engine errors) returns `Slow` before touching shared
  state and reruns the original pull logic, reproducing even error/no-error
  edges exactly. Cache fill mirrors the 10-block/`_len`/`_complete`
  semantics (answer objects reuse the cached prefix, preserving identity).
  `__next__`/iteration/slices/`xafter`/`__contains__` keep the per-item
  path, documented as the slow path.
- Consequences: 1.6–4.2x on the rrule query bench (count ~4x, after/before
  ~4x, between ~2x, sets ~2.5–4x; iteration unchanged); parser corpora
  unchanged within run noise. Cross-source merge order is wall order, which
  can differ from instant order only inside nonexistent spring-forward gap
  walls with cross-source interleaving there (uncovered by the suite;
  accepted residual, documented in code).
- Alternatives: full-instant merge via per-candidate `utcoffset` calls —
  rejected (reintroduces per-item crossings); detaching the GIL for engine
  stepping — deferred to the V3 task.
