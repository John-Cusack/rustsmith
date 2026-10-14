| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 scan 8.7KB manifests (8686 bytes, 40 docs) | representative | 47.577ms | 15.648ms | 3.04x | faster |
| R2 safe_load single service (217 bytes, 1 doc) | representative | 1.825ms | 0.599ms | 3.05x | faster |
| R3 dump nested object (~9KB emitted) | representative | 0.820ms | 0.427ms | 1.92x | faster |
| S1 load_all 65KB 300-doc stream (65290 bytes, 300 docs) | showcase | 716.790ms | 166.902ms | 4.29x | faster |
| X1 per-crossing overhead (tiny scan) (1 doc, 14 bytes) | overhead | 0.166ms | 0.135ms | 1.23x | faster |
