# ADR-043: upstream shape for strsimpy is pure-Python

Date: 2026-10-14. Lane: rs-upstream-sweep-6. Target: luozhouyang/python-string-similarity @ 115acaa.

## Context

The lane-spec default shape is a small optional speed core behind a
pure-Python fallback. The target repo rules it out:

- `setup.py`: setuptools build, zero runtime dependencies, zero native
  files anywhere in history; CI is flake8 + pytest on 3.5–3.9 via
  decade-old actions. A native toolchain would add a build backend and a
  platform matrix the maintainer never asked for.
- No CONTRIBUTING, no PR template, no compiled-accelerator precedent in
  either direction; the project is a small pure-Python algorithm
  collection (last release 2021).

## Decision

Pure-Python optimization of the proven-hot units only
(`LongestCommonSubsequence.length`: full matrix → two rolling rows;
`Levenshtein.distance`: hoisted loop invariants), zero behavior change
proven by full suite + 187-comparison differential oracle with 0
divergences. No extension, no dependency, no rewrite.

## Consequences

- The PR stays inside what the maintainer builds (setuptools pure
  wheel), tests, and accepts. Ceiling is whatever CPython per-cell
  dispatch can be removed, not a native multiple. Accepted: a real
  measured win in acceptable shape (2.3–2.6x wall and ~99.7% less peak
  memory on LCS workloads), same class as ADR-041/042.
- The larger Rust-mirror speedups (65x/9.2x/471x) are not shippable
  upstream and stay in the mirror.
