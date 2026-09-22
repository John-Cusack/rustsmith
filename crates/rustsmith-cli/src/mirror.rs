//! assembler (held-out blind by construction), grade->gate->merge loop with
//! same-commit module deletion, adversarial review plumbing, whole-repo grade.
//!
//! Fixture worker note: acceptance runs stub shell tasks (via `Agent`, real
//! subprocesses in real worktrees) that materialize each unit from the
//! reference port by following the bundle's PORTING.md. A production run
//! swaps the stub task for a real OMP worker task; grading, gating, merging,
//! and review are identical. The PROOF is in the gates, not the task.

use crate::units;
use rustsmith_adapters::{BuildBridge, CmakeBridge, CtestRunner, MaturinBridge, PytestRunner, UnitDag};
use rustsmith_agent::{Agent, UnitSpec};
use rustsmith_council::{Council, Proposal, Seat, SeatDriver, Stance, StubDriver};
use rustsmith_core::{BuildCtx, Cwd, Event, Gate, TestCommand, TestRunner};
use rustsmith_gates as gates;
use rustsmith_oracle::{execute_all, Oracle};
use rustsmith_sandbox::Sandbox;
use rustsmith_store::Store;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PROMPT_VERSION: &str = "mirror-v1";
const MAX_PARALLEL: usize = 16;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn ev(store: &Store, run_id: &str, kind: &str, detail: serde_json::Value) {
    let _ = store.append_event(&Event {
        ts: now(),
        run_id: run_id.into(),
        kind: kind.into(),
        detail,
    });
}

/// Reviewer assignment: two seats, distinct providers, never the implementer.
/// Providers come from config (never hardcoded); the seat->provider map is passed in.
pub fn assign_reviewers(
    implementer: Option<Seat>,
    providers: &HashMap<Seat, String>,
) -> (Seat, Seat) {
    let order = [Seat::Verifier, Seat::Performance, Seat::Architect, Seat::Scope];
    let mut out = Vec::new();
    for s in order {
        if Some(s) == implementer {
            continue;
        }
        // Keep providers distinct across the pair.
        if out.iter().any(|o: &Seat| providers.get(o) == providers.get(&s)) {
            continue;
        }
        out.push(s);
        if out.len() == 2 {
            break;
        }
    }
    // Fallback (should not happen with 4 distinct providers): first two non-implementer.
    while out.len() < 2 {
        for s in order {
            if Some(s) != implementer && !out.contains(&s) {
                out.push(s);
                break;
            }
        }
    }
    (out[0], out[1])
}

#[allow(dead_code)]
pub struct Bundle {
    pub dir: PathBuf,
}

