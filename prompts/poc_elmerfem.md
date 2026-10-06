# POC run: `elmerfem` (adapter probe, NOT a committed port target)

Probe target: Elmer FEM — large Fortran/C++ finite-element suite (CMake/CTest).
Purpose: test how far rustsmith's C++/Fortran support goes, not to produce a mirror.
Per README Languages, C++/Fortran adapters are **Planned (pinned repo)**, not
implemented — expect recon to work and mirror to fail. A clean, well-documented
failure with the exact missing piece is a successful outcome.

## Setup (run from this worktree root)

```sh
cargo build --release
```

You are running alongside two sibling runs (packaging, charset-normalizer). Isolation
is by explicit dirs — do NOT reuse their `--fork`/`--work`/`--store`/`--run-id`.

## Run — recon ONLY first

```sh
./target/release/rustsmith run \
  --repo https://github.com/ElmerCSC/elmerfem \
  --fork "$HOME/runs/elmer-fork" \
  --work "$HOME/runs/elmer-work" \
  --store "$HOME/runs/elmer-store.db" \
  --run-id elmer-poc \
  --stage recon
```

Inspect the recon output (language/build detection, module + call graph, test
baseline, `PORTING.md`, unit DAG) before deciding anything else:

```sh
./target/release/rustsmith status --run-id elmer-poc --store "$HOME/runs/elmer-store.db"
```

## Then, conditionally

- Recon clean with C++/Fortran correctly detected → try `--stage mirror` and report
  exactly where it stops (adapter gap, bridge gap, gate failure — file + symbol).
- Recon itself fails → stop there and report the failure (command output + which
  detection step broke). Do NOT force it, do NOT hand-roll an adapter.

## Scope notes

- This repo is orders of magnitude larger than the Python POC targets; do not
  attempt `full` or `optimize` stages. Recon, then at most mirror.
- Elmer's payoff per README is safety/maintainability, not speed — do not chase or
  claim speedups here.
- Report back: what recon detected (languages, build system, test inventory), the
  first blocking failure with file/line, and one concrete proposal (which adapter
  trait method needs implementing for this repo class).

## Do not

- Do not run `--stage full`. Do not touch sibling runs' paths.
- Do not push anywhere. The fork dir is a local git repo (`main`); publishing is a
  separate manual step after review.
