# Session prompt: Elmer S1 — scheduler scope-skip

Brief: `docs/IMPLEMENTATION_ELMER.md` §7 S1 (normative). Facts below were
verified 2026-10-05; the session re-checks them before relying on them.
Paste everything below the line into a fresh session.

---

You are continuing the rustsmith Elmer track. Work in a NEW git worktree
on a NEW branch `John-Cusack/elmer-s1`, created from
`origin/John-Cusack/rewrite-elmer`. That branch holds the current Elmer
strategy and is merged with main @ e816848. Repo:
git@github.com:John-Cusack/rustsmith.git (main checkout:
/home/john/repos/rustsmith). Commit per step. Do NOT push; report back.

## Read first (in this order)
1. docs/IMPLEMENTATION_ELMER.md: §2 worker contract, §4 landed/open,
   §5 honest-halt catalog, §7 brief S1 (your task; its acceptance is
   normative).
2. IMPLEMENTATION.md: "Polyglot spine" + "Elmer track" sections.
3. docs/adr/008 (polyglot design), 009 and 010 (recent script/held-out
   decisions).

## Ground truth (verified 2026-10-05; re-check, don't trust)
- Nothing of Elmer has been ported yet. The only real run
  (~/runs/elmer-store.db, run `elmer-poc`) halted on
  `substitute: no built object for 'python:umfpack/src/umfpack/deps.py'`
  with 2 units graded, 0 merged, 5646 queued (pre-coarsening ids).
- Elmer checkout: ~/runs/elmer-work/orig, at
  `release-26.2-641-g9f6af2f85` (NOT the release-26.2.1 @ a19504a the
  docs/ADR name; record the real one).
- Frozen recon: ~/runs/elmer-work/recon/dag.json, with keys
  units(3025), edges, leaf_first_order, out_of_scope(61), diagnostics.
  Unit ids are UnitIds like `fortran:fhutiter/src/...`.
- run_mirror (crates/rustsmith-cli/src/mirror.rs) does NOT read
  out_of_scope today, and RUSTSMITH_SCOPE does not exist yet.
- The "2-unit fixture proof" S1 must keep green is NOT committed
  anywhere. Only its package data exists: key `Mini` in
  crates/rustsmith-cli/data/repo-content.json (one BIND(C) porting rule
  + `mini_probe` differential probes).
- Treat ~/runs/* as READ-ONLY. Put every new fork/work/store/run-id
  under /tmp.
- CLI: `rustsmith recon --repo R --out O --heldout-out H --store S
  --run-id ID`; `rustsmith mirror --repo R --fork F --recon-out O
  --heldout H --store S --run-id ID --template mirror/Elmer`.

## Steps
0. Baseline: `cargo test --workspace` (expect 161 passed, 0 failed).
   Record the numbers. Anything red: STOP and report.
1. Make the fixture proof reproducible BEFORE changing the scheduler:
   commit a tiny CMake+CTest project under tests/fixtures/mini/ that
   matches the `Mini` data. It needs:
   (a) one Fortran file with a BIND(C) `mini_add`, plus a `mini_probe`
       driver;
   (b) one non-portable helper unit, e.g. a python script with no
       built object, so the skip path has something to skip;
   (c) a reference Rust port for (a), as in the §2 worker contract.
   Add tests/elmer_mini_proof.sh: recon -> mirror with a deterministic
   RUSTSMITH_WORKER_CMD that copies the reference port. It ends
   `MINI PROOF GREEN`. Today it should reach `mirror: 1/1 passed
   divergence=0.0000` for (a) and halt on (b) with the same
   `no built object` error. Record that pre-change behavior.
2. Implement S1 in mirror.rs (scheduler loop only, per the brief):
   - Units in dag.json `out_of_scope` -> status `skipped` + a store
     event with reason `out_of_scope`. No grade, no merge.
   - Env RUSTSMITH_SCOPE=comma-separated repo-relative prefixes
     (empty/unset = whole tree, today's behavior). Units matching no
     prefix are skipped the same way with reason `outside_scope`.
   - The ready-check treats `skipped` dependencies as satisfied (link,
     don't port).
   - Headers/helpers: standalone skip now. Write this decision in the
     doc.
   - Unit tests for all three rules. The mini proof now ends GREEN
     (helper skipped, BIND(C) unit merged).
3. Elmer acceptance (S1's own text): copy the frozen recon to /tmp,
   then run mirror on Elmer with RUSTSMITH_SCOPE=fhutiter (then
   matc). Leave the worker command unset (stub) for this step.
   Expected: only the slice is scheduled; everything else is
   `skipped` with reasons; the run reaches the first real unit's
   grade and halts HONESTLY on the ABI gate (`exports non-BIND(C)
   Fortran`) or on the missing-archive error. It must NOT halt on
   scope. Report: units scheduled / skipped (by reason), the halt
   line, wall time.
4. Regressions: workspace tests, m0..m9, tests/release_acceptance.sh,
   `git diff --check`. The chain takes about 45 min; run m9 as its own
   background job, because one combined job hits the background time
   limit.
5. Docs: in IMPLEMENTATION_ELMER.md, move open item 7 to Landed, record
   the S1 result numbers and the real Elmer pin. Update the status line
   in IMPLEMENTATION.md.

## Rules
- Never relax a gate (ABI gate included) and never edit an acceptance
  script to pass. Never modify tests, manifests or held-out files to
  pass a gate (tamper; report it).
- Keep Python-spine behavior byte-identical (the suite pins it).
- No speedup claims for Elmer; payoff is safety/maintainability only.
- Out of scope: real model workers (S3), stdin probes (S2), ABI shim
  (S4), full Elmer build (S5).

## Report back (numbers only)
Branch + base, files changed, baseline vs final test counts, mini proof
before/after, Elmer fhutiter/matc scheduled/skipped counts + halt line
+ wall time, m0..m9 + release results, anything blocked and why.
