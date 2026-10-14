| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 version.parse 2000 mixed (2000 version strings) | representative | 16.760ms | 4.115ms | 4.07x | faster |
| R2 marker.evaluate 2000 (2000 marker strings) | representative | 21.774ms | 15.407ms | 1.41x | faster |
| S1 resolver filter 15k versions (15000 versions x 1 specifier-set) | showcase | 75.781ms | 34.249ms | 2.21x | faster |
| X1 per-crossing overhead (one parse + one marker) (2 calls) | overhead | 0.067ms | 0.058ms | 1.16x | faster |
