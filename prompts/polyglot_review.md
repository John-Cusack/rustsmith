# Review: polyglot composite adapter proposal

You are a staff-level systems reviewer. Your job is an opinionated
accept/reject review of the design proposal, not a summary.

## Inputs (read all before judging)

1. `docs/adr/007-polyglot-composite-adapter.md` — the proposal under review.
2. `crates/rustsmith-adapters/src/lib.rs` — current seam (`Adapter`,
   `BuildInfo`, `CallGraph`, `TestInventory`, `UnitDag`, `PythonAdapter`).
3. `prompts/cpp_fortran_deep_research.md` + the Elmer FEM research findings
   (mixed Fortran+C+C++ CMake+CTest repo, `release-26.2.1 @ a19504a`).
4. Spot-check at least: `crates/rustsmith-cli/src/recon.rs:17`,
   `crates/rustsmith-oracle/src/lib.rs:269-365`,
   `crates/rustsmith-sandbox/src/lib.rs:198-223`,
   `crates/rustsmith-cli/src/mirror.rs:124-226`,
   `crates/rustsmith-profile/src/lib.rs:49-200`.

## Review against these criteria

- Correctness: does the Frontend/spine/composite split actually close every
  hardcoded site (pytest spawns+parsers, maturin/venv, py-spy/harness,
  rulebooks, `gates:372` literal, `fixture.rs` allowlist)?
- Generality: any repo-specific rule smuggled into `src/`? Any Elmer literal
  the design still depends on? Call it out.
- Multi-language: does the symbol-index merge hold for 1, 3, and 7 languages?
  Is the canonical unit-ID story sufficient or hand-waved?
- Spine adequacy: is one TestRunner/BuildBridge/Profiler per repo enough?
  Stress MPI launchers, multi-step configures, non-xUnit oracles
  (`TEST.PASSED`/norm-diff), generated sources (`.src`→`.F90`).
- Migration: is the dependency order safe? Anything that must land atomically?
- Trait sufficiency: what method is missing, misplaced, or overfit? Propose
  exact signature changes, not advice.

## Output format (strict)

1. **Verdict:** `accept` | `accept-with-changes` | `reject` — one sentence.
2. **Strongest 3 points** of the proposal (1 line each).
3. **Issues table:** `| severity (blocking/major/minor) | location in proposal | problem (1 sentence) | concrete fix (signature, type, or file-level change) |`.
4. **Answers** to the 4 review questions at the end of the proposal.
5. **Missing-case list:** at most 5 scenarios the proposal mishandles, each
   with a 1-line fix direction.

## Rules

- Every claim cites `file:line` + symbol or proposal section. No claim
  without a pointer.
- No code changes. No summaries of what you read. Opinion and evidence only.
- Mark uncertainty `[INFERENCE]`; prefer the boring/safe option under
  uncertainty.
