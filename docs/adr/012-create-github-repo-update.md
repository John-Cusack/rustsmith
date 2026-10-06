# ADR 012: `create-github-repo --update` is fast-forward-only

- Context: `create-github-repo` refused every non-empty repo, so the second
  release of a converted project (and the crc-rust re-publish onto the
  existing `John-Cusack/crc-rust`) had no shipped push path — only a
  hand-rolled `git push`, which the release lane forbids.
- Decision: `--update` permits pushing onto an existing non-empty repo only
  when the repo visibility matches the requested one and the remote `main`
  head is already an ancestor of the fork's `main` (clone + apply the
  prepared tree on top). The push stays plain `git push`: no force flags
  exist anywhere, so the server re-enforces fast-forward.
- Consequences: publishing history is append-only; visibility mismatches,
  unknown remote heads, and non-descendant forks refuse before any side
  effect. The store recording is the existing `github_repos` upsert.
- Alternatives: force-push (rewrites public history — rejected); manual push
  outside the tool (skips verification + recording — rejected); a new repo
  per release (breaks the `release.toml` trusted-publisher identity).
- Spec: `docs/RELEASE.md` §7 documents `--update`; refusal-before-side-effect
  semantics unchanged for every other case.
