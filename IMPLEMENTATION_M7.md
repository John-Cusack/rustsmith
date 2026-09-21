# IMPLEMENTATION M7 — Unattended end-to-end run (SPEC §16)

**Spec contract:** `SPEC.md` §16 (12-item check, normative) + §13 (CLI) + §9
Stage 3 + §17 (prevention-over-instruction). `SPEC_STAGE2.md` REPLACES `SPEC.md`
§9 Stage 2. Where any plan doc conflicts with the spec, the spec wins.
**Goal:** `rustsmith run <repo-url>` with no human input produces a graded,
reported, halt-safe run on crc AND reaches mirror parity on strsimpy.
**Non-goals:** no new adapters beyond the two pinned fixtures, no auto-opened
upstream PRs, no telemetry, no refactors "while you're in there".

Tool pins: Python 3.11+, pytest ≥ 7.2, hatchling (crc build backend), maturin
(extension build), cargo + nightly miri, cargo-clippy, cargo-geiger (if present).

## 0. Fixture dispatch (ADR, 5 lines)

Only two fixtures are pinned (`config/fixture.toml`); a fully generic
Python→Rust adapter is out of scope. All per-repo behavior branches on
`FixtureKind` detected once in recon and frozen into `recon/fixture.json`
(`{"fixture":"crc"|"strsimpy","package":...,"src_layout":bool}`); mirror /
optimize / report read that file and never re-detect (no drift). crc paths
keep byte-identical behavior; strsimpy gets a second branch + second template.

## 1. Crate surfaces

```rust
// crates/rustsmith-cli/src/fixture.rs  (NEW, no LLM deps)
pub enum FixtureKind { Crc, Strsimpy }
pub fn detect_fixture(repo: &Path) -> Result<FixtureKind, String>;
// pyproject [project] name or package-dir presence: `src/crc` -> Crc,
// `strsimpy/` -> Strsimpy; anything else -> Err (refuse, do not guess).
pub fn template_dir(kind: &FixtureKind) -> PathBuf; // mirror/crc | mirror/strsimpy

// mirror.rs — template mechanism replaces hardcoded crc paths (516-550,
// differential_pairs, merge_unit delete targets, orig_src join("src")).
pub struct TemplateSpec { // loaded from <template>/template.json
  pub files: Vec<(String, String)>,              // (template-rel, fork-rel) copies
  pub delete_on_merge: HashMap<String, Vec<String>>, // unit -> repo-rel deletions
  pub orig_source_for: HashMap<String, String>,  // unit -> repo-rel orig source
}
pub fn load_template(dir: &Path) -> Result<TemplateSpec, String>;
pub fn unit_task(spec: &TemplateSpec, template: &Path, unit: &str) -> Result<String, String>;
pub fn differential_pairs_for(kind: &FixtureKind, orig_py: &Path, venv_py: &Path,
  worktree: &Path, orig_src: &Path) -> Result<Vec<(String, String)>, String>;
// orig_src = repo/src if src_layout else repo (flat). Held-out blindness
// unchanged: bundle assembler takes no held-out path (existing canary stays).
// NEW halts in run_mirror: !integrity.passed -> set_halt("oracle_tamper <files>")
// + "tamper" event, abort; whole-repo divergence over threshold ->
// set_halt("heldout_divergence ..."), abort. Per-unit div fail still fails
// the unit only (unchanged).

// recon.rs — per-fixture branches, frozen alongside oracle:
pub fn invocation_for(kind: &FixtureKind) -> Vec<String>;
  // Crc: ["pytest test/unit", "pytest test/integration"] (split, PYTHONPATH=src).
  // Strsimpy: ["pytest -q"] at root, no PYTHONPATH (flat setuptools layout).
pub fn heldout_for(kind: &FixtureKind) -> Vec<(String, String)>;
  // Crc: existing suite. Strsimpy: host-only differential/property tests vs
  // original API on disjoint inputs (never mounted into run containers).
pub fn workload_contract_for(kind: &FixtureKind, repo: &Path) -> Result<WorkloadContract, String>;
pub fn porting_md_for(kind: &FixtureKind, repo: &Path) -> String; // crc: existing
  // 115+ rules; strsimpy: rulebook w/ type/error/naming/layout mapping (>=40 rules).

// optimize.rs — fixture-gated workloads; crc path byte-identical:
pub fn workloads_for(kind: &FixtureKind) -> OptWorkloads; // strsimpy: benign
  // string-similarity workloads; zero applicable candidates -> stop
  // "no candidate above ceiling" round 1, reports still emitted, zero merges OK.
// grade_plant / plants: add "test-edit" + "hardcode" live-loop plants (see §2).

// main.rs — full-pipeline orchestrator (replaces M0 skeleton; --stage recon
// keeps the exact M0 path so m0_acceptance.sh stays green):
//   rustsmith run --repo <url|path> --fork <dir> --work <dir> --store <db>
//     --run-id <id> [--plant-live test-edit|hardcode]
// No --stage (or --stage full) = clone/resolve -> recon -> mirror -> optimize
// -> report, unattended. --stage recon|mirror|optimize = that stage only
// (hand-invoked compat). --plant-live only in full mode; mutates through the
// REAL grade path, then halts (see §2). exit nonzero on halt.
pub fn cmd_run(args: &[String]) -> Result<(), String>; // rewritten
fn run_report(run_id, fork, orig, store, recon_out, opt) -> Result<serde_json::Value, String>;
// (extracted from cmd_report body; cmd_report keeps CLI shape, calls run_report)
```

## 2. Build order

