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

## Bench (measured 2026-10-14; driver `bench/bench.py`, rows in `bench/results.json`)

| Workload | Track | Baseline | Port | Speedup |
|---|---|---|---|---|
| R1 version.parse 2000 mixed | representative | 16.760ms | 4.115ms | 4.07x |
| R2 marker.evaluate 2000 | representative | 21.774ms | 15.407ms | 1.41x |
| S1 resolver filter 15k versions | showcase | 75.781ms | 34.249ms | 2.21x |
| X1 per-crossing overhead (one parse + one marker) | overhead | 0.067ms | 0.058ms | 1.16x |
