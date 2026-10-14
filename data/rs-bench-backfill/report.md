# Bench backfill report (Phase 2)

Machine: AMD Ryzen 9 5950X 16-Core (32 threads), 62GB RAM, Linux 6.14.0-37-generic,
governor `powersave` (no pinning). Python 3.12.3, rustc 1.89.0, maturin 1.14.1.
All figures rep-interleaved medians (MAD spread, Tukey outliers flagged never dropped),
7 reps (pygments 15), 3% noise floor (|delta| below reads no-change).
Memory per row: child-process peak RSS both arms (honest cross-side column) +
tracemalloc Python-level alloc peak (native allocs invisible — favors the ported
side; disclosed per row). Every row carries a parity digest; baseline/port digests
agree on all rows below (zero DIVERGED anywhere). Per-package tables are copied 1:1
from the machine-written `mirror/<pkg>/bench/results.json` (+ `rows.md`);
`mirror/python-dateutil/bench_parser/{before,after}.json` + `results_rrule.json`
for dateutil. Full records (MAD, digests, commands, timestamps) live in that JSON.

## crc — baseline `crc` 8.0.0 (PyPI)

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 crc32 64KB chunk | representative | 1308.534ms | 0.640ms | 2044.84x | faster | 17.4→17.4MB | 27.1→27.1KB |
| R2 crc16 custom-config 4KB frames x16 | representative | 1206.032ms | 0.694ms | 1737.09x | faster | 17.5→17.5MB | 28.4→28.4KB |
| S1 crc64 1MB bulk throughput | showcase | 17941.326ms | 9.725ms | 1844.80x | faster | 18.7→18.7MB | 26.8→26.9KB |
| X1 per-crossing overhead (1-byte checksum) | overhead | 0.078ms | 0.033ms | 2.36x | faster | 18.7→18.7MB | 90.8→90.8KB |

Pure-Python CRC is byte-loop bound; the port moves it to tables in Rust. X1 shows
the call floor both sides pay. MAD% ≤ 2 on all rows.

## strsimpy — baseline `strsimpy` 0.2.1 (PyPI)

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 levenshtein 200 short pairs | representative | 9.698ms | 0.149ms | 65.15x | faster | 17.3→17.3MB | 30.8→30.7KB |
| R2 cosine-2 100 doc pairs | representative | 115.915ms | 12.647ms | 9.17x | faster | 17.3→17.3MB | 31.9→30.1KB |
| S1 metric-lcs 2KB x 2KB | showcase | 11384.140ms | 24.156ms | 471.27x | faster | 85.6→85.6MB | 44045.1→27.4KB |
| X1 per-crossing overhead (one short distance) | overhead | 0.080ms | 0.039ms | 2.07x | faster | 85.6→85.6MB | 91.6→91.5KB |

S1 is the headline: the Python DP table allocates 44MB per call; the port keeps it
in Rust (27KB Python-visible). Only the baseline∩port API intersection is benched
(the port additionally ships JaroWinkler etc.).

## packaging — baseline `pypa/packaging` @ ea25b7d (2026-10-07)

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 version.parse 2000 mixed | representative | 16.760ms | 4.115ms | 4.07x | faster | 19.7→19.7MB | 776.9→811.9KB |
| R2 marker.evaluate 2000 | representative | 21.774ms | 15.407ms | 1.41x | faster | 21.0→21.0MB | 45.6→46.3KB |
| S1 resolver filter 15k versions | showcase | 75.781ms | 34.249ms | 2.21x | faster | 21.0→21.0MB | 107.1→101.2KB |
| X1 per-crossing overhead (one parse + one marker) | overhead | 0.067ms | 0.058ms | 1.16x | faster | 21.0→21.0MB | 297.9→119.1KB |

## charset-normalizer — baseline `charset-normalizer` 3.5.1 (PyPI)

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 from_bytes 3 mixed docs | representative | 11.692ms | 4.489ms | 2.60x | faster | 21.7→21.5MB | 150.3→31.1KB |
| R2 from_bytes 121KB ascii logs | representative | 0.418ms | 1.268ms | 0.33x | slower | 21.7→21.7MB | 137.2→29.4KB |
| S1 from_bytes 1MB latin bulk | showcase | 6.297ms | 11.117ms | 0.57x | slower | 26.3→26.3MB | 2112.1→28.0KB |
| X1 per-crossing overhead (tiny ascii detect) | overhead | 0.194ms | 0.055ms | 3.54x | faster | 26.3→26.3MB | 43.6→52.1KB |

