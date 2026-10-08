# ADR-021: `.http` test-data fixtures are non-source

Date: 2026-10-07
Status: accepted

## Context

`rustsmith recon` on `Kludex/python-multipart` halted: probe reported
`unclaimed share 0.60 above threshold 0.20`. The 30 unclaimed files are
`tests/test_data/http/*.http` — raw HTTP request messages consumed by the
test suite at runtime (each paired with a `.yaml` expectation file).

## Decision

Add `http` to `NON_SOURCE_EXTS` in `crates/rustsmith-adapters/src/lib.rs`.
`.http` files are REST-client/test-data documents; no frontend compiles
them, so they are data by definition (same class as the listed `txt`/`log`).

## Consequences

- python-multipart recon proceeds; the `.http` files still freeze as oracle
  fixtures (the pytest runner's `oracle_files` claims `tests/**` by path,
  independent of the probe census), so suite data stays hash-pinned.
- No gate weakened: no toolchain treats `.http` as compilable source, so a
  future repo cannot smuggle real source past the census with this extension.
- Narrower alternative rejected: generalizing the test-local fixture rule to
  every unclaimed extension under `tests/` would also silence genuinely
  unclaimed sources (e.g. a stray `tests/helper.rs` with no Rust frontend).
