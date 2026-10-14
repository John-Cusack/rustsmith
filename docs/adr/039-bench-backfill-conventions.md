# ADR 039: bench-backfill measurement conventions (results.json, seam arms, rrule alloc)

- Context: the backfill spec wants machine-written rows boards render from, but no
  retro-batch render path exists in-repo; rrule query loops allocate ~17k datetimes
  per iter, where tracemalloc distorts timing ~1000x (measured 0.2ms -> 236ms).
- Decision: per package `mirror/<pkg>/bench/{bench.py,results.json,rows.md}` written
  by the driver (board cells copy rows 1:1); seam ports (pyparsing, pyyaml, pygments)
  run both arms in one venv via extension-import blocking; rrule sets
  `BENCH_NO_TRACEMALLOC=1` (RSS stays the memory column, alloc_b null with reason).
- Consequences: 10/10 packages measured, zero DIVERGED digests; rrule rows carry
  alloc=null explicitly rather than traced numbers; `dateutil` keeps the existing
  `bench_parser/{before,after}.json` + `results_rrule.json` pair.
- Alternatives: traced rrule allocs (rejected: measures the tracer, not the workload);
  cross-mirror shared driver module (rejected: mirrors stay self-contained).
- Spec: backfill brief Phase 2; `data/rs-bench-backfill/report.md` holds the tables.
