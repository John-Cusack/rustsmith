# ADR 018: Merging same-path replacements must not `git rm` them

- Context: `merge_unit` deletes each unit's original source after merging
  the worker branch (`delete_on_merge`, defaulting to the orig rel). That
  models different-path ports (C++/CMake objects, `src/`-layout shims whose
  template lives elsewhere): the original path is untouched by the merge,
  so `git rm` removes it cleanly. Flat-layout Python ports
  (`markdown-it-py`) replace sources in place — the template file and the
  orig rel are the same path — so the merge stages the replacement and the
  subsequent `git rm` refuses it (`has changes staged in the index`),
  halting the run on the first unit.
- Decision: skip the `rm` for paths the merge already dirtied
  (`git diff --cached --quiet -- <path>` exits nonzero): the replacement
  content is already staged and lands in the merge commit via the following
  `git add -A`. Untouched originals are still removed exactly as before.
- Consequences: flat ports merge unit by unit; different-path ports see no
  behavior change (their deletes were never merge-dirty, otherwise the run
  halted). The skipped-`rm` path is observable in the merge commit (port
  content present, no deletion record) — reviewers check the file, not the
  operation.
- Alternatives: per-unit `delete_on_merge: []` data (60 stem keys per flat
  port, fragile against DAG changes); `git rm -f` (forces through real
  conflicts the refusal exists to catch) — rejected.
- Spec: no SPEC change; merge stays one commit per unit.
