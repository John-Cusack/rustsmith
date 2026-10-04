# Releasing converted projects (`release-prep`)

This document is about releasing **rustsmith's output** (a converted project
like `crc-rust`) — not about releasing rustsmith itself. For that
distinction, see `README.md` ("Releasing").

Status: **implemented** for Python-spine projects whose template ships the
publishable layout (`mirror/crc` is the first end-to-end example).
`mirror/strsimpy` still ships the old flat layout and is **not**
release-ready (planned: migrate it to the same split).

## What a release is

One implementation, two distributions from the converted project:

- a reusable Rust core crate on **crates.io** (e.g. `crc-rust-core`:
  `rlib`, no Python dependency — Rust consumers depend on it directly);
- a Python package on **PyPI** calling that same core (e.g. `crc-rust`:
  PyO3 extension + shims, import name unchanged so `pip install crc-rust`
  is a drop-in for `pip install crc`).

Both are distributable through their registries. Publishing itself is a
manual step after review; rustsmith never pushes anywhere.

## 1. Release configuration (`mirror/<package>/release.toml`)

One file per converted project. Copy `mirror/crc/release.toml` and fill in
the new values. Fields:

| Field | Meaning |
|---|---|
| `project` | Original package identity (matches `template.json` `package`). |
| `rust_crate` | Rust core crate name (crates.io). |
| `pypi_dist` | Python distribution name (PyPI). MUST differ from `project` (the upstream name stays with upstream). |
| `python_import` | Python import name. MUST equal `project` (zero import changes). |
| `version` | Release version. MUST equal `[package] version` in both `Cargo.toml` files and `[project] version` in `pyproject.toml`. |
| `requires_python` / `python_versions` | Floor (`>=X.Y`) and the tested versions (workflow matrix). Every listed version must satisfy the floor. |
| `platforms` | GitHub runners the workflow builds and tests on. |
| `github_repo` / `release_workflow` | Publishing repo (`owner/name`) and workflow path. These ARE the trusted-publisher identity. |
| `pypi_environment` / `testpypi_environment` | GitHub environments for trusted publishing. |
| `license_spdx` | SPDX tag (closed list: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, MPL-2.0, GPL variants). |
| `upstream`, `upstream_url`, `upstream_license`, `upstream_authors` | Attribution (must also appear in `NOTICE`). |
| `smoke_exprs` | Python expressions checked against the installed wheel (each must be truthy). |

## 2. Project layout prerequisites

`release-prep` refuses trees that do not satisfy these (each failure names
the missing prerequisite — see §6):

