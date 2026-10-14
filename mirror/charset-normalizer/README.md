# charset-normalizer-rust (Rust mirror of Ousret/charset_normalizer)

Stage-1 Rust mirror of [`Ousret/charset_normalizer`](https://github.com/Ousret/charset_normalizer)
(MIT). Behavior-identical port exposed as the `charset_normalizer._charset_normalizer`
extension module via PyO3, with `charset_normalizer/__init__.py` re-export shim.

## Layout (publishable)

One implementation, two distributions:

- `charset_normalizer-core/` — reusable pure-Rust core crate
  `charset-normalizer-core` (crates.io). No Python dependency: `rlib`,
  `cargo test` clean. Rust consumers depend on this crate directly.
- `src/lib.rs` — thin PyO3 binding crate `charset-normalizer-rust` calling
  that same core. Ships on PyPI as distribution **`charset-normalizer-rust`**;
  the import name is unchanged (`import charset_normalizer`), so users swap
  the install with zero code changes.
- `charset_normalizer/` — Python shims (`__init__.py`, `__main__.py`).
- `release.toml` — release configuration (names, version, platforms, Python
  versions, publishing repo + workflow identity, license, upstream
  attribution). Read by `rustsmith release-prep`; see `docs/RELEASE.md`.

## Bench (measured 2026-10-14; driver `bench/bench.py`, rows in `bench/results.json`)

| Workload | Track | Baseline | Port | Speedup |
|---|---|---|---|---|
| R1 from_bytes 3 mixed docs | representative | 11.692ms | 4.489ms | 2.60x |
| R2 from_bytes 121KB ascii logs | representative | 0.418ms | 1.268ms | 0.33x (slower: ascii fast path) |
| S1 from_bytes 1MB latin bulk | showcase | 6.297ms | 11.117ms | 0.57x (slower) |
| X1 per-crossing overhead (tiny ascii detect) | overhead | 0.194ms | 0.055ms | 3.54x |
