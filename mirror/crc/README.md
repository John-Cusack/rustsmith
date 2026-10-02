# crc-rust (Rust mirror of Nicoretti/crc)

Stage-1 Rust mirror of [`Nicoretti/crc`](https://github.com/Nicoretti/crc)
(BSD-2-Clause, original by Nicola Coretti). Behavior-identical port of
`src/crc/_crc.py` exposed as the `crc._crc` extension module via PyO3, with
thin `crc/__init__.py` + `crc/__main__.py` re-export shims preserving the
original public API (`Calculator`, `Register`, `TableBasedRegister`,
`Crc8/16/32/64`, `create_lookup_table`, `main`, ...).

## Layout (publishable)

One implementation, two distributions:

- `crc-core/` — reusable pure-Rust core crate `crc-rust-core` (crates.io).
  No Python dependency: `rlib`, `cargo test` clean, miri-clean. Rust
  consumers depend on this crate directly.
- `src/lib.rs` — thin PyO3 binding crate `crc-rust` calling that same core.
  Ships on PyPI as distribution **`crc-rust`**; the import name is unchanged
  (`import crc`), so users swap the install with zero code changes:
  `pip install crc-rust` instead of `pip install crc`.
- `crc/` — Python shims (`__init__.py`, `__main__.py`).
- `release.toml` — release configuration (names, version, platforms, Python
  versions, publishing repo + workflow identity, license, upstream
  attribution). Read by `rustsmith release-prep`; see `docs/RELEASE.md`.

No redesign: byte-at-a-time table lookup ships, same as the original.
Slice-by-8/16 is a Stage-2 optimization candidate, not part of the mirror.

## Install

```sh
pip install crc-rust        # Python (import crc)
cargo add crc-rust-core     # Rust
```

## Release

Publishing is a manual step after review; `rustsmith` never pushes.
Prepare and verify everything locally, then follow the generated setup
instructions:

```sh
rustsmith release-prep --project mirror/crc --fork <fork> --opt <opt> \
  --recon-out <recon> --out <dist>
```

See `docs/RELEASE.md` (releasing converted projects) and the generated
`TRUSTED_PUBLISHING_SETUP.md` in the output directory.
