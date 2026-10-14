# strsimpy-rust (Rust mirror of luozhouyang/python-string-similarity)

Stage-1 Rust mirror of
[`luozhouyang/python-string-similarity`](https://github.com/luozhouyang/python-string-similarity)
(MIT, original by ZhouYang Luo). Behavior-identical port of the `strsimpy`
package exposed as the `strsimpy._strsimpy` extension module via PyO3, with
a thin `strsimpy/__init__.py` re-export shim preserving the original public
API (`Levenshtein`, `Damerau`, `JaroWinkler`, `Cosine`, `SIFT4`, ...).

## Layout (publishable)

One implementation, two distributions:

- `strsimpy-core/` — reusable pure-Rust core crate `strsimpy-rust-core`
  (crates.io). No Python dependency: `rlib`, `cargo test` clean,
  miri-clean. Rust consumers depend on this crate directly.
- `src/lib.rs` — thin PyO3 binding crate `strsimpy-rust` calling that same
  core. Ships on PyPI as distribution **`strsimpy-rust`**; the import name
  is unchanged (`import strsimpy`), so users swap the install with zero
  code changes: `pip install strsimpy-rust` instead of `pip install
  strsimpy`.
- `strsimpy/` — Python shim (`__init__.py`).
- `release.toml` — release configuration (names, version, platforms, Python
  versions, publishing repo + workflow identity, license, upstream
  attribution). Read by `rustsmith release-prep`; see `docs/RELEASE.md`.

No redesign: every algorithm replicates the original op-for-op over chars
(never bytes) with identical accumulation order, so float outputs are
bit-exact.

## Install

```sh
pip install strsimpy-rust        # Python (import strsimpy)
cargo add strsimpy-rust-core     # Rust
```

## Release

Publishing is a manual step after review; `rustsmith` never pushes.
Prepare and verify everything locally, then follow the generated setup
instructions:

```sh
rustsmith release-prep --project mirror/strsimpy --fork <fork> --opt <opt> \
  --recon-out <recon> --out <dist>
```

See `docs/RELEASE.md` (releasing converted projects) and the generated
`TRUSTED_PUBLISHING_SETUP.md` in the output directory.

## Bench (measured 2026-10-14; driver `bench/bench.py`, rows in `bench/results.json`)

| Workload | Track | Baseline | Port | Speedup |
|---|---|---|---|---|
| R1 levenshtein 200 short pairs | representative | 9.698ms | 0.149ms | 65.15x |
| R2 cosine-2 100 doc pairs | representative | 115.915ms | 12.647ms | 9.17x |
| S1 metric-lcs 2KB x 2KB | showcase | 11384.140ms | 24.156ms | 471.27x |
| X1 per-crossing overhead (one short distance) | overhead | 0.080ms | 0.039ms | 2.07x |
