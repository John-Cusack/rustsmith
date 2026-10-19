# ADR-048: Symposium (`cargo agents`) evaluation — do not adopt

Date: 2026-10-19
Status: accepted

## Context

Symposium (https://github.com/symposium-dev/symposium, MIT OR Apache-2.0)
scans a Rust workspace, matches plugins against its dependency graph, and
installs crate-specific skills/hooks/MCP servers into AI coding agents via
`cargo agents init` / `cargo agents sync`. Evaluated for the rustsmith repo
on 2026-10-19 (symposium 0.5.1; 0.4.0 first since 0.5.1 needs a newer
toolchain — see below).

## What was tried

- `cargo install symposium` on the repo toolchain (rustc 1.89.0) fails:
  0.5.1 uses `if let` match guards (E0658). 0.4.0 installs; 0.5.1 installs
  only with `cargo +nightly install`. (`cargo-binstall` is not installable
  here either: it needs rustc 1.96+.)
- `cargo agents init --add-agent opencode --hook-scope project` writes only
  `~/.symposium/config.toml` (machine-level, nothing in the repo).
- `cargo agents sync` in the worktree, with the recommendations source
  provided as a local path clone (the default git registry never fetches
  here — every sync logs `failed to refresh registry
  registry=symposium-recommendations error=No such file or directory
  (os error 2)` although TCP connects to api.github.com:443 and `curl` to
  the same API returns 200; unfetched registry degrades to exit 0 with
  "no applicable skills found").

## What it installs for this workspace

Scan covers the workspace dependency graph; only two plugins match:

- `symposium` (`depends-on = ["*"]`, matches every Rust workspace) →
  `rust-best-practice` (12 lines: `cargo add`, fmt, clippy, test —
  overlaps AGENTS.md's Done-gates), `find-crate-source` (pointer to
  `cargo agents crate-info`).
- `error-battery-pack` (matches on `anyhow`/`thiserror`, both present) →
  `error-handling-basics`, `application-errors`, `library-errors`,
  fetched from the error-battery-pack crate itself (`source.cargo`;
  crates.io fetch works). Substantive and relevant, ~400 lines total.
- No plugin exists for the workspace's other characteristic deps
  (serde, rusqlite, criterion, insta, camino, ...); assert-struct, toasty,
  dial9, rtk correctly do not match.

Install location is `<repo>/.agents/skills/<skill>/SKILL.md`, but every
skill directory ships its own `.gitignore` containing `*`: sync output is
deliberately uncommittable local state. `git status` stays clean.

## Decision

Do not adopt. The model does not fit this repo:

1. Fresh worktrees never see the guidance. Port lanes and task workers run
   in disposable worktrees; symposium skills are gitignored per-machine
   state requiring `init` + network registry fetch per machine. The repo's
   agent-guidance channel is committed files (AGENTS.md, `prompts/`,
   `guidance/`), which propagate; symposium bypasses it.
2. Reproducibility is poor here: the default git registry fails to fetch
   (above), stable-toolchain users cannot build the current release, and
   0.4.0 cannot even parse the current recommendations manifests (0/6
   valid) — the floor and the ceiling disagree.
3. Freshness needs hooks: with a skills-only agent (opencode) there is no
   auto-sync, so installed skills go stale silently; hook-capable agents
   need global machine hooks (fleet-wide side effects: every hook runs
   `cargo agents sync`, auto-update defaults to on and reinstalls from the
   network) or committed per-agent settings files (new repo convention,
   churn). Neither is acceptable for zero committed benefit.
4. Value is thin for the cost: 2 generic skills overlapping AGENTS.md plus
   3 error-handling skills. No conflicts with oracle/gate rules (sync is a
   deterministic file copy + `cargo metadata`; no hooks installed; no LLM
   calls), just no gain.

## Consequences

- No repo files from symposium; this ADR is the only change.
- Machine state from the evaluation (`~/.symposium/config.toml` recording
  opencode/project-scope, `~/.cargo/bin/cargo-agents` 0.5.1, scratch clone
  under `/home/john/firstmate/data/rs-symposium-adopt/`) is reported, not
  committed, and changes no other checkout.
- Revisit if: the git registry fetches reliably, plugins cover serde /
  rusqlite / criterion-class deps, symposium ships a committable manifest
  mode so fresh worktrees see guidance without per-machine sync, or the
  current release builds on the repo toolchain.
