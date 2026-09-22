# Implementation: polyglot composite adapter (parallel tracks)

Parent: `docs/adr/008-polyglot-composite-adapter.md` (accepted). Review:
`docs/adr/007-polyglot-composite-adapter.review.md`.
Read-only research done; this doc is the build order for parallel agents.

## Shared context (all tracks)

- Contracts live in `rustsmith-core` first (S1): `TestCommand{Cwd{Tree,BuildDir,Rel},launcher,timeout,collect}`,
  `RunOutput`, `Outcome`, `ObservableSpec`, `Manifest v2{runner,languages,prepare,invocation,config_hash,observables}`
  + v1 reader, `TestRunner` trait. No track invents its own command/result types.
- `UnitId` = `<lang>:<repo-rel authoritative source>[#<symbol>]` (Fortran lowercased).
  Frozen `dag.json`/`recon.json` keys change once, atomically (track F).
- Constraints: no repo names in `src/` (Elmer is test data only); no `python3`/`pytest`/`maturin`/`PYTHONPATH`
  literals outside the Python frontend/runner/bridge; every spawn sets `cwd`; MPI ranks are
  `Launcher` data hashed in `config_hash`; `alloc_peak: Option<u64>`; all traits `Send + Sync`;
  unknown extensions warn into `ProbeReport`, halt above threshold. Skip formatters/linters/suites
  mid-flight; validate once at the end.

## Track A — core types + TestRunner (unblocks B–H, no dependencies)

Target: `crates/rustsmith-core/src/lib.rs` (Manifest v2, TestCommand, RunOutput, Outcome,
ObservableSpec, Observation, OracleFile, OracleKind, TestRunner trait, BuildCtx).
Change: add S1 types verbatim from ADR-008; keep v1 Manifest reader; `GradedResult.outcomes:
BTreeMap<String,Outcome>` with counts derived; `Baseline` unchanged this track.
Acceptance: `cargo check -p rustsmith-core` green; v1 manifest fixture still parses;
new types constructible from a unit test inside the track (no suite run needed).

## Track B — adapters: frontends + composite + probe (depends on A types only)

Target: `crates/rustsmith-adapters/src/lib.rs` (UnitId, Abi, Symbol, UnitDecl, Fragment,
FragmentCtx, Frontend, BuildBridge, Profiler, CompositeAdapter, select_composite, partition,
ProbeReport); re-express `PythonAdapter` as PythonFrontend+PytestRunner+MaturinBridge+PyProfiler.
Change: implement S2/S3-adapter signatures; probe = extension census + PROJECT() langs +
CTestTestfile presence; `.src`/generated excluded by origin (build dir + compile DB inputs);
`partition()` = union + symbol-index cross-edges + Kahn leaf-first + diagnostics.
Acceptance: probe on `mirror/crc` + `mirror/strsimpy` yields 1 frontend each, zero unclaimed
above threshold; composite emits byte-identical `recon.json`/`dag.json` to HEAD ignoring v2 fields.

## Track C — oracle + sandbox through runner (depends on A; parallel with B/D)

Target: `crates/rustsmith-oracle/src/lib.rs:198-365` (discover/detect/run/parse),
`crates/rustsmith-sandbox/src/lib.rs:17-300` (images, docker/host runs, parsers).
Change: delete `detect_invocation`, route freeze/grade through `runner.oracle_files/normalize_for_hash
(owns ADR-002)/invocation/grade/observe`; `RunOutput` carries artifacts; image parameterized
(`ImageSpec{base,packages,writable}`); absolute binary pins; host fallback refused for non-Python
runners; empty held-out halts (close `oracle:329-339` hole).
Acceptance: crc recon→grade output identical to HEAD (modulo v2 fields); pytest parsers at
`oracle:368`/`sandbox:225` deleted, single `grade()` owns parsing.

## Track D — recon manifest writer + RepoFacts probe half (depends on A+B)

Target: `crates/rustsmith-cli/src/recon.rs` (run_recon, frozen_manifest_with_benchmarks,
baseline_counts, runtime_deps, workload_contract*), new `recon/facts.json` probe section.
Change: `select_composite()` replaces `let adapter = PythonAdapter:17`; delete baseline/count
and `pyproject` parsers (delegate to runner/frontend); deterministic probe writes `facts.probe`
(workloads skeleton, differential probes, observables skeleton, api_surface, attribution);
manifest v2 writer (`prepare/invocation/config_hash/observables/runner/languages`).
Acceptance: crc/strsimpy `manifest.json` validates against v2 schema with v1-compat fields readable.

