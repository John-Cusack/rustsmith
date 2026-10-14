# ADR-040: upstream shape for markdown-it-py is pure-Python

Date: 2026-10-14. Lane: rs-upstream-sweep-3. Target: executablebooks/markdown-it-py @ 6f58654.

## Context

The lane-spec default shape is a small optional speed core behind a
pure-Python fallback. The target repo rules it out:

- `pyproject.toml`: flit_core build, zero native files, one runtime dep
  (`mdurl`); CI matrix includes PyPy (`tests.yml`).
- No compiled path anywhere in history. A 2022 mypyc suggestion (#198,
  last comment) was never adopted; the repo stays pure Python.
- The maintainer-owned Rust path already exists *outside* this repo
  (`markdown-it-pyrs`, benchmarked in `docs/performance.md`). A vendored
  PyO3 core here would duplicate it and add a build backend the
  maintainers never asked for.
- Maintainers do merge pure-Python speedups with proof: #389 fixed
  quadratic complexity in `fragments_join`/`text_join`.

## Decision

Pure-Python optimization of the proven-hot preamble only
(`StateBlock.__init__` line-marker precompute), zero behavior change
proven by full suite + render differential oracle. No extension, no
dependency, no rewrite. Port-faithful parsing logic (block/inline
rules tracked against the JS port in `port.yaml`) is untouched.

## Consequences

- The PR stays inside what maintainers build (flit sdist/wheel), test
  (pytest incl. PyPy, CommonMark spec suite, mdit-py-plugins
  downstream), and accept. Ceiling is whatever CPython dispatch can be
  removed (~1.2x here), not a native multiple. Accepted: a small real
  win in acceptable shape, same class as ADR-039.
