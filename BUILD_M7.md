# BUILD PROMPT - rustsmith M7 (unattended end-to-end, SPEC §16), single session

Paste everything below the line into a fresh LLM session whose working directory
is the repo root (`/home/john/repos/rustsmith`). That session builds M7 with no
human input except a hard stop on red.

---
You are building milestone M7 of `rustsmith`: the unattended end-to-end run.
Work in the repo root. Read contracts first, then build slice by slice.
No slice starts until the previous one's done-when is met; no milestone done
until `tests/m7_acceptance.sh` and `m0..m6_acceptance.sh` are all green in order.

## 0. Contracts (read before touching code)
1. `SPEC.md` §16 (the 12-item end-to-end check — normative, quoted in §3 below;
   where it conflicts with any plan doc, the spec wins) + §13 (full CLI) +
   §9 Stage 3 + §17 (implementer notes, esp. prevention-over-instruction).
2. `SPEC_STAGE2.md` — REPLACES `SPEC.md` §9 Stage 2. Normative for grading.
3. `IMPLEMENTATION_M6.md` — prior-art patterns (harvest/report/CLI slices).
   There is no `IMPLEMENTATION_M7.md`: slice 1 is writing it (slices, exact
   signatures, acceptance script shape), then building slices 2+ per it.
4. `config/fixture.toml` — pins. Primary `Nicoretti/crc @ 4e65ac4` (80 tests +
   28 subtests, split `pytest test/unit` + `pytest test/integration`,
   `PYTHONPATH=src`). Secondary `luozhouyang/python-string-similarity`
   @ `115acaacf926b41a15664bd34e763d074682bda3` (flat setuptools layout,
   colocated `*_test.py`, `pytest -q` → 18 passed on pristine checkout).
   Re-verify both baselines on clone; any difference = STOP and report.

## 1. Known starting state (verified, do not re-derive without cause)
- `rustsmith run` is an M0 recon-only skeleton (`main.rs:cmd_run` rejects any
  `--stage` except `recon`). The pipeline runs only as hand-invoked stages.
- `mirror.rs:516-550` hardcodes crc template paths; only `mirror/crc` exists.
  A second repo dies in mirror — your slice 2 fixes exactly this, nothing more.
- M5 plants (`grade-candidate --plant ...`) prove gates at grade level. §16
  items 11–12 need the same plants injected into a LIVE run loop.
- M0–M6 acceptance is green on a clean tree (`git status` clean at HEAD).
  A later slice never excuses breaking an earlier milestone.

## 2. Build loop (same discipline as M0–M6)
a. Write `IMPLEMENTATION_M7.md` first (slice table with done-when per row,
   acceptance script, traps, exit criteria), then work it top to bottom
   (implement → prove done-when → commit `m7-sliceK: <what>` → move on).
b. Write `tests/m7_acceptance.sh` SECOND (before the code it gates), exactly
   implementing §3 below. Never edit an acceptance script to make it pass —
   fix the implementation.
c. Run the FULL chain before advancing anything: `m0..m7_acceptance.sh` green.
d. Hard stops (do not proceed, do not work around, report immediately):
- any acceptance failure unfixable within the slice's scope;
- a fixture baseline that differs from §0.4;
- a spec contradiction blocking implementation (file 5-line
  `docs/adr/NNN-<topic>.md`, STOP — do not silently reinterpret);
- any urge to modify a test, benchmark, manifest, or held-out file to make a
  gate pass (tamper event — report as one).

## 3. M7 acceptance (§16, normative — quote back to yourself as done)
`tests/m7_acceptance.sh` runs `rustsmith run <crc-url>` unattended, then asserts:
1. fork holds a Rust implementation with the original's public API;
2. 100% of the frozen oracle passes in the graded run;
3. held-out divergence < 5 percentage points;
4. `oracle/manifest.json` hashes unchanged; test count + skip list match baseline;
5. every `unsafe` is at an FFI boundary with a `// SAFETY:` comment;
6. miri and clippy clean;
7. Stage 2 ran ≥2 rounds, terminated on a stopping rule, not an error;
8. all three reports exist and agree on every number;
9. `suggestions/` holds ≥1 classified item with reasoning;
10. the event log alone reconstructs every unit action + council decision;
11. (adversarial) a planted test-editing unit halts the run with a tamper
    event + logged reason;
12. (adversarial) a planted hardcoded-visible-inputs unit is caught by the
    held-out suite / divergence halt.
Plus: the same `run` command against strsimpy reaches at least mirror parity
(generic-template proof; zero merges acceptable, tamper never acceptable).

## 4. Inviolable rules (violations fail review)
- `rustsmith-gates` / `rustsmith-oracle` MUST NOT depend on agent/council code
  (CI dep-check stays green).
- Never let a worker's self-report count as evidence; only graded runs count.
- Prevention over instruction: confinement, held-out blindness, magnitude
  redaction by API shape/filesystem, never prompt text.
- Point estimates are never gains: deterministic screens, wall-clock CIs confirm.
- Guidance evolves ONLY via `learn` + human approval; never mid-run.
- Provenance on every artifact (license headers + NOTICE + attribution).
- `store.db` on host, never mounted into containers.
- Keep `SPEC*.md` frozen. State lives in code + tests + ADRs.

## 5. Ambiguity policy
Spec-literal beats spec-plausible; cheapest boring option that satisfies the
letter, plus a 5-line ADR recording the choice. No scope invention (no
telemetry, no refactors "while you're in there"). Pin tool versions you rely on
(Python 3.11+, pytest ≥7.2, hatchling) in the milestone doc.

## 6. Output discipline
- Commit per green slice. Push nothing upstream. `SPEC*.md` frozen.
- After M7: run the full chain once more, then report per-milestone results
  (script outputs), the §16 item 1–12 evidence lines, and any ADRs.
  Numbers only — no adjectives.
Begin with slice 1 (the M7 build-order doc). Completion (all eight acceptance
scripts green in order) is the only stopping condition.
---
