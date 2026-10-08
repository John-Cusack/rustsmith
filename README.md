# rustsmith

Trustworthy migration of existing software to verified, production-ready Rust — measured speedups, honest no-gain reports, artifacts you can install.

## Why this exists

The speed at which LLMs can iterate is becoming the speed at which the human race makes progress. And how fast an LLM can iterate depends heavily on how fast the code it writes can run: every test run, every benchmark, every agentic loop burns wall-clock time, and tokens-per-second is increasingly the binding constraint. Faster code means more iterations per dollar, per hour, per idea — compounding across every agent, everywhere.

That is what `rustsmith` is for: converting existing codebases in any language — starting with Python, where most LLM-targeted code still lives — into fast, correct Rust, so the agentic loop itself runs faster.

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
| 3 | python-dateutil | mirrored (oracle 2032/2032, heldout 41/41) + parser/isoparser port: oracle 2031 passed / 0 failed (incl. parser+iso suites, matches pure-Python baseline exactly), frozen-corpora parity 0 divergences, §5 bench 5.6x fuzzy-noisy / 3.8x strict-parse / 1.7x iso | 874M | 6,070 | `_parser.py::get_token`, `rrule.py::_iter`, `tz.py::_isdst` | [chrono](https://crates.io/crates/chrono) (strict only) / [dateparser](https://crates.io/crates/dateparser) (no rrule) / [jiff](https://crates.io/crates/jiff) (no recurrence) | Full parse path ported (ADR-015 supersedes the engine-only scope): fuzzy shares one `_parse` path with strict, so `parse()` users gain too |
| 4 | pyyaml | open | 867M | 8,566 | `scanner.py`, `parser.py`, `emitter.py` (pure-Python path) | [serde_yaml](https://crates.io/crates/serde_yaml) (deprecated) / [serde_yaml_ng](https://crates.io/crates/serde_yaml_ng) (C wrapper) / [unsafe-libyaml](https://crates.io/crates/unsafe-libyaml) (bindings) | No mature pure-Rust YAML 1.1 crate |
| 5 | markdown-it-py | mirrored (oracle 1031 passed / 0 failed / 1 env-skip of 1032, heldout 16/18 with 2 env-skips; graded 2026-10-07) | 427M | 508 | `parser_block.py`, `parser_inline.py`, `emphasis.py` | [pulldown-cmark](https://crates.io/crates/pulldown-cmark) (no plugin API) / [comrak](https://crates.io/crates/comrak) (GFM renderer) / [markdown-it](https://crates.io/crates/markdown-it) (stale since 2024-07) | Only direct Rust port is stale; plugin token-stream gap |
| 6 | python-multipart | mirrored (oracle 162/162, heldout 55/55) | 313M | 703 | `multipart.py::_internal_write` (both parsers), `parse_options_header` | [multer](https://crates.io/crates/multer) (async-only) / [multipart](https://crates.io/crates/multipart) (unmaintained since 2021) / [form-data](https://crates.io/crates/form-data) (async, tiny) | Untrusted-input byte state machine with CPU-exhaustion CVE history |
| 7 | pyparsing | mirrored (oracle 2104/2104, heldout 41/41 + 9 generated pins, graded 2026-10-08) | 298M | 1,663 | `core.py::parse_string`/`_parseNoCache`, `results.py` | [nom](https://crates.io/crates/nom) / [winnow](https://crates.io/crates/winnow) (code-first) / [pest](https://crates.io/crates/pest) (codegen) | Gap is specifically a runtime-constructible grammar API |

### In flight and landed outside KEEP

| Target | Language | Status | Notes |
|---|---|---|---|
| crc (`mirror/crc`) | Python | mirrored (oracle 80/80, heldout 11/11, divergence 0.0000; graded 2026-10-06); release work in progress (`John-Cusack/rewrite-packaging`) | Primary fixture; first core+binding release example |
| strsimpy (`mirror/strsimpy`) | Python | mirrored (oracle 18/18, heldout 16/16, divergence 0.0000; graded 2026-10-06) | Core+binding split (`strsimpy-rust-core` + `strsimpy-rust`); `release-prep` verified (11/11 checks) |
| Elmer FEM (`mirror/Elmer`) | C / C++ / Fortran | S1 scope-skip landed, step-4 verified 2026-10-06 (suite 181/0, `m0`–`m9` + `release_acceptance.sh` green, `tests/elmer_mini_proof.sh` 1/1 divergence=0.0000) | Session briefs S1–S5 in `docs/IMPLEMENTATION_ELMER.md`; S2–S5 not started |

Deliberately excluded: certifi, pytz (data-only bundles); iniconfig (trivial, I/O-bound); typing-inspection (thin typing dispatch, nothing to port); jinja2, jsonschema (mature Rust ports already exist: minijinja, the `jsonschema` crate); pandas (hot paths already Cython; polars covers new builds); beautifulsoup4 (scraper/html5ever cover the capability; remainder is API sugar).

## Status and docs

Built milestone by milestone, M0 → M9, each with verbatim acceptance criteria:

- `SPEC.md` — the contract (v1 + frozen sections)
- `SPEC_STAGE2.md` — replaces SPEC §9 Stage 2 (fixes benchmark gaming, representation wins, symptom-looping)
- `IMPLEMENTATION.md` — milestone map; `IMPLEMENTATION_M*.md` — per-milestone build orders
- `BUILD_M*.md` — build depth (signatures, slices, scripts, traps)
- `tests/m*_acceptance.sh` — acceptance probes, incl. adversarial plants (test edits, skip injection, fixture-keyed caches, tuned constants, phantom gains, RSS-for-speed, dead-path removal)

Pinned fixtures (`config/fixture.toml`): primary `Nicoretti/crc`, secondary `luozhouyang/python-string-similarity` for DAG checks.

## License

MIT — see [LICENSE](LICENSE).

## Support

If `rustsmith` saved you a rewrite, consider buying me a coffee:

☕ [buymeacoffee.com/johncusack](https://buymeacoffee.com/johncusack)
