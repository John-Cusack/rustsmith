# IMPLEMENTATION M8+M9 — Live agents, then the deferred slices

**Spec contract:** `SPEC.md` (whole; §13 CLI, §14 output, §17 implementer notes) +
`SPEC_STAGE2.md` (replaces SPEC §9 Stage 2) + `IMPLEMENTATION_M7.md` (pipeline
patterns reused verbatim). Spec wins over this doc.
**Goal:** worker-command + seat wiring live (M8); batch/config/charts/round-0/
finals/learn/generated-heldouts (M9). M0–M7 stay green on a clean tree.
**Non-goals:** no parallel-run execution, no flame graphs, no telemetry, no
refactors, no model-network calls, no credentials anywhere.

Tool pins: Python 3.11+, pytest ≥ 7.2, cargo + nightly miri, cargo-clippy,
maturin (via `maturin_cli()` probe order), `/usr/bin/git`. No new crates.

## 0. Env/config surface (all slices share; names frozen here)

| Name | Meaning |
|---|---|
| `RUSTSMITH_WORKER_CMD` | worker shell command; prompt on stdin, JSON `{"tokens_in":N,"tokens_out":M}` (+ optional `note`) on stdout |
| `RUSTSMITH_SEAT_CMD_<ARCHITECT\|VERIFIER\|PERFORMANCE\|SCOPE>` | seat shell command; seat prompt on stdin, JSON `{"stance":"approve"\|"reject","reasoning":"..."}` on stdout |
| `run --config PATH` / `run-batch --config PATH` | TOML overriding `config/default.toml` (`[optimize] max_rounds`, `round_gain_threshold_pct`; `[run]` ceilings; `[models]` identities) |

No model/provider literal outside `config/default.toml` + tests. Seats read
commands from env-or-config only.

## 1. Slice table

| # | Slice | Done when |
|---|---|---|
| 1 | This doc | table + signatures + script shapes + traps + exit committed |
| 2 | `tests/m8_acceptance.sh` + `tests/m9_acceptance.sh` (BEFORE code) | both implement §3 exactly; both fail on current tree |
| 3 | Worker-command interface | `Agent::spawn_worker` shells configurable command, prompt on stdin, JSON usage on stdout; `tokens_in/out` recorded; `prompt_version` never `m1-stub-v1` on that path; default (unset cmd) = current stub, offline green |
| 4 | Council seat wiring | each seat via worker-command interface with seat prompt; disagreeing seats → minority preserved + `architect_tiebreak` with justification; identities config-only |
| 5 | `run-batch` sequential | `run-batch --repo A,B --work DIR` full pipeline per repo, isolated fork/work dirs, one shared store, distinct run-ids; ADR defers concurrent runs |
| 6 | `--config` | `run`/`run-batch --config` overrides ceilings/thresholds/identities; overridden `max_rounds` visibly changes optimize report |
| 7 | Report visualization | HTML gains static inline SVG per-round-gain chart + unit DAG status table from existing `store.db` rows; ADR defers flame graphs (no stack source) |
| 8 | Round 0 apply | approved representation findings dispatch one serialized worker each through the real grade path; each gated parity + divergences + attribution; ≥1 Round-0 unit graded on crc, row recorded merged or `failed_optimizations` honestly |
| 9 | Finals harness | PGO instrumented-build → measure → accept/reject path exists; tools absent → honest refusal naming the tool; accept/reject decision logic unit-tested |
| 10 | Learn workflow | `learn propose` emits guidance-diff proposal file; `learn apply --human NAME --proposal FILE` records `guidance_revisions` row + pins `guidance_version` on later rows; mid-run edits impossible |
| 11 | Generated held-outs | control-plane generator seeded from frozen manifest, disjoint inputs (empty/singleton/max/unicode/shifted sizes), parametrized cases per fixture; green on pristine original + catches M7 hardcode plant |
| 12 | Full chain | `m0..m9` green in order, `git status` only intended files |

Slices 5–11 run after slice 4; each: implement → full chain → commit
`m8-sliceK:` / `m9-sliceK:`. Unfixable in scope → STOP per BUILD §3d.

## 2. Exact signatures

