# ADR-063: Slice-2 derived-only module-procedure shape — measured, killed

Date: 2026-10-29. Scope: rs-elmer-next §8 Slice 2 (one gate shape:
derived-only module-procedure opacity, attributes checked before opacity),
with MagneticSolve whole-library completion + 1–2 per-file module pilots
and its kill criteria. Verdict: **the shape is killed — it returns to
refused, no gate change merges.** The ABI facts measured along the way
(fixtures green) ship as reusable evidence; the §8 unlock arithmetic is
corrected.

## 1. Shape as implemented (measured, then reverted)

`gate_module_entry_opaque_dummy` (temporary, reverted): identical to the
T014 profile except a *scalar* `TYPE(...)` dummy with no
descriptor/hidden-arg-traveling attribute admits as opaque under the T020
accessor contract. Refuse list, checked BEFORE opacity (never the F77
template's early-`continue`): `POINTER`, `ALLOCATABLE`, `OPTIONAL`,
`VALUE`, procedure dummies, any array shape, `CLASS(...)` (polymorphic
descriptor travel unmeasured), derived function results, undeclared
dummies. Wired into `gate_file_verdict_impl` for `GateMode::PluginEntry`
only; substitution path byte-identical by construction (verified §4).
Five unit tests pinned the matrix (scalar admits; pointer/allocatable/
optional/value/array/result/class refuse); all green with the shape,
reverted with it.

## 2. Fixtures: DERIVED-OPAQUE PROOF GREEN (pinned gfortran 13.3.0)

`tests/fixtures/derived_opaque/` + `tests/t026_derived_opaque_proof.sh`
(ADR-027 discipline, lane scratch workdir, both SHIP):

- Admitted shape: `oval_scale(oval, factor)` — scalar `TYPE(oval_t)` +
  scalar `REAL(dp)`. Component access only via generated-style `BIND(C)`
  accessors compiled against the built `.mod`.
- Positive 1 (Rust caller links Fortran objects): byte-identical to the
  Fortran reference (`A=  3.00 N=3 / W=  3.00  3.00  3.00`).
- Positive 2 (Fortran-caller arm, the FourierLoss→MagnetoDynamics
  pattern): unported Fortran caller linked against the Rust staticlib
  exporting `__oval_mod_MOD_oval_scale` **without** `oval_mod.o` — the
  link itself proves Rust owns the symbol — byte-identical output.
- Negatives (forced through the opaque convention, all must fail
  observably): NULL array descriptor (`oval_fill`, allocatable array),
  NULL hidden result slot (`oval_make`, derived `RESULT`), NULL pointer
  descriptor (`oval_use_ptr`) — all three SIGSEGV, exit 139, never
  silently green. The gate refuses all three statically (unit-pinned in
  the temporary suite); the runtime arms prove the refusal is load-bearing
  (descriptor dereference / hidden slot / association state required).

Harness note: `set -o pipefail` + `grep -q` in a pipe is a lie — grep
closes the pipe on first match and the writer dies with SIGPIPE (141),
failing the chain despite the match. The proof script consumes full input
(`grep ... > /dev/null`) at every `nm` check.

## 3. Regrade: 312 → 315 admit, 0 admit→refuse; substitution bit-identical

Method: the scout's temporary 5-line env-switch driver patch
(`ELMER_REGRADE_MODE=entry`, reverted after; committed diff contains no
driver change — T023 owns that tooling). Same inputs as the census:
1,157 units, dag `/home/john/runs/elmer-work/recon/dag.json`, tree
`/home/john/runs/elmer-fork`.

- Entry, new gate: **315 admit / 842 refuse** (was 312/845).
  Refuse→admit (3): `fem/src/FreeSurface.F90`,
  `fem/src/SParIterPrecond.F90`, `elmerice/Utils/ProjUtils.F90`.
  **Admit→refuse: 0.**
- Substitution, same binary without the env var: **685/472, zero diff
  rows** against the archived census JSON — legacy path untouched.

Flip soundness (full dummy re-scan with the real parser, temporary test,
reverted): all three flipped files are fully clean — every derived dummy
scalar-plain (`TYPE(Model_t)` ×3 in FreeSurface; `TYPE(Matrix_t)` in
SParIterPrecond; `TYPE(pj_t)/TYPE(pj_context_t)` with `INTENT(in)` in
ProjUtils), everything else intrinsic. Genuine flips.

Disposition of the 3 flips: 2× fem-core internals (no dlopen boundary —
§5.3, not an unlock) + 1× ice-Utils (stranded until the Slice-3
baseline). **Gradeable-now: 0.**

## 4. Masking audit: audited module flips = 0 (pilots falsified)

Method: temporary same-crate dump test (real `parse_gate_file` +
per-dummy verdicts, reverted) over the 3 flipped files + all 26
modules file-units whose old first-refusal touched a module-procedure
derived dummy. Output archived at
`/home/john/firstmate/data/rs-elmer-t026/scratch/masking-dump.txt`.

Result: **every one of the 26 carries residual refusals behind the old
first-refusal.** Per-file residuals (entry gate):

| File(s) | Residual refusals masking the derived motif |
|---|---|
| MagneticSolve Maxwell/MaxwellAxiS/MaxwellGeneral | exactly 1 each: `Element`, `TYPE(Element_t),POINTER` in the Boundary proc (ground-truthed in source, e.g. `Maxwell.F90:324`); 9 other dummies newly admit via opacity |
| HeatSolve/HeatSolveVec, StatCurrentSolve/Vec, StatElecSolve/Vec, StressSolve | `CHARACTER(LEN=*)` delegate fronts + INTEGER `Perm(:)` residual callbacks (non-REAL(8) assumed-shape), + non-intrinsic function results |
| TurbulenceModels ×5 (KE, Komega, SST, Spalart-Allmaras, V2F) | 2–4× POINTER + 1× character + non-intrinsic `RESULT` |
| RichardsSolver, ResultOutputSolve Vtk/Vtu | POINTER + character + OPTIONAL + non-intrinsic `RESULT` |
| DXOutputSolver | 5× character |
| MagnetoDynamics/Utils | 12× POINTER, 11× non-REAL(8) assumed-shape, 2× ALLOCATABLE, 2× derived-array, OPTIONAL, derived `RESULT` |
| HydrostaticNS, IncompressibleNS | POINTER `Element`, rank>2, OPTIONAL, ALLOCATABLE |
| StatElecBoundary | character + 5× non-intrinsic results (incl. the `name`/`or`/`not` keyword-collision suspects, §5.4) |
| BatteryUtils | POINTER + OPTIONAL (+ALLOCATABLE behind) |
| ParStokes | 2× OPTIONAL |
| ADIOS2OutputSolver | POINTER + character (also no reference `.so` — refused-no-reference stands) |

**Audited module-procedure flip count: 0 of 26. MagneticSolve: 0 of 4
files (3 Maxwell blocked on POINTER, MagneticSolve.F90 already admits —
the library cannot complete).** The §8 prediction (6 modules + 3 Maxwell)
was a masked-first-refusal artifact; the ceiling logic the panel adopted
("flip counts are ceilings") is confirmed as the correct reading, and
this slice is its first measured instance: predicted ≤36
file-units campaign-wide → actual 3, of which 0 gradeable-now.

## 5. Caller census + F77-hazard liveness (supporting)

- Maxwell entries (`maxwell{,axis,general}_{boundary,compose}_`, 7
  dynsyms in `MagneticSolve.so`): no undefined refs from any of the 153
  module libs, `libelmersolver.so`, or `libmpi_stubs.so` (T021-B serial
  build tree) — dispatch is purely `.sif`-driven plugin dlopen. The
  census the spec requires for pilots is moot (no surviving pilots) but
  would have passed.
- F77 early-`continue` hazard (§8 skeptic finding, out of scope):
  scanned all 314 admitted Fortran files' F77 (non-module) dummies for
  derived-with-descriptor-attrs — **0 live cases**. Latent-only; flagged
  for a future recon/gate-truth PR per the panel's separate-attribution
  rule, deliberately NOT fixed here (any fix risks admit→refuse flips
  and is outside the one-shape scope).

## 6. Kill evaluation (spec §8, Slice 2 criteria)

1. *"If the masking audit cuts module flips below the pilot set (pilots
   don't survive their own re-scan), the shape returns to refused — no
   merge on weakened terms."* FIRES: 0 module flips vs a 4–5-file-unit
   pilot set (MagneticSolve + 1–2 modules); all three Maxwell pilots
   refuse on source-ground-truthed POINTER dummies.
2. *"If fixture work exceeds the cost of the ports it enables on a
   one-slice horizon, justify as step 1 of the 3-shape sequence or
   stop."* FIRES toward stop: ports enabled = 0, so any fixture cost
   exceeds it; the step-1 justification is rejected — a new admit path
   with zero current flips is pure risk surface, and each queued shape
   (character-callee, non-RE8-descriptor) needs its own fixtures anyway.

**Decision: STOP. No gate change merges; `lib.rs` and `elmer-regrade.rs`
are at HEAD. No port PR (no surviving pilots), no backstop splice
(nothing to splice), no POC-board change (0 graded units).**

## 7. What ships and what it corrects

This PR ships fixtures + proof script + this ADR only (zero gate
semantics). It preserves: the ABI facts any future opacity proposal
needs (§2 — scalar-derived travels as a plain address both directions;
descriptor/hidden-slot/association travel is load-bearing and
fail-fast); the negative-control methodology; the pipefail lesson.
It corrects the campaign record: §8's "≤36 motif-flips, gradeable-soon
≈ 6 modules + 1 library" becomes **3 actual flips, 0 gradeable-now** —
the masking caveat is now measured, not hypothetical. The queued shapes
are unaffected and their motifs are confirmed present behind the masking
(table §4): character-callee (delegate fronts) and non-RE8-descriptor
(residual `Perm`) remain the real next unlocks, each its own slice.

Evidence: lane `/home/john/firstmate/data/rs-elmer-t026/` —
`scratch/regrade-entry-t026.json` (315/842),
`scratch/regrade-subst-t026.json` (685/472, bit-identical),
`scratch/masking-dump.txt`, `scratch/admitted-dump.txt`,
`scratch/proof-opaque/` (proof log). No oracle, gate, threshold, or
tolerance was touched; the committed tree refuses exactly what it
refused before this slice.
