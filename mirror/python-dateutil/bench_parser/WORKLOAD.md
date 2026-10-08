# Bench workload contract: dateutil parse port

Frozen real corpus + citable harness methodology. The three original
corpora are blessed unchanged (hashes below); tiers C/D extend breadth
without touching them.

## Frozen corpora

| Tier | File | N | Contract |
|---|---|---|---|
| A/B clean | `corpus_clean.txt` | 10 | every line parses under strict `parse()` |
| A/B noisy | `corpus_noisy.txt` | 10 | every line raises strict, parses `fuzzy=True` |
| Boundary | `corpus_boundary.txt` | 18 | exact accept/reject pins (`--mode parity`) |
| C edge | `corpus_edge.txt` | 16 | bench-only; all parse `fuzzy=True`, strict may raise |
| D large-N | `corpus_large.txt` | 2000 | bench-only log-ingest bulk; all parse `fuzzy=True` |

Tier D generator (deterministic, seed-fixed): 2000 lines sampled from 8
log-shaped templates over host/service/month/weekday variants
(`random.Random(20240115)` over `range(100000)`). Junk carries no digits
on purpose: stray digits in trailing junk change fuzzy accept/reject, so
digit-bearing variables (ports, IPs, ticket ids) are excluded and every
generated line is verified `fuzzy=True`-parseable before freezing.

## Five-property blessing

- **Relevance.** Track A mirrors log ingest (80/20 clean/noisy mix plus a
  100%-clean control); track B is the adversarial fuzzy ceiling
  (junk-word runs, dot-runs, `fuzzy_with_tokens`); tier C covers
  tokenizer-hostile one-liners; tier D covers production bulk volume.
- **Reproducibility.** Corpora frozen by SHA-256 (`MANIFEST.sha256`);
  fixed `default=datetime(2000, 1, 1)`; exact command below; every figure
  record carries `{median, spread, reps, iters, host, commit, command}`.
- **Fairness.** Strict-capable lines go through the strict path; noisy
  lines never do (they raise by contract). No workload is shaped to favor
  either side of a comparison.
- **Verifiability.** `--mode parity` pins accept/reject per line;
  contracts fail the run (exit 2) before any timing prints; JSON output
  carries all rep samples plus Tukey fences for independent recomputation.
- **Usability.** One command reproduces the table; `--out` JSON feeds
  `--baseline` before/after verdicts; per-row memory columns travel with
  each record.

## Exact repro command

```sh
python bench_parser/bench.py --reps 7 --iters 2000 --warmup 2
python bench_parser/bench.py --reps 7 --iters 2000 --warmup 2 --out after.json
python bench_parser/bench.py --mode parity
python bench_parser/bench.py --baseline before.json --out after.json --no-mem
```

## Methodology (enforced by `bench.py`)

- Report **median + MAD** over reps, never min/best-of.
- Tukey-fence (1.5 x IQR) outliers are **flagged, never dropped**.
- Explicit noise threshold `--noise-pct` (default 3.0%): |delta| below it
  reads **no-change** in `--baseline` verdicts.
- Rep-major **interleaving** (direction alternates each rep) with
  `--warmup` reps executed and discarded.
- Memory per row from `peak_memory()`: fresh child process, `rss_kb`
  (covers both sides) + tracemalloc `alloc_peak_b`. Blind spot, stated
  honestly: tracemalloc sees Python-level allocs only; native (Rust-side)
  allocs are invisible to it, favoring the ported side on allocation
  deltas. `rss_kb` is the honest cross-side column.

## Host checklist (doc-only until a staged container allows enforcement)

Record `host`/`commit`/`command` come free with every record. For runs
you intend to cite, additionally note: CPU governor setting, task
pinning (`taskset`) where permitted, tmpfs vs disk for the checkout,
and anything else sharing the box. Re-run the baseline on the same host
before quoting a delta.