1. **The Rust core is independent of Python.** The core crate (found through
   the binding's path dependency) must not depend on `pyo3` (or similar),
   must build as a plain library (never `cdylib`), and must carry
   `src/lib.rs`. This is checked, not assumed.
2. **The final accepted source is retained.** The optimize stage merges
   winners into the `--opt` work tree, not back into the fork.
   `release-prep` therefore takes `--opt` when it exists (the final
   accepted revision) and falls back to `--fork`. The recorded `source_sha`
   is that tree's git HEAD.
3. **Metadata agrees everywhere.** Crate name/version, dist name/version,
   `requires-python`, maturin `module-name` (must live under the import
   package and match the `[lib]` target), the `<import>/__init__.py` shim,
   and `NOTICE` (upstream + license tag) are all cross-checked.

## 3. `release-prep` (local, no uploads)

```sh
rustsmith release-prep --project mirror/crc --fork <fork> [--opt <opt>] \
  --recon-out <recon> --out <dist>
```

It stages a clean copy of the final accepted source, then:

1. injects the measured-performance receipt into the staged `README.md`
   (`collect_readme_perf` over the `--opt` tree's `optimize-report.json`,
   `render_readme_perf`, `inject_readme_perf` between the
   `RUSTSMITH-PERF:BEGIN/END` markers; a source with no report receipts
   the mirror baseline). The block is generated — never hand-edit it —
   and carries measured outcomes only: artifact hashes live in
   `release-manifest.json`, never in the README. Recorded in
   `verification.json` as `readme_perf`;
2. runs the core's `cargo test` (the same core Rust consumers use);
3. builds a throwaway downstream crate depending on the core by path with
   no Python anywhere (reusability proof) and runs `cargo package` on the
   core (what crates.io receives);
4. builds the wheel (`maturin build --release`) and the sdist
   (`maturin sdist`);
5. installs the wheel from disk into a fresh venv (`pip install --no-index
   --no-deps`) and checks the dist name/version plus every `smoke_exprs`;
6. unpacks the sdist, asserts it contains everything needed to rebuild
   (both manifests, `pyproject.toml`, the import shim, `NOTICE`), and
   rebuilds the wheel from the unpacked sdist (proves the core ships
   inside);
7. retains artifacts (wheel, sdist, `.crate`), `SHA256SUMS`,
   `release-manifest.json` (config snapshot + `source_sha` + artifact
   hashes), `verification.json` (every check above with pass/fail),
   `release-state.json` (per-registry tracking, all `pending`), the
   generated `.github/workflows/release.yml`, and
   `TRUSTED_PUBLISHING_SETUP.md` (exact first-time setup).

Any failure stops the run with the failing check and its log tail; nothing
is published, and `release-status` on the (unwritten) state is meaningless
— fix the cause and re-run.

## 4. Generated workflow (`release.yml`)

Generated from `release.toml` (reusable across projects — no hardcoded
names). Lanes, in order:

1. `build`: matrix over the configured platforms × Python versions; builds
   one wheel per cell and uploads each as an artifact; core `cargo test`
   runs on the reference cell.
2. `sdist`: builds the sdist (asserts the core crate is inside) and uploads
   it.
3. `test-wheels`: downloads the built wheels, installs from disk, runs the
   smoke vectors, and re-uploads the SAME files as `tested-dist`.
4. `publish-testpypi`: downloads `tested-dist` + sdist (never rebuilds)
   and publishes to TestPyPI via trusted publishing (`id-token: write`,
   environment `testpypi_environment`).
5. `verify-testpypi`: fresh `pip install` from the TestPyPI index plus the
   smoke vectors (proves the index, not the local file).
6. `release` (manual only: `workflow_dispatch` with `publish_production:
   true`): publishes the SAME `tested-dist` files to PyPI (trusted
   publishing, environment `pypi_environment` with required reviewers) and
   runs `cargo publish` for the Rust core on crates.io.

## 5. Publication tracking (per registry, retries explicit)

`release-state.json` holds one outcome per registry (`testpypi`, `pypi`,
`crates-io`): `pending` | `success` | `failed`, each with an append-only
attempt history.

```sh
rustsmith release-record --state <dist>/release-state.json \
  --registry testpypi --result success --detail "workflow run 42"
rustsmith release-status --state <dist>/release-state.json
```

- `release-status` exits 0 and prints `complete` ONLY when all three
  registries read `success`. One success out of three prints `partial`
  (exit 1) — retry the remaining registries, never re-report finished ones.
- A failure prints `failed`/`partial` with history; re-running the same
  `--registry` after fixing the cause appends a new attempt (retries are
  per-registry, never "publish everything again").
- Recording an unknown registry or result is refused (it would corrupt the
  state the workflow and humans share).

## 6. Troubleshooting (invalid metadata / prerequisites)

Every error names the field and the file. Common ones:

- `` `pypi_dist` "crc" collides with the original package `` — rename the
  converted dist to `<orig>-rust`.
- `` Cargo.toml version "9.9.9" != release.toml version "0.1.0" `` —
  versions drifted; bump (or revert) so all three manifests agree.
- `` core crate depends on pyo3 (the Rust core must be independent ...) ``
  — move the binding into the extension crate; the core must build without
  Python.
- `` missing `crc/__init__.py` `` / `` missing NOTICE `` — the import shim
  and upstream attribution must ship.
- `` sdist is missing rebuild inputs `` / `` sdist rebuild failed `` — the
  sdist does not carry the core (check `sdist-include` and path deps).
- `` release partial (not complete) `` — not an error in the artifacts:
  publish/record the remaining registries.

## 7. Releasing rustsmith itself

Out of scope for this document: rustsmith itself ships as a Cargo workspace
binary (`cargo build --release`); it is not published to crates.io or PyPI.
See `README.md`.