Honest split: the port wins non-ascii detection (R1) but loses where upstream
short-circuits (R2 pure-ascii fast path the port does not take) and on large
non-ascii bulk (S1). Port allocs are lower throughout (native work invisible to
tracemalloc) — the loss is compute shape, not memory.

## markdown-it-py — baseline `markdown-it-py` 4.2.0 (PyPI)

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 render 16.7KB mixed doc (default) | representative | 34.145ms | 14.439ms | 2.36x | faster | 21.0→21.0MB | 379.2→129.2KB |
| R2 render 16.7KB mixed doc (commonmark) | representative | 34.333ms | 14.422ms | 2.38x | faster | 21.4→21.4MB | 379.2→130.4KB |
| S1 render 280KB bulk doc (default) | showcase | 587.379ms | 2972.431ms | 0.20x | slower | 35.3→35.3MB | 5898.9→1338.9KB |
| X1 per-crossing overhead (tiny render) | overhead | 0.097ms | 0.070ms | 1.37x | faster | 35.3→35.3MB | 98.1→102.4KB |

S1 is a real cliff (confirmed reps=7, MAD% 0.2–0.9): per-token Python callbacks
dominate at 280KB while the port allocs 4x less. Small/medium docs win; bulk
single-doc render needs a follow-up (per-crossing batching).

## python-multipart — baseline `python-multipart` 0.0.32 (PyPI)

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 multipart parse 1.7MB upload | representative | 8.646ms | 4.645ms | 1.86x | faster | 21.4→21.4MB | 26.5→26.6KB |
| R2 parse_options_header 3000 | representative | 50.078ms | 18.413ms | 2.72x | faster | 23.6→23.6MB | 2513.7→2545.3KB |
| S1 multipart parse 17.7MB bulk | showcase | 80.541ms | 45.704ms | 1.76x | faster | 53.0→53.0MB | 26.5→26.6KB |
| X1 per-crossing overhead (one header parse) | overhead | 0.046ms | 0.038ms | 1.20x | faster | 53.0→53.0MB | 91.2→107.2KB |

## pyparsing — baseline same tree, `pyparsing._pyparsing` blocked

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 word-list 500 lines | representative | 83.936ms | 85.543ms | 0.98x | no-change | 23.9→23.9MB | 350.8→350.8KB |
| R2 config-lines 500 | representative | 155.166ms | 159.000ms | 0.98x | no-change | 24.9→24.9MB | 747.0→747.0KB |
| S1 charsnotin 1MB scan | showcase | 429.416ms | 2.131ms | 201.50x | faster | 28.9→28.9MB | 1058.6→1058.6KB |
| X1 per-crossing overhead (tiny word parse) | overhead | 0.118ms | 0.119ms | 0.99x | no-change | 28.9→28.9MB | 97.6→97.6KB |

Textbook seam result: grammar overhead dominates real parses (no-change, correctly
under the 3% floor), the ported remainder-scan kernel is 200x at ceiling.

## pyyaml — baseline same tree, `yaml._rust` blocked

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 scan 8.7KB manifests | representative | 47.577ms | 15.648ms | 3.04x | faster | 18.1→18.1MB | 69.8→75.3KB |
| R2 safe_load single service | representative | 1.825ms | 0.599ms | 3.05x | faster | 18.1→18.1MB | 29.9→28.9KB |
| R3 dump nested object | representative | 0.820ms | 0.427ms | 1.92x | faster | 18.1→18.1MB | 27.8→27.9KB |
| S1 load_all 65KB 300-doc stream | showcase | 716.790ms | 166.902ms | 4.29x | faster | 20.3→20.3MB | 1481.7→1540.3KB |
| X1 per-crossing overhead (tiny scan) | overhead | 0.166ms | 0.135ms | 1.23x | faster | 20.3→20.3MB | 93.4→92.8KB |

Supersedes the lane-report figures (different box): scan 3.04x (was 1.9x),
safe_load path 3.05x on a single service doc, dump 1.92x on this shape (lane saw
0.93x on synthetic docs — workload-dependent, both honest), bulk 4.29x.

## python-dateutil — baseline `python-dateutil` 2.9.0.post0 (PyPI)

