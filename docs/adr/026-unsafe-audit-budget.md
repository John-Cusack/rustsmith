# ADR-026: unsafe audit collects budgeted sites instead of erroring

Date: 2026-10-07. Scope: tool (`mirror::audit_unsafe` + whole-repo gate).

## Problem

`audit_unsafe` errored on ANY `unsafe` token, which made the documented
`unsafe_budget` gate (budget + FFI boundary + `// SAFETY:`, evaluated two
lines later) dead code: `unsafe_sites` was always empty when reached. The
first port needing a raw FFI read — pyparsing's unicode scan buffer
(`PyUnicode_DATA`, no UTF-8 assumption so lone surrogates compare exactly)
— passed 12/12 units plus whole-repo parity/divergence, then died in the
audit with "14 unsafe tokens found".

## Decision

- `audit_unsafe` returns `Vec<UnsafeSite>` (file, FFI-boundary flag,
  SAFETY flag) and never errors on count. FFI boundary is a file property
  (names `pyo3`/`::ffi`/`pymodule`); `SAFETY:` must sit within 15 lines
  above the site.
- The whole-repo grade now halts when `unsafe_budget(5.0, 10)` fails, like
  every other whole-repo gate (previously recorded-but-ignored).
- The pyparsing port holds exactly one site (slice construction at the FFI
  boundary); its core crate is `unsafe`-free by construction (borrows
  `&[u8]`, Miri-suitable).

## Consequences

- Zero-`unsafe` ports (all prior) are unaffected: 0 sites passes as before.
- Ports with budgeted FFI `unsafe` + SAFETY notes can now converge; raw
  reads outside binding files, missing notes, or over-budget counts halt
  honestly with the file named.
