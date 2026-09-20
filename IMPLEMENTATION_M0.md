# IMPLEMENTATION M0 — Oracle, Gates, Store, Sandbox

**Spec contract:** `SPEC.md` v1 §§4–8 + §15 M0. Spec commit: _fill in `git rev-parse --short HEAD` before starting._
**Goal:** trustworthy grading with zero agents. Everything later (M1+ workers, council, Stage 2) depends on this being ungameable.
**Non-goals:** no `rustsmith-agent`, no `rustsmith-council`, no profiling/benchmarks beyond test-pass counting, no Stage 2/3 code. `benchmark`/`no_regression` gates stub as `not_applicable` until M5.

## 1. Crate surfaces (build in this order)

```
crates/
  rustsmith-core/     # domain types only, no I/O. No deps on any other rustsmith crate.
  rustsmith-store/    # SQLite + JSONL. Depends: core.
  rustsmith-oracle/   # freeze, hash, graded-run orchestration. Depends: core, store. MUST NOT depend on agent/council.
  rustsmith-gates/    # pure functions: evidence -> pass/fail. Depends: core only. MUST NOT depend on agent/council/store/oracle.
  rustsmith-sandbox/  # containers + images. Depends: core.
  rustsmith-cli/      # `run --stage recon` skeleton, `audit`. Depends: all above.
```

CI hard rule (implement day 1 or gates are not gates):

```sh
# fails if oracle/gates can reach an LLM
cargo deny check bans  # or: grep -r 'rustsmith-agent\|rustsmith-council' crates/rustsmith-gates crates/rustsmith-oracle && exit 1
```

## 2. Interfaces (write these signatures first, then bodies)

```rust
// rustsmith-core — plain data, serde + thiserror only
pub struct Manifest { pub version: u32, pub invocation: Vec<String>, pub files: Vec<FileHash>, pub baseline: Baseline }
pub struct FileHash { pub path: Utf8PathBuf, pub sha256: String }
pub struct Baseline { pub test_count: u32, pub skipped: Vec<String>, pub xfailed: Vec<String> }
pub struct GradedResult { pub exit_code: i32, pub passed: u32, pub failed: u32, pub skipped: Vec<String>, pub xfailed: Vec<String>, pub deselected: Vec<String>, pub stdout: String }
pub enum HaltReason { OracleTamper { path: String }, SkipListMismatch { diff: String }, HeldoutLeak, DivergenceOverThreshold { divergence: f64 } }
pub enum Gate { OracleIntegrity, OracleParity, HeldoutDivergence, Differential, UnsafeBudget, Miri, Clippy }

// rustsmith-store — rusqlite, one connection, WAL mode
impl Store {
  pub fn open(path: &Path) -> Result<Self>;          // creates SPEC §6 tables verbatim
  pub fn append_event(&self, ev: &Event) -> Result<()>; // JSONL append-only, fsync
  pub fn record_gate(&self, unit: &str, gate: Gate, passed: bool, detail: &serde_json::Value) -> Result<()>;
}

// rustsmith-oracle
impl Oracle {
  pub fn freeze(repo_root: &Path) -> Result<Manifest>;   // §7.1: tests + conftest/pytest.ini/pyproject test sections/tox.ini + fixtures/data + CI workflow; sha256 each; manifest stays on host
  pub fn verify_hashes(manifest: &Manifest, tree: &Path) -> Result<()>; // mismatch => HaltReason::OracleTamper
  pub fn graded_run(manifest: &Manifest, artifact: &Path, heldout: &HeldoutSuite) -> Result<(GradedResult, f64)>; // §7.3+7.4: fresh container, no net, clean oracle checkout; returns result + divergence
}
// Held-out suite: generated from public API (property/differential/fuzz), stored host-only, never mounted into run/grading containers visible to workers.

// rustsmith-gates — pure, no fs/net except passed-in evidence
pub fn oracle_integrity(manifest: &Manifest, tree_hashes: &[FileHash], base: &Baseline, got: &GradedResult) -> GateVerdict;
pub fn oracle_parity(got: &GradedResult) -> GateVerdict;          // 100% frozen oracle passes
pub fn heldout_divergence(visible_rate: f64, heldout_rate: f64, threshold: f64) -> GateVerdict;
pub struct GateVerdict { pub passed: bool, pub detail: serde_json::Value }

// rustsmith-sandbox
impl Sandbox {
  pub fn ensure_images() -> Result<(String, String)>;   // (run_image, grading_image) from containers/
  pub fn grading_run(manifest: &Manifest, artifact: &Path) -> Result<GradedResult>; // ephemeral, --network=none, read-only oracle mount
}
```

