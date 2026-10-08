# ADR-025: live seat dispatch

## Context

M8 wired seats through shell commands (`RUSTSMITH_SEAT_CMD_*`) with stub
fallbacks, but three gaps kept live review off: reviewer seating used a
built-in placeholder provider map, `--config` set only optimize knobs (seat
identities needed a second env switch), and no seat command spoke to a real
model with the bare-JSON stdout contract.

## Decision

1. Reviewer seating reads `rustsmith_council::configured_providers()` (the
   configured `[models]` providers); `assign_reviewers` still refuses
   same-provider pairs fail-closed.
2. `--config PATH` carrying a `[models]` table is the one documented switch
   for seat identities (overrides `RUSTSMITH_MODELS_CONFIG` for
   `run`/`run-batch`/`seat-probe`/`mirror`). A `--config` without `[models]`
   leaves identities untouched.
3. One dispatcher, `scripts/rustsmith-seat.sh`, serves all four seat commands:
   it reads seat/model/provider from the prompt on stdin and routes
   anthropic seats to the Claude Code wrapper (`claude -p --output-format
   json`, verdict taken from the `result` field), meta seats to
   `omp --model muse-code/<model>`, openai seats to
   `omp --model openai-codex/<model>`. Stdout is the bare
   `{"stance":...,"reasoning":...}` object; any failure exits nonzero
   (fail-closed, never an approving silence).

## Consequences

Gates and oracle stay LLM-free (unchanged dependency surface, enforced by
`tests/m8_acceptance.sh` dep-check). Live review costs one model call per
seat per decision; the default path (no seat commands) is still deterministic
stubs.
