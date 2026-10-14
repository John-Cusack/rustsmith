# ADR-042: upstream shape for python-multipart is pure-Python

Date: 2026-10-14. Lane: rs-upstream-sweep-5. Target: Kludex/python-multipart @ 51ff1a4.

## Context

The lane-spec default shape is a small optional speed core behind a
pure-Python fallback. The target repo rules it out:

- `pyproject.toml`: hatchling build, zero runtime dependencies, zero
  native files anywhere in history; `requires-python = >=3.11`.
- CI matrix is 3.11–3.15 × ubuntu-latest via `uv sync --frozen`, plus a
  CodSpeed benchmark workflow. A native toolchain would add a build
  backend and a platform matrix the maintainers never asked for.
- Maintainers already merge pure-Python speedups with proof, including
  the exact transformation used here: #316 hoisted `QuerystringParser`
  callback dispatch out of the per-byte loop with `_noop` fallbacks
  (preceded by #295, #300, #328, #330, #336 in the same vein).

## Decision

Pure-Python optimization of the proven-hot dispatch only
(`MultipartParser._internal_write`: hoist the nine multipart callback
lookups to one-per-`write()` with `_noop` fallbacks; `data_callback`
takes the resolved function directly), zero behavior change proven by
full suite + callback-event differential oracle + new
`TestMultipartParser` chunking tests. No extension, no dependency, no
rewrite. One disclosed semantic edge, inherited from the accepted #316
shape: callbacks resolve once per `write()`, so a `set_callback` from
inside a callback takes effect on the next `write()`.

## Consequences

- The PR stays inside what maintainers build (hatchling pure wheel),
  test (pytest, CodSpeed), and accept. Ceiling is whatever CPython
  per-event dispatch can be removed (~1.27x on the event-heavy form),
  not a native multiple. Accepted: a real measured win in acceptable
  shape, same class as ADR-039/040/041.
