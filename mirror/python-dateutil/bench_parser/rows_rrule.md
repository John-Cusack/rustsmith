| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 between monthly over 2-year daily set (731 occurrences, 24 window queries) | representative | 12.221ms | 2.278ms | 5.37x | faster |
| R2 after/before 200 queries (200 queries over 365-occurrence set) | representative | 131.976ms | 22.941ms | 5.75x | faster |
| S1 count 10-year hourly set (cold) (87600 occurrences) | showcase | 149.690ms | 17.085ms | 8.76x | faster |
| X1 per-crossing overhead (one after query) (1 query) | overhead | 0.210ms | 0.044ms | 4.75x | faster |
