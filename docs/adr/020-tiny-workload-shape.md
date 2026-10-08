# ADR 020: `tiny` workload accepts object or one-element array

- Context: Stage-2 smoke workloads (`tiny`) are a single workload in older
  data entries (`crc`, `strsimpy`, `charset-normalizer`, `python-dateutil`)
  but a one-element array in newer ones (`packaging`,
  `markdown-it-py`, written by analogy with `visible`/`heldout`).
  `workloads_for` deserialized `tiny` as an object unconditionally, so an
  array-shaped `tiny` became `{name:"", setup:"", stmt:"", iters:0}` and the
  profiler harness failed with `IndentationError` on the empty `stmt` —
  halting Stage 2 before Round 0 on the first package that ran it that way.
- Decision: normalize at load — arrays take the first element (error when
  empty), objects pass through, anything else errors honestly. No data
  rewrite: both shapes stay valid.
- Consequences: `packaging`'s latent entry bug is fixed without touching
  its data; older dict-shaped entries load identically.
- Alternatives: rewriting newer entries to objects (leaves the silent
  empty-harness trap for the next author) — rejected.
- Spec: no SPEC change; loader robustness only.
