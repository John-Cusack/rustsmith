# ADR-033: one probe-census fixture rule, per-extension carve-outs retired

Date: 2026-10-11
Status: accepted

## Context

Four ADRs carved the same principle per extension or directory: ADR-014
(test-local fortran/cxx), ADR-021 (`.http`), ADR-025 (`examples/`), ADR-030
(unknown-extension test-local fixtures). ADR-021 explicitly rejected the
generalization ADR-030 later adopted; both readings stood in the code as the
`NON_SOURCE_EXTS` `http` entry plus the test-local fixture branch.

## Decision

One SPEC rule (Stage 0 step 1): test-local files (`test/`, `tests/`,
`bench/`, `example/`, `examples/`) that no frontend claims count as claimed
fixtures and never flip the spine. The `http` entry leaves
`NON_SOURCE_EXTS` — multipart's `tests/test_data/http/*.http` stays claimed
via test-locality, while a non-test-local `.http` file is now (correctly)
unclaimed source. No new per-extension allowlist entries; future corpora
need no tool change.

## Consequences

- Probe unit tests pin both directions: test-local unknown/`.http`/`.c`
  fixtures keep the Python spine; the same bytes outside test dirs stay
  unclaimed.
- ADR-021 remains history; its narrower alternative is decided against by
  the pyyaml precedent (574 opaque files no extension list could cover).
