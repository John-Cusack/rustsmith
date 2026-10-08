# ADR-027: non-BIND(C) Fortran ABI shim design (Elmer S4)

Date: 2026-10-07. Scope: design only; `CmakeBridge::check_substitutable`
stays as-is. No gate relaxation in this change.

## Problem

`check_substitutable` (`crates/rustsmith-adapters/src/lib.rs`) refuses every
unit with a non-`BIND(C)` Fortran export, which is ~99% of Elmer's Fortran.
The refusal is correct today: a raw-pointer Rust stub behind a mangled name
is silently wrong for assumed-shape dummies, not merely unlinked. Measured
on gfortran 13.3.0 / x86-64 (probes kept outside the worktree): calling
`__huti_mod_MOD_huti_assumed(a(:,:))` with a raw `double*` returned `s = 0`
instead of `10` with exit 0 — no crash, no signal, wrong science. A zeroed
descriptor guess segfaulted (exit 139). Both failure modes argue for a
pinned layout, not braver stubs.

## Decision

Port non-`BIND(C)` Fortran at file granularity through a shim crate that
reproduces the gfortran calling convention exactly. Three pinned pieces:

### 1. Symbol mapping (all measured with `nm` on gfortran 13.3.0 objects)

| Fortran source | object symbol | Rust shim |
|---|---|---|
| module procedure, no `BIND(C)` | `__<mod>_MOD_<proc>`, all lowercase | `#[export_name = "__<mod>_MOD_<proc>"] pub extern "C" fn` |
| F77 global subroutine/function | `<name>_` lowercase | `#[export_name = "<name>_"]` |
| `COMMON /name/` | global `<name>_` (`D` with `BLOCK DATA` init, BSS/common otherwise) | refused (see below) |
| `BLOCK DATA name` | `<name>_` (`B`) | refused with its `COMMON` |
| `BIND(C, name="x")` | `x` verbatim | existing scaffold, unchanged |

The frozen recon linkage `__<mod>_MOD_<proc>` matches: the parser stores
scope/proc names from the lowercased line, and gfortran lowercases. Note:
the `fhutiter` porting rule example (`export_name = "huti_dcgsolv_"` for a
MODULE procedure) has the wrong mangling shape — module procedures take the
`__mod_MOD_proc` form, trailing underscore is F77 globals only. That example
needs a one-line fix in a follow-up session; this ADR records the measured
form.

### 2. Descriptor layout (measured by dumping a real descriptor through an
explicit assumed-shape interface, then round-tripping a C-fabricated one)

`real(8) :: a(2,2)` whole array, rank-1 `v(5)`, section `a(2:1:-1,:)`,
section `v(2:5)` — total size is `40 + 24·rank` bytes:

| off | size | field | value (`real(8)`, rank 2) |
|---|---|---|---|
| 0 | 8 | `base_addr` | address of first element actually passed (section-aware) |
| 8 | 8 | `offset` (signed, elements) | `-(Σ lb·stride)`; whole 2x2: `-3`; `a(2:1:-1,:)`: `-1` |
| 16 | 8 | element byte size | `8` |
| 24 | 4 | `dtype.version` | `0` |
| 28 | 1 | `dtype.rank` | `1` / `2` (confirmed both) |
| 29 | 1 | `dtype.type` | `3` (`BT_REAL`; only code measured) |
| 30 | 2 | `dtype.attribute` | `0` (also `0` for an allocatable actual arg) |
| 32 | 8 | second size field | `8` (name per `libgfortran.h`; value pinned, name not) |
| 40+ | 24/dim | `{stride (elements, signed), lower, upper} × rank` | `(1,1,2),(2,1,2)`; section: `(-1,1,2)`; `v(2:5)`: `(1,1,4)` |

A C-fabricated descriptor with this layout called into Fortran `sum` +
`lbound`/`ubound` returned `s = 10`, bounds `(1..2, 1..2)` — the layout is
proven callable, not just observed. Integer/complex/character/logical
`dtype.type` codes and kinds were NOT measured; the proof fixture must
leak-dump one descriptor per `(type, kind, rank)` the port uses before the
shim template hardcodes it. Layout is gfortran-version-specific: the fixture
pins `gfortran --version` and re-dumps on mismatch, and the Elmer build-flag
audit (`-fdefault-real-8`, `-fdefault-integer-8` change element codes)
is part of the proof, not assumed.

### 3. Assumed-shape vs explicit-shape matrix (each cell measured)

| dummy form | wire argument | shim treatment |
|---|---|---|
| scalar (`integer`, `real(8)`) | address of value (by reference) | plain raw pointer |
| explicit-shape `x(n)`, `x(m,n)` | raw base address | `&[f64]` / `&mut [f64]`; `debug_assert_eq` on the extent contract (matches `fhutiter` rule) |
| assumed-size `x(*)` | raw base address (measured `s = 10`) | same as explicit-shape |
| assumed-shape `x(:)` / `x(:,:)` | descriptor pointer (table above) | Rust builds the descriptor over the Rust-owned buffer; callee never retains it (copy-in/out for `intent(out)` sections) |
| `allocatable` / `pointer` dummy | same wire shape (bytes identical to plain in the dump) | REFUSED phase 1: callee may reallocate via libgfortran (`malloc` contract + in-place `base_addr` update) |
| `external` callback dummy (matvec) | raw code address | `extern "C" fn` pointer, identical sequence, never a closure (matches `fhutiter` rule) |
| character length / `optional` / `present`-test dummies | hidden trailing args (length `size_t`, presence flags) | REFUSED phase 1: not measured, no template |

### What stays refused

- F77 `COMMON` (any unit whose file contains one, plus its `BLOCK DATA`):
  the symbol is global and link-merges across files, so a Rust owner must
  reproduce the exact byte size/layout including `EQUIVALENCE` padding and
  `BLOCK DATA` init order; a second definition (Rust + surviving Fortran
  owner) is a multiple-definition link error, a missing one is silent
  zero-init. Whole-file port does not help — the coupling is whole-program.
- `allocatable`/`pointer` dummies, character-length and `optional` hidden
  params (above): unmeasured or allocator-coupled.
- Relaxation rule: `check_substitutable` gains an allowlist only per unit
  shape already green in the fixture below — file-granularity,
  COMMON-free, descriptor kinds pinned. Each relaxation is its own PR with
  graded-run numbers, per repo rules.

### `ld -r` composition (no flow change)

`substitute` already overwrites `<target>.dir/<rel>.o` with
`ld -r --whole-archive librust.a` and rebuilds the tree. The shim composes
unchanged: the Rust staticlib must define every global the removed `.o`
defined (mangled procs + F77 globals via `export_name`), otherwise the final
link fails fast on an undefined reference — honest, never silent. No
link-line surgery, no `CMakeLists.txt` edit.

## Consequences

- No code changes here: gate, scaffold note, and recon linkage are already
  consistent with this design; the `fhutiter` example mangling fix and any
  template work wait for the build session after review.
- Fixture proof plan (new `tests/fixtures/fortran_shim/`, mirror-style):
  one module with explicit-shape add, assumed-shape sum, F77 assumed-size
  scale (orig Fortran + reference outputs); Rust port with constructed
  descriptors graded to divergence 0; two negative fixtures that must NOT
  go green — raw-pointer call into assumed-shape (diverges, documents the
  old refusal) and zeroed-dtype descriptor (fails fast); one COMMON unit
  pinning the `check_substitutable` refusal message. Gate relaxation only
  after all four are observed, on the grade image's gfortran.
