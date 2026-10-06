# POC run: `charset-normalizer` (README KEEP #2)

Port target: `charset-normalizer` — universal encoding detection. 1.15B dl/mo, strongest
speedup thesis on the board: `from_bytes` brute-force-decodes all candidate encodings,
then `mess_ratio`/`coherence_ratio` score every char. No full-IANA-parity Rust detector
(`chardetng` legacy-web only, `charset-normalizer-rs` a subset, `encoding_rs` no
detection at all).

## Setup (run from this worktree root)

```sh
cargo build --release
```

You are running alongside two sibling runs (packaging, elmerfem). Isolation is by
explicit dirs — do NOT reuse their `--fork`/`--work`/`--store`/`--run-id`.

## Run

```sh
./target/release/rustsmith run \
  --repo https://github.com/Ousret/charset_normalizer \
  --fork "$HOME/runs/charset-fork" \
  --work "$HOME/runs/charset-work" \
  --store "$HOME/runs/charset-store.db" \
  --run-id charset-poc \
  --stage full
```

## Watch / follow up

```sh
./target/release/rustsmith status --run-id charset-poc --store "$HOME/runs/charset-store.db"
./target/release/rustsmith report --run-id charset-poc --store "$HOME/runs/charset-store.db"
```

## Scope notes

- Hot paths: `src/charset_normalizer/api.py` (`from_bytes` — the decode-all loop),
  `src/charset_normalizer/md.py` (`mess_ratio`), `src/charset_normalizer/cd.py`
  (`coherence_ratio`). The language-model tables are data — classify `port`, keep
  them byte-identical, do not "optimize" them.
- Pure Python, detection quality is the correctness bar: oracle parity + held-out
  divergence matter more here than elsewhere. Any Stage 2 win that changes detection
  outcomes is a failure, not a speedup.
- Success bar: 100% oracle parity with identical detection results on the held-out
  suite, then measured Stage 2 speedup on multi-encoding detection workloads.
  Report the `RUSTSMITH_REPORT.md` parity + speedup numbers back, not vibes.

## Do not

- Do not touch `--fork`/`--work`/`--store` paths belonging to the sibling runs.
- Do not push anywhere. The fork dir is a local git repo (`main`); publishing is a
  separate manual step after review.