Key decisions locked here (ADR if changed): manifest = `oracle/manifest.json` on host only; grading container gets clean oracle checkout + artifact copy, never the worker tree; held-out suite never enters any container image or mount — host-side `pytest` invocation only.

## 3. Build order (each row is a verifiable slice)

| # | Slice | Done when |
|---|-------|-----------|
| 1 | Workspace + `core` types + `store` tables verbatim (§6: `runs, units, gate_results, decisions`; `optimizations` DDL present, unwritten) + JSONL append | `cargo test -p rustsmith-store` passes; reopening DB preserves rows |
| 2 | `oracle::freeze` + `verify_hashes` | Freeze a real small Python repo (<5k lines, e.g. string-distance lib); `manifest.json` lists every test/config/fixture hash + invocation |
| 3 | `sandbox::grading_run` + `containers/` Dockerfiles (run + grading) | Unmodified repo passes graded run; `test_count`/skip list equals host baseline |
| 4 | `oracle::graded_run` + held-out stub (hand-written property tests for M0, generator comes M3) host-only | Held-out runs on host; `find / -name '*heldout*'` inside grading container returns nothing |
| 5 | `gates::oracle_integrity/oracle_parity/heldout_divergence` | Unit tests with fabricated `GradedResult`s: hash mismatch fails, count/skip/xfail/deselect drift fails, divergence math exact |
| 6 | `gates::differential/unsafe_budget/miri/clippy` (real impl, M0 needs them for §16 items 5–6) | `cargo geiger` count + `// SAFETY:` + FFI-boundary check; `cargo clippy -- -D warnings`; miri on runnable subset |
| 7 | `cli` skeleton: `run --stage recon`, `audit --run-id` (replay JSONL), dep-check in CI | `rustsmith audit` reconstructs freeze→grade→gate sequence from events alone |
| 8 | M0 acceptance script `tests/m0_acceptance.sh` (section 4) | Green on a fresh checkout, no agent code in tree (`git grep -l 'rustsmith-agent' -- crates/ \| grep -v cli` empty) |

## 4. M0 acceptance (quoted from SPEC §15 — run exactly this)

> Freeze a real Python repo's oracle; run a graded pass in a clean container; hand-modify a test file and confirm `oracle_integrity` fails; hand-add `@pytest.mark.skip` and confirm the skip-count check fails; confirm the held-out suite runs on the host only.

```sh
tests/m0_acceptance.sh <python-fixture-repo-url>
# 1. freeze -> oracle/manifest.json written, baseline test_count + skip list recorded
# 2. graded pass on unmodified tree -> oracle_parity PASS, divergence < 0.05
# 3. echo '# tamper' >> tests/test_x.py -> verify_hashes FAILS, runs.halt_reason='oracle_tamper', event logged
# 4. restore; add @pytest.mark.skip to one test -> oracle_integrity FAILS on skip-list mismatch (require_zero_skipped=true)
# 5. exec into grading image, prove held-out absent; run held-out on host -> passes
```

Fixture pick (SPEC §16): small pure-Python, <5,000 lines, comprehensive suite, no Rust core — hashing/checksum, string-distance, or date-parser class.

## 5. Risks / traps

* Test discovery lies: `pytest --collect-only -q` count is the baseline, not a glob of files. Parametrize, `conftest` collection tweaks, and `deselect` all change the count — capture all four lists (passed/skipped/xfailed/deselected) or step 4 gives false passes.
* Hash the invocation, not just files: `pytest.ini`/`tox.ini`/CI workflow change what "the suite" means. Manifest must include the exact command or a config edit silently shrinks the suite.
* Container determinism: pin base image digests; `--network=none`; no volume mounts except read-only oracle + artifact copy. Anything else and "clean container" is fiction.
* `store.db` lives on host, never mounted into containers (same rule §17 will need for `heldout_gain_pct`).

## 6. Exit criteria

`tests/m0_acceptance.sh` green + dep-check green + `audit` replay sufficient. Then M1 may start. No agent prompts, no `PORTING.md`, no optimization code in this milestone — review rejects any diff containing them.
