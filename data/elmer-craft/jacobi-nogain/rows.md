| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base->port | Harness overhead base->port |
|---|---|---|---|---|---|---|---|
| generalized Jacobi eigen n=128 (craft variant: value caching + hoisted bounds) | representative | 210.291 ms | 208.028 ms | 1.01x | no-change | 3096 KiB -> 3080 KiB (-0.5%) | 0.062 ms -> 0.063 ms |
