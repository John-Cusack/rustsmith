# ADR 016: dateutil publishing repo created empty; recon license record deferred

- Context: the `mirror/python-dateutil` port has no `release.toml`, no
  prepared fork/opt tree, and no recon run (`recon.json`/`facts.json`).
  `create-github-repo --yes` needs all three — it records against a run-id
  and pushes the fork's `main` — so the tool path cannot create the repo
  without fabricating lane inputs, and the task orders creation-only
  (no tree push; the later publish owns it).
- Decision: author `mirror/python-dateutil/release.toml` with
  `project = "dateutil"` (hyphen-free module name per `validate_names`;
  gives the lane-canonical `John-Cusack/dateutil-rust`, the `dateutil-core`
  dir, and `python_import == project`), then create the repo directly as an
  empty public repo. License (`Apache-2.0`) verified from genuine sources —
  upstream sdist LICENSE text, both manifests, mirror NOTICE — never
  defaulted; the missing recon license record is reported, not guessed.
  Remote + run events recorded as a JSON record in the task data dir (no
  run store exists yet); the publish lane re-records via the tool on `--yes`.
- Consequences: an empty public repo asserts no license and publishes no
  code. LICENSE (upstream Apache-2.0 text) + NOTICE arrive with the later
  publish's tree push, which re-verifies everything through `release-prep`
  and the tool's dry-run before pushing.
- Alternatives: fabricating a recon-out/fork/run-id to drive the tool —
  rejected (guessed inputs); pushing any tree now — rejected (out of scope).
