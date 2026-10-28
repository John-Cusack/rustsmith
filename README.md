# rustsmith

Trustworthy migration of existing software to verified, production-ready Rust — measured speedups, honest no-gain reports, artifacts you can install.

## Why this exists

Faster code makes the world wealthier: wealth creation and poverty alleviation are one and the same, and both run on doing more with less compute. LLM iteration speed is part of the same argument — the speed at which LLMs can iterate is becoming the speed at which the human race makes progress, and faster code means more iterations per dollar, per hour, per idea, compounding across every agent, everywhere. Rust makes code much faster.

That gives `rustsmith` three jobs, in order: (1) make existing popular Python libraries faster by porting them or their hot parts to Rust, and get the faster versions used; (2) record how each port was done and publish it as training data for LLMs that write Rust — publication stays human-review-gated; (3) review how we write Rust and distill general patterns for writing fast Rust.

How fast an LLM can iterate depends heavily on how fast the code it writes can run: every test run, every benchmark, every agentic loop burns wall-clock time, and tokens-per-second is increasingly the binding constraint. That is what `rustsmith` is for: converting existing codebases in any language — starting with Python, where most LLM-targeted code still lives — into fast, correct Rust, so the agentic loop itself runs faster.

Rust is also the language LLMs should prefer going forward. Implemented correctly, it has fewer bugs and fewer safety issues than the languages it replaces: the borrow checker, `Send`/`Sync`, exhaustive matching, and `Result`/`Option` catch at compile time what becomes a hallucinated `None` check or a data race elsewhere. Fewer bug classes means fewer wasted iterations chasing memory corruption, data races, and undefined behavior — exactly the failures that cost agents the most time.

The obstacle is that rewriting legacy code in Rust is slow, manual, and risky, and the fast paths are untrustworthy — an LLM will port the code, quietly weaken the tests to match its own bugs, overfit the benchmarks it can see, and report a speedup that evaporates on real input.

`rustsmith` exists to make the fast path trustworthy. It splits the system in two: agents do the porting and optimizing, and a deterministic gate system written in plain Rust with no LLM involvement decides what passes. No agent can override a gate, and the agent that changes an implementation can never influence how that change is graded.

## Product vision

`rustsmith` turns existing software into verified, production-ready Rust implementations that reduce execution time and compute cost. Gains matter twice: during development, when agents repeatedly run tests and benchmarks, and after deployment, when production systems run the resulting code.

For a Python source project, the preferred destination is a **reusable Rust core crate with Python bindings that preserve the supported Python interface**. The Rust crate should also be usable directly by Rust projects where practical. A project may migrate component by component, with a complete Rust implementation as a possible eventual outcome. The choice of boundary follows the software's architecture and measured workloads — never a fixed whole-repo assumption.

`rustsmith`'s value is the migration process, not the language: discover behavior and performance characteristics, choose useful boundaries, generate implementations and bindings, verify compatibility independently of whoever made the change, benchmark representative workloads, and produce artifacts developers can install and maintain. Rust is a means to improve measured outcomes; a rewrite alone is never assumed to be faster.

Success means supported behavior is preserved, representative workloads improve in time or cost, and the result is deployable and maintainable. Regressions and cases where Rust brings no gain are reported honestly — a `no_gain` verdict is a valid output, not a failure to hide.

## Intended outputs

For each source project, a run should produce:

1. A **reusable Rust core crate** (`rlib`) with a public API covering the migrated components.
2. **Python bindings** (PyO3/maturin) preserving the supported Python interface, so existing callers keep working.
3. **Native packaging**: versioned wheels (and sdist) installable via `pip`; the Rust crate published for direct Rust use where practical.
4. A **verification + benchmark record**: independent compatibility checks, representative-workload measurements before/after, and honest regression/no-gain notes.
5. A **suggestions report** of changes worth contributing back upstream (patches where the win is language-independent, native accelerators with pure fallbacks where it is module-local).

## Current state (what runs today)

Implemented and exercised end to end on the two pinned fixtures (`crc`, `strsimpy`):

- Deterministic recon: behavior freeze, held-out suite generation, `PORTING.md` rulebook, leaf-first unit DAG, frozen `WORKLOAD.md` contract.
- Mirror loop with real grading: per-unit builds in isolated venvs, oracle + held-out + differential gates, merge on pass, whole-repo grade on the fork before finishing.
- Optimize loop with real measurement: interleaved parent/candidate sampling, 9-gate suite per candidate, wall-clock confirmation of merged rounds.
- Harvest/report: per-row classification, `RUSTSMITH_REPORT.md`/JSON/HTML from one struct, backport patches for language-independent wins, `negative_results` for losers.

