# ADR-039: upstream shape for python-dateutil is pure-Python, Python-2-compatible

Date: 2026-10-14. Lane: rs-upstream-sweep-2. Target: dateutil/dateutil @ 2642afa.

## Context

The lane-spec default shape is a small optional speed core behind a
pure-Python fallback. The target repo rules it out:

- `setup.cfg`: `python_requires = >=2.7, !=3.0.*, !=3.1.*, !=3.2.*`,
  `[bdist_wheel] universal = 1`; tox still lists `py27`. New syntax
  (f-strings, keyword-only args, walrus) is unshippable.
- Zero required dependencies, pure-setuptools build, no compiled path
  anywhere in history. A Rust/PyO3 toolchain would be a new build
  backend, a platform-matrix burden, and a non-starter for a
  conservative low-traffic library (same class of objection as ADR-037,
  stronger: there is not even a Cython precedent to mirror into).

## Decision

Pure-Python optimization of the proven-hot units only
(`_timelex.get_token`, the `_parse` numeric dispatch), zero behavior
change proven by differential oracle + full suite, with tests and a
towncrier fragment per CONTRIBUTING. No extension, no dependency, no
rewrite.

## Consequences

- The PR stays inside what maintainers build (setuptools universal
  wheel), test (pytest/tox incl. py27), and accept. Absolute ceiling is
  whatever CPython dispatch can be removed (~1.2x here), not a native
  multiple. Accepted: a small real win in acceptable shape.
- Every candidate touching deliberate numeric semantics (`Decimal` in
  `_to_decimal`, GH 427/662) or time-dependent global state (parserinfo
  caching) is out of bounds regardless of measured gain.
