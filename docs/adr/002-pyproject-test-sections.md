# ADR 002: Freeze pyproject test-sections, not whole file
- Context: SPEC §7.1 freezes "pyproject.toml test sections"; whole-file hash
  would flag the legitimate hatchling->maturin packaging change as tamper.
- Decision: hash normalized [tool.pytest*/coverage/tox/hypothesis] sections;
  other files hash whole. Packaging edits pass; test-config edits halt.
- Consequences: M0 stays green (test-file tamper unaffected); M4 fork may
  own packaging. Adding a new test section changes the hash => tamper.
- Alternatives: allowlist packaging in M4 grading (exception-based) — rejected.
- Spec: literal "test sections" wins.
