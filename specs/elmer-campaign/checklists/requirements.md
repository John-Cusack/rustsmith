# Requirements Checklist: Elmer full-port campaign

**Purpose**: reviewer-owned requirements-quality gate for the campaign spec/plan/tasks
**Created**: 2026-10-08
**Feature**: `specs/elmer-campaign/spec.md`

**Note**: Generated per the spec-kit checklist step from spec.md, plan.md, tasks.md.
**Review Ownership**: reviewer-owned. Mark `[x]` only when the reviewer determines the criterion is satisfied.
**Marker Semantics**: `[x]` = reviewed and satisfied for requirements quality; not implementation-complete.

## Completeness

- [ ] CHK001 Every FR (FR-001–FR-009) has a slice task that discharges it (T001–T019 trace).
- [ ] CHK002 Every user story (US1–US4) has an independent test that can run without other stories.
- [ ] CHK003 Edge cases name an owner behavior each (K=3 backstop rule, EOF-hang, archive-ordering, multi-unit link, probe-path relocation).
- [ ] CHK004 `fhutiter` (no-binary) slice states its value (porting-rule coverage) and its halt explicitly.

## Clarity

- [ ] CHK005 Port scope arithmetic is exact (3025 − 61 − 1587 − 193 ≈ 1184) and traceable to frozen recon.
- [ ] CHK006 Wall-time math shows inputs, host, mix assumption (60/40), and excluded costs (model effort, COMMON).
- [ ] CHK007 Each tasks.md slice lists goal, file ownership, acceptance, non-goals — consumable by a worker with only repo + this directory.
- [ ] CHK008 Honest-halt table maps every cataloged halt to meaning + fix, none resolve to "bypass".

## Consistency

- [ ] CHK009 Constitution Check gates (plan.md) pass against `specs/constitution.md` DRAFT without contradiction.
- [ ] CHK010 Tool-fix vs port PR separation holds for every task touching `crates/` + `rust/` in one slice (none do).
- [ ] CHK011 K=3 rule is identical in spec (SC-002/clarification 6), plan (done definition), and tasks (T002/T013).
- [ ] CHK012 No task relaxes a gate, re-freezes a baseline, or invents a probe path.

## Measurability

- [ ] CHK013 SC-001–SC-005 each name a number, a source log, and a comparison baseline.
- [ ] CHK014 Equivalence criterion is stated as an equation with K, not prose.
- [ ] CHK015 Shim relaxation requires graded-run numbers per shape (one PR per shape).

## Notes

- Constitution itself is DRAFT pending captain review; this checklist gates the campaign artifacts, not ratification.
- `/speckit-implement` (or a campaign worker) reads checkbox state as a gate and must not modify markers.
