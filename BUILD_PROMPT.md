# BUILD PROMPT — rustsmith, full project, M0→M6 unattended

Paste everything below the line into a fresh LLM session whose working directory
is the repo root (`/home/john/repos/rustsmith`). That session builds the entire
project, milestone by milestone, with no human input except a hard stop on red.

---

You are building `rustsmith`: an autonomous rewrite-in-Rust orchestrator.
Work in the repo root. Read contracts first, then build M0→M6 in strict order.
No milestone starts until the previous one's acceptance script is green.

## 0. Contracts (read all four before touching code)

1. `SPEC.md` — the product contract (what + why + acceptance). Normative.
   Where it conflicts with any plan doc, the spec wins.
2. `SPEC_STAGE2.md` — REPLACES `SPEC.md` §9 Stage 2 in full. Normative.
3. `IMPLEMENTATION.md` — the map (milestone order, fixtures, cross-cutting rules).
4. `IMPLEMENTATION_M0.md` … `IMPLEMENTATION_M6.md` — the build orders. Each one
   is self-contained for its milestone: exact signatures, slice table with
   done-when, acceptance script, traps, exit criteria. Follow the active
   milestone doc literally.
5. `config/fixture.toml` — pinned lab rats. Primary `Nicoretti/crc @ 4e65ac4`
   (all oracle/mirror/optimize/harvest acceptance runs here). Secondary
   `luozhouyang/python-string-similarity @ 115acaa` is for the M3 DAG check
   ONLY. Baseline: 80 tests + 28 subtests, 0 skipped, 0 xfailed,
   split invocation (`pytest test/unit` + `pytest test/integration`),
   src-layout (`pip install -e .` or `PYTHONPATH=src`), `test/bench/` is NOT
   oracle. Re-verify this baseline on clone; if it differs, STOP and report.

## 1. Build loop (same for every milestone MN)

For N = 0..6, in order:
  a. Read `IMPLEMENTATION_MN.md` fully. Quote its acceptance block back to
     yourself as the definition of done — paraphrase is not acceptance.
  b. Fill in the spec-commit pin it asks for (`git rev-parse --short HEAD`).
  c. Work its slice table top to bottom (each row: implement → unit-test the
     done-when → move on). Keep slices small; commit after each green slice.
  d. Write the milestone's `tests/mN_acceptance.sh` FIRST if it does not exist
     yet, exactly as the doc specifies, then make it pass. Never edit an
     acceptance script to make it pass — fix the implementation.
  e. Run the FULL chain before advancing: `m0..mN_acceptance.sh` all green.
     A later milestone never excuses breaking an earlier one.
  f. Hard stops (do not proceed, do not work around, report immediately):
     - any acceptance failure you cannot fix within the milestone's scope;
     - a fixture baseline that differs from `config/fixture.toml`;
     - a spec contradiction that blocks implementation (file it as a 5-line
       `docs/adr/NNN-<topic>.md` and STOP — do not silently reinterpret);
     - any urge to modify a test, benchmark, manifest, or held-out file to
       make a gate pass. That is a tamper event in the spec's taxonomy and a
       build failure in yours. Report it as one.

## 2. Inviolable rules (from the spec — violations fail review)

- `rustsmith-gates` and `rustsmith-oracle` MUST NOT depend on `rustsmith-agent`
  or `rustsmith-council`. There is a CI dep-check from M0; keep it green.
- No agent/council code before M1/M2. M0 contains zero LLM calls — review
  rejects any diff containing them.
- Never let a worker's self-report count as evidence. Only graded runs count.
- Prevention over instruction: confinement (M1), held-out blindness (M4),
  magnitude redaction (M5) are enforced by API shape and filesystem, never by
  prompt text. `grep` for instruction-based enforcement before each gate.
- Statistical honesty: point estimates are never gains. Deterministic
  instrument screens, wall-clock CIs confirm (M5). A number without its
  interval is not a result.
- Guidance/prompt evolution ONLY via `learn` + human approval (M5 P2).
  Never mutate guidance mid-run; every attempt row pins its versions.
- Provenance on every artifact (M6): license headers + NOTICE + attribution
  preserved. A missing header fails the artifact.
- `store.db` lives on host, never mounted into containers. Held-out magnitudes
  never appear in worker-visible strings (M5 redaction test proves it).

## 3. Ambiguity policy (so you never need to ask)

- Spec-literal beats spec-plausible. Ambiguous → cheapest boring option that
  satisfies the letter, plus a 5-line ADR recording the choice.
- Missing tool versions: pin what the fixture era used (Python 3.11+,
  pytest ≥7.2, hatchling build) and record pins in the milestone doc.
- Do not invent scope: no retries frameworks, telemetry, or refactors "while
  you're in there." Delete weightless code; refuse needless abstractions.

## 4. Output discipline

- Commit per green slice (`mN-sliceK: <what>`). Push nothing upstream.
- Keep `SPEC*.md` frozen. Implementation state lives in code + tests + ADRs,
  never as edits to the spec.
- After M6: run the full chain once more from a clean checkout, then report:
  per-milestone acceptance results (script outputs), the M6 forecast for crc
  (expected: slice-by-8/16 `language_independent` patch, in-Python gain,
  dead-end tiers listed), and any ADRs filed. Numbers only — no adjectives.

Begin now with M0 slice 1. Do not stop at phase boundaries, todo flips, or
sub-step completions. Completion (all seven acceptance scripts green in order)
is the only stopping condition.
