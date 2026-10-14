| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 render 16.7KB mixed doc (default preset) (16712 bytes) | representative | 34.145ms | 14.439ms | 2.36x | faster |
| R2 render 16.7KB mixed doc (commonmark preset) (16712 bytes) | representative | 34.333ms | 14.422ms | 2.38x | faster |
| S1 render 280KB bulk doc (default preset) (279687 bytes) | showcase | 587.379ms | 2972.431ms | 0.20x | slower |
| X1 per-crossing overhead (tiny render) (4 bytes) | overhead | 0.097ms | 0.070ms | 1.37x | faster |
