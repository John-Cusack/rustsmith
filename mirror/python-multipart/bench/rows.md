| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 multipart parse 1.7MB upload (1767432 bytes, 60 parts) | representative | 8.646ms | 4.645ms | 1.86x | faster |
| R2 parse_options_header 3000 (3000 headers) | representative | 50.078ms | 18.413ms | 2.72x | faster |
| S1 multipart parse 17.7MB bulk (17674095 bytes, 600 parts) | showcase | 80.541ms | 45.704ms | 1.76x | faster |
| X1 per-crossing overhead (one header parse) (1 header) | overhead | 0.046ms | 0.038ms | 1.20x | faster |
