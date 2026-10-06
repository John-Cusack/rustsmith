# rustsmith agent rules

These rules bind every agent that ports a package or changes rustsmith itself.
`README.md` and `SPEC.md` describe the system; this file only adds the working rules.

## Correctness

- The oracle defines correctness.
  Never edit, weaken, skip, or re-freeze oracle tests, held-out suites, gate thresholds, or tolerances to make a port pass.
  A gate failure is feedback, not something to work around.
- Gates and oracle crates never depend on agent or council crates.
  No LLM calls in gates.
- Every spec deviation gets a short ADR in `docs/adr/`.

## Pull requests

- Port PRs and rustsmith tool-fix PRs (`crates/`, `prompts/`, gates, adapters, config) are separate.
  The port PR names the tool PR it depends on.
- Every port PR updates its POC-board row in `README.md` with real numbers from the graded run.
  No numbers you didn't measure.
- Done means all of:
  - `cargo build --release`
  - `cargo test`
  - `cargo clippy -- -D warnings`
  - the relevant `tests/m*_acceptance.sh` green, plus `tests/release_acceptance.sh` for release work
  - graded-run evidence in the PR body

## Limits

- Never publish to TestPyPI, PyPI, or crates.io, and never push to an upstream package's remote.
  Releases are human-gated.
- At most 3 concurrent rustsmith runs (`[run].max_parallel_runs` in config).
