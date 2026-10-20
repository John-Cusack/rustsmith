# ADR-050: T017 shape B — the module-dummy substitution shape

Date: 2026-10-20. Scope: one gate relaxation, one shape, one PR.
Context: ADR-027 (shim design + phase-1 shape matrix), ADR-047
(lane-fem-b round-2 judgments), ADR-049 (T017 shape A, the F77-external
shape), `specs/elmer-campaign/tasks.md` T017 (one PR per shape).

## Problem

T014 admitted a module procedure whose dummies are scalar intrinsics or
assumed-shape `REAL(8)` rank 1-2. The lane-fem-b round-2 regrade left
**35** of the 214 refused fem-b scopes refused on their **module dummy**
shape: the procedure's array dummies are explicit-shape (`x(n)`,
`x(3,3)`) or assumed-size (`x(*)`), i.e. they travel as a **raw base
address** (ADR-027 §3) — a wire form the T014 profile does not name, so
the gate refused them with "explicit-shape dummy (later shape)".

## Decision

`check_substitutable` admits a third shape: a **module procedure whose
array dummies are explicit-shape or assumed-size**. Those dummies
travel as a raw base address; every other rule of the T014 profile is
unchanged (scalar intrinsics by reference, assumed-shape `REAL(8)` rank
1-2 as a descriptor, and the same forbidden features).

Two forms the shape analysis must keep apart:

| dummy form | wire argument | status |
|---|---|---|
| explicit-shape `x(n)`, `x(3,3)` | raw base address | **admitted** (this ADR) |
| assumed-size `x(*)` | raw base address | **admitted** (this ADR) |
| assumed-shape `x(:)` | descriptor pointer | T014 profile (`REAL(8)` rank 1-2 only) |
| assumed-shape with bounds `x(lb:ub)`, `x(:n)` | descriptor pointer | **refused** (explicitly) |

The assumed-shape-with-bounds case is why the shape analysis needs care:
it contains a `:` like the plain assumed-shape form, but it is still a
**descriptor** dummy, never a raw base address. T014 refused it by
accident — it fell through to the "explicit-shape" refusal — and this
ADR replaces that accident with an explicit refusal
("assumed-shape with bounds (descriptor)").

### Measured ABI: an array result travels as a descriptor

The real fem-b unit of this shape (`VecUtils`) returns arrays, so the
result wire had to be measured, not assumed:

| callee | arguments passed | result |
|---|---|---|
| module `fres(a)` with `real(8) :: fres(3)` | `rdi` = array **descriptor**, `rsi` = `&a` | written through the descriptor |
| module `fmod2(n, a(n))` (scalar result) | `rdi` = `&n`, `rsi` = `&a` | `xmm0` |
| F77 `eres(a)` with `real*8 eres(3)` | `rdi` = array **descriptor**, `rsi` = `&a` | written through the descriptor |

The callee reads the descriptor's base at offset 0 and the dimension
stride at offset 40 (the ADR-027 §2 layout) and writes element `i` at
`base + (i-1) * stride` — the very descriptor an assumed-shape dummy
receives.

So the hidden result argument is **not** a plain pointer. A port that
assumes a pointer writes the result into the descriptor itself and
silently corrupts the caller's data. The first port draft did exactly
that; the equivalence corpus caught it (all zeros where the reference
printed the norm). This is the reason the shape is admitted only with a
measured result wire, and the reason the ADR records it.

## Result

Real unit `fem/src/modules/CoordinateTransform.F90#vecutils` (module
`VecUtils`: `norm2`, `normalized`, `jac`, `transform2rank` — explicit-shape
dummies `a(3)`, `OldCoord(3,3)`, `A(3,3)`, `jac(3,3)`, and array
results) now grades instead of refusing.

- **equivalence corpus**: 7 cases → 14 printed values at 17 significant
  figures (`ES25.17E3`), **byte-identical** between the Fortran
  reference and the Rust port, stderr empty on both sides. The vector
  cases pass a **rank-1 section** actual (gfortran packs the section and
  passes the packed pointer — the same raw-base-address wire); the
  matrix cases pass whole 3x3 arrays (identity and two non-trivial).
- **strong-symbol gate**: the port defines all four unit symbols as `T`;
  the intersection with the reference `CoordinateTransform.F90.o` is
  exactly those four → **zero STRONG port-level dups** beyond the
  replaced symbols, and the port claims no sibling symbol.
- **negative controls**: an admitted-shape module procedure carrying
  `COMMON`, `OPTIONAL`, `ALLOCATABLE` or a derived-type dummy still
  refuses with the pinned message; the non-`REAL(8)` assumed-shape, bare
  `REAL` assumed-shape, rank-3+ and assumed-shape-with-bounds shapes
  still refuse; the file's F77 sibling `rotmsolver_init` (derived-type
  dummies) still refuses.

Proof: `tests/module_dummy_shape_proof.sh`; port crate
`crates/elmer-fem-b/ports/vecutils` in the port repo (elmer-rust PR);
machine rows in the grading store (`gate_results`: admit, corpus,
strong-symbol, negative controls).

### Graded-vs-skipped counts (lane-fem-b, the 214 refused scopes)

| gate state | scopes |
|---|---|
| refused before T017 | 214 |
| **admit** with shape A + shape B landed | **2** (`#vecutils`, `#particledynamicsstuff`) |
| still refused | 212 |

Shape A alone admits **0** of them: every F77-external scope in
`fem/src/modules/` is a solver entry point whose dummies are the
`Model_t`/`Solver_t` derived types (whole-program layout, refused by
ADR-027). The remaining 212 refusals are dominated by the same derived
type dummies (33 of the 35 module-dummy scopes), plus `COMMON`, plus
the assumed-shape shapes this ADR does not admit.

Of the two admitted scopes, **one** is ported and graded end to end
(`#vecutils`). `#particledynamicsstuff` is gate-admitted but **not**
ported: its procedures read solver parameters through Elmer's `List`
API (`ListGetRealArray`, `GetSolverParams`), which is the Elmer world,
not a shim port. It is recorded as a skip, not claimed.

## Limitations (honest)

- The substitution is **driver-level**, not a whole-file splice: the
  file's two F77 globals (`rotmsolver_`, `rotmsolver_init_`) stay
  refused, so the reference object cannot be replaced whole. The port
  defines the unit's four symbols and claims no sibling (ADR-027's
  limitation, unchanged).
- The gate does not distinguish a scalar result from an array result;
  that was already true in T014 (the result's *shape* was never
  inspected) and stays true here. The array-result wire is now
  measured and documented above, and the port implements it.
- The module path still reads only `::`-style declarations, so a module
  procedure with an old-style declaration keeps refusing (unchanged
  from T014; the old-style scan belongs to the F77-external path).
- **Timing**: the lane-fem-b `bench_sample_unit`
  (`fortran:fem/src/modules/HeatSolve.F90`) stays refused, so **no
  timing sample runs for this shape**; per-unit grades carry no timing
  claim (ADR-045).
