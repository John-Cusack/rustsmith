# ADR 019: Stage the whole tree for flat layouts in optimize

- Context: Stage-2 orig staging guessed the import dir as
  `orig.join(package.replace('-', "_"))`. That holds when the dist name
  maps onto the import name (`charset-normalizer` →
  `charset_normalizer`), but not in general: `markdown-it-py` imports as
  `markdown_it`, `python-dateutil` as `dateutil`. The guess pointed at a
  nonexistent dir and the run halted before Round 0 (`No such file or
  directory`). Two call sites shared the guess (run entry staging,
  candidate parent staging).
- Decision: flat layouts stage the whole orig tree, exactly as the mirror
  stage already does for differential grading (`orig_src`); probes resolve
  the package via cwd. `src/` and `lib/` layouts keep staging their
  verified roots (`is_src_layout`/`is_lib_layout` check the marker files,
  so those branches never guess).
- Consequences: slightly larger scratch copies for flat repos (whole tree
  instead of one package dir); no behavior change for `src/`/`lib/`
  repos. Future flat ports (`pyyaml`, `python-multipart`, `pyparsing`)
  inherit the fix.
- Alternatives: a new package-keyed `import_dir` data field (more
  per-repo data for something the tree already determines); deriving from
  template file paths (optimize must not depend on mirror templates) —
  rejected.
- Spec: no SPEC change; staging stays host-side scratch.
