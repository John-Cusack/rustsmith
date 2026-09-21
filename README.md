# rustsmith

Autonomous rewrite-in-Rust orchestrator. Point it at repos in any language, get back complete Rust mirrors plus backportable suggestions — with no human in the loop during the run.

## Why this exists

The speed at which LLMs can iterate is becoming the speed at which the human race makes progress. And how fast an LLM can iterate depends heavily on how fast the code it writes can run: every test run, every benchmark, every agentic loop burns wall-clock time, and tokens-per-second is increasingly the binding constraint. Faster code means more iterations per dollar, per hour, per idea — compounding across every agent, everywhere.

That is what `rustsmith` is for: converting existing codebases in any language — starting with Python, where most LLM-targeted code still lives — into fast, correct Rust, so the agentic loop itself runs faster.

Rust is also the language LLMs should prefer going forward. Implemented correctly, it has fewer bugs and fewer safety issues than the languages it replaces: the borrow checker, `Send`/`Sync`, exhaustive matching, and `Result`/`Option` catch at compile time what becomes a hallucinated `None` check or a data race elsewhere. Fewer bug classes means fewer wasted iterations chasing memory corruption, data races, and undefined behavior — exactly the failures that cost agents the most time.

The obstacle is that rewriting legacy code in Rust is slow, manual, and risky, and the fast paths are untrustworthy — an LLM will port the code, quietly weaken the tests to match its own bugs, overfit the benchmarks it can see, and report a speedup that evaporates on real input.

`rustsmith` exists to make the fast path trustworthy. It splits the system in two: agents do the porting and optimizing, and a deterministic gate system written in plain Rust with no LLM involvement decides what passes. No agent can override a gate, and the agent that changes an implementation can never influence how that change is graded.

## What it does

Given a list of source repositories, without human intervention during the run, `rustsmith` produces for each one:

1. **A fork repo containing a complete Rust mirror** of the original — passing the original's test suite, then optimized in bounded rounds.
2. **A suggestions report** of changes discovered during optimization that could be contributed back to the original project (ranked by expected gain / review burden).

Workers do the porting. A 4-seat council (Architect, Verifier, Performance, Scope) plans the work and makes the decisions that would normally escalate to a human. Deterministic gates decide what passes.

## How it works

**The oracle is the definition of correctness.** At run start, `rustsmith-oracle` freezes the original test suite, its config/fixtures, and the exact invocation into `oracle/manifest.json` (SHA-256 per file, stored on the host). Workers get a read-only mount for advisory self-testing. The only run that counts is the **graded run**: the control plane verifies manifest hashes (any mismatch = tamper event), copies the artifact into a fresh ephemeral grading container with no network, and runs the frozen oracle plus a host-only held-out suite no agent ever sees. Pass bar for mirror: 100% oracle parity, test count/skip list identical to baseline, `visible − heldout` divergence under threshold (default 5pp).

**Gates are pure Rust, no LLM, no network.** `rustsmith-gates` enforces `oracle_integrity`, `oracle_parity`, `heldout_divergence`, `differential`, `unsafe_budget` (FFI-only + `// SAFETY:`, under budget), `miri`, `clippy -D warnings`, plus optimizer gates (`workload_divergence`, `causal_attribution`, `benchmark`, widened `no_regression`, `optimization_scope`). Gate failure fails the unit and feeds detail back to the worker — never fatal on its own. `gates` and `oracle` must not depend on `agent`/`council` (CI dep-check).

**Stages:**

- **Stage 0 — Recon.** Deterministic analysis (language/build detection, module + call graph, test baseline, dependency classify `port|bind|keep`, license record, profiling, held-out suite generation), then the Architect writes `PORTING.md` (few-hundred concrete type/error/naming/ownership rules) and a leaf-first unit DAG. Also freezes `WORKLOAD.md` (the performance contract: one primary metric, input distribution, resource budgets) and the benchmark oracle. Council reviews before proceeding.
- **Stage 1 — Mirror.** Behavior-identical Rust, same module boundaries. No redesign. Workers take units in dependency order (parallel up to `max_parallel_workers`), graded on completion, merged on pass. Adversarial review: implementer never reviews its own diff; two reviewers on different providers see only the diff.
- **Stage 2 — Optimize.** Rounds, not an open loop, so it terminates. Round 0 (Architect-owned, serialized) fixes cross-cutting representation debt (`HashMap<String,V>` → structs, per-call `String`/`Vec` → borrows, `Rc<RefCell>` → arenas, AoS → SoA, `Box<dyn>` → enums). Then per round: deterministic profile (Cachegrind primary, wall-clock confirmation on quiesced host) → bound classification (compute, bandwidth, latency, branch, frontend, allocation, syscall_io, contention, work_volume) → ceiling arithmetic `share × (1 − 1/cap)` → dispatch only above `min_candidate_ceiling_pct`, ranked by `ceiling/cost` → proposal-before-code review → parallel implement → full gating (incl. workload-divergence and revert-attribution) → merge winners. Losers go to `failed_optimizations` (in-run memory + report negative-results section). Zero merged optimizations is a valid, honest outcome.
- **Stage 3 — Harvest.** Every accepted optimization is classified: `language_independent` (patch in the original language + proof), `module_local` (optional native accelerator with pure fallback, e.g. PyO3 for Python), `port_only` (needs the full port). Emits `RUSTSMITH_REPORT.md` + HTML + JSON (must agree), `suggestions/{patches/,accelerators/}`.

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
| `[models]` | swappable provider/model identities per seat — never hardcoded elsewhere; reviewers on distinct providers is structural |
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
| Python | Supported | First adapter; pure-Python + PyO3/maturin bridges |
| C++ | Planned (pinned repo) | CMake/CTest + GTest/Catch2; payoff is safety/maintainability, not speed |
| Fortran | Planned (pinned repo) | CMake/CTest + pFUnit; F90+ first, F77 `COMMON`-block code later |
| TypeScript / JavaScript (Node.js) | Candidate | Backend/CLI/tooling only — browser-DOM rendering stays in JS (Rust would need a separate WASM path, not a rustsmith mirror) |
| Go, Ruby | Candidate | Slow-or-loose-typed backend code with large corpora; cheap adapters once the runner/bridge/profiler seam lands |

Rule of thumb: slow, easy-to-type backend code (Python, Ruby, Node) is the best mirror target — large speedup + type-safety win. C++/Fortran already run fast; port those for memory safety and maintainability. Browser-frontend JS is out of scope.

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