/// Assemble the exact worker input bundle. Held-out blindness is structural:
/// this function takes no held-out path argument, so a leak is impossible to
/// express. The canary test asserts the emitted bundle never contains a
/// held-out marker even when one exists on the host.
pub fn assemble_bundle(
    bundle_dir: &Path,
    unit_id: &str,
    module: &str,
    depends_on: &[String],
    porting_md: &str,
    original_source: &str,
    oracle_path: &str,
) -> Result<Bundle, String> {
    std::fs::create_dir_all(bundle_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        bundle_dir.join("spec.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "unit_id": unit_id,
            "module": module,
            "depends_on": depends_on,
            "instructions": "Port this module to behavior-identical Rust per PORTING.md. No redesign.",
            "prompt_version": PROMPT_VERSION,
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(bundle_dir.join("PORTING.md"), porting_md).map_err(|e| e.to_string())?;
    std::fs::write(bundle_dir.join("original_source.py"), original_source)
        .map_err(|e| e.to_string())?;
    std::fs::write(
        bundle_dir.join("contracts.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "depends_on": depends_on,
            "oracle_path": oracle_path,
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(Bundle {
        dir: bundle_dir.to_path_buf(),
    })
}

/// Grade-venv interpreter path. The interpreter literal lives in
/// `PytestRunner::python_program`; this is a pure path join.
pub(crate) fn grade_venv_python(venv: &Path) -> PathBuf {
    venv.join("bin/python")
}

/// `PATH` with the venv's bin prepended so the bridged build resolves its
/// tools inside the venv.
fn venv_bin_path_prepend(venv: &Path) -> String {
    let mut path = venv.join("bin").as_os_str().to_owned();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    path.to_string_lossy().into_owned()
}

/// Grade venv: `--system-site-packages` (no network). Venv creation is a stage
/// concern (`MaturinBridge::prepare` is empty by design); the interpreter
/// program comes from the runner so no `python3` literal lives here.
pub(crate) fn ensure_grade_venv(venv: &Path) -> Result<(), String> {
    if venv.join("bin/activate").exists() {
        return Ok(());
    }
    let base = venv
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(venv);
    let cmd = TestCommand {
        program: PytestRunner::python_program(),
        args: vec![
            "-m".to_string(),
            "venv".to_string(),
            "--system-site-packages".to_string(),
            venv.display().to_string(),
        ],
        cwd: Cwd::Tree,
        env_set: Vec::new(),
        env_remove: Vec::new(),
        launcher: None,
        timeout_secs: None,
        collect: Vec::new(),
    };
    let runs = execute_all(base, base, &[cmd]).map_err(|e| e.to_string())?;
    if runs.first().map(|r| r.exit_code) != Some(0) {
        return Err("venv create failed".into());
    }
    if venv.join("bin/activate").exists() {
        Ok(())
    } else {
        Err("venv create failed".into())
    }
}

/// Build the fork's extension into the venv (no build isolation: no network).
/// Argv comes from `bridge.build`; the grade-venv binding (VIRTUAL_ENV/PATH)
/// is applied here because the bridge describes hermetic invocations only.
pub(crate) fn build_ext(worktree: &Path, venv: &Path) -> Result<String, String> {
    let bridge = MaturinBridge;
    let cx = BuildCtx { tree: worktree, build_dir: worktree, release: false };
    let mut cmds = bridge.build(&cx);
    for c in &mut cmds {
        c.env_set.push(("VIRTUAL_ENV".to_string(), venv.display().to_string()));
        c.env_set.push(("PATH".to_string(), venv_bin_path_prepend(venv)));
    }
    let runs = execute_all(worktree, worktree, &cmds).map_err(|e| e.to_string())?;
    let mut log = String::new();
    for r in &runs {
        log.push_str(&r.stdout);
        log.push_str(&r.stderr);
    }
    if runs.iter().any(|r| r.exit_code != 0) {
        return Err(format!("maturin develop failed:\n{log}"));
    }
    Ok(log)
}

/// Spine predicate on frozen `build.languages`: single-Python (or a missing
/// list, the pre-rollout shape) keeps the exact HEAD grade path; anything
/// else grades through the CMake/CTest spine. Same rule as recon's
/// `is_python_spine` and the `select_composite` spine choice.
fn is_python_spine_langs(languages: &[String]) -> bool {
    languages.is_empty() || *languages == vec!["python".to_string()]
}

/// Out-of-source build dir for a CMake worktree: configure/build outputs
/// never pollute the git tree (the fork `.gitignore` does not exclude them).
pub(crate) fn cmake_build_dir(tree: &Path) -> PathBuf {
    tree.join("build")
}

/// Worker-contract path for the unit's built Rust staticlib: the worker
/// builds the scaffold crate and places the archive here; `substitute`
/// splices it in place of the unit objects. A missing archive halts honestly
/// (the stub path stops here; nothing merges).
pub(crate) fn expected_rust_lib(build_dir: &Path, unit: &str) -> PathBuf {
    build_dir.join("rust").join(format!(
        "lib{}.a",
        rustsmith_adapters::scaffold_crate_name(crate::units::unit_stem(unit))
    ))
}

/// CONTRACT (FileApiSlice, `rustsmith-adapters::write_file_api_query`):
/// create the CMake File API query for the `client-rustsmith` client — a
/// `codemodel-v2` request at `<build>/.cmake/api/v1/query/client-rustsmith/`
/// `query.json` — so CMake answers at configure time under
/// `.cmake/api/v1/reply/`. This private fallback keeps the identical on-disk
/// behavior until this worktree sees that helper; the integrator swaps the
/// body below for `rustsmith_adapters::write_file_api_query(build_dir)` 1:1
/// (same path, same JSON, same `Result<PathBuf, String>`). Zero adapter
/// edits by design (sibling-owned file).
fn request_cmake_file_api(build_dir: &Path) -> Result<PathBuf, String> {
    let dir = build_dir.join(".cmake/api/v1/query/client-rustsmith");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("query.json");
    let body = serde_json::json!({"requests": [{"kind": "codemodel", "version": 2}]});
    let text = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Configure a CMake worktree out-of-source (fail-fast with the log).
/// Argv comes from `CmakeBridge::prepare` with a worktree-anchored context;
/// the frozen manifest `prepare` stays the audit record. The File API query
/// goes down first so every configured build carries a codemodel reply for
/// merge-time target resolution; a query failure propagates (never a silent
/// unconfigured reply).
pub(crate) fn cmake_configure(tree: &Path) -> Result<String, String> {
    let build_dir = cmake_build_dir(tree);
    request_cmake_file_api(&build_dir)?;
    let cx = BuildCtx { tree, build_dir: &build_dir, release: false };
    let cmds = CmakeBridge.prepare(&cx);
    let runs = execute_all(tree, &build_dir, &cmds).map_err(|e| e.to_string())?;
    let mut log = String::new();
    for r in &runs {
        log.push_str(&r.stdout);
        log.push_str(&r.stderr);
    }
    if runs.iter().any(|r| r.exit_code != 0) {
        return Err(format!("cmake configure failed:\n{log}"));
    }
    Ok(log)
}

/// Full build of a configured CMake tree (fail-fast with the log). Per-unit
/// grading builds pristine objects first so the splice overwrites real
/// outputs; the post-splice rebuild inside `substitute` is then incremental.
/// The pristine side of the differential is the shared per-run build (see
/// `build_shared_pristine`); per-unit orig builds are gone.
pub(crate) fn cmake_build(tree: &Path, build_dir: &Path) -> Result<String, String> {
    let cx = BuildCtx { tree, build_dir, release: false };
    let cmds = CmakeBridge.build(&cx);
    let runs = execute_all(tree, build_dir, &cmds).map_err(|e| e.to_string())?;
    let mut log = String::new();
    for r in &runs {
        log.push_str(&r.stdout);
        log.push_str(&r.stderr);
    }
    if runs.iter().any(|r| r.exit_code != 0) {
        return Err(format!("cmake build failed:\n{log}"));
    }
    Ok(log)
}

/// Shared pristine build: stage the original ONCE per mirror run at the
/// deterministic ignored path `<fork>/orig_src` and configure + build it
/// there, returning its build dir for every unit's differential to reuse.
/// Per-unit orig builds (configure + build per unit) are gone: at Elmer
/// scale two full builds per unit is prohibitive, while one shared build is
/// O(1) per run. The stage path is git-ignored by the fork `.gitignore`
/// (`orig_src/`) and skipped by the target-list scan like the old per-unit
/// stage, so it never enters a merge commit; the caller removes it after the
/// unit loop (early-error exits leave it behind, ignored and documented).
/// Emits one `pristine_build` store event per run — the audit proof that the
/// build ran once (the fixture run log asserts exactly one).
fn build_shared_pristine(
    fork: &Path,
    orig_src: &Path,
    store: &Store,
    run_id: &str,
) -> Result<PathBuf, String> {
    let stage = fork.join("orig_src");
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    copy_tree(orig_src, &stage)?;
    cmake_configure(&stage)?;
    let build = cmake_build_dir(&stage);
    cmake_build(&stage, &build)?;
    ev(
        store,
        run_id,
        "pristine_build",
        serde_json::json!({"build": build.display().to_string()}),
    );
    Ok(build)
}

/// Run the frozen oracle invocation against a CMake build dir and grade it
/// with the runner. No venv: `ctest` runs the built tree in place.
pub(crate) fn run_ctest_oracle(
    tree: &Path,
    build_dir: &Path,
    manifest: &rustsmith_core::Manifest,
) -> Result<rustsmith_core::GradedResult, String> {
    let runner = CtestRunner;
    let cx = BuildCtx { tree, build_dir, release: false };
    let base: Vec<TestCommand> = if manifest.version == 2 && !manifest.invocation.is_empty() {
        manifest.invocation.clone()
    } else {
        runner.invocation(&cx)
    };
    let runs = execute_all(tree, build_dir, &base).map_err(|e| e.to_string())?;
    runner.grade(&runs).map_err(|e| e.to_string())
}

/// Held-out rate through `ctest -R`: the runner's held-out shape run in the
/// build dir, parsed by the runner grade. An empty held-out set matches no
/// tests and fails honestly here (never a silent pass).
pub(crate) fn run_ctest_heldout(
    tree: &Path,
    build_dir: &Path,
    suite: &Path,
) -> Result<f64, String> {
    let runner = CtestRunner;
    let cx = BuildCtx { tree, build_dir, release: false };
    let cmds = runner.heldout(suite, &cx);
    let runs = execute_all(tree, build_dir, &cmds).map_err(|e| e.to_string())?;
    Ok(runner.grade(&runs).map_err(|e| e.to_string())?.pass_rate())
}

/// CTest differential: run frozen probe commands in the original and mirror
/// builds and pair trimmed stdouts. Probes come from package-keyed data
/// (`differential.probes`: program + argv, programs resolved against each
/// build dir); no toolchain literal lives here. Missing probes halt honestly
/// (never a vacuous pass); a nonzero probe run halts with its location.
/// The original side is the shared per-run pristine build (see
/// `build_shared_pristine`): one configure + build per mirror run, reused
/// across units. Per-unit orig builds are gone.
pub fn differential_ctest_pairs(
    package: &str,
    orig_build: &Path,
    mirror_build: &Path,
) -> Result<Vec<(String, String)>, String> {
    let entry = crate::repo_content::entry(package)?;
    let probes = entry["differential"]["probes"].as_array().cloned().unwrap_or_default();
    if probes.is_empty() {
        return Err(format!("no differential probes for package '{package}'"));
    }
    let run_one = |build: &Path, program: &str, args: &[String]| -> Result<String, String> {
        let cmd = TestCommand {
            program: build.join(program).display().to_string(),
            args: args.to_vec(),
            cwd: Cwd::BuildDir,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: Some(600),
            collect: Vec::new(),
        };
        let runs = execute_all(build, build, &[cmd]).map_err(|e| e.to_string())?;
        let run = runs.first().ok_or("differential probe produced no output")?;
        if run.exit_code != 0 {
            return Err(format!(
                "differential probe {program} failed in {}:\n{}{}",
                build.display(),
                run.stdout,
                run.stderr
            ));
        }
        Ok(output_value(&run.stdout))
    };
    let mut pairs = Vec::new();
    for probe in &probes {
        let program = probe["program"].as_str().unwrap_or("");
        if program.is_empty() {
            return Err(format!("bad differential probe entry for package '{package}'"));
        }
        let args: Vec<String> = probe["args"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|a| a.as_str().map(str::to_string))
            .collect();
        pairs.push((run_one(orig_build, program, &args)?, run_one(mirror_build, program, &args)?));
    }
    Ok(pairs)
}

/// Stub-validation archive: materialize the bridge scaffold for `decl` into
/// `build/rust/<crate>/`, build it with the shared `cargo_build`
/// invocation, and place the archive at the worker-contract path. Scaffold
/// bodies are `unimplemented!()` stubs, so grading downstream fails
/// honestly; this step exists to run the splice/rebuild/grade machinery
/// against truthful inputs, never to fake a port. Units the bridge cannot
/// scaffold halt here (never silently).
pub(crate) fn build_scaffold_stub(
    build_dir: &Path,
    unit: &str,
    decl: &rustsmith_adapters::UnitDecl,
) -> Result<PathBuf, String> {
    let files = CmakeBridge.scaffold(decl).map_err(|e| e.to_string())?;
    let stem = crate::units::unit_stem(unit);
    let crate_name = rustsmith_adapters::scaffold_crate_name(stem);
    if crate_name.is_empty() {
        return Err(format!("unit '{unit}' has no crate name"));
    }
    let dir = build_dir.join("rust").join(&crate_name);
    for (rel, content) in &files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, content).map_err(|e| e.to_string())?;
    }
    let runs =
        execute_all(&dir, &dir, &CmakeBridge::cargo_build()).map_err(|e| e.to_string())?;
    let mut log = String::new();
    for r in &runs {
        log.push_str(&r.stdout);
        log.push_str(&r.stderr);
    }
    if runs.iter().any(|r| r.exit_code != 0) {
        return Err(format!("scaffold crate build failed for unit {unit}:\n{log}"));
    }
    let built = dir
        .join("target/debug")
        .join(format!("lib{crate_name}.a"));
    if !built.is_file() {
        return Err(format!(
            "scaffold crate for unit {unit} built no archive at {}",
            built.display()
        ));
    }
    let dest = expected_rust_lib(build_dir, unit);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&built, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Build a tracked port crate (`rust/<stem>/` in the tree) and place its
/// archive at the worker-contract path. Whole-repo grade rebuilds every
/// merged port from merged sources; per-unit grade uses the worker's own
/// archive (or the stub-validation build) instead.
pub(crate) fn build_port_crate(crate_dir: &Path, dest: &Path) -> Result<(), String> {
    let runs =
        execute_all(crate_dir, crate_dir, &CmakeBridge::cargo_build()).map_err(|e| e.to_string())?;
    let mut log = String::new();
    for r in &runs {
        log.push_str(&r.stdout);
        log.push_str(&r.stderr);
    }
    if runs.iter().any(|r| r.exit_code != 0) {
        return Err(format!(
            "port crate build failed in {}:\n{log}",
            crate_dir.display()
        ));
    }
    let name = crate_dir
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("port crate dir has no name: {}", crate_dir.display()))?;
    let built = crate_dir.join("target/debug").join(format!("lib{name}.a"));
    if !built.is_file() {
        return Err(format!(
            "port crate build produced no archive at {}",
            built.display()
        ));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&built, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Run the frozen oracle invocation inside the venv with the installed Rust
/// extension as the implementation. Commands are runner-built in both schema
/// versions (native v2 TestCommands verbatim; v1 layout-derived groups, which
/// match the frozen split by construction) and rebound onto the grade venv by
/// the runner (venv interpreter, installed ext wins); grading is
/// `runner.grade`.
pub(crate) fn run_oracle_in_venv(
    venv: &Path,
    worktree: &Path,
    manifest: &rustsmith_core::Manifest,
) -> Result<rustsmith_core::GradedResult, String> {
    let runner = PytestRunner;
    let venv_py = grade_venv_python(venv);
    let cx = BuildCtx { tree: worktree, build_dir: worktree, release: false };
    let base: Vec<TestCommand> = if manifest.version == 2 && !manifest.invocation.is_empty() {
        manifest.invocation.clone()
    } else {
        runner.invocation(&cx)
    };
    let cmds = PytestRunner::bind_venv(&base, &venv_py);
    let runs = execute_all(worktree, worktree, &cmds).map_err(|e| e.to_string())?;
    runner.grade(&runs).map_err(|e| e.to_string())
}

/// Held-out rate inside the venv: the runner's held-out shape rebound onto
/// the grade interpreter, parsed with the shared quiet-output rule.
pub(crate) fn run_heldout_in_venv(
    venv: &Path,
    worktree: &Path,
    suite: &Path,
) -> Result<f64, String> {
    let runner = PytestRunner;
    let cx = BuildCtx { tree: worktree, build_dir: worktree, release: false };
    let cmds = PytestRunner::bind_venv(&runner.heldout(suite, &cx), &grade_venv_python(venv));
    let runs = execute_all(worktree, worktree, &cmds).map_err(|e| e.to_string())?;
    let t = runs
        .iter()
        .map(|r| format!("{}\n{}", r.stdout, r.stderr))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(parse_heldout_rate(&t))
}

/// Value lines of executor-captured stdout. The executor records a `$ <argv>`
/// transcript first line; data parsing (differential pairs, ext paths) must
/// skip it. Pass-through when no transcript is present.
pub(crate) fn output_value(stdout: &str) -> String {
    let mut lines = stdout.lines();
    match lines.next() {
        Some(first) if first.starts_with("$ ") => {
            lines.collect::<Vec<_>>().join("\n").trim().to_string()
        }
        _ => stdout.trim().to_string(),
    }
}
/// Differential: original vs mirror outputs across generated inputs. The probe
/// script + argv lists come from package-keyed data; the pairs are the same
/// shape for every repo (trimmed stdout values, exit codes ignored).
pub fn differential_pairs_for(
    package: &str,
    orig_py: &Path,
    venv_py: &Path,
    worktree: &Path,
) -> Result<Vec<(String, String)>, String> {
    let entry = crate::repo_content::entry(package)?;
    let script = entry["differential"]["script"].as_str().unwrap_or("");
    if script.is_empty() {
        return Err(format!("no differential probe for package '{package}'"));
    }
    let argvs: Vec<Vec<String>> = entry["differential"]["argvs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| {
            a.as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .collect();
    run_probe_pairs(orig_py, venv_py, worktree, script, &argvs)
}
/// One `-c` probe invocation as a `TestCommand`. The interpreter program is
/// the caller's path (system interpreter or venv python); `cwd` does the
/// module resolution, so no `PYTHONPATH` literal appears on this path.
fn probe_command(program: &Path, script: &str, args: &[String]) -> TestCommand {
    let mut cmd_args = vec!["-c".to_string(), script.to_string()];
    cmd_args.extend(args.iter().cloned());
    TestCommand {
        program: program.display().to_string(),
        args: cmd_args,
        cwd: Cwd::Tree,
        env_set: Vec::new(),
        env_remove: Vec::new(),
        launcher: None,
        timeout_secs: None,
        collect: Vec::new(),
    }
}
/// Original vs mirror outputs across probe inputs. The original side runs with
/// `cwd` = staged original sources (the `-c` interpreter puts `cwd` on
/// `sys.path`, replacing the legacy `PYTHONPATH` override); the mirror side
/// runs in the worktree against the installed extension. Only trimmed stdout
/// values are compared, exit codes ignored — identical to the legacy loop.
fn run_probe_pairs(
    orig_py: &Path,
    venv_py: &Path,
    worktree: &Path,
    script: &str,
    inputs: &[Vec<String>],
) -> Result<Vec<(String, String)>, String> {
    let orig_src = worktree.join("orig_src");
    let orig_cmds: Vec<TestCommand> =
        inputs.iter().map(|a| probe_command(orig_py, script, a)).collect();
    let mirror_cmds: Vec<TestCommand> =
        inputs.iter().map(|a| probe_command(venv_py, script, a)).collect();
    let o1 = execute_all(&orig_src, &orig_src, &orig_cmds).map_err(|e| e.to_string())?;
    let o2 = execute_all(worktree, worktree, &mirror_cmds).map_err(|e| e.to_string())?;
    Ok(o1
        .iter()
        .zip(o2.iter())
        .map(|(a, b)| (output_value(&a.stdout), output_value(&b.stdout)))
        .collect())
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("/usr/bin/git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Minimal single-struct report (M6 expands to the full crate; agreement is
/// structural here: all three emitters read the same `Report`).
#[derive(Debug, Clone)]
pub struct Report {
    pub passed: u32,
    pub failed: u32,
    pub divergence: f64,
    pub unsafe_count: usize,
    pub units: Vec<(String, String)>,
    pub decisions: usize,
}

pub fn emit_md(r: &Report) -> String {
    format!(
        "# RUSTSMITH_REPORT\n\n- parity: {}/{} passed\n- divergence: {:.4}\n- unsafe blocks: {}\n- units: {}\n- decisions: {}\n",
        r.passed,
        r.passed + r.failed,
        r.divergence,
        r.unsafe_count,
        r.units.iter().map(|(u, s)| format!("{u}={s}")).collect::<Vec<_>>().join(", "),
        r.decisions
    )
}

pub fn emit_json(r: &Report) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "passed": r.passed, "failed": r.failed, "divergence": r.divergence,
        "unsafe_count": r.unsafe_count, "units": r.units, "decisions": r.decisions,
    }))
    .unwrap()
}

pub fn emit_html(r: &Report) -> String {
    format!(
        "<html><body><h1>rustsmith report</h1><p>parity: {}/{} passed</p><p>divergence: {:.4}</p><p>unsafe blocks: {}</p><p>units: {}</p><p>decisions: {}</p></body></html>",
        r.passed, r.passed + r.failed, r.divergence, r.unsafe_count,
        r.units.iter().map(|(u, s)| format!("{u}={s}")).collect::<Vec<_>>().join(", "),
        r.decisions
    )
}

pub struct MirrorArgs {
    pub repo: PathBuf,
    pub fork: PathBuf,
    pub recon_out: PathBuf,
    pub heldout: PathBuf,
    pub store_path: PathBuf,
    pub run_id: String,
    pub template: PathBuf,
}

/// Scaffold-audit bridge follows the frozen recon spine: the single-Python
/// case keeps `MaturinBridge`; any non-Python language ports through the
/// CMake/CTest spine (`CmakeBridge`). Same rule as `select_composite` spine
/// choice and the grade branching below.
fn scaffold_bridge(languages: &[String]) -> Box<dyn BuildBridge> {
    if languages.iter().any(|l| l != "python") {
        Box::new(CmakeBridge)
    } else {
        Box::new(MaturinBridge)
    }
}

/// Grade-time unit declaration: frozen id + authoritative source plus
/// re-derived exports (same frontends recon used, deterministic). Export
/// re-derivation never halts grading: unparseable files fall back to the
/// stem shape the scaffold derives itself.
fn grade_unit_decl(
    repo: &Path,
    unit: &str,
    rel: &str,
) -> rustsmith_adapters::UnitDecl {
    let exports =
        rustsmith_adapters::fragment_unit_exports(repo, rel).unwrap_or_default();
    rustsmith_adapters::UnitDecl {
        id: rustsmith_adapters::UnitId(unit.to_string()),
        files: vec![rel.to_string()],
        generated_from: None,
        exports,
        imports: Vec::new(),
    }
}

pub fn run_mirror(a: &MirrorArgs, store: &Store) -> Result<Report, String> {
    let run_id = &a.run_id;
    // Load recon artifacts. Unit keys are UnitIds
    // (`<lang>:<repo-rel>[#<symbol>]`); pre-rollout dag.json files with bare
    // stems (`id` or `module`) still read: the scheduler keys on the frozen
    // `id` opaquely and every per-unit lookup goes through the units compat
    // shims. Branch/worktree/bundle names use the sanitized form (`:` is
    // illegal in git refs, `/` nests paths) while the store keeps the UnitId.
    let dag_text = std::fs::read_to_string(a.recon_out.join("dag.json")).map_err(|e| e.to_string())?;
    let dag_json: serde_json::Value =
        serde_json::from_str(&dag_text).map_err(|e| e.to_string())?;
    let order: Vec<String> = serde_json::from_value(dag_json["leaf_first_order"].clone())
        .map_err(|e| e.to_string())?;
    let units_json = dag_json["units"].as_array().cloned().unwrap_or_default();
    let mut depends: HashMap<String, Vec<String>> = HashMap::new();
    // `module` audit stems (new writes) double as the old-reader key: prefer
    // `id`, fall back to `module` for pre-rollout dag files.
    let mut dag_module: HashMap<String, String> = HashMap::new();
    for u in &units_json {
        let id = u["id"]
            .as_str()
            .or_else(|| u["module"].as_str())
            .unwrap_or("")
            .to_string();
        if !id.is_empty() {
            if let Some(m) = u["module"].as_str() {
                dag_module.insert(id.clone(), m.to_string());
            }
            depends.insert(
                id,
                u["depends_on"]
                    .as_array()
                    .map(|v| v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default(),
            );
        }
    }
    // Scheduler works over this DAG: leaf-first order, ready = deps passed.
    let unit_dag = UnitDag {
        units: order
            .iter()
            .map(|id| rustsmith_adapters::Unit {
                id: id.clone(),
                module: dag_module.get(id).cloned().unwrap_or_else(|| crate::units::unit_stem(id).to_string()),
                depends_on: depends.get(id).cloned().unwrap_or_default(),
            })
            .collect(),
        edges: vec![],
    };
    let porting_md =
        std::fs::read_to_string(a.recon_out.join("PORTING.md")).map_err(|e| e.to_string())?;
    let manifest_text =
        std::fs::read_to_string(a.recon_out.join("manifest.json")).map_err(|e| e.to_string())?;
    // v1-compat upgrade: current recon output is v1 (string invocation);
    // native v2 (TestCommand invocation) grades through the same runner.
    let manifest: rustsmith_core::Manifest =
        rustsmith_core::parse_manifest_json(&manifest_text).map_err(|e| e.to_string())?;

    // Package + template: identity comes from the frozen facts.json (written
    // by recon from the repo itself), the template manifest lists every file
    // the port needs. A template for another package is a refused
    // misconfiguration, never a weird run.
    let package = crate::repo::facts_package(&a.recon_out)?;
    let tspec = units::load_template(&a.template)?;
    if tspec.package != package {
        return Err(format!("template package {} does not match repo package {package}", tspec.package));
    }
    let layout_src = crate::repo::is_src_layout(&a.repo, &package);
    if tspec.src_layout != layout_src {
        return Err(format!("template layout does not match repo package {package}"));
    }
    // Keep an orig-src copy for differential grading (src-layout stages `src`,
    // flat stages the whole tree so the package dir resolves via `cwd`).
    let orig_src = if layout_src {
        a.repo.join("src")
    } else {
        a.repo.clone()
    };
    let recon_modules = units::read_recon_modules(&a.recon_out);
    // Init fork from the original (full copy minus .git), run branch.
    if a.fork.exists() {
        std::fs::remove_dir_all(&a.fork).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&a.fork).map_err(|e| e.to_string())?;
    copy_tree_except_git(&a.repo, &a.fork)?;
    git(&a.fork, &["init", "-q", "-b", "main"])?;
    git(&a.fork, &["config", "user.email", "t@t"])?;
    git(&a.fork, &["config", "user.name", "t"])?;
    // Scratch + build outputs must never enter the fork. Port sources live
    // in tracked `rust/<stem>/`; their cargo/cmake outputs stay out.
    std::fs::write(a.fork.join(".gitignore"), "worktree-*\n.bundles/\n.grade-venv/\norig_src/\n__pycache__/\ntarget/\n*.so\nbuild/\nrust/*/target/\n").map_err(|e| e.to_string())?;
    git(&a.fork, &["add", "-A"])?;
    git(&a.fork, &["commit", "-qm", "seed from original"])?;
    let mirror_lang = crate::units::read_recon_build_languages(&a.recon_out)
        .first()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    // Full frozen language list drives the scaffold-audit bridge (single
    // Python keeps MaturinBridge; any other language selects CmakeBridge).
    let build_languages = crate::units::read_recon_build_languages(&a.recon_out);
    // Grade spine: single-Python keeps the exact HEAD venv/maturin/pytest
    // path below; anything else configures + grades through CMake/CTest.
    let python_spine = is_python_spine_langs(&build_languages);
    store
        .create_run(run_id, &a.repo.display().to_string(), &mirror_lang, "mirror")
        .map_err(|e| e.to_string())?;
    ev(store, run_id, "mirror_start", serde_json::json!({"units": order}));
    for id in &order {
        let deps = depends.get(id).cloned().unwrap_or_default();
        store
            .create_unit(
                id,
                run_id,
                "mirror",
                &serde_json::json!({"module": crate::units::unit_stem(id)}).to_string(),
            )
            .map_err(|e| e.to_string())?;
        store
            .set_unit_depends(id, &serde_json::to_string(&deps).unwrap())
            .map_err(|e| e.to_string())?;
    }

    let sandbox = Sandbox::new("containers".into());
    let agent = Agent::new(Some(store.events_path.clone()));
    let venv = a.fork.join(".grade-venv");
    // The grade venv is a Python-spine stage concern; the CTest spine
    // configures each worktree out-of-source at grade time instead.
    if python_spine {
        ensure_grade_venv(&venv)?;
    }
    // Shared pristine build (CTest spine only): the staged original builds
    // ONCE per run and every unit's differential reuses it (see
    // `build_shared_pristine`). Python keeps its per-worktree `orig_src`
    // staging below; an empty DAG needs no pristine side at all.
    let shared_orig_build: Option<PathBuf> = if python_spine || order.is_empty() {
        None
    } else {
        Some(build_shared_pristine(&a.fork, &orig_src, store, run_id)?)
    };

    // Scheduler: leaf-first over the DAG; ready = all deps passed.
    // Parallel cap MAX_PARALLEL bounds independent units (runs serialize here).
    let _ = MAX_PARALLEL;
    for u in &unit_dag.units {
        let id = &u.id;
        // Ready check.
        for d in &u.depends_on {
            let st = store.unit_status(d).map_err(|e| e.to_string())?.unwrap_or_default();
            if st != "passed" {
                return Err(format!("unit {id} scheduled before dep {d} passed (got {st})"));
            }
        }
        store.set_unit_status(id, "running").map_err(|e| e.to_string())?;
        ev(store, run_id, "unit_start", serde_json::json!({"unit": id}));
        // Worktree on branch unit/<sanitized id>: raw UnitIds contain `:` (and
        // `/`), both illegal in git refs. The store keeps the canonical UnitId.
        let wt_name = crate::units::unit_fs_name(id);
        let wt = sandbox
            .alloc_worktree(&a.fork, run_id, &wt_name)
            .map_err(|e| e.to_string())?;
        store
            .set_unit_worktree(id, wt.as_str())
            .map_err(|e| e.to_string())?;
        // Bundle (held-out blind: no held-out input exists on this path).
        // Orig source + task come from the template manifest, never hardcodes.
        let bundle_dir = a.fork.join(format!(".bundles/{wt_name}"));
        let (orig_rel, _) =
            crate::units::unit_sources(&tspec, &recon_modules, id)?;
        let orig_file = a.repo.join(&orig_rel);
        let orig_source =
            std::fs::read_to_string(&orig_file).unwrap_or_else(|_| String::from("// empty"));
        assemble_bundle(
            &bundle_dir,
            id,
            &u.module,
            &depends.get(id).cloned().unwrap_or_default(),
            &porting_md,
            &orig_source,
            "frozen-oracle(read-only)",
        )?;
        // Canonical scaffold audit: the spine-selected bridge is the authority
        // on port scaffolds (MaturinBridge for single-Python, CmakeBridge
        // otherwise), so resolve this unit through `bridge.scaffold()` and
        // freeze the resulting file list next to the bundle. The declaration
        // carries re-derived exports (same frontends recon used); unparseable
        // files fall back to the stem shape, never a grade halt. Units the
        // bridge cannot scaffold fall back to the template compat mapping
        // recorded here; materialization below still follows the template task.
        let bridge = scaffold_bridge(&build_languages);
        let decl = grade_unit_decl(&a.repo, id, &orig_rel);
        let scaffold_note = match bridge.scaffold(&decl) {
            Ok(files) => serde_json::json!({
                "scaffolded": true,
                "files": files.iter().map(|(p, _)| p).collect::<Vec<_>>(),
            }),
            Err(e) => serde_json::json!({
                "scaffolded": false,
                "fallback": "template",
                "reason": e.to_string(),
            }),
        };
        std::fs::write(
            bundle_dir.join("scaffold.json"),
            serde_json::to_string_pretty(&scaffold_note).unwrap(),
        )
        .map_err(|e| e.to_string())?;
        // M8 slice-3: live worker-command turn (additive). Prompt = stable
        // prefix + PORTING.md + unit bundle, written to prompt.txt; worker
        // stdout usage recorded as tokens (never evidence). Files still come
        // from the stub task below, so grading is unchanged offline or live.
        let mut prompt_version = agent.prompt_version.clone();
        if let Some(wcmd) = rustsmith_agent::worker_cmd_from_env() {
            let prompt = rustsmith_agent::build_worker_prompt(id, &porting_md, &orig_source);
            std::fs::write(bundle_dir.join("prompt.txt"), &prompt).map_err(|e| e.to_string())?;
            let wspec = UnitSpec {
                unit_id: id.clone(),
                worktree: wt.clone(),
                task: "worker-command".into(),
                token_ceiling: 1_000_000,
            };
            let w = agent.spawn_worker(&wspec, &prompt, &wcmd).map_err(|e| e.to_string())?;
            store.set_unit_tokens(id, w.tokens_in as i64, w.tokens_out as i64).map_err(|e| e.to_string())?;
            prompt_version = rustsmith_agent::WORKER_PROMPT_VERSION.into();
        }
        // Worker stub task (real subprocess via Agent): materialize the unit,
        // then commit on its branch so the merge carries the files.
        let task = crate::units::unit_task(&tspec, &a.template, id)?;
        let spec = UnitSpec {
            unit_id: id.clone(),
            worktree: wt.clone(),
            task,
            token_ceiling: 1_000_000,
        };
        let h = agent.spawn(&spec).map_err(|e| e.to_string())?;
        let res = agent.wait(h, Duration::from_secs(60)).map_err(|e| e.to_string())?;
        if res.exit_code != 0 {
            let n = store.bump_attempts(id).map_err(|e| e.to_string())?;
            if n >= 3 {
                escalate(store, run_id, id)?;
            }
            return Err(format!("worker for {id} exited {}", res.exit_code));
        }
        store.set_unit_status(id, "gated").map_err(|e| e.to_string())?;
        // Grade in the worktree. Python spine: build the extension into the
        // grade venv, then oracle + heldout + differential through pytest.
        // (Configure via bridge.prepare is empty for maturin.)
        // CTest spine: configure out-of-source, splice the worker's Rust
        // archive in place of the unit objects, rebuild, and grade through
        // ctest. `substitute` refuses a missing archive honestly, so the
        // stub path stops before anything merges.
        let wt_path = wt.as_std_path();
        let (integrity, parity, div, diff_result): (_, _, _, Result<_, String>) = if python_spine {
            build_ext(wt_path, &venv)?;
            let got = run_oracle_in_venv(&venv, wt_path, &manifest)?;
            // Gates.
            let runner = PytestRunner;
            let hashes = Oracle::current_hashes(&manifest, wt_path, &runner);
            // NOTE: worktree pyproject was replaced (maturin) — section-hash covers
            // test config only (ADR-002), so packaging change does not trip tamper.
            let integrity = gates::oracle_integrity(&manifest, &hashes, &manifest.baseline, &got);
            let parity = gates::oracle_parity(&got);
            let rate = rate_of(&got);
            // Held-out suite against the worktree build via the venv runner.
            let held_rate = run_heldout_in_venv(&venv, wt_path, &a.heldout)?;
            let div = gates::heldout_divergence(rate, held_rate, 0.05);
            // Stage orig_src for the differential probes (resolved via cwd).
            std::fs::create_dir_all(wt_path.join("orig_src")).map_err(|e| e.to_string())?;
            copy_tree(&orig_src, &wt_path.join("orig_src"))?;
            let diff_pairs = differential_pairs_for(
                &package,
                &PathBuf::from(PytestRunner::python_program()),
                &grade_venv_python(&venv),
                wt_path,
            )?;
            let _ = std::fs::remove_dir_all(wt_path.join("orig_src"));
            // Exact-equality behind the per-observable tolerance: the default path
            // passes 0.0/None, preserving legacy math byte-for-byte.
            let diff = gates::differential(&diff_pairs, 0.0, None);
            (integrity, parity, div, Ok(diff))
        } else {
            cmake_configure(wt_path)?;
            let build_dir = cmake_build_dir(wt_path);
            // Pristine build first: the splice below overwrites real built
            // objects, and the post-splice rebuild is then incremental.
            cmake_build(wt_path, &build_dir)?;
            let cx = BuildCtx { tree: wt_path, build_dir: &build_dir, release: false };
            // Port archive: the worker's build output when present; without
            // a worker, build the bridge scaffold itself so the splice /
            // rebuild / grade machinery below runs against truthful inputs
            // (scaffold stubs panic, so grading fails honestly; nothing
            // merges). Python path untouched.
            let archive = expected_rust_lib(&build_dir, id);
            if !archive.is_file() {
                build_scaffold_stub(&build_dir, id, &decl)?;
            }
            let sub_cmds = CmakeBridge
                .substitute(&cx, &decl, &archive)
                .map_err(|e| e.to_string())?;
            let runs = execute_all(wt_path, &build_dir, &sub_cmds).map_err(|e| e.to_string())?;
            let mut log = String::new();
            for r in &runs {
                log.push_str(&r.stdout);
                log.push_str(&r.stderr);
            }
            if runs.iter().any(|r| r.exit_code != 0) {
                return Err(format!("substitute failed for unit {id}:\n{log}"));
            }
            let got = run_ctest_oracle(wt_path, &build_dir, &manifest)?;
            let runner = CtestRunner;
            let hashes = Oracle::current_hashes(&manifest, wt_path, &runner);
            let integrity = gates::oracle_integrity(&manifest, &hashes, &manifest.baseline, &got);
            let parity = gates::oracle_parity(&got);
            let rate = rate_of(&got);
            let held_rate = run_ctest_heldout(wt_path, &build_dir, &a.heldout)?;
            let div = gates::heldout_divergence(rate, held_rate, 0.05);
            // Differential against the shared pristine build (configured +
            // built once per run above, reused across units). Probes are
            // package-keyed data; missing probes halt honestly above.
            let orig_build = shared_orig_build.as_deref().ok_or(
                "non-Python spine needs the shared pristine build",
            )?;
            let diff_pairs = differential_ctest_pairs(&package, orig_build, &build_dir)?;
            let diff = gates::differential(&diff_pairs, 0.0, None);
            (integrity, parity, div, Ok(diff))
        };
        for (g, v) in [
            (Gate::OracleIntegrity, &integrity),
            (Gate::OracleParity, &parity),
            (Gate::HeldoutDivergence, &div),
        ] {
            let mut detail = v.detail.clone();
            detail["prompt_version"] = serde_json::json!(prompt_version);
            store
                .record_gate(id, g, v.passed, &detail)
                .map_err(|e| e.to_string())?;
        }
        // Differential resolves last: the CTest spine halts here honestly
        // (recorded gates above stay as evidence). Python missing probes
        // already halted inside its arm, as before.
        let diff = diff_result?;
        ev(
            store,
            run_id,
            "gate",
            serde_json::json!({"unit": id, "integrity": integrity.passed, "parity": parity.passed, "divergence": div.passed, "differential": diff.passed}),
        );
        // Tamper is an immediate run halt (SPEC 10.3), not a unit retry.
        if !integrity.passed {
            let reason = format!("oracle_tamper unit {id}: {}", integrity.detail);
            store.set_halt(run_id, &reason).map_err(|e| e.to_string())?;
            ev(store, run_id, "tamper", serde_json::json!({"unit": id, "reason": reason}));
            return Err(reason);
        }
        if !(parity.passed && div.passed && diff.passed) {
            let n = store.bump_attempts(id).map_err(|e| e.to_string())?;
            if n >= 3 {
                escalate(store, run_id, id)?;
            }
            return Err(format!("unit {id} failed gates"));
        }
        // Review: two seats, never the implementer (worker => any two seats).
        let providers = default_providers();
        let (r1, r2) = assign_reviewers(None, &providers);
        record_review(store, run_id, id, &format!("unit {id} diff"), r1, r2)?;
        // Merge + delete mirrored module in the SAME commit (targets from template).
        let (_, deletes) = crate::units::unit_sources(&tspec, &recon_modules, id)?;
        merge_unit(&a.fork, wt.as_str(), id, &deletes)?;
        let sha = git(&a.fork, &["rev-parse", "HEAD"])?;
        store.set_unit_commit(id, &sha).map_err(|e| e.to_string())?;
        store.set_unit_status(id, "passed").map_err(|e| e.to_string())?;
        ev(store, run_id, "merge", serde_json::json!({"unit": id, "sha": sha}));
        let _ = sandbox.drop_worktree(&a.fork, wt.as_std_path());
    }
    // The shared pristine stage served every differential above; remove it
    // before the whole-repo grade so the fork holds only merged sources
    // (it was git-ignored throughout, never committed).
    let _ = std::fs::remove_dir_all(a.fork.join("orig_src"));

    // Whole-repo grade on the run branch (rebuild from merged sources first).
    // Python spine: venv build + pytest oracle. CTest spine: configure +
    // full build out-of-source, then the ctest oracle. Integrity/parity and
    // the held-out divergence halt below are spine-agnostic.
    let (got, integrity, parity) = if python_spine {
        build_ext(&a.fork, &venv)?;
        let got = run_oracle_in_venv(&venv, &a.fork, &manifest)?;
        let hashes = Oracle::current_hashes(&manifest, &a.fork, &PytestRunner);
        let integrity = gates::oracle_integrity(&manifest, &hashes, &manifest.baseline, &got);
        let parity = gates::oracle_parity(&got);
        (got, integrity, parity)
    } else {
        // Port archives first: the merged CMakeLists links them by
        // worker-contract path, so they must exist before configuring.
        // Then configure + full build (links archives in), then the oracle.
        // No object splice here: merged sources are gone from their targets,
        // so there are no unit objects to overwrite (per-unit worktrees keep
        // the ld -r path, where sources still exist).
        let build_dir = cmake_build_dir(&a.fork);
        let merged = store.list_units(run_id).map_err(|e| e.to_string())?;
        for (uid, status, _) in &merged {
            if status != "passed" {
                continue;
            }
            let stem = crate::units::unit_stem(uid);
            let crate_dir = a.fork.join("rust").join(rustsmith_adapters::scaffold_crate_name(stem));
            build_port_crate(&crate_dir, &expected_rust_lib(&build_dir, uid))?;
        }
        cmake_configure(&a.fork)?;
        cmake_build(&a.fork, &build_dir)?;
        let got = run_ctest_oracle(&a.fork, &build_dir, &manifest)?;
        let runner = CtestRunner;
        let hashes = Oracle::current_hashes(&manifest, &a.fork, &runner);
        let integrity = gates::oracle_integrity(&manifest, &hashes, &manifest.baseline, &got);
        let parity = gates::oracle_parity(&got);
        (got, integrity, parity)
    };
    if !integrity.passed {
        let reason = format!("oracle_tamper whole-repo: {}", integrity.detail);
        store.set_halt(run_id, &reason).map_err(|e| e.to_string())?;
        ev(store, run_id, "tamper", serde_json::json!({"reason": reason}));
        return Err(reason);
    }
    if !parity.passed {
        return Err("whole-repo grade failed".into());
    }
    // Held-out suite through the spine runner (ctest `-R` on the CTest
    // spine; an empty held-out set matches nothing and fails honestly).
    let held_rate_all = if python_spine {
        run_heldout_in_venv(&venv, &a.fork, &a.heldout)?
    } else {
        run_ctest_heldout(&a.fork, &cmake_build_dir(&a.fork), &a.heldout)?
    };
    let whole_div = rate_of(&got) - held_rate_all;
    // Held-out divergence over threshold halts the run (SPEC 10.3).
    let wdiv = gates::heldout_divergence(rate_of(&got), held_rate_all, 0.05);
    if !wdiv.passed {
        let reason = format!("heldout_divergence whole-repo: {}", wdiv.detail);
        store.set_halt(run_id, &reason).map_err(|e| e.to_string())?;
        ev(
            store,
            run_id,
            "heldout_divergence",
            serde_json::json!({"reason": reason, "visible": rate_of(&got)}),
        );
        return Err(reason);
    }
    // Unsafe audit: count + SAFETY + FFI-boundary (0 expected on the mirror).
    let unsafe_sites = audit_unsafe(&a.fork)?;
    let uv = gates::unsafe_budget(&unsafe_sites, 5.0, 10);
    let units = store.list_units(run_id).map_err(|e| e.to_string())?;
    let decisions = store.count_decisions(run_id).map_err(|e| e.to_string())? as usize;
    let report = Report {
        passed: got.passed,
        failed: got.failed,
        divergence: whole_div,
        unsafe_count: unsafe_sites.len(),
        units: units.iter().map(|(u, s, _)| (u.clone(), s.clone())).collect(),
        decisions,
    };
    std::fs::write(a.fork.join("RUSTSMITH_REPORT.md"), emit_md(&report))
        .map_err(|e| e.to_string())?;
    std::fs::write(
        a.fork.join("rustsmith-report.json"),
        emit_json(&report),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        a.fork.join("rustsmith-report.html"),
        emit_html(&report),
    )
    .map_err(|e| e.to_string())?;
    git(&a.fork, &["add", "-A"])?;
    git(&a.fork, &["commit", "-qm", "reports"])?;
    ev(
        store,
        run_id,
        "mirror_done",
        serde_json::json!({"passed": got.passed, "unsafe": unsafe_sites.len(), "clippy": "see-acceptance"}),
    );
    let _ = uv;
    Ok(report)
}

fn escalate(store: &Store, run_id: &str, unit_id: &str) -> Result<(), String> {
    use std::collections::HashMap as Map;
    let mut d: Map<Seat, Box<dyn SeatDriver>> = Map::new();
    for s in [Seat::Architect, Seat::Verifier, Seat::Performance, Seat::Scope] {
        d.insert(
            s,
            Box::new(StubDriver {
                stance: Stance::Approve,
                reasoning: "replan".into(),
            }),
        );
    }
    let c = Council::new(d);
    let _ = c
        .escalate_replan(store, run_id, unit_id, &["gate failed 3x".into()], None)
        .map_err(|e| e.to_string())?;
    store
        .set_unit_status(unit_id, "parked")
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn record_review(
    store: &Store,
    run_id: &str,
    unit_id: &str,
    diff: &str,
    r1: Seat,
    r2: Seat,
) -> Result<(), String> {
    use std::collections::HashMap as Map;
    // Implementer (worker) never reviews; two seats on different providers, diff-only.
    let mut d: Map<Seat, Box<dyn SeatDriver>> = Map::new();
    for s in [Seat::Architect, Seat::Verifier, Seat::Performance, Seat::Scope] {
        d.insert(
            s,
            Box::new(StubDriver {
                stance: Stance::Approve,
                reasoning: format!("{} approves diff-only review of {unit_id}", s.as_str()),
            }),
        );
    }
    let c = Council::new(d);
    // Third critic distinct from both reviewers (protocol needs two blind critics).
    let third = [Seat::Architect, Seat::Scope, Seat::Verifier, Seat::Performance]
        .into_iter()
        .find(|s| *s != r1 && *s != r2)
        .unwrap_or(Seat::Scope);
    let res = c
        .decide(
            store,
            run_id,
            Proposal {
                question: format!("review {unit_id}"),
                artifact_ref: diff.into(),
                proposer: r1,
                reasoning: "diff-only review".into(),
            },
            diff.as_bytes(),
            (r2, third),
        )
        .map_err(|e| e.to_string())?;
    let _ = res;
    Ok(())
}

/// CONTRACT (FileApiSlice,
/// `rustsmith-adapters::file_api_target_for_source`): owning File API target
/// for a repo-rel source — exact repo-rel match over target sources first,
/// then a basename fallback (same file name, different directory); first
/// sorted target wins each pass; `None` when no target lists the source.
/// Identical semantics (never stems, extensions, or fuzzy matching); the
/// integrator swaps the body for that helper 1:1. Zero adapter edits by
/// design (sibling-owned file).
fn lookup_file_api_target(targets: &[rustsmith_adapters::FileApiTarget], rel: &str) -> Option<String> {
    let mut exact: Vec<&str> = targets
        .iter()
        .filter(|t| t.sources.iter().any(|s| s.path == rel))
        .map(|t| t.name.as_str())
        .collect();
    exact.sort();
    if let Some(first) = exact.into_iter().next() {
        return Some(first.to_string());
    }
    let base = rel.rsplit('/').next().unwrap_or(rel);
    let mut fallback: Vec<&str> = targets
        .iter()
        .filter(|t| {
            t.sources.iter().any(|s| s.path.rsplit('/').next().unwrap_or(&s.path) == base)
        })
        .map(|t| t.name.as_str())
        .collect();
    fallback.sort();
    fallback.into_iter().next().map(String::from)
}

/// File API owner for `rel` from this tree's own configure reply
/// (`<tree>/build/.cmake/api/v1/reply`, present because `cmake_configure`
/// drops the query first). `None` when the tree was never configured, the
/// reply is unparsable, or no target lists the source — all three fall back
/// to the token heuristic, never a halt.
fn file_api_owner(tree: &Path, rel: &str) -> Option<String> {
    let reply = cmake_build_dir(tree).join(".cmake/api/v1/reply");
    let targets = rustsmith_adapters::parse_cmake_file_api_reply(&reply, tree).ok()?;
    lookup_file_api_target(&targets, rel)
}

/// Remove merged-away sources from CMake target lists so the fork stays
/// buildable after every merge commit. Returns one `(deleted rel, list
/// file, owning target)` per removal for link splicing. The owning target
/// resolves File-API-first: the tree's own codemodel reply (see
/// `file_api_owner`) knows the owner even when the source reaches its target
/// through a variable (`set()`/`list()`) with no `add_*` opener above the
/// token — that unblocks exactly the variable-list halt below. Absent or
/// unmapped replies fall back to the token heuristic. Scans
/// `**/CMakeLists.txt` for the deleted file (exact repo-rel token, else
/// basename) and removes the token (identifier boundaries, quotes
/// tolerated). No CMakeLists anywhere = not a CMake tree (Python-spine
/// no-op). Zero or several matches, or no owning target from either path,
/// halt honestly.
/// `target_link_libraries` is untouched here; the caller splices it.
pub(crate) fn cmake_remove_sources(
    tree: &Path,
    deleted: &[String],
) -> Result<Vec<(String, PathBuf, String)>, String> {
    // Scratch dirs never own target lists: worktrees (which nest under the
    // fork), bundles, and build outputs all carry CMakeLists copies or none
    // at all. Same exclusion discipline as the probe walk and fork .gitignore.
    let lists: Vec<PathBuf> = walkdir_simple(tree)
        .into_iter()
        .filter(|p| {
            !p.components().any(|c| {
                let s = c.as_os_str().to_str().unwrap_or("");
                s == ".git"
                    || s == ".bundles"
                    || s == "build"
                    || s == "target"
                    || s == ".grade-venv"
                    || s == "orig_src"
                    || s.starts_with("worktree-")
            })
        })
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("CMakeLists.txt"))
        .collect();
    if lists.is_empty() {
        return Ok(Vec::new());
    }
    let mut edits = Vec::new();
    for rel in deleted {
        // File API first (see `file_api_owner`): authoritative even for
        // variable-fed targets. The token edit below stays form-exact either
        // way; only the owner source changes.
        let file_api_target = file_api_owner(tree, rel);
        let base = rel.rsplit('/').next().unwrap_or(rel);
        let mut done: Option<(PathBuf, String)> = None;
        // Exact repo-rel token first, then basename (subdir-relative lists).
        for form in [rel.as_str(), base] {
            let mut total = 0;
            let mut site: Option<(PathBuf, String)> = None;
            for list in &lists {
                let text = std::fs::read_to_string(list).map_err(|e| e.to_string())?;
                let c = count_token(&text, form);
                if c > 0 {
                    total += c;
                    site = Some((list.clone(), text));
                }
            }
            if total == 1 {
                let (path, text) = site.unwrap_or_default();
                let target = match file_api_target.clone() {
                    Some(t) => t,
                    None => owning_target(&text, form).ok_or_else(|| {
                        format!("cannot resolve owning target for deleted '{rel}' (source lists in variables need File API resolution)")
                    })?,
                };
                std::fs::write(&path, remove_token(&text, form)).map_err(|e| e.to_string())?;
                done = Some((path, target));
                break;
            }
            if total > 1 {
                return Err(format!(
                    "ambiguous CMakeLists match for deleted '{rel}' (form '{form}')"
                ));
            }
        }
        match done {
            Some((path, target)) => edits.push((rel.clone(), path, target)),
            None => {
                return Err(match file_api_target {
                    Some(t) => format!(
                        "deleted '{rel}' (File API owner '{t}') matches no CMakeLists source entry"
                    ),
                    None => format!(
                        "deleted '{rel}' matches no CMakeLists source entry"
                    ),
                })
            }
        }
    }
    Ok(edits)
}

/// Owning CMake target for a source token: nearest preceding
/// `add_library(` / `add_executable(` opener and its first argument.
/// Variable/glob source lists (`${SRCS}`) carry no filename token, so the
/// caller already failed to match there; a token with no opener above it is
/// refused the same way (never guessed).
fn owning_target(lists_text: &str, token: &str) -> Option<String> {
    let (s, _) = token_hits(lists_text, token).into_iter().next()?;
    let upto: Vec<&str> = lists_text[..s].lines().collect();
    for line in upto.iter().rev() {
        let t = line.trim_start();
        let lower = t.to_ascii_lowercase();
        if !(lower.starts_with("add_library(") || lower.starts_with("add_executable(")) {
            continue;
        }
        // First argument on the opener line; keep the original case from `t`.
        let after = &t[t.find('(').unwrap_or(0) + 1..];
        let name = after
            .split(|c: char| c.is_whitespace() || c == ')')
            .find(|w| !w.is_empty())?;
        return Some(name.trim_matches('"').to_string());
    }
    None
}
/// Identifier boundary for token matching: quotes, parens, whitespace and
/// list separators count; alphanumerics, `_`, `.`, `/`, `-`, `+` don't (so
/// `add.F90` never matches inside `old_add.F90`).
fn is_token_boundary(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(c) => !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-' | '+')),
    }
}

/// Bounded token occurrences in `text`.
fn token_hits(text: &str, token: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if token.is_empty() {
        return out;
    }
    let mut from = 0;
    while let Some(i) = text[from..].find(token) {
        let s = from + i;
        let e = s + token.len();
        if is_token_boundary(text[..s].chars().next_back())
            && is_token_boundary(text[e..].chars().next())
        {
            out.push((s, e));
        }
        from = e;
    }
    out
}

/// Token occurrences with identifier boundaries (see [`is_token_boundary`]).
fn count_token(text: &str, token: &str) -> usize {
    token_hits(text, token).len()
}

/// Remove the single token occurrence (caller verified exactly one via
/// [`count_token`], so the bounded hit below always exists).
fn remove_token(text: &str, token: &str) -> String {
    let Some((mut s, mut e)) = token_hits(text, token).into_iter().next() else {
        return text.to_string();
    };
    // Back up over an opening quote the removal would strand, and drop its
    // closer too (`"x"` vanishes entirely instead of leaving `""`).
    let bytes = text.as_bytes();
    if s > 0 && bytes[s - 1] == b'"' && text[e..].starts_with('"') {
        s -= 1;
        e += 1;
    }
    let mut out = text[..s].to_string();
    out.push_str(&text[e..]);
    out
}


fn merge_unit(fork: &Path, _worktree: &str, unit_id: &str, deletes: &[String]) -> Result<(), String> {
    // Merge worker branch with --no-commit, delete the mirrored
    // original-language modules (template manifest), drop the deleted
    // sources from CMake target lists (each owning target gains the shared
    // empty TU plus its port archive link), then commit ONCE: merge +
    // deletion + build edit land in the same commit so the fork stays
    // buildable and drift surfaces now.
    // Branch names are sanitized (UnitIds contain `:`/`/`, illegal in git
    // refs); this matches the `alloc_worktree` name in `run_mirror`.
    let wt_branch = format!("unit/{}", crate::units::unit_fs_name(unit_id));
    git(fork, &["merge", "--no-commit", "--no-ff", &wt_branch])?;
    for t in deletes {
        if fork.join(t).exists() {
            git(fork, &["rm", "-q", t])?;
        }
    }
    // The fork must stay buildable: a CMake tree keeps referencing deleted
    // sources until they leave the target lists, and each owning target
    // gains the port archive in their place. Non-CMake trees no-op here.
    // Archive paths are worker-contract conventional; whole-repo grade
    // builds them before configuring, so they exist when CMake globs them.
    for (rel, list, target) in cmake_remove_sources(fork, deletes)? {
        let stem = rustsmith_adapters::scaffold_crate_name(crate::units::unit_stem(&rel));
        let archive = fork
            .join("build/rust")
            .join(format!("lib{stem}.a"))
            .display()
            .to_string();
        let mut text = std::fs::read_to_string(&list).map_err(|e| e.to_string())?;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("# rustsmith: port of {rel} (merged unit)\n"));
        if !text.contains("rustsmith_empty.c") {
            text.push_str(&format!(
                "target_sources({target} PRIVATE rustsmith_empty.c)\n"
            ));
        }
        text.push_str(&format!(
            "target_link_libraries({target} PRIVATE {archive})\n"
        ));
        std::fs::write(&list, text).map_err(|e| e.to_string())?;
        // An emptied target is a CMake error (`No SOURCES given`), so every
        // edited list dir gains the shared empty TU (uniform: no emptiness
        // detection, negligible cost, committed with the merge).
        let stub = list.parent().unwrap_or(fork).join("rustsmith_empty.c");
        if !stub.is_file() {
            std::fs::write(&stub, "/* rustsmith: empty TU keeping ported targets non-empty. */\n")
                .map_err(|e| e.to_string())?;
        }
    }
    git(fork, &["add", "-A"])?;
    git(fork, &["commit", "-qm", &format!("merge {unit_id} + delete mirrored module")])?;
    Ok(())
}
fn rate_of(gr: &rustsmith_core::GradedResult) -> f64 {
    gr.pass_rate()
}

fn default_providers() -> HashMap<Seat, String> {
    // Matches config/default.toml (swappable; never hardcoded elsewhere).
    HashMap::from([
        (Seat::Architect, "provider-a".into()),
        (Seat::Verifier, "provider-b".into()),
        (Seat::Performance, "provider-c".into()),
        (Seat::Scope, "provider-d".into()),
    ])
}

pub(crate) fn parse_heldout_rate(t: &str) -> f64 {
    // Parse "N passed" / failures from quiet pytest output.
    let (mut p, mut f) = (0u32, 0u32);
    for line in t.lines() {
        let l = line.to_lowercase();
        let toks: Vec<&str> = l
            .split(|c: char| c == ',' || c == ' ')
            .filter(|x| !x.is_empty())
            .collect();
        let mut i = 0;
        while i < toks.len() {
            if let Ok(n) = toks[i].parse::<u32>() {
                if i + 1 < toks.len() {
                    match toks[i + 1] {
                        w if w.starts_with("passed") => p = p.max(n),
                        w if w.starts_with("failed") => f = f.max(n),
                        _ => {}
                    }
                }
            }
            i += 1;
        }
    }
    let t = p + f;
    if t == 0 {
        1.0
    } else {
        p as f64 / t as f64
    }
}

pub(crate) fn audit_unsafe(fork: &Path) -> Result<Vec<gates::UnsafeSite>, String> {
    // `cargo geiger` cross-check would go here; the mirror ships zero unsafe,
    // so a textual audit plus geiger-if-present is the honest check.
    let mut count = 0usize;
    for entry in walkdir_simple(&fork.join("src")) {
        if entry.extension().map(|x| x == "rs").unwrap_or(false) {
            let t = std::fs::read_to_string(&entry).map_err(|e| e.to_string())?;
            // Strip comments? No: any `unsafe` token counts (conservative).
            for line in t.lines() {
                let s = line.trim();
                if s.starts_with("//") {
                    continue;
                }
                if s.contains("unsafe") {
                    count += 1;
                }
            }
        }
    }
    if count > 0 {
        return Err(format!("{count} unsafe tokens found"));
    }
    // Also try cargo-geiger when installed (informational).
    let _ = std::process::Command::new("cargo")
        .args(["geiger", "--manifest-path", &fork.join("Cargo.toml").display().to_string()])
        .output();
    Ok(vec![])
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn copy_tree_except_git(src: &Path, dst: &Path) -> Result<(), String> {
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        if e.file_name() == ".git" {
            continue;
        }
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            std::fs::create_dir_all(&t).map_err(|e| e.to_string())?;
            copy_tree_except_git(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn walkdir_simple(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    stack.push(e.path());
                }
            }
        } else {
            out.push(p);
        }
    }
    out
}

mod tests {
    use super::*;

    #[test]
    fn reviewers_never_include_implementer_and_span_providers() {
        let p = default_providers();
        for imp in [None, Some(Seat::Verifier), Some(Seat::Architect)] {
            let (a, b) = assign_reviewers(imp, &p);
            assert_ne!(a, b);
            if let Some(i) = imp {
                assert_ne!(a, i);
                assert_ne!(b, i);
            }
            assert_ne!(p[&a], p[&b]);
        }
    }

    #[test]
    fn bundle_has_no_heldout_channel() {
        let dir = tempfile::tempdir().unwrap();
        let b = assemble_bundle(
            &dir.path().join("b"),
            "u1",
            "_crc",
            &[],
            "PORTING",
            "source",
            "oracle-path",
        )
        .unwrap();
        let mut text = String::new();
        for e in std::fs::read_dir(&b.dir).unwrap().flatten() {
            text.push_str(&std::fs::read_to_string(e.path()).unwrap());
        }
        assert!(!text.contains("heldout_tests_never_here"));
        assert!(text.contains("mirror-v1"));
    }
    #[test]
    fn grade_routing_rebinds_onto_venv_python() {
        use rustsmith_adapters::select_composite;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pkgmod.py"), "VALUE = 1\n").unwrap();
        let (composite, _) = select_composite(dir.path()).unwrap();
        let cx = BuildCtx {
            tree: dir.path(),
            build_dir: dir.path(),
            release: false,
        };
        // Routing originates from the runner: interpreter literal owned by
        // `PytestRunner::python_program`, argv opens with `-m pytest`.
        let invocation = composite.runner.invocation(&cx);
        assert!(!invocation.is_empty(), "runner must freeze an invocation");
        for cmd in &invocation {
            assert_eq!(cmd.program, PytestRunner::python_program());
            assert!(
                cmd.args.len() >= 2 && cmd.args[0] == "-m" && cmd.args[1] == "pytest",
                "oracle invocation must be `-m pytest ...`, got {:?}",
                cmd.args
            );
        }
        // Grade rebinding swaps only the interpreter onto the venv python.
        let venv = tempfile::tempdir().unwrap();
        let venv_py = grade_venv_python(venv.path());
        assert!(
            venv_py.ends_with("bin/python"),
            "grade interpreter must be the venv python"
        );
        let rebound = PytestRunner::bind_venv(&invocation, &venv_py);
        assert_eq!(rebound.len(), invocation.len());
        for (orig, bound) in invocation.iter().zip(rebound.iter()) {
            assert_eq!(
                bound.program,
                venv_py.to_string_lossy().into_owned(),
                "rebound command must run under the venv interpreter"
            );
            assert_eq!(bound.args, orig.args, "rebinding keeps argv");
            assert_eq!(bound.cwd, orig.cwd, "rebinding keeps cwd");
            assert!(
                !bound.env_set.iter().any(|(k, _)| k == "PYTHONPATH"),
                "installed extension must win: no PYTHONPATH survives"
            );
        }
    }

    #[test]
    fn differential_none_tols_preserves_exact_equality() {
        // Mirror + optimize grade with `tolerance = 0.0, tols = None`: any
        // textual drift fails (no silent float slop on exact outputs).
        let drift = vec![("1.000".to_string(), "1.005".to_string())];
        assert!(
            !gates::differential(&drift, 0.0, None).passed,
            "0.0/None must reject 1.000 vs 1.005"
        );
        let identical = vec![("1.000".to_string(), "1.000".to_string())];
        assert!(gates::differential(&identical, 0.0, None).passed);
    }

    #[test]
    fn scaffold_bridge_follows_frozen_spine() {
        // Single-Python keeps the maturin audit shape (pyo3/cdylib).
        let py = scaffold_bridge(&["python".to_string()]);
        let decl = crate::units::unit_decl_for_scaffold("python:src/pkg/mod.py", "src/pkg/mod.py");
        let files = py.scaffold(&decl).unwrap();
        let lib = files.iter().find(|(p, _)| p == "src/lib.rs").unwrap().1.clone();
        assert!(lib.contains("#[pymodule]"), "python spine lost maturin shape: {lib}");
        // Any non-Python language selects the CMake audit shape
        // (staticlib + port_ fns), e.g. Elmer's frozen languages.
        let cmake = scaffold_bridge(&["cxx".to_string(), "fortran".to_string(), "python".to_string()]);
        let decl = crate::units::unit_decl_for_scaffold("fortran:fem/src/foo.F90", "fem/src/foo.F90");
        let files = cmake.scaffold(&decl).unwrap();
        let lib = files.iter().find(|(p, _)| p == "src/lib.rs").unwrap().1.clone();
        assert!(lib.contains("staticlib") || lib.contains("port_"), "cmake spine lost staticlib shape: {lib}");
        assert!(!lib.contains("#[pymodule]"), "cmake spine must not emit pyo3: {lib}");
    }

    #[test]
    fn grade_spine_predicate_matches_recon_rule() {
        assert!(is_python_spine_langs(&["python".to_string()]));
        // Pre-rollout shape (missing list) keeps HEAD behavior.
        assert!(is_python_spine_langs(&[]));
        assert!(!is_python_spine_langs(&[
            "cxx".to_string(),
            "fortran".to_string(),
            "python".to_string()
        ]));
        assert!(!is_python_spine_langs(&["fortran".to_string()]));
    }

    #[test]
    fn cmake_worktree_paths_are_out_of_source() {
        let tree = Path::new("/wt");
        assert_eq!(cmake_build_dir(tree), PathBuf::from("/wt/build"));
        // Worker-contract archive path is deterministic per unit.
        assert_eq!(
            expected_rust_lib(&cmake_build_dir(tree), "fortran:fem/src/solver.F90"),
            PathBuf::from("/wt/build/rust/libsolver.a")
        );
        // … and matches the scaffold's sanitized crate name, not the raw stem.
        assert_eq!(
            expected_rust_lib(&cmake_build_dir(tree), "fortran:fem/src/DefUtils.F90"),
            PathBuf::from("/wt/build/rust/libdefutils.a")
        );
    }

    #[test]
    fn ctest_differential_refuses_missing_probes() {
        // Unknown packages halt (never a vacuous pass) …
        let err =
            differential_ctest_pairs("no-such-pkg", Path::new("/o"), Path::new("/m"))
                .unwrap_err();
        assert!(err.contains("no-such-pkg"), "unexpected: {err}");
        // … as do entries without a probes list (python-shaped data).
        let err =
            differential_ctest_pairs("crc", Path::new("/o"), Path::new("/m")).unwrap_err();
        assert!(err.contains("no differential probes"), "unexpected: {err}");
    }

    fn write_lists(dir: &std::path::Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
    }

    #[test]
    fn cmake_remove_sources_edits_exact_token_only() {
        let dir = tempfile::tempdir().unwrap();
        write_lists(
            dir.path(),
            "CMakeLists.txt",
            "add_library(add STATIC\n  src/mini_add.F90\n  src/old_add.F90\n)\n",
        );
        write_lists(
            dir.path(),
            "sub/CMakeLists.txt",
            "add_executable(tool \"tool_main.c\")\n",
        );
        // Exact repo-rel token removed; lookalike `old_add.F90` untouched.
        cmake_remove_sources(dir.path(), &["src/mini_add.F90".to_string()]).unwrap();
        let top = std::fs::read_to_string(dir.path().join("CMakeLists.txt")).unwrap();
        assert!(!top.contains("mini_add"), "token not removed: {top}");
        assert!(top.contains("src/old_add.F90"), "lookalike damaged: {top}");
        // Basename fallback for subdir-relative lists.
        cmake_remove_sources(dir.path(), &["sub/tool_main.c".to_string()]).unwrap();
        let sub = std::fs::read_to_string(dir.path().join("sub/CMakeLists.txt")).unwrap();
        assert!(!sub.contains("tool_main"), "basename not removed: {sub}");
        // No CMakeLists anywhere: not a CMake tree, silent no-op.
        let plain = tempfile::tempdir().unwrap();
        cmake_remove_sources(plain.path(), &["a/b.c".to_string()]).unwrap();
        // Present tree, absent source: honest halt. Twice-matched: halt.
        let err = cmake_remove_sources(dir.path(), &["src/gone.F90".to_string()]).unwrap_err();
        assert!(err.contains("matches no CMakeLists"), "unexpected: {err}");
        write_lists(dir.path(), "other/CMakeLists.txt", "add_library(x src/dup.c)\n");
        write_lists(dir.path(), "CMakeLists.txt", "add_library(y src/dup.c)\n");
        let err = cmake_remove_sources(dir.path(), &["src/dup.c".to_string()]).unwrap_err();
        assert!(err.contains("ambiguous"), "unexpected: {err}");
    }

    #[test]
    fn cmake_remove_sources_ignores_nested_worktrees() {
        // Worktrees nest under the fork with their own CMakeLists copies;
        // only the top tree owns target lists.
        let dir = tempfile::tempdir().unwrap();
        write_lists(
            dir.path(),
            "CMakeLists.txt",
            "add_library(add STATIC src/mini_add.F90)\n",
        );
        write_lists(
            dir.path(),
            "worktree-u1/CMakeLists.txt",
            "add_library(add STATIC src/mini_add.F90)\n",
        );
        write_lists(
            dir.path(),
            "build/CMakeLists.txt",
            "add_library(add STATIC src/mini_add.F90)\n",
        );
        cmake_remove_sources(dir.path(), &["src/mini_add.F90".to_string()]).unwrap();
        let top = std::fs::read_to_string(dir.path().join("CMakeLists.txt")).unwrap();
        assert!(!top.contains("mini_add"), "top token not removed: {top}");
        let nested =
            std::fs::read_to_string(dir.path().join("worktree-u1/CMakeLists.txt")).unwrap();
        assert!(nested.contains("mini_add"), "nested copy touched: {nested}");
    }

    #[test]
    fn cmake_configure_requests_file_api() {
        // The query goes down before prepare so the reply exists for merge
        // time: minimal C project, real configure, then the query + reply.
        let dir = tempfile::tempdir().unwrap();
        write_lists(
            dir.path(),
            "CMakeLists.txt",
            "cmake_minimum_required(VERSION 3.16)\nproject(qtest C)\n",
        );
        cmake_configure(dir.path()).unwrap();
        let query = dir
            .path()
            .join("build/.cmake/api/v1/query/client-rustsmith/query.json");
        assert!(query.is_file(), "query not written: {}", query.display());
        let body: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&query).unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"requests": [{"kind": "codemodel", "version": 2}]}),
        );
        let replies: Vec<_> = std::fs::read_dir(dir.path().join("build/.cmake/api/v1/reply"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("index-")
            })
            .collect();
        assert!(!replies.is_empty(), "configure answered no File API reply");
    }

    fn file_api_target(name: &str, paths: &[&str]) -> rustsmith_adapters::FileApiTarget {
        rustsmith_adapters::FileApiTarget {
            name: name.to_string(),
            sources: paths
                .iter()
                .map(|p| rustsmith_adapters::FileApiSource {
                    language: "c".to_string(),
                    path: p.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn file_api_lookup_exact_then_basename_sorted_first() {
        let targets = vec![
            file_api_target("zeta", &["src/var.c"]),
            file_api_target("alpha", &["src/var.c", "src/other.c"]),
            file_api_target("mid", &["other/var.c"]),
        ];
        // Exact repo-rel match wins over the basename fallback, first sorted.
        assert_eq!(
            lookup_file_api_target(&targets, "src/var.c"),
            Some("alpha".to_string()),
        );
        // No exact match: basename fallback, first sorted target wins.
        assert_eq!(
            lookup_file_api_target(&targets, "elsewhere/var.c"),
            Some("alpha".to_string()),
        );
        assert_eq!(lookup_file_api_target(&targets, "src/gone.c"), None);
        assert_eq!(lookup_file_api_target(&[], "src/var.c"), None);
    }

    fn write_reply(dir: &std::path::Path, target: &str, abs_source: &str) {
        let reply = dir.join("build/.cmake/api/v1/reply");
        std::fs::create_dir_all(&reply).unwrap();
        std::fs::write(
            reply.join("index-0.json"),
            r#"{"objects": [{"kind": "target", "jsonFile": "target-0.json"}]}"#,
        )
        .unwrap();
        std::fs::write(
            reply.join("target-0.json"),
            serde_json::json!({
                "name": target,
                "sources": [{"path": abs_source}],
                "compileGroups": [{"language": "C", "sourceIndexes": [0]}],
            })
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn cmake_remove_sources_prefers_file_api_for_variable_lists() {
        // Variable-fed target (`set()` + `${VAR}`, no `add_*` opener above
        // the token): the token heuristic halts, the File API reply resolves.
        let dir = tempfile::tempdir().unwrap();
        write_lists(
            dir.path(),
            "CMakeLists.txt",
            "cmake_minimum_required(VERSION 3.16)\nproject(vartest C)\nset(SRCS src/var.c)\nadd_library(var_owner STATIC ${SRCS})\n",
        );
        write_lists(dir.path(), "src/var.c", "int var_fn(void) { return 1; }\n");
        let abs = dir.path().join("src/var.c").display().to_string();
        write_reply(dir.path(), "var_owner", &abs);
        let edits = cmake_remove_sources(dir.path(), &["src/var.c".to_string()]).unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].0, "src/var.c");
        assert_eq!(edits[0].2, "var_owner");
        let text = std::fs::read_to_string(dir.path().join("CMakeLists.txt")).unwrap();
        assert!(!text.contains("var.c"), "token not removed: {text}");
        assert!(text.contains("add_library(var_owner"), "owner damaged: {text}");
        // Same tree without the reply: the heuristic halt survives (fallback,
        // never a guess).
        std::fs::remove_dir_all(dir.path().join("build")).unwrap();
        write_lists(
            dir.path(),
            "CMakeLists.txt",
            "cmake_minimum_required(VERSION 3.16)\nproject(vartest C)\nset(SRCS src/var.c)\nadd_library(var_owner STATIC ${SRCS})\n",
        );
        let err = cmake_remove_sources(dir.path(), &["src/var.c".to_string()]).unwrap_err();
        assert!(err.contains("cannot resolve owning target"), "unexpected: {err}");
    }

    #[test]
    fn shared_pristine_build_stages_once_per_call() {
        // One helper call = one staged original + one configured + built
        // tree at the deterministic ignored path, with the audit event.
        let repo = tempfile::tempdir().unwrap();
        write_lists(
            repo.path(),
            "CMakeLists.txt",
            "cmake_minimum_required(VERSION 3.16)\nproject(sharedtest C)\nadd_library(sharedtest STATIC src/a.c)\n",
        );
        write_lists(repo.path(), "src/a.c", "int a_fn(void) { return 41; }\n");
        let fork = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let store = Store::open(&store_dir.path().join("store.db")).unwrap();
        let build = build_shared_pristine(fork.path(), repo.path(), &store, "r1").unwrap();
        assert_eq!(build, fork.path().join("orig_src/build"));
        assert!(
            build.join("CMakeCache.txt").is_file(),
            "shared build never configured"
        );
        assert!(
            fork.path().join("orig_src/src/a.c").is_file(),
            "original not staged"
        );
        let events = std::fs::read_to_string(&store.events_path).unwrap();
        assert_eq!(
            events.lines().filter(|l| l.contains("pristine_build")).count(),
            1,
            "one call must emit exactly one pristine_build event:\n{events}"
        );
        // Deterministic restage: a second call rebuilds the same path.
        let build2 = build_shared_pristine(fork.path(), repo.path(), &store, "r1").unwrap();
        assert_eq!(build, build2);
    }
}
