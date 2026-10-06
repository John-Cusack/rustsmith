# packaging-rust (Rust mirror of pypa/packaging)

Stage-1 Rust mirror of [`pypa/packaging`](https://github.com/pypa/packaging)
(Apache-2.0 OR BSD-2-Clause). Behavior-identical port of `src/packaging/*.py`
exposed as the `packaging._packaging` extension module via PyO3, with thin
`packaging/*.py` re-export shims preserving the original public API
(`Version`, `SpecifierSet`, `Requirement`, `Marker`, `sys_tags`, ...).

## Layout (publishable)

One implementation, two distributions:

- `packaging-core/` — reusable pure-Rust core crate `packaging-rust-core`
  (crates.io). No Python dependency: `rlib`, `cargo test` clean. Rust
  consumers depend on this crate directly.
- `src/lib.rs` — thin PyO3 binding crate `packaging-rust` calling that same
  core. Ships on PyPI as distribution **`packaging-rust`**; the import name
  is unchanged (`import packaging`), so users swap the install with zero
  code changes: `pip install packaging-rust` instead of `pip install
  packaging`.
- `packaging/` — Python shims (one per original module; `errors.py` keeps
  the original version-branch glue verbatim since it selects the stdlib
  `ExceptionGroup` on 3.11+).
- `release.toml` — release configuration (names, version, platforms, Python
  versions, publishing repo + workflow identity, license, upstream
  attribution). Read by `rustsmith release-prep`; see `docs/RELEASE.md`.

No redesign: same parsing, same ordering, same error messages.

## Install

```sh
pip install packaging-rust
python -c "from packaging.version import Version; print(Version('1.0'))"
```
