# ADR-005: sequential run-batch (concurrency deferred)

`run-batch` runs repos sequentially, one pipeline at a time.
Concurrent runs need cgroup isolation + quiesced wall-clock grading (unscoped here).
`max_parallel_runs` stays a config value; parallel execution is a later milestone.
Proceed: sequential batch; revisit when the grading host can quiesce per run.
