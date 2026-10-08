# ADR 017: adopt external perf survey P0 (guidance, bindings, workload/report/gates)

- Context: the external perf survey (`data/rs-external-perf/report.md`, scout
  lane, 2026-10-07) returned 9 P0 items, all shippable without new environment
  or dependencies: FFI/boundary economics (C12/D), the PyO3 six-rule checklist
  (C11), parse-once + no-callbacks (D), stable-SIMD + vectorizer checklist
  (E20/E21), layout-by-access moves (E22), Tukey + noise threshold (A1),
  maturin `-r` assertion (C13), SPEC five-property WORKLOAD acceptance +
  real-corpus + Mytkowicz interleave citation (F24/F26/F28), Hoefler report
  fields (F25), `#[bench]` ban (A3). P1/P2 (hyperfine, remarks, staged
  valgrind/perf, free-threaded matrix, cap calibration) are explicitly out.
- Decision: adopt all 9 as three commits on one branch (guidance+profile;
  bindings+release; workload/report/gates): `guidance/optimize.md` v1→v2,
  `caller_side_check` gains the ~1µs FFI budget, 8 binding rules appended to
  all 5 Python packages in `repo-content.json` (existing R-ids stable),
  `verification.json` retains the wheel argv as `release_profile` (asserted in
  `release_acceptance.sh`; `build_wheel` pinned by unit test), `WORKLOAD.md`
  gains a Corpus section (backfilled from each package's distribution text),
  `RUSTSMITH_REPORT` gains a mandatory Measurement section + per-round CIs
  (host/commands render "unrecorded" until the pipeline captures them),
  `rustsmith-gates` gains `tukey_outliers` + `SCREEN_NOISE_THRESHOLD` +
  `above_noise` (warn path wired at the optimize baseline; report-never-drop),
  SPEC_STAGE2 §§3.2/4.1/5/10.4 record the rules, m5 greps `#[bench]` out of
  `crates/`.
- Blocking tool fix (found by m7, not from the survey): held-out float pins
  drifted 1 ulp in transit. `generate_from_manifest` pins probe stdout
  (`0.9538461538461539`) but serde_json's default float parser rounded it to
  `0.953846153846154`, failing the pristine original against its own pin —
  and every mirror with it (strsimpy `heldout_divergence` 15/16 vs 0.05
  threshold). Fix: workspace `float_roundtrip` (exactly-rounded parsing;
  strictly more correct everywhere) + two bit-level regression tests. No
  held-out data, suite, gate, or threshold was edited — the generator now
  emits what the original produced, which was always its contract.
- Consequences: guidance rows pin v2; crc PORTING.md grows 115→123 rules
  (m3 ≥100 holds). Deliberately NOT changed: the per-candidate
  `det_gain > floor` leg (m5 pins a real `slicing-by-8` merge above floor —
  tightening it to the threshold band risks fixture behavior, staged as
  follow-up), mirror-stage `maturin develop` flags (debug iteration builds are
  out of scope), report host/commands population (fields enforced, capture
  staged).
- Alternatives: three stacked PRs — rejected (direct-PR delivery contract
  ships one branch; the three commits stay separable for review).
