# crc (Rust mirror)

Stage-1 Rust mirror of [`Nicoretti/crc`](https://github.com/Nicoretti/crc)
(BSD-2-Clause, original by Nicola Coretti). Behavior-identical port of
`src/crc/_crc.py` exposed as the `crc._crc` extension module via PyO3, with
thin `crc/__init__.py` + `crc/__main__.py` re-export shims preserving the
original public API (`Calculator`, `Register`, `TableBasedRegister`,
`Crc8/16/32/64`, `create_lookup_table`, `main`, ...).

No redesign: byte-at-a-time table lookup ships, same as the original.
Slice-by-8/16 is a Stage-2 optimization candidate, not part of the mirror.