## Track E — mirror + optimize through spine (depends on A+B; parallel with C/D)

Target: `crates/rustsmith-cli/src/mirror.rs:124-416,488-763` and `optimize.rs:130-703`
(maturin/venv/pytest/PYTHONPATH paths, parsers, differential pairs, build_release).
Change: route configure/build/grade/held-out through `bridge.prepare/build/substitute/scaffold/
link_deps/artifacts` + `runner` + `profiler{setup,timed}`; core `wait4` rusage measures;
`differential` keeps exact-equality only behind per-observable tol (default path unchanged for Python).
Acceptance: crc mirror dry-run reaches grade call with `TestCommand{program: <venv python>,
args: [-m,pytest,…]}` built by the Python runner — no `Command::new("python3")` literals remain
outside runners/bridges (grep proves it).

## Track F — UnitId rollout, atomic (depends on B+D; serialize after them)

Target: `recon.rs:87-99` dag.json writer, `recon.rs:104` build key, `fixture.rs:107-265`
TemplateSpec/unit_sources/read_recon_modules, `mirror/<fixture>/template.json` keys,
`mirror.rs:488-493` dag reader. Change: rename frozen `build.language`→`build.languages`,
unit keys → UnitId strings, `template.json orig_source/delete_on_merge` → `scaffold()` semantics;
land writer+readers together. Acceptance: old fixtures read (compat shim) and new writes validate;
`fixture.rs` dispatch still present this track (deleted in H).

## Track G — gates + profile types (depends on A; parallel with B–F)

Target: `crates/rustsmith-gates/src/lib.rs:24-33,78-87,276,366-378` and
`crates/rustsmith-profile/src/lib.rs:49-200` (Workload{setup_py→setup,stmt_py→stmt},
harness_py, PYTHONHASHSEED, tracemalloc).
Change: `oracle_integrity` compares `outcomes` key sets; `differential` takes optional per-observable
tols (callers pass `0.0` to preserve current behavior); `alloc_*` → `Option`; profile keeps Python
behavior via PyProfiler, deletes `src/crc` probes/`crc._crc` literals to RepoFacts-driven inputs.
Acceptance: gates unit math unchanged on existing fixtures; no `pyproject.toml` literal remains in gates.

## Track H — content + fixture deletion + containers (depends on D+F+G)

Target: `cli/porting.rs`, `cli/heldout.rs`, `cli/fixture.rs`, `cli/main.rs:434,945,1257`,
`rustsmith-harvest:111-116`, containers Dockerfiles, `prompts/architect.md`.
Change: `Frontend::language_rules()->Vec<Rule>` (ABI/layout only), repo API rules → RepoFacts.porting_rules;
held-outs via `TestRunner::heldout`; delete `FixtureKind`/detect/resolve/template_dir; attribution via
RepoFacts; images parameterized (python:3.11 vs gcc:14+cmake+perf); architect prompt templated on
`{{languages}}`. Acceptance: `grep -rn FixtureKind crates/ | wc -l` == 0; crc/strsimpy flows read
`facts.json`, never `fixture.toml`, for behavior (toml remains as test data).

## Track I — second frontend + CMake/CTest spine (depends on A–H green)

Target: new `FortranFrontend` (claims F90/F/src, fparser2 USE parse after preprocess),
`CxxFrontend` (claims via compile DB), `CtestRunner`, `CmakeBridge`, `PerfProfiler`.
Change: implement against pinned-tree transcripts (test_macros/runtest at pin, `ctest -N` MPI diff,
one substitution spike for gfortran ABI limit); record loaded `.so` via LD_DEBUG→units through
File API; per-unit coverage marks out-of-scope/vendored units.
Acceptance: probe on pinned tree yields 3 frontends; `ctest -L quick` subset grades through the
runner; substitution spike on one `BIND(C)` unit rebuilds + passes its test.

## Integration order

A → (B, C, D, E, G in parallel on A types) → F (after B+D) → H (after D+F+G) → I.
Parity gates: after B (recon/dag identical), after C+E (grade identical on Python fixtures),
full suite once at end. Integration owner: track A author merges type changes; siblings coordinate
via hub before touching `crates/rustsmith-core/src/lib.rs`.
