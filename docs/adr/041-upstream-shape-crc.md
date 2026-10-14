# ADR-041: upstream shape for crc is pure-Python

Date: 2026-10-14. Lane: rs-upstream-sweep-4. Target: Nicoretti/crc @ 4e65ac4.

## Context

The lane-spec default shape is a small optional speed core behind a
pure-Python fallback. The target repo rules it out:

- `pyproject.toml`: hatchling build, description literally "Pure Python
  CRC library", zero runtime dependencies, zero native files anywhere in
  history; `requires-python = >=3.11`.
- Gates are format (black/isort), mypy strict on `src/`, pylint
  (`fail-under = 9.0`), pytest with coverage on 3.11–3.14 ×
  ubuntu/macos/windows. A native toolchain would add a build backend and
  a platform matrix the single maintainer never asked for.

## Decision

Pure-Python optimization of the proven-hot unit only
(`TableBasedRegister.update`: per-byte `Byte` boxing + property dispatch),
zero behavior change proven by full suite + differential oracle. No
extension, no dependency, no rewrite.

## Consequences

- The PR stays inside what the maintainer builds (hatchling pure wheel),
  tests, and accepts. Ceiling is whatever CPython per-byte dispatch can be
  removed, not a native multiple. Accepted: a real measured win in
  acceptable shape, same class as ADR-039/040.