Limited prototypes (real code, narrow scope):

- Only two packages have repo data (`crates/rustsmith-cli/data/repo-content.json`); unknown packages halt by design. Adding a target means authoring its data entry (probes, workloads, rules, templates).
- The mirror worker task copies reference-port files from `mirror/<package>/` templates — no model writes code on the default path, and the four council seats approve via stub drivers. Live models are opt-in (`RUSTSMITH_WORKER_CMD`, `RUSTSMITH_SEAT_CMD_*`); tokens are recorded as usage only.
- The `crc` and `strsimpy` templates ship the milestone-1 layout: reusable core crate (`crc-core` / `strsimpy-core`, no Python dependency) plus a thin PyO3 binding that delegates to it; other targets still ship a single extension crate. There is no boundary selection yet (always whole-DAG mirror), no versioned wheel/sdist output, and `suggestions/accelerators/` carries a note, not code.
- Accepted optimizations publish to a canonical revision (`work/opt/deliver/` + `ACCEPTED.json` with merged techniques and per-file hashes; rejected work never enters); the report carries a Delivered-artifact section. The headline end-to-end speedup remains a compounded estimate unless a milestone remeasures it directly.
- No LLM runs anywhere on the default path: the pipeline is fully deterministic and offline unless the `RUSTSMITH_*_CMD` env wiring is set.

Planned: per-target data packs (charset-normalizer and the rest of the POC board), live worker/council wiring, boundary selection, native packaging (crc + strsimpy core/binding splits done; versioned wheels/sdist pending; `pip` binary shim decided; `cargo publish` metadata landed, crates unpublished), shipped accelerators, Cachegrind/`perf` instruments, concurrent runs, flame graphs. Out of scope as before: upstream PR automation, distributed execution, web UI, model hosting.

## What it does

Given a list of source repositories, without human intervention during the run, `rustsmith` produces for each one:

1. A fork repo with the ported tree plus `RUSTSMITH_REPORT.md`/JSON/HTML and a `suggestions/` directory (backport patches, accelerator notes, ranked follow-ups).
2. Measured evidence: frozen oracle results, held-out divergence, per-candidate gate verdicts, and workload before/after numbers — including `no_gain` where Rust didn't help.

Workers do the porting (template-driven on the default offline path; live models opt in via env). A 4-seat council (Architect, Verifier, Performance, Scope) plans and reviews; deterministic gates decide what passes.

## How it works

**The oracle is the definition of correctness.** At run start, `rustsmith-oracle` freezes the original test suite, its config/fixtures, and the exact invocation into `oracle/manifest.json` (SHA-256 per file, stored on the host). Workers get a read-only mount for advisory self-testing. The run that counts is the **graded run**: the control plane verifies manifest hashes (any mismatch = tamper event) and runs the frozen oracle plus a host-only held-out suite no agent ever sees — in isolated venvs on the host for mirror/optimize units (the ephemeral no-network container path serves the legacy recon skeleton). Pass bar for mirror: 100% oracle parity, test count/skip list identical to baseline, `visible − heldout` divergence under threshold (default 5pp).

**Gates are pure Rust, no LLM, no network.** `rustsmith-gates` enforces `oracle_integrity`, `oracle_parity`, `heldout_divergence`, `differential`, plus optimizer gates (`workload_divergence`, `causal_attribution`, `benchmark`, widened `no_regression`, `optimization_scope`) and `provenance` on report writes. `unsafe_budget`, `miri`, and `clippy` exist as pure gate functions but are not wired into run grading yet. Gate failure fails the unit and feeds detail back to the worker — never fatal on its own. `gates` and `oracle` must not depend on `agent`/`council` (CI dep-check).

**Stages:**

