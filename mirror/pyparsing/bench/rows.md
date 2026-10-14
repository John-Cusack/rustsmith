| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 word-list 500 lines (500 lines x ~60 chars) | representative | 83.936ms | 85.543ms | 0.98x | no-change |
| R2 config-lines 500 (500 lines x ~80 chars) | representative | 155.166ms | 159.000ms | 0.98x | no-change |
| S1 charsnotin 1MB scan (1048576 chars) | showcase | 429.416ms | 2.131ms | 201.50x | faster |
| X1 per-crossing overhead (tiny word parse) (1 token) | overhead | 0.118ms | 0.119ms | 0.99x | no-change |
