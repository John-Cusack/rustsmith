# ADR-049: release lane for ports that are not mirror templates

Date: 2026-10-20
Status: accepted

## Context

The release lane (`docs/RELEASE.md`, `rustsmith release-prep`) is built for
**mirror templates**: a project under `mirror/<pkg>/` whose published Python
distribution is a drop-in replacement for the upstream package, so
`validate_names` can require `project == python_import` and
`pypi_dist == <upstream>-rust` with the upstream name staying with
upstream.

Two ports are not mirror templates:

- `sympy-rust` — a kernel-scoped port of one SymPy seam (`dup_*` / `dmp_mul`
  over `QQ`), not a port of the `sympy` package. The published import
  identity is `sympy_poly`; the upstream `sympy` import stays upstream and
  the port's Python layer consumes `sympy.QQ`.
- `docutils-rust` — a clean-room port of the reStructuredText parser,
  transform pipeline, and publishers, published as `docutils_rust` so it
  never shadows the upstream `docutils` import.

Both live in their own public repositories (repo-early rule), not under
`mirror/`.

The lane tooling cannot express either shape without lying:

- `validate_names` forces `python_import == project`, so the published
  import name would have to be the upstream name (`sympy` / `docutils`) —
  i.e. the published wheel would shadow the upstream package with a partial
  implementation. `pypi_dist` must also differ from `project` after PEP 503
  normalization, which forbids the natural `docutils-rust` / `docutils_rust`
  pair.
- `validate_tree` requires the drop-in layout (root binding crate,
  `<python_import>/__init__.py` at the tree root, a path dependency on the
  core) and `release-prep` installs the wheel with `--no-index --no-deps`,
  which cannot satisfy a declared runtime dependency (`sympy`).

Forcing the mirror shape would have produced a wheel that claims to be
`import docutils` / `import sympy` while covering a fraction of the
upstream behavior.

## Decision

Release prep for a non-mirror-template port runs the **same battery** as
`release-prep`, driven by a lane-equivalent `release.toml` in the package's
own repository:

- config: `release.toml` with the lane field names, but `project` /
  `python_import` = the port's import identity (`sympy_poly`,
  `docutils_rust`) and `pypi_dist` = `<port>-rust` (`sympy-poly-rust`,
  `docutils-rust`). `runtime_dependencies` is explicit;
- artifacts: wheel + sdist + `.crate`, with `SHA256SUMS`,
  `release-manifest.json`, `verification.json`, and `release-state.json`
  (one lane per registry, all `pending` at prep);
- battery: core unit tests -> downstream Rust consumer built against the
  core by path (no Python) -> `cargo package` on the core ->
  `maturin build --release` -> `maturin sdist` -> clean venv + wheel from
  disk (dist identity + every `smoke_exprs`) -> sdist unpacked, wheel
  rebuilt from it, installed into a second clean venv, same smoke vectors
  -> the port's own parity suite re-run against the installed wheel;
- publishing: the lane's own `rustsmith release-publish` (policy gate,
  same-SHA256 promotion, remote hash reconciliation, index install
  verification, sticky per-registry state).

The bundle layout is the one `release-publish` consumes, so the upload
discipline (TestPyPI first, production only behind
`RUSTSMITH_ALLOW_PROD_PUBLISH=1` plus a fresh TestPyPI success) is
unchanged.

## Consequences

- The two ports ship honest metadata: the upstream import name is never
  shadowed, and the published README states the subset scope.
- The lane's naming rule (`project == python_import`) stays as it is for
  mirror templates; a future lane that wants non-drop-in ports in
  `mirror/` should relax the rule in `validate_names` rather than
  hand-rolling a battery.
- `release-prep` remains the entry point for mirror templates; this ADR
  records the fallback shape, not a replacement tool.
- `release-publish` needs a **absolute** `--state` path: it runs twine with
  the bundle directory as cwd, so a relative state path makes the artifact
  paths unresolvable (the first TestPyPI attempt in the 2026-10-20 lane
  failed that way — nothing was uploaded).
- The production lane's post-upload check is index-propagation sensitive:
  a successful upload can still be reported as failed when the index has
  not yet served the new project. Verify through the index JSON API (file
  names + SHA-256) and a pinned install before recording a failure.