Parser/iso (`bench_parser/bench.py`, release-gated, parity 0 mismatches both arms):

| Row | Baseline | Port | Speedup | Verdict | RSS base→port |
|---|---|---|---|---|---|
| A-control clean strict | 30.747us | 4.480us | 6.86x | faster | 16.3→17.4MB |
| A-mix strict-try/fuzzy-fallback | 40.044us | 5.062us | 7.91x | faster | 16.2→17.4MB |
| A-noisy fuzzy | 49.201us | 4.706us | 10.46x | faster | 16.2→17.5MB |
| B-noisy fuzzy | 49.403us | 4.693us | 10.53x | faster | 16.2→17.5MB |
| B-noisy fuzzy_with_tokens | 52.117us | 4.815us | 10.82x | faster | 16.2→17.4MB |
| B-clean fuzzy | 31.233us | 4.488us | 6.96x | faster | 16.2→17.5MB |
| iso control | 3.684us | 1.765us | 2.09x | faster | 16.1→17.4MB |
| C-edge fuzzy | 71.309us | 5.682us | 12.55x | faster | 16.2→17.4MB |
| C-edge strict-try/fuzzy-fallback | 98.629us | 7.260us | 13.59x | faster | 16.2→17.5MB |
| D-large-N fuzzy | 53.606us | 5.032us | 10.65x | faster | 16.2→17.5MB |

Supersedes the §5 board figures (different box): fuzzy-noisy ~10.5x (was 5.6x),
strict-parse ~6.9x (was 3.8x), iso 2.09x (was 1.7x). Port RSS runs ~1.2MB above
baseline (extension resident).

rrule queries (`bench_parser/bench_rrule.py`, ADR-021 paths; alloc_n/a —
per-datetime tracemalloc distorts query loops ~1000x, RSS is the column):

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port |
|---|---|---|---|---|---|---|
| R1 between monthly over 2-year daily set | representative | 12.221ms | 2.278ms | 5.37x | faster | 16.8→16.8MB |
| R2 after/before 200 queries | representative | 131.976ms | 22.941ms | 5.75x | faster | 16.8→16.8MB |
| S1 count 10-year hourly set (cold) | showcase | 149.690ms | 17.085ms | 8.76x | faster | 16.8→16.8MB |
| X1 per-crossing overhead (one after query) | overhead | 0.210ms | 0.044ms | 4.75x | faster | 16.8→16.8MB |

Cold counts compare engines fairly; note repeat `count()` on one object favors
upstream `_len` caching (the port recomputes) — workload-shaped, documented in
the driver.

## pygments — baseline same tree, seam-blocked

| Workload | Track | Baseline | Port | Speedup | Verdict | RSS base→port | Alloc base→port |
|---|---|---|---|---|---|---|---|
| R1 highlight 110KB python→HTML | representative | 116.476ms | 113.469ms | 1.03x | no-change | 449.2→449.2MB | 1049.8→1049.8KB |
| R2 lex-only 110KB python | representative | 91.005ms | 91.236ms | 1.00x | no-change | 449.2→449.2MB | 476.9→476.9KB |
| R3 highlight 70KB escape-heavy JS→HTML | representative | 27.978ms | 27.924ms | 1.00x | no-change | 449.2→449.2MB | 682.0→682.0KB |
| S1 regex_opt C-keyword list | showcase | 0.195ms | 0.021ms | 9.32x | faster | 449.2→449.2MB | 43.4→0.8KB |
| X1 per-crossing overhead (tiny highlight) | overhead | 0.032ms | 0.033ms | 0.99x | no-change | 449.2→449.2MB | 3.0→3.0KB |

`re` engine dominates the pipeline (ADR-038): only the import-time builder path
wins. Supersedes the prior 9.8x/1.05x/1.00x (same story, this box).

## Coverage accounting (done-gate)

10/10 ported packages have measured rows: crc, strsimpy, packaging,
charset-normalizer, markdown-it-py, python-multipart, pyparsing, pyyaml,
python-dateutil (parser + rrule), pygments. Zero guessed figures, zero
unmeasured-with-reason rows needed — everything proved measurable.
Per standing rule, crc / strsimpy / markdown-it-py / pyparsing move to the
Ported board (install + measured figures both present); packaging /
charset-normalizer / python-multipart stay parity-graded (measured, no install
lane); pygments stays (measured, publishing owned by release lane).
