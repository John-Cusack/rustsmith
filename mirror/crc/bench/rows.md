| Workload | Track | Baseline | Port | Speedup | Verdict |
|---|---|---|---|---|---|
| R1 crc32 64KB chunk (65536 bytes) | representative | 1308.534ms | 0.640ms | 2044.84x | faster |
| R2 crc16 custom-config 4KB frames x16 (16 x 4096 bytes) | representative | 1206.032ms | 0.694ms | 1737.09x | faster |
| S1 crc64 1MB bulk throughput (1048576 bytes) | showcase | 17941.326ms | 9.725ms | 1844.80x | faster |
| X1 per-crossing overhead (1-byte checksum) (1 byte) | overhead | 0.078ms | 0.033ms | 2.36x | faster |
