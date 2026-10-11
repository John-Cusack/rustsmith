# ADR-031: `release-prep --init` scaffolds `release.toml` from recon facts

Date: 2026-10-11
Status: accepted

## Context

Five lanes hand-authored `release.toml` and paid the same naming tax:
`validate_names`' hyphen rule (`project` hyphen-free module name ==
`python_import`; `pypi_dist` upstream + `-rust`; repo `<project>-rust`)
was fixed per lane in dateutil (ADR-016), charset (#43), markdown-it (#49),
multipart (#51), and pyyaml (#57), each with identical shape.

## Decision

`rustsmith release-prep --init --project <dir> --recon-out <dir>` derives
the canonical names from frozen recon facts (`probe.package` for the
upstream dist, `probe.attribution` for upstream/license defaults) and writes
`<project>/release.toml`:

- `project`/`python_import` = dist with `-`/`.` → `_`, unless `--module`
  overrides (lanes where the import name differs from the dist:
  `pyyaml`→`yaml`, `python-dateutil`→`dateutil`, `markdown-it-py`→`markdown_it`);
- `pypi_dist` = upstream dist + `-rust`;
- `github_repo` = `--github-owner` (else the upstream owner) + `/` + `<project>-rust`;
- `version` = `--version`, else the fork's `[project] version`, else `0.1.0`;
- `license_spdx` = `--license-spdx`, else the recon-detected id when it is a
  known SPDX tag, else `--init` refuses (no guessed license ever ships);
- `rust_crate`, matrix, and environments are reviewed defaults; the smoke
  vector is a `__name__` placeholder the lane replaces with real vectors.

The scaffold parses and passes `validate_names` by construction (the pure
derivation lives in `rustsmith-release::scaffold_release_toml`, unit-tested);
existing files are refused unless `--force`.

## Consequences

- New lanes never hand-author this file; the eighth port starts from facts.
- The `validate_names` gate is unchanged: a reviewed scaffold that drifts
  still fails prep naming its field.
