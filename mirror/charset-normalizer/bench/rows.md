| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 from_bytes 3 mixed docs (30492 + 36000 + 120727 bytes) | representative | 11.692ms | 4.489ms | 2.60x | faster |
| R2 from_bytes 121KB ascii logs (120727 bytes) | representative | 0.418ms | 1.268ms | 0.33x | slower |
| S1 from_bytes 1MB latin bulk (1048576 bytes) | showcase | 6.297ms | 11.117ms | 0.57x | slower |
| X1 per-crossing overhead (tiny ascii detect) (11 bytes) | overhead | 0.194ms | 0.055ms | 3.54x | faster |
