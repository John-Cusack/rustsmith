# fortran_shim proof fixture (T003, ADR-027)

Proves the non-`BIND(C)` Fortran calling convention is callable from Rust
before any `check_substitutable` relaxation. Pinned to gfortran 13.3.0;
re-dump on version mismatch.

| file | shape | wire argument |
|---|---|---|
| `src/shim_mod.F90:explicit_add` | explicit-shape `a(n)` | raw base address |
| `src/shim_mod.F90:assumed_sum` | assumed-shape `a(:,:)` | descriptor pointer (40 + 24·rank bytes) |
| `src/scale.F:fscale` | F77 assumed-size `x(*)` | raw base address |
| `src/common_state.F:cstep` | `COMMON /shstate/` | REFUSED — whole-program coupling |

Proof (`tests/fortran_shim_proof.sh`): orig objects + `tests/probe_shim.F90`
give `ref/expected_stdout.txt`; linking the probe against
`ref/rust/shim_ports` instead grades divergence 0 (substitute mechanics:
drop the Fortran `.o`, link the Rust staticlib). Negatives:
`ref/rust/shim_neg_rawptr` (raw pointer into assumed-shape) MUST diverge
with exit 0; `tests/driver_zeroed_dtype.c` (zeroed dtype) MUST die by
signal. `src/common_state.F` MUST keep `check_substitutable` refusing
(pinned by `fortran_shim_common_refusal_pins_message` in
`crates/rustsmith-adapters`).
