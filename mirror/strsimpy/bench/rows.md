| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 levenshtein 200 short pairs (200 pairs x 10-40 chars) | representative | 9.698ms | 0.149ms | 65.15x | faster |
| R2 cosine-2 100 doc pairs (100 pairs x 200-800 chars) | representative | 115.915ms | 12.647ms | 9.17x | faster |
| S1 metric-lcs 2KB x 2KB (2 x 2048 chars) | showcase | 11384.140ms | 24.156ms | 471.27x | faster |
| X1 per-crossing overhead (one short distance) (2 x 6 chars) | overhead | 0.080ms | 0.039ms | 2.07x | faster |