```rust
// crates/rustsmith-agent/src/lib.rs (NEW; existing spawn/wait untouched)
pub struct WorkerOutput { pub tokens_in: u64, pub tokens_out: u64, pub note: String }
pub const WORKER_PROMPT_VERSION: &str = "worker-cmd-v1";
pub fn worker_cmd_from_env() -> Option<String>; // RUSTSMITH_WORKER_CMD
pub fn build_worker_prompt(unit_id: &str, porting_md: &str, unit_bundle: &str) -> String;
  // stable prefix + PORTING.md + unit bundle; contains "unit <id>" line
impl Agent {
  pub fn spawn_worker(&self, spec: &UnitSpec, prompt: &str, cmd: &str) -> Result<WorkerOutput, AgentError>;
  // shells `sh -c cmd` with prompt on stdin (cwd=worktree, env scrubbed as spawn);
  // parses stdout JSON {tokens_in, tokens_out}; non-JSON/exit!=0 => Err
}
// crates/rustsmith-store/src/lib.rs (NEW)
pub fn set_unit_tokens(&self, id: &str, tokens_in: i64, tokens_out: i64) -> Result<(), StoreError>;
pub fn insert_guidance_revision(&self, version: &str, summary: &str, evidence_query: &str,
  stats_json: &str, runs_included_json: &str, decided_by: &str, prompt_diff: &str) -> Result<(), StoreError>;
pub fn latest_guidance_version(&self) -> Result<Option<String>, StoreError>;
pub fn list_guidance_revisions(&self) -> Result<Vec<GuidanceRow>, StoreError>;

// crates/rustsmith-council/src/lib.rs (NEW)
pub struct WorkerSeatDriver { pub seat: Seat, pub command: String, pub model: String }
impl SeatDriver for WorkerSeatDriver {
  fn critique(&self, seat: Seat, proposal: &Proposal, artifact: &[u8]) -> Result<Position, CouncilError>;
  // seat prompt (question + artifact_ref + seat name, NEVER other seats'
  // positions) on stdin; stdout JSON {stance, reasoning}; model recorded in reasoning header
}
pub fn seat_commands_from_env() -> HashMap<Seat, String>; // RUSTSMITH_SEAT_CMD_*

// crates/rustsmith-cli/src/main.rs (NEW/CHANGED)
pub fn cmd_run_batch(args: &[String]) -> Result<(), String>; // run-batch
pub fn cmd_worker_probe(args: &[String]) -> Result<(), String>; // m8 slice-3 probe
pub fn cmd_seat_probe(args: &[String]) -> Result<(), String>;   // m8 slice-4 probe
fn load_run_config(path: Option<&str>) -> Result<RunConfig, String>; // --config merge
fn run_one_repo(repo_spec, fork, work, store_path, run_id, plant, cfg) -> Result<(), String>; // shared by run + run-batch
// learn: `learn propose --store S --out FILE [--evidence-sql Q]`,
//        `learn apply --store S --human NAME --proposal FILE [--guidance PATH]`
// optimize.rs (NEW)
pub fn run_round0_apply(ctx: &OptCtx, work: &Path, findings: &[String], ...) -> Result<Vec<serde_json::Value>, String>;
  // one serialized grade_candidate per finding (round 0); merge iff passed
pub fn decide_final(name: &str, gain: Option<f64>, floor: f64, tool_ok: bool, missing_tool: &str) -> serde_json::Value;
  // pure accept/reject; unit-tested; gated_finals calls it per final
// heldout.rs (NEW)
pub fn generate_from_manifest(manifest_json: &str, fixture: &str) -> Vec<(String, String)>;
  // seeded (sha256 of manifest) disjoint params: empty/singleton/max/unicode/shifted sizes;
// mirror.rs (CHANGED, additive): when worker cmd set, write bundle prompt.txt via
// build_worker_prompt, call spawn_worker, set_unit_tokens, stamp gate detail with prompt_version.
// report (CHANGED, additive): emit_html gains <svg id="round-gains"> bars +
// <table id="unit-dag"> rows from store.list_units + list_optimizations.

// config file (NEW, --config target): TOML with [run] [optimize] [models];
// missing file => Err; unknown keys ignored; every override echoed into optimize-report.json "config".
```

## 3. Acceptance shapes

```sh
tests/m8_acceptance.sh  # pins: crc 4e65ac4
# 0. baseline: crc clone, PYTHONPATH=src pytest -> 80 passed + 28 subtests else STOP.
# 1. worker-probe: fake script (stdin->prompt file, stdout fixed JSON usage)
#    -> exit 0; prompt file contains unit id; store units.tokens_* > 0;
#    gate detail prompt_version == worker-cmd-v1 (never m1-stub-v1).
# 2. seat-probe: two disagreeing fake seat scripts -> minority reasoning in
#    decisions row verbatim; resolved_by == architect_tiebreak with justification.
# 3. dep-check: gates/oracle still free of agent/council refs.
tests/m9_acceptance.sh  # pins: crc 4e65ac4, strsimpy 115acaa(full SHA)
# 0. baselines (crc 80+28; strsimpy 18) else STOP.
# 1. run-batch 2 local repos -> both run-ids done, forks hold lib.rs, shared store has 2 runs.
# 2. --config max_rounds=1 -> optimize-report.json rounds differ vs default; "config" echoes override.
# 3. report html has <svg id="round-gains"> + <table id="unit-dag">; numbers agree with json.
# 4. Round-0: optimizations|failed_optimizations has round==0 row on crc.
# 5. finals refusal names llvm-profdata/llvm-bolt; cargo test decide_final green.
# 6. learn propose->apply on scratch store: guidance_revisions row; version pinned.
# 7. generated held-outs: green on pristine original; hardcode plant halts heldout_divergence.
```

## 4. Traps

* Worker self-report never evidence: usage JSON feeds tokens only; all
  pass/fail from graded gates. Fake workers never touch grading inputs.
* Held-out blindness by API shape (`generate_from_manifest` takes manifest
  JSON + fixture only, never a worker path) + filesystem (generated files go
  host-only heldout dir; bundle assembler signature unchanged, still takes no
  held-out path). Never prompt text.
* Magnitude redaction: `worker_message` for divergence carries no numbers
  (existing regex canary stays; Round-0 reuses `grade_candidate` so inherits it).
* Point estimates never gains: chart plots graded `visible_gain_pct` per round
  only; finals need `decide_final` accept on measured gain > floor.
* Guidance evolves only via `learn`+human: `run_optimize` reads version once
  at start; no mid-run re-read; `learn apply` requires `--human`.
* Provenance: every new artifact (prompt.txt, proposal files, chart) carries
  license/attribution header where it ships in-fork; `store.db` host-only.
* Never edit acceptance scripts to pass; never touch test/bench/manifest/
  held-out files to pass a gate (tamper — report as one).

## 5. Exit criteria

`m0..m9_acceptance.sh` green in order on pinned fixtures; `git status` only
intended files; `SPEC*.md` untouched; per-slice commits; new ADRs:
`005-sequential-batch.md` (concurrency deferred), `006-flamegraph-deferred.md`
(no stack source). Final report: per-milestone outputs, §5 evidence lines,
ADRs. Numbers only.
