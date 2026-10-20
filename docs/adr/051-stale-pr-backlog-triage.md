# ADR-051: Stale open-PR backlog triage (rustsmith + mirror repos)

Date: 2026-10-20
Status: accepted

## Context

The porting effort had five open pull requests that nobody had decided
on, across two repos:

- `John-Cusack/rustsmith` #73, #55, #54, #53 (all opened 2026-10-09 /
  2026-10-13, all from the `fm/rs-elmer-c5-*` and `fm/rs-publish-pygments`
  lanes),
- `John-Cusack/crc-rust` #1 (opened 2026-10-13).

A stale open PR is worse than a closed one: it hides unfinished work
behind a green "open" state and nobody can tell whether the change is
still wanted, already landed by another route, or simply wrong. The
triage rule for this record: every open PR gets a decision — merge it,
close it as superseded, or re-implement it — with the evidence that
decided it.

## Decision

Per PR, on 2026-10-20, against `rustsmith` main `480c5f2` and the
`crc-rust` main tree:

| Repo | PR | Decision | Evidence |
| --- | --- | --- | --- |
| `John-Cusack/rustsmith` | [#73](https://github.com/John-Cusack/rustsmith/pull/73) | merged (`480c5f2`) | Still required: the release harness evaluates each smoke vector as `import pygments; assert <expr>` (`crates/rustsmith-cli/src/release.rs`), and `pygments/__init__.py` exposes only `lex`/`format`/`highlight`/`__version__`, so main's `pygments.lexers` / `pygments.token` / `pygments.formatters` vectors raise `AttributeError` (reproduced locally). The PR's `__import__` vectors pass. |
| `John-Cusack/rustsmith` | [#55](https://github.com/John-Cusack/rustsmith/pull/55) | merged (`1f474ae`) | Still required: it implements the K=3 rule pinned by `specs/elmer-campaign/baseline.md:41`, `runbook.md:19` and spec clarification 6 (`gates::parity_with_known_failures` + `campaign_parity` + `backstop_exact`). Main carried none of it (`git grep parity_with_known_failures origin/main` → 0 hits before the merge), so the strict zero-failure parity gate could never pass an Elmer slice. |
| `John-Cusack/rustsmith` | [#54](https://github.com/John-Cusack/rustsmith/pull/54) | merged (`f0dfd3b`) | Still required: the spec edge case "the merge path needs the same treatment" (`-Wl,--allow-multiple-definition`, `specs/elmer-campaign/spec.md`), plus the CMake link-signature rule — main's `merge_unit` always wrote `target_link_libraries(<t> PRIVATE <archive>)`, which is a configure error on a tree that links the target plainly (Elmer: `TARGET_LINK_LIBRARIES(Matc_bin matc)`). |
| `John-Cusack/rustsmith` | [#53](https://github.com/John-Cusack/rustsmith/pull/53) | superseded — re-implemented here, old PR closed with a pointer | The behaviour is still required (spec US2 acceptance: a helper/header unit with no compilable claim records `skipped`, never a grade halt), but the branch no longer merges: `mergeable_state: dirty`, conflicting with the adapters T014 probe (PR #92) and the T017 shape-A change (PR #105). This PR re-implements the same behaviour on current main. |
| `John-Cusack/crc-rust` | [#1](https://github.com/John-Cusack/crc-rust/pull/1) | merged (`f3a2e6b`) | Still required: ADR-032 records that `Topic :: System :: Checksums` is not a trove classifier, and the embedded table (`crates/rustsmith-release/src/trove.rs`, `TROVE_LIST_VERSION 2026.9.21.13`) has 0 hits for `Checksums` and 1 for `Topic :: Software Development :: Libraries :: Python Modules`. `mirror/crc` already carried the valid tag; the published `crc-rust` tree did not. Metadata only — no code, no rebuild, no re-upload (`crc-rust` 0.1.0 stays as published). |

`John-Cusack/elmer-rust` #9 was left open on purpose: it was created
2026-10-20T21:51Z, minutes before this triage, from the live lane
branch `elmer/craft-tight-loop` (lane worktree `fm-rs-elmer-rustcraft`
carries uncommitted work). It is in-flight work, not a stale PR, and
closing it would destroy unlanded work.

Every other public mirror repo checked
(`elmer-rust`, `crc-rust`, `dateutil-rust`, `packaging-rust`,
`strsimpy-rust`, `markdown_it-rust`, `python_multipart-rust`,
`pyparsing-rust`, `pygments-rust`, `sympy-rust`, `docutils-rust`,
`charset_normalizer-rust`, `yaml-rust`) had no other open PR.

## Verification

Each merged PR head was checked before the merge, on the PR's own
branch and on the simulated combined tree (`git merge-tree --write-tree`
over main → #55 → #54 → #73, conflict-free):

- `cargo build --release --workspace` — exit 0
- `cargo clippy -- -D warnings` — exit 0
- `cargo test --workspace` — exit 0
- `tests/m4_acceptance.sh` (mirror scheduler/review/bundle, Python
  spine) — `M4 ACCEPTANCE GREEN`, so the scheduler change is a no-op on
  the spine that builds no pristine tree.

The re-implementation of #53 was proven on the CTest spine, where the
claim check applies. Same mini fixture, same scope, one variable — the
scheduled unit is the header `c:include/mini_helper.h`:

| tree | mirror exit | store row for the header |
| --- | --- | --- |
| main `480c5f2` (before) | 1 — `scaffold crate for unit c:include/mini_helper.h built no archive` | `gated` (the run halts) |
| this PR (after) | 0 — `mirror: 1/1 passed divergence=0.0000` | `skipped` (never graded, never merged) |

`tests/elmer_mini_proof.sh` fails identically on main `480c5f2` and on
this branch (`port crate build produced no archive at
.../rust/mini_add/target/debug/libmini_add.a`, a worker-side crate
build that never lands an archive) — a pre-existing failure of that
script, not a regression from this change. It was left untouched.


## Consequences

- The backlog is empty: no open PR in the porting effort is undecided.
  The only open PR (`elmer-rust` #9) is live lane work.
- A stale PR is closed with the exact superseding commit or PR, never
  silently, and a needed-but-conflicting change is re-landed as a fresh
  PR on current main instead of being force-pushed.
- The three rustsmith merges are tool-fix PRs (`crates/`); no port code
  moved into `rustsmith` main.
