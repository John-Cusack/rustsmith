# POC run: `packaging` (README KEEP #1)

Port target: `packaging` — PEP 440/508 parsing + marker/tag evaluation. Ranked #1 by
downloads (1.56B/mo) with no single stable full-feature Rust equivalent
(`pep440_rs` versions-only, `pep508_rs` markers-only, `uv-pep440` unstable internal).

## Setup (run from this worktree root)

```sh
cargo build --release
```

You are running alongside two sibling runs (charset-normalizer, elmerfem). Isolation is
by explicit dirs — do NOT reuse their `--fork`/`--work`/`--store`/`--run-id`.

## Run

```sh
./target/release/rustsmith run \
  --repo https://github.com/pypa/packaging \
  --fork "$HOME/runs/packaging-fork" \
  --work "$HOME/runs/packaging-work" \
  --store "$HOME/runs/packaging-store.db" \
  --run-id packaging-poc \
  --stage full
```

## Watch / follow up

```sh
./target/release/rustsmith status --run-id packaging-poc --store "$HOME/runs/packaging-store.db"
./target/release/rustsmith report --run-id packaging-poc --store "$HOME/runs/packaging-store.db"
```

## Scope notes

- Hot paths (resolver hot loop): `src/packaging/version.py` (`Version.parse`),
  `src/packaging/markers.py` (`Marker.evaluate`), `src/packaging/tags.py` (`sys_tags`).
- Pure Python, small surface, no C extensions — expect recon + mirror to go clean;
  the win is Stage 2 optimize on version/marker evaluation.
- `src/packaging/benchmarks/` + `perf.yml` exist upstream — use them as the
  workload reference, not microbenchmarks you invent.
- Success bar: 100% oracle parity, held-out divergence under threshold, then a
  measured Stage 2 speedup on version/marker/tag workloads. Report the
  `RUSTSMITH_REPORT.md` parity + speedup numbers back, not vibes.

## Do not

- Do not touch `--fork`/`--work`/`--store` paths belonging to the sibling runs.
- Do not push anywhere. The fork dir is a local git repo (`main`); publishing is a
  separate manual step after review.
