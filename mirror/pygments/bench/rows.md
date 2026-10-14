| Workload | Track | Rust | Fallback | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 highlight 110KB python->HTML | representative | 113.469ms | 116.476ms | 1.03x | no-change |
| R2 lex-only 110KB python | representative | 91.236ms | 91.005ms | 1.00x | no-change |
| R3 highlight 70KB escape-heavy JS->HTML | representative | 27.924ms | 27.978ms | 1.00x | no-change |
| S1 regex_opt C-keyword list | showcase | 0.021ms | 0.195ms | 9.32x | faster |
| X1 per-crossing overhead (tiny highlight) | overhead | 0.033ms | 0.032ms | 0.99x | no-change |
