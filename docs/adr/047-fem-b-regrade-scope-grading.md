# ADR-047: lane-fem-b round-2 scope-level grading + vacuous admits

Date: 2026-10-15. Context: T014 (PR92) admits exactly one non-`BIND(C)`
shape; lane-fem-b re-grade ran the relaxed gate over all 184 files
(218 recon scopes): 4 scopes admit, 214 refuse.

## Decision 1: grade per recon scope, splice at driver level

The gate admits per recon unit (scope), but the splice replaces the whole
`.o`. All 4 admitted scopes share files with refused siblings, so no file
is whole-file portable and no full-build splice links (the link would fail
fast on the missing sibling, never silently — the limitation T014 already
recorded). The one scope with portable procedures (`#harmutils`) is graded
green with driver-level substitution (drop the Fortran `.o`, link exactly
one `--whole-archive` provider) plus the unchanged chain steps that apply
(differential, strong-symbol gate, machine rows). Full-tree backstop and
scoped oracle parity are honestly not re-run: no tree was modified, so they
would measure the pristine tree, not the port (recorded in EVIDENCE.md).

## Decision 2: admitted-but-vacuous scopes are recorded skips, not ports

3 admitted scopes own no portable procedures: `EMWaveSolverUtils` (empty
module, emits no symbols) and `GlobMat` / `GlobMatC` (one module-level
`ALLOCATABLE` each — the `B`-class `__globmat_MOD_matrix` /
`__globmatc_MOD_matrix` descriptor symbols are an unproven shape, T017).
They admit only because the gate sees no non-`BIND(C)` procedure to check.
Claiming them as ports would inflate the count (triage precedent:
no-inflation ruling on elmer-rust#4). They are recorded skips with reason
`skipped-vacuous-admit`.

## Result

1 graded (`#harmutils`, 11/11 differential incl. 2 held out), 217 skips
(179 external-subroutine scopes, 33 derived-type, 2 pure explicit-shape,
3 vacuous admits; 0 COMMON). elmer-rust PR6; lane store.db runs
`fem-b2-harmutils-grade` + `fem-b2-regrade`.
