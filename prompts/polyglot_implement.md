# Implement: polyglot composite adapter (parallel tracks A–I)

Goal: implement `docs/adr/008-polyglot-composite-adapter.md` (accepted) per
`docs/IMPLEMENTATION_POLYGLOT.md`, closing every hardcoded Python site without
breaking crc/strsimpy parity. Review context:
`docs/adr/007-polyglot-composite-adapter.review.md`.

## Read first (all agents)

1. `docs/adr/008-polyglot-composite-adapter.md` — signatures are normative. Follow them;
   record why in code comments only if the code forces a deviation.
2. `docs/IMPLEMENTATION_POLYGLOT.md` — your track's Target/Change/Acceptance + integration order.
3. Current code you own (see track). Do not open files hoping — read your owned ranges only.

## Shared contracts (verbatim, no invention)

- New shared types live in `rustsmith-core` ONLY: `TestCommand{Cwd{Tree,BuildDir,Rel},launcher,
  timeout,collect}`, `RunOutput`, `Outcome`, `ObservableSpec`, `Manifest v2` + v1 reader,
  `TestRunner` trait. No track defines its own command/result types.
- `UnitId` = `<lang>:<repo-rel authoritative source>[#<symbol>]`, Fortran lowercased.
- Constraints: zero repo names in `src/`; no `python3`/`pytest`/`maturin`/`PYTHONPATH` literals
  outside Python frontend/runner/bridge; every spawn sets `cwd`; MPI ranks are `Launcher` data;
  `alloc_peak: Option<u64>`; all traits `Send + Sync`; unknown extensions warn into `ProbeReport`,
  halt above threshold. Elmer is test data, never a code path.

## Tracks (one agent each; skip validation mid-flight, coordinate via hub on shared files)

- **A — core types.** Target `crates/rustsmith-core/src/lib.rs`. Add S1 types + `TestRunner`
  + v1 reader. Acceptance: `cargo check -p rustsmith-core` green; v1 fixture parses.
- **B — adapters + probe.** Target `crates/rustsmith-adapters/src/lib.rs`. S2/S3-adapter +
  `select_composite` + `partition`. Re-express Python. Acceptance: crc/strsimpy probe = 1
  frontend, recon/dag output byte-identical ignoring v2 fields.
- **C — oracle + sandbox.** Target `oracle:198-365`, `sandbox:17-300`. Route through runner;
  parameterized image; absolute binary pins; host fallback refused for non-Python; empty
  held-out halts. Acceptance: crc grade identical; `oracle:368`/`sandbox:225` parsers deleted.
- **D — recon writer + facts probe.** Target `cli/recon.rs`. `select_composite()`; delete
  baseline/`pyproject` parsers; write `facts.probe` + Manifest v2. Acceptance: manifest validates.
- **E — mirror + optimize.** Target `cli/mirror.rs:124-763`, `optimize.rs:130-703`. Route via
  bridge/runner/profiler; `wait4` measures. Acceptance: no `Command::new("python3")` outside
  runners/bridges (grep proves it); crc dry-run builds runner-made `TestCommand`.
- **F — UnitId rollout (after B+D).** Target dag.json writer/reader, recon `build` key,
  `fixture.rs:107-265`, `template.json` keys. Writer+readers atomically. Acceptance: compat
  shim reads old, writes validate new.
- **G — gates + profile types.** Target `gates:24-378`, `profile:49-200`. `outcomes` key-set
  integrity; optional per-observable tols; `alloc: Option`; facts-driven inputs, crc literals gone.
  Acceptance: gate math unchanged on fixtures; no `pyproject.toml` literal in gates.
- **H — content + fixture deletion (after D+F+G).** Target `porting.rs`, `heldout.rs`,
  `fixture.rs`, `main.rs:434,945,1257`, `harvest:111-116`, containers, architect prompt.
  `language_rules` per frontend; runner-owned held-outs; `grep -rn FixtureKind crates/ | wc -l` == 0.
- **I — second frontend + spine (after A–H green).** Fortran/C frontends, CTest runner, CMake
  bridge, perf profiler against pinned-tree transcripts. Acceptance: 3 frontends on pinned tree;
  `ctest -L quick` grades; one `BIND(C)` substitution spike rebuilds + passes.

## Integration order

A → (B, C, D, E, G parallel) → F → H → I. A author owns `core` merges. Parity gates after B
and C+E; full suite once at end. Phase boundary never yields: same turn to green.

## Rules

- Real edits only; no stubs, placeholders, `TODO: implement`, or re-exports of old paths.
  Migrate every caller; delete the obsolete code the cutover replaces.
- No claim without a pointer (`file:line` + symbol). Mark uncertainty `[INFERENCE]`.
- Batch todo flips with real work; never a todo-only turn.
