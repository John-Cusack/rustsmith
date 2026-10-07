# ADR 012: pytest collection firewall for version-skewed deprecations
- Context: pinned upstream suites are frozen byte-for-byte by the oracle, so
  they cannot adapt when a newer pytest promotes a collection-time
  deprecation to an error under the suite's own `filterwarnings = error`
  (seen: `PytestRemovedIn10Warning` for generator-fed `parametrize` in a
  frozen `test_isoparser.py`, which aborts the whole oracle collection).
- Decision: `PytestRunner::pytest_command` appends a message-scoped CLI `-W`
  ignore (`ignore:Passing a non-Collection iterable to parametrize`), which
  takes precedence over ini filters. Message-scoped (not class-scoped) so it
  is a no-op on older pytest versions that never emit the text.
- Consequences: frozen suites stay collectible across pytest upgrades; every
  other warning still honors the repo's `filterwarnings` config, and no test
  file, assertion, or skip list is touched.
- Alternatives: edit the frozen tests (oracle tamper — rejected); pin the
  toolchain pytest (host env is not version-pinned — rejected).
- Spec: §7 (the oracle is the frozen suite; grading must run it) wins over
  the literal frozen-invocation bytes, which gain one warning filter.
