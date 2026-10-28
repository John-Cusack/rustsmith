# rs-port-sqlparse retro

Date: 2026-10-28. Outcome: port complete and proven; publish blocked on
missing registry credentials (no accounts/tokens created, per rule).

## What worked

- Kernel-in-Rust / tree-stays-Python (ADR-053) made the entire Python tree
  API exact by construction; all effort went into the kernel, verified by
  differential fuzz instead of hand-written tree tests.
- The lexer's CPython `re` semantics (folding edges, backtracking,
  greedy-with-backtrack quotes, tuple-vs-single `in`) were each nailed by
  a probe-then-fuzz loop, not by reasoning — 6+ such edges found only by
  the fuzzer.
- Snapshot-iteration in `_group` (stale tokens keep voting) was the one
  deep semantic surprise; the upstream suite passed without it, the
  fuzzer caught it.
- `release-prep` accepted the external repo path directly (project dir +
  fork + recon-out), battery green first try.

## What cost

- Three `edit`-tool range slips on large generated files (lexer.rs,
  group.rs, format.rs) each needed repair reads; smaller append-only
  chunks would have avoided them.
- RSS needed two passes (tuple tree, then direct construction) — should
  have built direct construction first (peak RSS is a first-class citizen
  for a parser port).

## Numbers

Probe 0.75 → rewrite; wall 4.3–12.4x (kill line 2x cleared everywhere);
peak RSS +0–20% bulk; upstream 506 + fuzz/seam suites green;
release-prep battery green.

## Publish state (blocked)

No `TWINE_USERNAME`/`TWINE_PASSWORD`, no `CARGO_REGISTRY_TOKEN`, no
`~/.pypirc` on this machine; PyPI/TestPyPI have no `sqlparse-rust`
project and crates.io has no `sqlparse-rust-core` crate yet. Trusted
publishing (workflow + setup notes committed in the mirror repo) still
needs human one-time setup: PyPI/TestPyPI/crates.io accounts, publisher
identities, environments — all human-gated, none created here.
Mirror: https://github.com/John-Cusack/sqlparse-rust
Bundle: `data/rs-port-sqlparse/release-prep` (in firstmate data dir,
outside the worktree) with wheel + sdist + `.crate`, SHA256SUMS,
release-manifest, verification.json, trusted-publishing setup.
