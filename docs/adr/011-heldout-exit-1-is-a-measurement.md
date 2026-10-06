# ADR 011: Held-out pytest exit 1 is a measurement, not an error
- Context: REWORK (`21dcd3c`) made `run_heldout_in_venv` error on any nonzero
  exit. pytest exits 1 whenever a held-out test fails — exactly the case the
  gate exists to measure. All four call sites `?` the error, so a hardcoded
  port aborted before `heldout_divergence` ran: the m7/m9 hardcode plants
  exited nonzero with no `halt_reason` (SPEC §10.3/§16 item 12 unmet), and a
  per-unit or per-candidate held-out failure aborted the whole stage instead
  of failing that unit/candidate.
- Decision: accept pytest exit 0 or 1 (suite ran to completion); every other
  code (2 interrupted, 3 internal, 4 usage, 5 no tests collected, timeouts)
  still errors, and `parse_heldout_rate` still errors on unparseable output.
- Consequences: the REWORK fail-closed intent holds (no vacuous rate from an
  unrunnable suite); failing held-out tests now lower the rate and trip
  `heldout_divergence`, which halts with a recorded reason.
- Alternatives: map the error to a halt at each call site (loses the rate,
  conflates infra failure with divergence) — rejected.
- Spec: §7.4/§10.3 (divergence halts with reason) win over the literal
  "requires exit 0" wording of the REWORK sync note.
