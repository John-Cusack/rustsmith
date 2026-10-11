# rustsmith Constitution

**Status**: DRAFT — pending captain review. Do NOT treat as ratified.
Derived from standing rules (`AGENTS.md`, `SPEC.md`, `README.md`).
In any conflict, `AGENTS.md` governs until this document is ratified.

## Core Principles

### I. Oracle Defines Correctness (NON-NEGOTIABLE)

The oracle defines correctness. Never edit, weaken, skip, or re-freeze
oracle tests, held-out suites, gate thresholds, or tolerances to make a
port pass. A gate failure is feedback, not something to work around.

### II. Measured-Only Claims

Every status, estimate, and POC-board row carries real numbers from a
graded run on the named host and commit. No numbers you didn't measure.
Estimates are labeled as estimates with their extrapolation math shown.

### III. No Silent Scope Growth

Every spec deviation gets a short ADR in `docs/adr/`.
Port PRs and rustsmith tool-fix PRs (`crates/`, `prompts/`, gates,
adapters, config) are separate; the port PR names the tool PR it
depends on. Never solve a symptom (suppress a warning, special-case an
input, relax a gate) where a mechanism is owed.

### IV. Green + In-Scope Merge Only

Nothing merges unless its gates are green and its units are in scope.
The merge owns build edits (source removal + `target_sources` /
`target_link_libraries` splice, same commit as the deletion).
Skipped units (`out_of_scope`, `outside_scope`) link, don't port;
ready-check treats `skipped` deps as satisfied. An honest halt
(missing archive, non-`BIND(C)` refusal, vacuous-probe refusal) is
never bypassed — it is fixed at the source or scoped out explicitly.

### V. Gate/Agent Separation

Gates and oracle crates never depend on agent or council crates.
No LLM calls in gates. The worker produces code; the harness grades
and merges. Worker failure (bad exit, malformed output) is never
graded as a port result.

### VI. Boring Design, Six-Month Maintainability

Prefer existing patterns over new conventions. Delete dead weight.
Compiled code carries no avoidable allocation, copying, or computation.
`AGENTS.md`/`CLAUDE.md` are edited only to correct factually wrong
text, never to append knowledge.

## Limits & Release Discipline

- No unreviewed publish — TestPyPI via the release lane with a human gate,
  PyPI/crates.io human-gated per package. Never push to an upstream
  package's remote. (Amended 2026-10-11 per captain ruling: the prior
  "never publish to TestPyPI" read contradicted four TestPyPI-held lanes;
  flagged for ratification at constitution review.)
- At most 3 concurrent rustsmith runs (`[run].max_parallel_runs`).
- Run dirs are isolated (`--fork` / `--work` / `--store` / `--run-id`
  under `/tmp`, never `~/runs/*`). Never `--stage full` on Elmer until
  a pilot slice reports green costs. Never push from a session.
- Python-spine behavior is frozen by the suite: keep it byte-identical.

## Development Workflow

Done means all of:

- `cargo build --release`
- `cargo test`
- `cargo clippy -- -D warnings`
- the relevant `tests/m*_acceptance.sh` green, plus
  `tests/release_acceptance.sh` for release work
- graded-run evidence in the PR body

## Governance

This constitution supersedes session-level practices once ratified.
Until ratification, it is a draft for review: amendments arrive as
review comments on the pilot PR, and ratification is a captain
decision recorded here with date.

**Version**: 0.1.0 (DRAFT) | **Ratified**: UNRATIFIED — pending captain review | **Last Amended**: 2026-10-11