| # | Slice | Done when |
|---|-------|-----------|
| 1 | This doc | slice table + signatures + script shape + traps + exit criteria committed |
| 2 | `tests/m7_acceptance.sh` (BEFORE code) | script implements §3 exactly; fails on current tree (no `run` pipeline) |
| 3 | `fixture.rs` + template mechanism in mirror.rs (`template.json` for crc mirroring current hardcoded behavior file-for-file) | `mirror --template mirror/crc` green on crc with zero behavior change (m4 green via new path) |
| 4 | `mirror/strsimpy` reference port (PyO3; single ext + `strsimpy/__init__.py` re-exporting submodule aliases via `sys.modules` so `strsimpy.<mod>` imports work; zero `unsafe`) | manual `mirror` on strsimpy clone: 18/18 oracle green, divergence < 5pp, clippy + miri clean |
| 5 | Per-fixture recon (invocation, manifest freeze, heldout suite, WORKLOAD.md, PORTING.md, DAG leaf-first) | `recon` on strsimpy: baseline 18 passed 0 skipped; heldout dir non-empty host-only; dag order valid |
| 6 | Per-fixture optimize + differential (`differential_pairs_for`, strsimpy workloads, early-stop path, reports always emitted) | `optimize` on strsimpy fork terminates on a stopping rule, zero merges, no tamper; crc optimize unchanged (m5 green) |
| 7 | `run` orchestrator + `--plant-live` (test-edit: scratch worktree edits frozen test -> real `oracle_integrity` fail -> `set_halt("oracle_tamper ...")` + tamper event, abort pre-mirror; hardcode: post-mirror fork patch with visible-input-only correct answers -> real whole-repo grade: parity passes, held-out fails -> `set_halt("heldout_divergence ...")`, abort pre-optimize) | `run` on local crc path unattended recon->report; both plants halt with reason logged; `run <url>` clones |
| 8 | Full chain | `m0..m7_acceptance.sh` green in order, `git status` only intended files |

## 3. M7 acceptance (== SPEC §16 items 1–12 + generic-template proof)

```sh
tests/m7_acceptance.sh   # pins: crc 4e65ac4, strsimpy 115acaa(full SHA re-pinned)
# 0. baselines: clone both; crc `PYTHONPATH=src pytest test/unit test/integration`
#    -> 80 passed + 28 subtests; strsimpy `pytest -q` -> 18 passed. ANY DIFFERENCE = STOP.
# 1. `rustsmith run --repo <crc-url@pin> ...` unattended, exit 0.
#    fork has src/lib.rs + Cargo.toml; crc public API spot-check via built ext
#    (Calculator/Register/TableBasedRegister/Crc8/Crc16/Crc32/Configuration,
#    checksum(b'123456789') == 0xF4).
# 2. graded run 100% frozen oracle (80 passed, 0 failed) IN THE FORK venv.
# 3. held-out divergence < 5pp (visible rate - heldout rate, recomputed, not trusted).
# 4. `verify --manifest recon/manifest.json --tree fork` green + grade json:
#    integrity+parity passed; test count + skip list match baseline.
# 5. no `unsafe` tokens outside comments in fork/src (0 expected); every unsafe
#    (if any) at FFI boundary with `// SAFETY:` (geiger cross-check if installed).
# 6. `cargo clippy -- -D warnings` clean; `cargo +nightly miri test --lib` clean.
# 7. optimize-report.json: rounds >= 2, stop reason is a stopping rule
#    (gain-threshold|max-rounds|no-ceiling|spend-guard), merged >= 1 with
#    recorded delta above floor; no_regression proven by m5 plant suite (not re-run).
# 8. RUSTSMITH_REPORT.md + rustsmith-report.{json,html} exist; md/json/html agree
#    on parity, divergence, unsafe, per-round deltas (same check as m6 §check-4).
# 9. suggestions/ >= 1 classified item with reasoning paragraph (expect slice-by-8).
# 10. `audit --run-id` contains unit_start/gate/merge/mirror_done + round events;
#     decisions table non-empty; every unit action + council decision replayable.
# 11. `run --plant-live test-edit` exits nonzero; runs.halt_reason starts with
#     `oracle_tamper`; event log has tamper event with file + reason.
# 12. `run --plant-live hardcode` exits nonzero; halt_reason starts with
#     `heldout_divergence`; held-out gate failed while visible parity passed.
# 13. strsimpy: same `run` command -> mirror parity (18/18 in fork venv,
#     divergence < 5pp, manifest verify green, clippy+miri clean); zero merges
#     acceptable; ANY halt_reason = FAIL (tamper never acceptable).
```

## 4. Traps

* Worker self-report is never evidence: plants are graded by rebuild + frozen
  oracle + host-only held-out only; `worker_message` for divergence carries NO
  magnitudes (redaction asserted by regex, same as M5 plant 19).
* Held-out blindness is by API shape (bundle fn takes no held-out path) and
  filesystem (heldout/ never mounted/copied into fork, worktrees, or bundles);
  never by prompt text. New strsimpy paths get the same canary treatment.
* Guidance evolves ONLY via `learn` + human approval; `run` pins
  `guidance_version` per row and never edits `guidance/optimize.md`.
* `store.db` stays on host; grading venvs use `--system-site-packages`, no network.
* `mirror/<name>/template.json` is load-bearing: any file the port needs must be
  listed; an unlisted-file port that passes locally but fails in worktrees is
  the failure mode (crc slice 3 proves the mechanism before strsimpy uses it).
* Never edit an acceptance script to make it pass; never modify a test,
  benchmark, manifest, or held-out file to make a gate pass (tamper — report).

## 5. Exit criteria

`m0..m7_acceptance.sh` all green in order on pinned fixtures; `git status`
shows only intended files; `SPEC*.md` untouched; per-slice commits
`m7-sliceK: <what>`; nothing pushed upstream. Final report: per-milestone
script outputs, §16 item 1–12 evidence lines, ADRs. Numbers only.