- **Stage 0 — Recon.** Deterministic analysis (language/build detection, module + call graph, test baseline, dependency classify `port|bind|keep`, license record, profiling, held-out suite generation), then the Architect writes `PORTING.md` (few-hundred concrete type/error/naming/ownership rules) and a leaf-first unit DAG. Also freezes `WORKLOAD.md` (the performance contract: one primary metric, input distribution, resource budgets) and the benchmark oracle. Council reviews before proceeding.
- **Stage 1 — Mirror.** Behavior-identical Rust, same module boundaries. No redesign. Workers take units in dependency order (parallel up to `max_parallel_workers`), graded on completion, merged on pass. Adversarial review: implementer never reviews its own diff; two reviewers on different providers see only the diff. Default worker task materializes reference-port files from `mirror/<package>/` templates (no model writes code); model-authored ports are the production swap, wired via `RUSTSMITH_WORKER_CMD`.
- **Stage 2 — Optimize.** Rounds, not an open loop, so it terminates. Round 0 (Architect-owned, serialized) fixes cross-cutting representation debt (`HashMap<String,V>` → structs, per-call `String`/`Vec` → borrows, `Rc<RefCell>` → arenas, AoS → SoA, `Box<dyn>` → enums). Then per round: deterministic profile (process-CPU median per ADR-003 — Cachegrind/`perf` unavailable in this environment; wall-clock confirmation of merged rounds) → bound classification (compute, bandwidth, latency, branch, frontend, allocation, syscall_io, contention, work_volume) → ceiling arithmetic `share × (1 − 1/cap)` → dispatch only above `min_candidate_ceiling_pct`, ranked by `ceiling/cost` → proposal-before-code review → parallel implement → full gating (incl. workload-divergence and revert-attribution) → merge winners. Candidates come from deterministic source transforms (every number measured and gated; dispatched LLM workers are the production swap, not the default). Losers go to `failed_opt…
- **Stage 3 — Harvest.** Every accepted optimization is classified: `language_independent` (patch in the original language + proof), `module_local` (optional native accelerator with pure fallback, e.g. PyO3 for Python), `port_only` (needs the full port). Emits `RUSTSMITH_REPORT.md` + HTML + JSON (must agree), `suggestions/{patches/,accelerators/}`. Accelerator files are currently a stub: a README note ships when no module-local template exists; backport patches are real for language-independent wins.

Every run leaves an audit trail: SQLite `store.db` (`runs, units, gate_results, decisions, optimizations, failed_optimizations, rounds, guidance_revisions`) plus append-only `events.jsonl` sufficient to reconstruct the run.

## Quickstart

```sh
cargo build --release
./target/release/rustsmith run --repo <url[#pin]|path> --fork <dir> --work <dir> [--stage full|recon|mirror|optimize|harvest] [--config config/default.toml]
./target/release/rustsmith status --run-id <id>
./target/release/rustsmith report --run-id <id>
./target/release/rustsmith audit --run-id <id>
```

Batch, kill switch, and resume (non-tamper halts only):

```sh
./target/release/rustsmith run-batch --repo <a[,b...]> --work <dir>
./target/release/rustsmith halt --run-id <id>
./target/release/rustsmith resume --run-id <id>
```

Cross-run retrospective (proposes a `guidance/optimize.md` diff; human approves):

```sh
./target/release/rustsmith learn
```

One container per run (repo + toolchains, cgroup-limited, one git worktree per worker); grading in a separate ephemeral no-network container. The control plane never runs agent-authored code on the host. `store.db` and held-out suites live on the host and are never mounted into run containers.

## Releasing

Two different things, do not confuse them:

- **Releasing rustsmith itself:** this repo ships as a Cargo workspace binary
  (`cargo build --release`). It is not published to crates.io or PyPI.
- **Releasing a converted project** (implemented): each project rustsmith
  generates is a reusable Rust core crate plus a Python package calling that
  same core, distributable through crates.io and PyPI respectively. Given a
  finished run, `release-prep` builds and verifies everything locally (Rust
  package, wheels, sdist, installed-package smoke, sdist rebuild including
  its Rust core), retains artifacts/hashes/verification in the output
  directory, generates a GitHub Actions release workflow (TestPyPI first,
  then manual PyPI + crates.io, same tested files throughout) and exact
  trusted-publisher setup instructions. Publication is tracked per registry;
  retries are per-registry and a partial publish never reports complete.

```sh
./target/release/rustsmith release-prep --project mirror/crc --fork <fork> [--opt <opt>] --recon-out <recon> --out <dist>
./target/release/rustsmith release-status --state <dist>/release-state.json
```

Per-project config lives in `mirror/<package>/release.toml` (crate/dist/
import names, version, platforms, Python versions, publishing repo +
workflow identity, license, upstream attribution). Full procedure, state
semantics, and troubleshooting: `docs/RELEASE.md`.

Status: `crc` is the first end-to-end release example (publishable
core+binding split, `crc-rust-core` + `crc-rust`); `strsimpy` follows the
same split (`strsimpy-rust-core` + `strsimpy-rust`, `release-prep`
verified). Planned, not implemented: migrating the remaining templates,
version-bump automation, signed provenance attestations.

## Configuration

`config/default.toml` is the reference; `--config` overrides fields (unknown keys ignored, missing file is an error). Key knobs:

| Section | Knobs |
|---|---|
| `[run]` | `max_parallel_runs`, `max_parallel_workers`, `unit_max_attempts`, token ceilings |
| `[oracle]` | `heldout_divergence_threshold`, `require_zero_skipped` |
| `[gates]` | `unsafe_budget_pct`, `unsafe_boundary_only`, `benchmark_noise_floor_pct` |
| `[optimize]` | `hotspots_per_round`, `iterations_per_hotspot`, `round_gain_threshold_pct`, `max_rounds`, Round 0 mode, `stage2_token_ceiling` |
| `[optimize.heldout]` | workload divergence threshold/halt, max failures per run |
| `[optimize.regression]` | RSS / alloc / binary-size / compile-time tolerances |
| `[optimize.final]` | PGO / BOLT / allocator-swap gating (never unconditional) |
| `[measure]` / `[workload]` | primary instrument, wall-clock confirmation, workload contract |
| `[models]` | swappable provider/model identities per seat — never hardcoded elsewhere; reviewers on distinct providers is structural. A `--config` file carrying `[models]` (e.g. `config/live.toml`) also selects seat identities for that run; live seats run through `scripts/rustsmith-seat.sh` (Claude seats via Claude Code, Muse/Codex via omp), printing bare `{stance, reasoning}` JSON |
| `[privacy]` | `allow_training_tier_on_private_repos = false` enforced in code |

## Repository layout

```
crates/
  rustsmith-cli/       binary: arg parsing, run lifecycle
  rustsmith-core/      domain types, traits, state machine
  rustsmith-store/     SQLite + JSONL event log
  rustsmith-agent/     OMP subprocess driver, prompt assembly
  rustsmith-council/   four seats, adversarial review protocol
  rustsmith-gates/     deterministic gates (NO LLM CALLS EVER)
  rustsmith-oracle/    freeze, hash, graded runs, held-out suite
  rustsmith-sandbox/   container + worktree + cgroup management
  rustsmith-adapters/  per-source-language plugins (Python implemented first; trait admits further languages)
  rustsmith-profile/   profiling, benchmarks, hotspot ranking
  rustsmith-harvest/   Stage 3 classification
  rustsmith-report/    Markdown + HTML + JSON output
prompts/               versioned prompt templates, one file per role
guidance/optimize.md   worker guidance (versioned, pinned per attempt, evolved only via learn)
containers/            run + grading Dockerfiles, wrapper.sh
config/                default.toml, fixture.toml
tests/                 m0–m9 acceptance scripts
docs/adr/              every spec deviation in 5 lines
SPEC.md / SPEC_STAGE2.md / IMPLEMENTATION*.md / BUILD*.md
```

First adapter: Python. The adapter trait (`rustsmith-adapters`: `BuildInfo` / `CallGraph` / `TestInventory` / `Attribution` / `UnitDag`) is language-agnostic by design — new languages arrive as plugins, not rewrites. Also out of scope: auto-opening upstream PRs (never pushes to a remote the human doesn't own), distributed execution, web UI beyond the static report, training/model hosting, guaranteeing the Rust output is faster (speed is measured and reported, not assumed).

## Languages

| Language | Status | Scope |
|---|---|---|
| Python | Prototype (crc/strsimpy fixtures) | Template-provided PyO3/maturin extension; worker task copies reference-port files, no model porting on default path |
| C++ | Planned (pinned repo) | CMake/CTest + GTest/Catch2; payoff is safety/maintainability, not speed |
| Fortran | Planned (pinned repo) | CMake/CTest + pFUnit; F90+ first, F77 `COMMON`-block code later |
| TypeScript / JavaScript (Node.js) | Candidate | Backend/CLI/tooling only — browser-DOM rendering stays in JS (Rust would need a separate WASM path, not a rustsmith mirror) |
| Go, Ruby | Candidate | Slow-or-loose-typed backend code with large corpora; cheap adapters once the runner/bridge/profiler seam lands |

Rule of thumb: slow, easy-to-type backend code (Python, Ruby, Node) is the best mirror target — large speedup + type-safety win. C++/Fortran already run fast; port those for memory safety and maintainability. Browser-frontend JS is out of scope.

## POC board

Open port targets from the Python→Rust triage (2026-09-21). Method: PyPI dl/mo (pypistats, mirrors included) × CPU headroom × text-gap; dependents via ecosyste.ms (libraries.io blocked); nearest-crate search on crates.io/lib.rs. KEEP = CPU-bound with no mature full-feature Rust equivalent.

Status is one of: `open` | `in progress (<branch>)` | `mirrored (oracle N/N, heldout N/N)` | `released (<registry>, <version>)`. Every port PR updates its row with numbers from its graded run; a count that was not measured stays unrecorded.

### KEEP (ranked by dl/mo)

| # | Target | Status | dl/mo | Dependents | Hot path | Nearest Rust crate + gap | Speedup thesis |
|---|---|---|---|---|---|---|---|
| 1 | packaging | mirrored (oracle 62483/62483, heldout 13/13) | 1.56B | 4,628 | `version.py::parse`, `markers.py::evaluate`, `tags.py::sys_tags` | [pep440_rs](https://crates.io/crates/pep440_rs) (versions only) / [pep508_rs](https://crates.io/crates/pep508_rs) (markers only) / [uv-pep440](https://crates.io/crates/uv-pep440) (unstable internal) | Resolver evaluates versions/markers thousands× per solve; no single stable full crate |
| 2 | charset-normalizer | mirrored (oracle 297/297, heldout 20/20) | 1.15B | 2,462 | `api.py::from_bytes`, `md.py::mess_ratio`, `cd.py::coherence_ratio` | [chardetng](https://crates.io/crates/chardetng) (legacy-web only) / [charset-normalizer-rs](https://crates.io/crates/charset-normalizer-rs) (subset) / [encoding_rs](https://crates.io/crates/encoding_rs) (no detection) | Brute-force decode-all + per-char scoring is pure CPU; strongest thesis |
| 3 | python-dateutil | mirrored (oracle 2032/2032, heldout 41/41) + parser/isoparser port: oracle 2031 passed / 0 failed (incl. parser+iso suites, matches pure-Python baseline exactly), frozen-corpora parity 0 divergences, bench re-measure 2026-10-14: 10.5x fuzzy-noisy / 6.9x strict-parse / 2.1x iso (`mirror/python-dateutil/bench_parser/`), rrule 5.4–8.8x (`bench_rrule`) | 874M | 6,070 | `_parser.py::get_token`, `rrule.py::_iter`, `tz.py::_isdst` | [chrono](https://crates.io/crates/chrono) (strict only) / [dateparser](https://crates.io/crates/dateparser) (no rrule) / [jiff](https://crates.io/crates/jiff) (no recurrence) | Full parse path ported (ADR-015 supersedes the engine-only scope): fuzzy shares one `_parse` path with strict, so `parse()` users gain too |
| 4 | pyyaml | mirrored (oracle 2608/2608, heldout 36/36 + 9 generated pins, graded 2026-10-09); prod PyPI parked 2026-10-20 (account new-project quota) | 867M | 8,566 | `scanner.py`, `parser.py`, `emitter.py` (pure-Python path) | [serde_yaml](https://crates.io/crates/serde_yaml) (deprecated) / [serde_yaml_ng](https://crates.io/crates/serde_yaml_ng) (C wrapper) / [unsafe-libyaml](https://crates.io/crates/unsafe-libyaml) (bindings) | No mature pure-Rust YAML 1.1 crate |
| 5 | markdown-it-py | mirrored (oracle 1031 passed / 0 failed / 1 env-skip of 1032, heldout 16/18 with 2 env-skips; graded 2026-10-07) | 427M | 508 | `parser_block.py`, `parser_inline.py`, `emphasis.py` | [pulldown-cmark](https://crates.io/crates/pulldown-cmark) (no plugin API) / [comrak](https://crates.io/crates/comrak) (GFM renderer) / [markdown-it](https://crates.io/crates/markdown-it) (stale since 2024-07) | Only direct Rust port is stale; plugin token-stream gap |
| 6 | python-multipart | released (PyPI 0.0.32, crates.io 0.0.32); oracle 162/162, heldout 55/55, divergence 0.0000 (run mp-port-graded2) | 313M | 703 | `multipart.py::_internal_write` (both parsers), `parse_options_header` | [multer](https://crates.io/crates/multer) (async-only) / [multipart](https://crates.io/crates/multipart) (unmaintained since 2021) / [form-data](https://crates.io/crates/form-data) (async, tiny) | Untrusted-input byte state machine with CPU-exhaustion CVE history |
| 7 | pyparsing | released (PyPI 3.3.3, crates.io 3.3.3); oracle 2104/2104, heldout 41/41 + 9 generated pins, divergence 0.0000 (run pp-mirror2; graded 2026-10-08) | 298M | 1,663 | `core.py::parse_string`/`_parseNoCache`, `results.py` | [nom](https://crates.io/crates/nom) / [winnow](https://crates.io/crates/winnow) (code-first) / [pest](https://crates.io/crates/pest) (codegen) | Gap is specifically a runtime-constructible grammar API |

### In flight and landed outside KEEP

| Target | Language | Status | Notes |
|---|---|---|---|
| crc (`mirror/crc`) | Python | mirrored (oracle 80/80, heldout 11/11, divergence 0.0000; graded 2026-10-06); release work in progress (`John-Cusack/rewrite-packaging`) | Primary fixture; first core+binding release example |
| strsimpy (`mirror/strsimpy`) | Python | mirrored (oracle 18/18, heldout 16/16, divergence 0.0000; graded 2026-10-06); released (TestPyPI, 0.2.1) | Core+binding split (`strsimpy-rust-core` + `strsimpy-rust`); `release-prep` verified (11/11 checks); [mirror](https://github.com/John-Cusack/strsimpy-rust) · [strsimpy-rust (TestPyPI)](https://test.pypi.org/project/strsimpy-rust/0.2.1/) |
| Elmer FEM (`mirror/Elmer`) | C / C++ / Fortran | Campaign green on merged slices (Elmer @ `9f6af2f85`, K=3 upstream segfaults #80/#536/#537): 49 whole-file ports backstopped 479/482 — matc 22+4 helpers (test 156.52 s), elmergrid 8 (163.60 s after elmer-rust#14 `unite_merge` fix), fhutiter `huti_aux` (164.50 s), fem 18 (163.49 s); pristine anchor 151.74 s. Scoped-grading equivalence T007 22/22 + T011 10/10 AGREE (rustsmith#117, #122, #123). T020 tool PR: gate truth fixes regrade 1157 units 684→685 admit (sole flip `solve_cmplx` refuse→admit; zero admit→refuse), `procedure-not-found` instances 1217→1 (documented string-literal residual); +1 graded port `solvecmplx` (5-case corpus byte-identical, strong-symbol gate). Port PR (elmer-rust `elmer/t020-solver-entry`): Poisson.F90 Rust cdylib, `nm -D` equality, 2 `.sif` differentials + rerun byte-identical, serial-config backstop 451/454 both arms (campaign MPI config unbuildable here — `mpirun` hangs) | Ports live in [elmer-rust](https://github.com/John-Cusack/elmer-rust) (this repo carries evidence only); also merged: ice 5/184 (elmer-rust#8), gui-c 21 graded-build (differential probes halted, link-closure-only), gui-a/b 0. Refused shapes: `COMMON`-block Fortran, C++-only meshgen2d surface (0/60), proc-pointer/abstract-interface/BLAS fhutiter 9, descriptor array dummies. Measured per-unit oracle walls: matc scoped ~200–240 s (≈270 s leaf w/ overhead), elmergrid scoped ~360–385 s (near-full sets), fhutiter fallback-full ~444–471 s; fem core ~480 s/unit is the plan budget input (no fem timing sample measured, rustsmith#109). Remains: T016 (fem lane B, 176 units), T012 (equivalence tables for the fem/ice/gui slices) |
| sympy (polys kernel) | Python | headroom-graded 2026-10-17 (polys-domains 0.85 rewrite / simplify-core 0.49 upstream_only); kernel slice v0.1.0: equivalence 9/9 exact, core 14/14, upstream kernel subset 105 passed | Port lives at [sympy-rust](https://github.com/John-Cusack/sympy-rust) (not a mirror/ template): `dup/dmp` over QQ + PRS gcd; bench 5.06x dup_mul_800 / 8.75x dmp_mul_3var / 0.17x dup_gcd_70 slower row honest; released 0.1.0: [sympy-poly-rust (PyPI)](https://pypi.org/project/sympy-poly-rust/0.1.0/) · [sympy-poly-core (crates.io)](https://crates.io/crates/sympy-poly-core) (TestPyPI carries the same 0.1.0 files) |
| marshmallow | Python | headroom-graded 2026-10-28 (nested-batch-500x4 0.81 rewrite, pure 95%, pipeline); port v0.1.0: full upstream suite 1188/1188 green via import alias (mypy fixtures excluded, fail identically on upstream), seeded differential fuzz (200 randomized schemas + hostile data), port tests 8/8, clippy clean | Port lives at [marshmallow-rust](https://github.com/John-Cusack/marshmallow-rust): Schema dump/load kernel, whole-batch crossing, zero-call fast paths for native values, opaque per-value nodes for custom fields (ADR-053); bench 4.53x nested-batch-500x4 (dump+load many x4, 500 nested objects; `benches/two_track.py`, measured 2026-10-28); release-prep done (sdist+wheel build, fresh-venv smoke 4/4, versions 0.1.0 everywhere), uploads human-gated per AGENTS.md Limits |

## Ported packages

Every merged port below passed its graded run at 100% oracle parity (frozen upstream suite, test count/skip list identical to baseline), modulo explicitly disclosed environment skips where a row says so, plus a host-only held-out suite no agent ever sees. Speedup cells cite measured workloads only; anything unmeasured reads "not benchmarked" — never a guess. Excluded here: Elmer (method work, not a package).

Standing rule (2026-10-11): a row counts as `ported` only with both (a) an install option (PyPI/TestPyPI link, crates.io where applicable — every link below verified live) and (b) measured benchmark figures on the board. Grade-green rows missing either sit under `parity-graded — install/bench pending`: never deleted, never inflated.

### Ported

| Package | Upstream | Accuracy proof | Measured speedup | Install |
|---|---|---|---|---|
| python-dateutil | [dateutil/dateutil](https://github.com/dateutil/dateutil) | oracle 2032/2032, heldout 41/41 | 10.5x fuzzy-noisy / 6.9x strict-parse / 2.1x iso (`bench_parser/`, re-measured 2026-10-14); 5.4–8.8x rrule queries (`bench_rrule`) | [python-dateutil-rust (PyPI)](https://pypi.org/project/python-dateutil-rust/) · [dateutil-core (crates.io)](https://crates.io/crates/dateutil-core) · [mirror](https://github.com/John-Cusack/dateutil-rust) |
| pyyaml | [yaml/pyyaml](https://github.com/yaml/pyyaml) | oracle 2608/2608, heldout 36/36 + 9 pins (graded 2026-10-09) | 3.04x scan / 3.05x safe_load / 1.92x dump / 4.29x bulk (`bench/`, measured 2026-10-14) | [pyyaml-rust (TestPyPI)](https://test.pypi.org/project/pyyaml-rust/6.0.2/) · [yaml-rust-core (crates.io)](https://crates.io/crates/yaml-rust-core) · [mirror](https://github.com/John-Cusack/yaml-rust) (prod PyPI parked 2026-10-20: account new-project quota) |
| crc | [Nicoretti/crc](https://github.com/Nicoretti/crc) | oracle 80/80, heldout 11/11, divergence 0.0000 (graded 2026-10-06) | 2045x crc32-64KB / 1737x crc16-frames / 1845x crc64-1MB (`bench/`, measured 2026-10-14) | [crc-rust (PyPI)](https://pypi.org/project/crc-rust/) · [crc-rust-core (crates.io)](https://crates.io/crates/crc-rust-core) · [mirror](https://github.com/John-Cusack/crc-rust) |
| strsimpy | [luozhouyang/python-string-similarity](https://github.com/luozhouyang/python-string-similarity) | oracle 18/18, heldout 16/16, divergence 0.0000 (graded 2026-10-06) | 65x levenshtein / 9.2x cosine-2 / 471x metric-LCS (`bench/`, measured 2026-10-14) | [strsimpy-rust (TestPyPI)](https://test.pypi.org/project/strsimpy-rust/0.2.1/) · [mirror](https://github.com/John-Cusack/strsimpy-rust) |
| markdown-it-py | [executablebooks/markdown-it-py](https://github.com/executablebooks/markdown-it-py) | oracle 1031 passed / 0 failed / 1 env-skip of 1032; heldout 16/18 (2 env-skips) (graded 2026-10-07) | 2.36x render-16KB / 0.20x render-280KB bulk, slower (`bench/`, measured 2026-10-14) | [markdown-it-py-rust (TestPyPI)](https://test.pypi.org/project/markdown-it-py-rust/4.2.0/) · [mirror](https://github.com/John-Cusack/markdown_it-rust) |
| pyparsing | [pyparsing/pyparsing](https://github.com/pyparsing/pyparsing) | oracle 2104/2104, heldout 41/41 + 9 pins, divergence 0.0000 (run pp-mirror2; graded 2026-10-08) | 201x scan ceiling / 1.00x grammars no-change (`bench/`, measured 2026-10-14) | [pyparsing-rust (PyPI)](https://pypi.org/project/pyparsing-rust/3.3.3/) · [pyparsing-rust-core (crates.io)](https://crates.io/crates/pyparsing-rust-core) · [mirror](https://github.com/John-Cusack/pyparsing-rust) |
| python-multipart | [Kludex/python-multipart](https://github.com/Kludex/python-multipart) | oracle 162/162, heldout 55/55, divergence 0.0000 (run mp-port-graded2) | 1.86x parse-1.7MB / 2.72x headers / 1.76x bulk-17.7MB (`bench/`, measured 2026-10-14) | [python-multipart-rust (PyPI)](https://pypi.org/project/python-multipart-rust/0.0.32/) · [multipart-rust-core (crates.io)](https://crates.io/crates/multipart-rust-core) · [mirror](https://github.com/John-Cusack/python_multipart-rust) |
| pygments | [pygments/pygments](https://github.com/pygments/pygments) | oracle 5175 passed / 0 failed / 16 skipped of 5191 (upstream 2.19.1, graded 2026-10-13, Rust + fallback paths identical); port tests 11/11, core 12/12; heldout n/a (direct-PR lane, no control-plane held-out suite) | 9.32x regex_opt / 1.03x highlight no-change / 1.00x lex-only (`bench/`, measured 2026-10-14) | [pygments-rust (TestPyPI)](https://test.pypi.org/project/pygments-rust/2.19.1/) · [pygments-rust-core (crates.io)](https://crates.io/crates/pygments-rust-core) · [mirror](https://github.com/John-Cusack/pygments-rust) (prod PyPI parked 2026-10-20: account new-project quota) |
| docutils | [python/docutils](https://github.com/python/docutils) | equiv corpus 7/7 (doctree text, message levels, HTML tag profile exact vs 0.23, graded 2026-10-17); port tests 17/17, clippy clean; heldout n/a (direct-PR lane, no control-plane held-out suite) | 67x bulk-256KB / 17-52x small docs / ~13x peak RSS (`benches/two_track.py`, measured 2026-10-17) | [docutils-rust (PyPI)](https://pypi.org/project/docutils-rust/0.1.0/) · [docutils-core (crates.io)](https://crates.io/crates/docutils-core) · [mirror](https://github.com/John-Cusack/docutils-rust) |

### Parity-graded — install/bench pending

| Package | Upstream | Accuracy proof | Measured speedup | Install |
|---|---|---|---|---|
| packaging | [pypa/packaging](https://github.com/pypa/packaging) | oracle 62483/62483, heldout 13/13 | 4.07x version.parse / 1.41x marker.eval / 2.21x resolver-filter (`bench/`, measured 2026-10-14) | — |
| tabulate | [astanin/python-tabulate](https://github.com/astanin/python-tabulate) | upstream suite 318 passed / 1 intentional skip + equiv corpus 400/400 kernel-vs-pure (graded 2026-10-28) | 10.1x grid / 11.3x github / 11.9x rst / 11.6x plain 400-row (`benches/two_track.py`, measured 2026-10-28) | pending (TestPyPI/PyPI/crates.io blocked on credentials; [mirror](https://github.com/John-Cusack/tabulate-rust)) |
| pycparser | [eliben/pycparser](https://github.com/eliben/pycparser) | upstream suite 451/451 + 500 subtests (3.12, graded 2026-10-28, Rust + fallback paths identical); port tests 14/14, core 13/13, clippy clean; heldout n/a (direct-PR lane, no control-plane held-out suite) | 2.4–8.0x parse / 1.9–4.6x roundtrip, 3 runs (`benches/two_track.py`, measured 2026-10-28, loaded box) | parity-graded; install pending (release-prep green 0.1.0, TestPyPI blocked: missing TWINE_USERNAME) |
| babel | [python-babel/babel](https://github.com/python-babel/babel) | upstream suite 7473 passed / 7 skipped / 2 xfailed both modes (2.18.0, graded 2026-10-28) + three-way equiv 69,401 comparisons 0 divergences across 1082 locales, kernel hit 99.9%; heldout n/a (direct-PR lane) | decimal 2.2–2.3x / currency 3.4x / percent 1.9x / date 3.1–3.4x / time 2.2x / datetime 3.3–3.5x, frozen workload 3.3x (`benches/two_track.py`, 3 runs 2026-10-28) | parity-graded; install pending (release-prep green 2.18.0: wheel+sdist built, twine check passed, smoke green; TestPyPI/PyPI/crates.io blocked: no TWINE_\*/CARGO_REGISTRY_TOKEN; [mirror](https://github.com/John-Cusack/babel-rust)) |

## Status and docs

Built milestone by milestone, M0 → M9, each with verbatim acceptance criteria:

- `SPEC.md` — the contract (v1 + frozen sections)
- `SPEC_STAGE2.md` — replaces SPEC §9 Stage 2 (fixes benchmark gaming, representation wins, symptom-looping)
- `tests/m*_acceptance.sh` — acceptance probes, incl. adversarial plants (test edits, skip injection, fixture-keyed caches, tuned constants, phantom gains, RSS-for-speed, dead-path removal)
- `docs/RUST_PATTERNS.md` — the living pattern guide: what made each port faster (and where it didn't), every claim workload-labeled with measured wall + peak RSS
- `docs/PATTERN_REVIEW.md` — the post-port review process + brief checklist that keeps the guide current (`scripts/gather_pattern_review.sh` builds the review bundle)

Pinned fixtures (`config/fixture.toml`): primary `Nicoretti/crc`, secondary `luozhouyang/python-string-similarity` for DAG checks.

## License

MIT — see [LICENSE](LICENSE).

## Support

If `rustsmith` saved you a rewrite, consider buying me a coffee:

☕ [buymeacoffee.com/johncusack](https://buymeacoffee.com/johncusack)
