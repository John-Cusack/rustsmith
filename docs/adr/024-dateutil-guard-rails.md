# ADR 024: dateutil guard rails — release-profile gate (V15) + bench ban (V17)

- Context: re-audit `data/rs-dateutil-reaudit/report.md` V15 (no
  release-profile assertion on the measured wheel) and V17 (`#[bench]` ban
  compliant in code but uncodified). The C13 release-pipeline half already
  landed via the perf-P0 adoption (ADR-017): release-prep retains the wheel
  argv as `release_profile` in `verification.json`, asserted in
  `release_acceptance.sh`, `build_wheel` pinned by unit test. What was still
  open: the bench harness itself — the thing producing speed claims —
  measured whatever `import dateutil` resolved to, with no profile guard.
- Decision (V15): `dateutil._dateutil` exposes `__build_profile__`
  (`cfg!(debug_assertions)` probe: "release"/"debug"); `bench.py` bench
  mode refuses (exit 3) unless the profile is `release` (or `pure-python`,
  the upstream baseline side), stamps every figure record with `profile` +
  `debug_allowed`, and offers `--allow-debug` for iteration figures that
  are non-citable by construction. Parity mode stays ungated (correctness,
  not speed). `WORKLOAD.md` makes release-built wheels a MUST for cited
  figures and records the exact build command. Pre-probe wheels read
  `unknown` and are refused: rebuild with `maturin -r/--release`.
- Decision (V17): append `measure: libtest #[bench] banned` (R050) to the
  python-dateutil PORTING rules (append-only; existing R-ids stable), and
  extend the m5 structural grep to `mirror/**/*.rs` while scoping both
  greps to `*.rs` so prose naming the ban never trips them. SPEC_STAGE2
  §10.4 and `guidance/optimize.md` already state the ban; this closes the
  PORTING/acceptance-docs loop the re-audit asked for.
- Consequences: debug-built wheels fail loudly at measure time instead of
  silently producing unoptimized "port" numbers; a future worker reaching
  for `#[bench]` hits the PORTING rule, the spec, and the acceptance grep.
  Verified: release build probes `release`, debug build probes `debug`;
  gate refuses debug/unknown (exit 3), stamps `--allow-debug` figures;
  parity still green; `cargo test -p rustsmith-cli` 84/84.
- Alternatives: self-attested `--built-with` flag (rejected: not
  structural); failing closed on `pure-python` too (rejected: that side is
  the upstream baseline, nothing Rust to mis-measure); rewording the rule
  to dodge the literal token (rejected: the rulebook must name what it
  bans — the grep scoping is the correct fix).
