# crc (Rust mirror)

Stage-1 Rust mirror of [`Nicoretti/crc`](https://github.com/Nicoretti/crc)
(BSD-2-Clause, original by Nicola Coretti). Behavior-identical port of
`src/crc/_crc.py` exposed as the `crc._crc` extension module via PyO3, with
thin `crc/__init__.py` + `crc/__main__.py` re-export shims preserving the
original public API (`Calculator`, `Register`, `TableBasedRegister`,
`Crc8/16/32/64`, `create_lookup_table`, `main`, ...).

Layout (ADR-009): a Cargo workspace with two crates from one source tree.
`crc-core/` is the reusable Rust core (`rlib`, no Python dependency):
algorithms, catalogs, and register state machines (`BitRegister`,
`TableRegister`, `Crc`) with a plain-Rust API. `src/lib.rs` is the thin
PyO3 binding (`cdylib` `_crc`) that converts Python objects to core types
and calls the core — it contains no second implementation. Rust consumers
depend on `crc-core` directly; Python consumers install the built wheel.

No redesign: byte-at-a-time table lookup ships, same as the original.
Slice-by-8/16 is a Stage-2 optimization candidate, applied to `crc-core`
by the deterministic `slicing-by-8` transform (never to the binding).
