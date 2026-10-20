# ADR-049: T017 shape A — the F77-external substitution shape

Date: 2026-10-20. Scope: one gate relaxation, one shape, one PR.
Context: ADR-027 (shim design + phase-1 shape matrix), ADR-047
(lane-fem-b round-2 judgments), `data/elmer-lanes/lane-fem-b.regrade.json`
(179 of the 214 refused fem-b scopes refuse on the *linkage*),
`specs/elmer-campaign/tasks.md` T017 (one PR per shape). Sibling ADR:
the module-dummy shape (shape B) lands in its own PR.

## Problem

The T014 relaxation admits exactly one shape: a real `MODULE` procedure
whose dummies are scalar intrinsics or assumed-shape `REAL(8)` rank 1-2.
The lane-fem-b round-2 regrade over the 184 `fem/src/modules/` files
(218 scopes) refused 214; **179** of those refusals are the *linkage*
refusal — the unit exports an F77 global, not a `__<mod>_MOD_<proc>`
symbol.

The recon models a file-scope external subprogram as a scope named after
it, so its exports arrive as that scope name plus `__<scope>_MOD_<proc>`
(`__solve_ufric_MOD_solve_ufric`), while gfortran emits `solve_ufric_`.
The linkage alone therefore cannot prove the shape: the gate has to
resolve the export to the real external subprogram in the source, and the
scope must not be a module scope.

## Decision

`check_substitutable` admits a second shape: an **F77 global external
subprogram** (gfortran symbol `<name>_`) in a `COMMON`/`BLOCK DATA`-free
file, whose dummies fit the proven profile:

| dummy form | wire argument | source |
|---|---|---|
| scalar intrinsic | address of the value (by reference) | ADR-027 §3 |
| explicit-shape `x(n)` | raw base address | ADR-027 §3 |
| assumed-size `x(*)` | raw base address | ADR-027 §3 (measured `s = 10`) |
| assumed-shape `REAL(8)` rank 1-2 | descriptor pointer | T014 profile |

A function result must be a **scalar** intrinsic. An array result travels
as a hidden result pointer; that wire form is not admitted here (shape B
may take it separately).

Everything else keeps the pinned refusal: `COMMON`/`BLOCK DATA`,
`optional`/`allocatable`/`pointer`/`value`/`procedure`/character/
derived-type dummies, an undeclared dummy (implicit typing), a complex
result, an unreadable source, and — for an external scope — an internal
procedure (see below).

### The shape analysis the relaxation needs

Three facts about F77 external code the T014 scanner did not model. All
three live in `parse_gate_file` (`crates/rustsmith-adapters`):

1. **Old-style declarations.** F77 external code declares its dummies
   without `::` (`DOUBLE PRECISION DENSIT,VISCOS,DIST,ROUGH,UT,UFRIC,DFX`).
   The T014 scanner recorded only `::` declarations, so every F77 dummy
   read as *undeclared* and the unit refused for the wrong reason. The
   scanner now also records old-style declarations (and the old-style
   `DIMENSION name(shape)` statement) into a **separate** list, so the
   T014 module profile stays exactly as proven.
2. **The function type prefix.** `DOUBLE PRECISION FUNCTION f(...)` types
   its own result; the result variable carries no separate declaration.
   The scanner records the prefix and uses it when the result variable is
   undeclared.
3. **Internal procedures.** A `CONTAINS` inside an external scope opens
   internal procedures, which emit **no global symbol**. The scanner
   excludes them from the scope; a unit whose export list still names
   one therefore refuses (fail closed).

## Measured ABI (gfortran 13.3.0, x86-64, SysV)

Read off the call sites of a compiled caller (`gfortran -O0 -S`):

| callee | arguments passed | result |
|---|---|---|
| module `pmod(n, a(n), b(n))` | `rdi=&n`, `rsi=&a`, `rdx=&b` | — |
| module `fmod2(n, a(n))` | `rdi=&n`, `rsi=&a` | `xmm0` |
| F77 `pext(n, a(n), b(n))` | `rdi=&n`, `rsi=&a`, `rdx=&b` | — |
| F77 `psize(a(*), b, n)` | `rdi=&a`, `rsi=&b`, `rdx=&n` | — |
| F77 `fext(n, a(n))` | `rdi=&n`, `rsi=&a` | `xmm0` |

So an external procedure and a module procedure agree on the dummy wire
(raw base address for explicit-shape and assumed-size arrays, by
reference for scalars); they differ only in the *symbol* they are
reached through (`<name>_` versus `__<mod>_MOD_<proc>`).

## Result

Real unit `fem/src/Walls.F90` (four F77 globals: `solve_ufric`,
`wall_law`, `d_wall_law`, `kewall` — all dummies scalar intrinsics) now
grades instead of refusing:

- **equivalence corpus**: 12 cases × 7 printed values = 84 lines at 17
  significant figures (`ES25.17E3`), **byte-identical** between the
  Fortran reference and the Rust port, stderr empty on both sides.
  Cases 11-12 were held out (low-viscosity and high-roughness regimes).
- **strong-symbol gate**: the port defines all four unit symbols as `T`,
  and the intersection with the reference `Walls.F90.o` is exactly those
  four — **zero STRONG port-level dups** beyond the replaced symbols, so
  the port replaces `Walls.F90.o` whole (whole-file portable).
- **negative controls**: an F77 external carrying `COMMON`,
  a derived-type dummy, `OPTIONAL`, `ALLOCATABLE`, a character dummy or
  an internal procedure still refuses with the pinned message; the
  module-procedure shapes (explicit-shape, assumed-size, non-`REAL(8)`,
  rank 3+) still refuse — they are shape B, not this one.
- **ADR-027 fixture**: `tests/fixtures/fortran_shim/src/scale.F` (F77
  assumed-size) still grades green, and now passes the gate too.

Proof: `tests/f77_external_shape_proof.sh`; port crate
`crates/elmer-fem-a/ports/walls` in the port repo (elmer-rust PR);
machine rows in the grading store (`gate_results`: admit, fixture,
corpus, strong-symbol, negative controls).

### Porting notes worth recording

Two fidelity facts the corpus caught (both are real ABI/FP facts, not
port bugs):

- `-0.33` in `D_WALL_LAW` is a **default-real literal**; gfortran widens
  `REAL(4)` to double in a double-precision expression, so the port must
  use the widened `REAL(4)` value (the difference is ~1e-8 relative).
- `x**n` with an **integer** exponent is gfortran's `__powidf2`, i.e.
  repeated multiplication — **not** `pow(x, n)`. For `n = 3` the two
  differ by one ulp.

## Limitations

- A file whose external subprogram contains **internal procedures** still
  refuses: the frozen inventory lists them as exports, and the gate
  cannot prove they are not global symbols without a recon-side fix.
  Recorded as a follow-up, not worked around here.
- The frozen campaign inventory models a file with several F77 globals as
  **one** unit carrying every scope export (`fortran:fem/src/Walls.F90`
  has 8), while the current recon splits it per scope. The gate admits
  both shapes; both are covered by tests.
- The `COMMON`/`BLOCK DATA` coupling refusal is unchanged (ADR-027).
- Timing: lane-fem-a has no `bench_sample_unit` (null at dispatch), so
  **no timing sample runs for this shape**; per-unit grades carry no
  timing claim (ADR-045).
