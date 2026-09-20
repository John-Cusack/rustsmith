# IMPLEMENTATION M1 — Sandbox Confinement + Worker Driver

**Spec contract:** `SPEC.md` §§4–5, §10.3 halt 5, §11, §15 M1. Builds on M0 (grading split, `store.db` host-only).
**Goal:** 8 concurrent workers in one container, each confined to its own worktree, with out-of-worktree `git`/`cargo` impossible (not merely instructed) and every spawn/deny logged.
**Non-goals:** no council, no prompts engineering, no real porting work — workers run a stub OMP task (e.g. `echo`/fixed output) until M4. No benchmark/profile code.

## 1. Crate surfaces

```
rustsmith-sandbox/   # ADD: worktrees, cgroups, build mutex, PATH wrapper
rustsmith-agent/     # NEW: OMP subprocess driver
containers/          # ADD: run image with wrapper on PATH, cgroup mounts
tests/m1_acceptance.sh
```

```rust
// rustsmith-sandbox — additions
impl Sandbox {
  pub fn alloc_worktree(&self, run_id: &str, unit_id: &str) -> Result<Utf8PathBuf>;
  pub fn drop_worktree(&self, worktree: &Path) -> Result<()>;
}
pub struct BuildLease { /* RAII guard */ }
impl BuildLease { pub fn acquire(run_id: &str, timeout: Duration) -> Result<Self>; }
// RAII release on drop; one holder per run container at a time.

pub struct CgroupLimits { pub cpu_shares: u64, pub mem_bytes: u64 }
pub fn apply_limits(pid: u32, limits: &CgroupLimits) -> Result<()>;

// rustsmith-agent — new
pub struct UnitSpec { pub unit_id: String, pub worktree: Utf8PathBuf, pub task: String, pub token_ceiling: u64 }
pub struct AgentResult { pub exit_code: i32, pub stdout: String, pub tokens_in: u64, pub tokens_out: u64, pub prompt_version: String }
impl Agent {
  pub fn spawn(&self, spec: &UnitSpec) -> Result<ChildHandle>;  // one OMP CLI subprocess, env scrubbed, cwd=worktree
  pub fn wait(&self, h: ChildHandle, timeout: Duration) -> Result<AgentResult>; // structured-output parse, kill on timeout
}
```

Wrapper (shell, installed first on `PATH` in run image): intercepts `git` and `cargo`, resolves `cwd` vs `$UNIT_WORKTREE`; denies (exit 127 + syslog line + `events.jsonl` halt-trigger entry) anything targeting outside — including `git -C <other>`, `--git-dir=`, `cargo --manifest-path <other>`, absolute paths. Allowlist: invocations fully inside `$UNIT_WORKTREE`. Filesystem backstop: worktrees owned by per-unit uid or mode `0700` where uids unavailable; run branch ref writable only by control plane uid.

## 2. Build order

| # | Slice | Done when |
|---|-------|-----------|
| 1 | `alloc/drop_worktree` (`git worktree add/remove`, branch `unit/<id>`) | alloc 8, `git worktree list` shows 8, drop cleans all |
| 2 | `BuildLease` (flock on `$RUN_DIR/.build.lock`, blocking with timeout, RAII) | 8 threads contend → serialized `cargo build` timestamps never overlap |
| 3 | `apply_limits` (cgroup v2 `cpu.max` + `memory.max`, best-effort with fallback log when host lacks delegation) | limited `stress`-child stays under cap or denial is logged with reason |
| 4 | `PATH` wrapper + image install + filesystem perms | in-worktree `git status`/`cargo --version` pass; `git -C /other`, `--git-dir`, `--manifest-path /other`, absolute-path escapes all exit 127 |
| 5 | `Agent::spawn/wait` (env: `UNIT_WORKTREE` set, `GIT_DIR`/`CARGO_HOME` scrubbed, `cwd=worktree`; stdout JSON parse; timeout kill; tokens recorded to `units` + event log) | stub task returns parsed result; timeout task killed + recorded |
| 6 | `tests/m1_acceptance.sh` | green (section 3) |

## 3. M1 acceptance (quoted from SPEC §15)

> "spawn 8 concurrent OMP subprocesses in one container; confirm each is confined to its worktree; confirm an attempt to run `git checkout` on the run branch is blocked and logged as a halt trigger."

```sh
tests/m1_acceptance.sh
# 1. alloc 8 worktrees in one run container
# 2. spawn 8 stub OMP tasks concurrently (each writes $UNIT_WORKTREE/whoami.txt)
# 3. assert: 8 outputs land in own worktrees, zero cross-worktree writes (find + content check)
# 4. negative: one worker runs `git checkout <run-branch>` and one runs `cargo --manifest-path /other/Cargo.toml`
#    -> both exit 127, both appended to events.jsonl as halt-trigger entries with unit_id + command
# 5. assert: run branch HEAD unchanged (`git rev-parse` before == after)
# 6. drop all worktrees, `git worktree list` clean
```

## 4. Traps

* Env leaks: `GIT_DIR`, `GIT_WORK_TREE`, `CARGO_TARGET_DIR` must be scrubbed or the wrapper is bypassable in one line. Test the bypasses explicitly, not just the happy path.
* Build-cache thrash (SPEC §11 calls this out from production ports): without the lease, 8 concurrent `cargo` invocations exhaust IOPS — the lease is load-bearing, not polite.
* OMP CLI version drift: pin the CLI version in the run image digest; record `prompt_version` per turn from M1 onward even for stub tasks so M4 replay works.
* `events.jsonl` is the audit trail: every spawn, exit, timeout, deny, lease-wait must append. `rustsmith audit` replays M1 from the log alone.

## 5. Exit criteria

`m1_acceptance.sh` green + M0 acceptance still green + `git grep -rn 'prompt.*instruct.*not to' -- prompts/ containers/` empty (no instruction-based confinement anywhere). Then M2.
