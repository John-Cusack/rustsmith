//! Frozen-test oracle, routed through the runner (ADR-008 track C).
//!
//! The oracle owns hashing ([`sha256_hex`]) and the freeze/verify/grade
//! plumbing, but every runner-shaped decision lives on `dyn TestRunner`
//! (core trait): file membership (`oracle_files`), per-file normalization for
//! hashing (`normalize_for_hash`, owns ADR-002), the frozen commands
//! (`invocation`), result parsing (`grade`), numeric extraction (`observe`),
//! held-out commands (`heldout`), and config identity (`config_hash`).
//! Nothing here names a language toolchain.

use camino::Utf8PathBuf;
use rustsmith_core::{
    AdapterError, Baseline, BuildCtx, Cwd, FileHash, GradedResult, HaltReason, Manifest,
    Observation, RunOutput, TestCommand, TestRunner, MANIFEST_VERSION,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OracleError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("runner: {0}")]
    Runner(String),
    #[error("halt: {0}")]
    Halt(HaltReason),
    /// Missing or empty held-out suite. A vacuous `passed: 1` here used to let
    /// grading pass without testing anything; that hole is closed.
    #[error("held-out suite missing or empty: {0}")]
    EmptyHeldout(String),
}

impl From<AdapterError> for OracleError {
    fn from(e: AdapterError) -> Self {
        OracleError::Runner(e.to_string())
    }
}

/// Host-only held-out suite. Never mounted into containers.
#[derive(Debug, Clone)]
pub struct HeldoutSuite {
    /// Directory on host containing held-out tests (never copied into grading image).
    pub path: PathBuf,
    pub threshold: f64,
}

impl HeldoutSuite {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            threshold: 0.05,
        }
    }
}

pub struct Oracle;

impl Oracle {
    /// Freeze the oracle through the runner.
    ///
    /// File membership comes from `runner.oracle_files`; each file is hashed
    /// with `runner.normalize_for_hash` applied first (`Some(bytes)` replaces
    /// the file bytes for hashing, `None` hashes the raw bytes). The baseline
    /// runs `runner.invocation` and parses it with `runner.grade`.
    ///
    /// `prepare`/`languages`/`observables` stay empty here: they are owned by
    /// the composite/recon writer (track D), which fills them from the bridge
    /// and RepoFacts.
    pub fn freeze(cx: &BuildCtx, runner: &dyn TestRunner) -> Result<Manifest, OracleError> {
        let mut hashed = Vec::new();
        for f in runner.oracle_files(cx.tree)? {
            let bytes = std::fs::read(cx.tree.join(&f.path))?;
            let effective = runner
                .normalize_for_hash(&f.path, &bytes)
                .unwrap_or(bytes);
            hashed.push(FileHash {
                path: Utf8PathBuf::from(f.path),
                sha256: sha256_hex(&effective),
            });
        }
        hashed.sort_by(|a, b| a.path.cmp(&b.path));
        let invocation = runner.invocation(cx);
        let config_hash = runner.config_hash(cx)?;
        let runs = execute_all(cx.tree, cx.build_dir, &invocation)?;
        let graded = runner.grade(&runs)?;
        let baseline = baseline_of(&graded);
        Ok(Manifest {
            version: MANIFEST_VERSION,
            runner: runner.id().to_string(),
            languages: Vec::new(),
            prepare: Vec::new(),
            invocation,
            config_hash,
            observables: Vec::new(),
            files: hashed,
            baseline,
        })
    }

    /// Mismatch => Halt OracleTamper. Missing => tamper. Extra files in tree are allowed.
    pub fn verify_hashes(
        manifest: &Manifest,
        tree: &Path,
        runner: &dyn TestRunner,
    ) -> Result<(), OracleError> {
        for f in &manifest.files {
            let abs = tree.join(f.path.as_str());
            let bytes = std::fs::read(&abs).map_err(|_| {
                OracleError::Halt(HaltReason::OracleTamper {
                    path: f.path.to_string(),
                })
            })?;
            let effective = runner
                .normalize_for_hash(f.path.as_str(), &bytes)
                .unwrap_or(bytes);
            if sha256_hex(&effective) != f.sha256 {
                return Err(OracleError::Halt(HaltReason::OracleTamper {
                    path: f.path.to_string(),
                }));
            }
        }
        Ok(())
    }

    /// Re-hash the manifest-listed files in `tree`. Missing files are skipped
    /// (verify_hashes reports them as tamper); extras are ignored.
    pub fn current_hashes(
        manifest: &Manifest,
        tree: &Path,
        runner: &dyn TestRunner,
    ) -> Vec<FileHash> {
        let mut out = Vec::new();
        for f in &manifest.files {
            if let Ok(bytes) = std::fs::read(tree.join(f.path.as_str())) {
                let effective = runner
                    .normalize_for_hash(f.path.as_str(), &bytes)
                    .unwrap_or(bytes);
                out.push(FileHash {
                    path: f.path.clone(),
                    sha256: sha256_hex(&effective),
                });
            }
        }
        out
    }

    /// Run the manifest invocation on `tree` WITHOUT hash verification.
    /// The caller evaluates `oracle_integrity` (counts-first) to get the
    /// precise failure reason. Used by `rustsmith grade` and acceptance probes.
    pub fn measure_tree(
        manifest: &Manifest,
        tree: &Path,
        build_dir: &Path,
        runner: &dyn TestRunner,
    ) -> Result<GradedResult, OracleError> {
        let runs = execute_all(tree, build_dir, &manifest.invocation)?;
        Ok(runner.grade(&runs)?)
    }

    /// Extract frozen observables from a tree without grading.
    pub fn observe(
        manifest: &Manifest,
        tree: &Path,
        build_dir: &Path,
        runner: &dyn TestRunner,
    ) -> Result<Vec<Observation>, OracleError> {
        let runs = execute_all(tree, build_dir, &manifest.invocation)?;
        Ok(runner.observe(&runs, &manifest.observables))
    }

    /// Authoritative graded run (host-side; container isolation proved separately by
    /// `rustsmith-sandbox::grading_run`). Returns (visible result, divergence).
    pub fn graded_run(
        manifest: &Manifest,
        artifact: &Path,
        build_dir: &Path,
        heldout: &HeldoutSuite,
        runner: &dyn TestRunner,
    ) -> Result<(GradedResult, f64), OracleError> {
        Self::verify_hashes(manifest, artifact, runner)?;
        let visible = Self::measure_tree(manifest, artifact, build_dir, runner)?;
        let heldout_result = Self::grade_heldout(artifact, build_dir, heldout, runner)?;
        let visible_rate = visible.pass_rate();
        let heldout_rate = heldout_result.pass_rate();
        let divergence = visible_rate - heldout_rate;
        Ok((visible, divergence))
    }

    fn grade_heldout(
        artifact: &Path,
        build_dir: &Path,
        heldout: &HeldoutSuite,
        runner: &dyn TestRunner,
    ) -> Result<GradedResult, OracleError> {
        if !has_any_file(&heldout.path) {
            return Err(OracleError::EmptyHeldout(
                heldout.path.display().to_string(),
            ));
        }
        let cx = BuildCtx {
            tree: artifact,
            build_dir,
            release: false,
        };
        let cmds = runner.heldout(heldout.path.as_path(), &cx);
        if cmds.is_empty() {
            return Err(OracleError::EmptyHeldout(format!(
                "runner '{}' declares no held-out commands for {}",
                runner.id(),
                heldout.path.display()
            )));
        }
        let runs = execute_all(artifact, build_dir, &cmds)?;
        Ok(runner.grade(&runs)?)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn baseline_of(graded: &GradedResult) -> Baseline {
    Baseline {
        test_count: graded.total(),
        skipped: graded.skipped.clone(),
        xfailed: graded.xfailed.clone(),
        deselected: graded.deselected.clone(),
    }
}

pub fn measure_baseline(
    cx: &BuildCtx,
    runner: &dyn TestRunner,
) -> Result<Baseline, OracleError> {
    let runs = execute_all(cx.tree, cx.build_dir, &runner.invocation(cx))?;
    Ok(baseline_of(&runner.grade(&runs)?))
}

/// True when `dir` exists and contains at least one file.
fn has_any_file(dir: &Path) -> bool {
    walk_files(dir).map(|v| !v.is_empty()).unwrap_or(false)
}

fn walk_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        let md = match std::fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if md.is_dir() {
            if p.file_name().map(|n| n == ".git").unwrap_or(false) {
                continue;
            }
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    stack.push(e.path());
                }
            }
        } else if md.is_file() {
            out.push(p);
        }
    }
    Ok(out)
}

fn resolve_cwd(tree: &Path, build_dir: &Path, cwd: &Cwd) -> PathBuf {
    match cwd {
        Cwd::Tree => tree.to_path_buf(),
        Cwd::BuildDir => build_dir.to_path_buf(),
        Cwd::Rel(r) => tree.join(r),
    }
}

/// Resolve `program` to the absolute binary that is actually spawned.
/// Paths containing `/` are used verbatim (the runner pins absolute
/// build-dir binaries there); bare names are looked up on `PATH` left to
/// right, and the resolved absolute path is what gets spawned and recorded.
fn resolve_program(program: &str) -> PathBuf {
    if program.contains('/') {
        return PathBuf::from(program);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let cand = dir.join(program);
            if cand.is_file() {
                return cand;
            }
        }
    }
    PathBuf::from(program)
}

/// Execute every command in order, returning one [`RunOutput`] per command.
/// This is the canonical host executor: every spawn sets `cwd` from the
/// command, bare programs resolve to absolute paths at spawn, the `$` record
/// line carries the absolute program, and `collect` artifacts are gathered.
/// (The sandbox keeps a private mirror so it never depends on this crate.)
pub fn execute_all(
    tree: &Path,
    build_dir: &Path,
    cmds: &[TestCommand],
) -> Result<Vec<RunOutput>, OracleError> {
    let mut runs = Vec::with_capacity(cmds.len());
    for cmd in cmds {
        runs.push(execute_one(cmd, tree, build_dir)?);
    }
    Ok(runs)
}

/// Execute a single [`TestCommand`] on the host. See [`execute_all`].
pub fn execute_one(
    cmd: &TestCommand,
    tree: &Path,
    build_dir: &Path,
) -> Result<RunOutput, OracleError> {
    let dir = resolve_cwd(tree, build_dir, &cmd.cwd);
    let program = resolve_program(&cmd.program);
    // The launcher (e.g. `mpiexec -n 8`) wraps the command so MPI ranks stay
    // plain data hashed into `config_hash`, never shell text.
    let mut argv: Vec<String> = Vec::new();
    if let Some(l) = &cmd.launcher {
        argv.push(l.program.clone());
        argv.push(l.np_flag.clone());
        argv.push(l.np.to_string());
        argv.extend(l.extra.iter().cloned());
    }
    argv.push(program.display().to_string());
    argv.extend(cmd.args.iter().cloned());
    let mut child = Command::new(&argv[0]);
    child.args(&argv[1..]);
    // Every spawn sets cwd via TestCommand.cwd.
    child.current_dir(&dir);
    for (k, v) in &cmd.env_set {
        child.env(k, v);
    }
    for k in &cmd.env_remove {
        child.env_remove(k);
    }
    let (exit_code, so, se) =
        spawn_capture(&mut child, cmd.timeout_secs).map_err(OracleError::Io)?;
    let artifacts = collect_artifacts(tree, build_dir, cmd);
    // The `$` invocation line is recorded here (absolute program) because
    // `grade(&[RunOutput])` never sees the commands; the runner parses what
    // follows and appends stderr itself.
    let mut stdout = format!("$ {}\n", argv.join(" "));
    stdout.push_str(&String::from_utf8_lossy(&so));
    Ok(RunOutput {
        exit_code,
        stdout,
        stderr: String::from_utf8_lossy(&se).into_owned(),
        artifacts,
    })
}

/// Exit code for a command killed after `timeout_secs`.
pub const TIMEOUT_EXIT_CODE: i32 = 124;

fn spawn_capture(
    cmd: &mut Command,
    timeout_secs: Option<u32>,
) -> std::io::Result<(i32, Vec<u8>, Vec<u8>)> {
    use std::io::Read;
    use std::process::Stdio;
    if timeout_secs.is_none() {
        let out = cmd.output()?;
        return Ok((
            out.status.code().unwrap_or(-1),
            out.stdout,
            out.stderr,
        ));
    }
    let limit = Duration::from_secs(u64::from(timeout_secs.unwrap_or(0)));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let read_out = child.stdout.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let read_err = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait()? {
            Some(s) => break s,
            None => {
                if start.elapsed() >= limit {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait()?;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    };
    let so = read_out.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    let se = read_err.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    if timed_out {
        return Ok((TIMEOUT_EXIT_CODE, so, se));
    }
    Ok((status.code().unwrap_or(-1), so, se))
}

/// Match `path` (slash-separated, relative) against a `collect` glob.
/// Supports `*`/`?` within a segment and `**` across segments.
fn glob_match(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let segs: Vec<&str> = path.split('/').collect();
    match_segs(&pat, &segs)
}

fn match_segs(pat: &[&str], segs: &[&str]) -> bool {
    if pat.is_empty() {
        return segs.is_empty();
    }
    if pat[0] == "**" {
        return (0..=segs.len()).any(|i| match_segs(&pat[1..], &segs[i..]));
    }
    if segs.is_empty() || !match_seg(pat[0], segs[0]) {
        return false;
    }
    match_segs(&pat[1..], &segs[1..])
}

fn match_seg(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let c: Vec<char> = s.chars().collect();
    let (mut pi, mut si, mut star, mut mark) = (0usize, 0usize, None, 0usize);
    while si < c.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == c[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Gather `cmd.collect` artifact globs from both roots (build wins on
/// collision; a single root is walked once). Bounded: 64 files, 8 MiB each.
fn collect_artifacts(
    tree: &Path,
    build_dir: &Path,
    cmd: &TestCommand,
) -> BTreeMap<String, Vec<u8>> {
    const MAX_FILES: usize = 64;
    const MAX_BYTES: u64 = 8 << 20;
    let mut out = BTreeMap::new();
    if cmd.collect.is_empty() {
        return out;
    }
    let mut roots = vec![tree];
    if build_dir != tree {
        roots.push(build_dir);
    }
    for root in roots {
        for f in walk_files(root).unwrap_or_default() {
            let rel = match f.strip_prefix(root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            if rel.is_empty() {
                continue;
            }
            if !cmd.collect.iter().any(|p| glob_match(p, &rel)) {
                continue;
            }
            if f.metadata().map(|m| m.len() > MAX_BYTES).unwrap_or(true) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&f) {
                out.insert(rel, bytes);
                if out.len() >= MAX_FILES {
                    return out;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod polyglot_regression_tests {
    use super::*;
    use rustsmith_core::{
        AdapterError, Baseline, BuildCtx, Cwd, GradedResult, Manifest, ObservableSpec,
        Observation, OracleFile, OracleKind, RunOutput, TestCommand, TestRunner,
        MANIFEST_VERSION,
    };
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tmpdir(prefix: &str) -> PathBuf {
        static CTR: AtomicU64 = AtomicU64::new(0);
        let n = CTR.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "rustsmith-oracle-{prefix}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rm(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    fn simple_cmd(program: &str, args: &[&str], cwd: Cwd) -> TestCommand {
        TestCommand {
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: None,
            collect: Vec::new(),
        }
    }

    fn graded_fixture(passed: u32, failed: u32, marker: &str) -> GradedResult {
        GradedResult {
            exit_code: 0,
            passed,
            failed,
            skipped: Vec::new(),
            xfailed: Vec::new(),
            deselected: Vec::new(),
            stdout: marker.to_string(),
            outcomes: BTreeMap::new(),
        }
    }

    /// In-test mock: every runner-shaped decision is scripted, no toolchain.
    struct MockRunner {
        id: &'static str,
        commands: Vec<TestCommand>,
        graded: GradedResult,
        files: Vec<(String, OracleKind)>,
        normalize_to: Option<Vec<u8>>,
        heldout_cmds: Vec<TestCommand>,
        config_hash: String,
    }

    impl MockRunner {
        fn new(id: &'static str) -> Self {
            Self {
                id,
                commands: Vec::new(),
                graded: graded_fixture(0, 0, "mock-graded"),
                files: Vec::new(),
                normalize_to: None,
                heldout_cmds: Vec::new(),
                config_hash: "mock-config".to_string(),
            }
        }
    }

    impl TestRunner for MockRunner {
        fn id(&self) -> &'static str {
            self.id
        }
        fn invocation(&self, _cx: &BuildCtx) -> Vec<TestCommand> {
            self.commands.clone()
        }
        fn grade(&self, _runs: &[RunOutput]) -> Result<GradedResult, AdapterError> {
            Ok(self.graded.clone())
        }
        fn observe(
            &self,
            _runs: &[RunOutput],
            _specs: &[ObservableSpec],
        ) -> Vec<Observation> {
            Vec::new()
        }
        fn oracle_files(&self, _repo: &Path) -> Result<Vec<OracleFile>, AdapterError> {
            Ok(self
                .files
                .iter()
                .map(|(p, k)| OracleFile {
                    path: p.clone(),
                    kind: *k,
                })
                .collect())
        }
        fn normalize_for_hash(&self, _rel: &str, _bytes: &[u8]) -> Option<Vec<u8>> {
            self.normalize_to.clone()
        }
        fn heldout(&self, _suite: &Path, _cx: &BuildCtx) -> Vec<TestCommand> {
            self.heldout_cmds.clone()
        }
        fn config_hash(&self, _cx: &BuildCtx) -> Result<String, AdapterError> {
            Ok(self.config_hash.clone())
        }
    }

    fn empty_manifest(runner_id: &str) -> Manifest {
        Manifest {
            version: MANIFEST_VERSION,
            runner: runner_id.to_string(),
            languages: Vec::new(),
            prepare: Vec::new(),
            invocation: Vec::new(),
            config_hash: "cfg".to_string(),
            observables: Vec::new(),
            files: Vec::new(),
            baseline: Baseline {
                test_count: 0,
                skipped: Vec::new(),
                xfailed: Vec::new(),
                deselected: Vec::new(),
            },
        }
    }

    #[test]
    fn freeze_routes_invocation_grade_and_config_through_runner() {
        for runner_id in ["pytest", "ctest"] {
            let tree = tmpdir("freeze");
            let build = tmpdir("freeze-build");
            std::fs::write(tree.join("a.txt"), b"hello").unwrap();
            let graded = graded_fixture(3, 1, format!("grade-marker-{runner_id}").as_str());
            let mut runner = MockRunner::new(runner_id);
            runner.commands = vec![simple_cmd("/bin/true", &[], Cwd::Tree)];
            runner.graded = graded.clone();
            runner.files = vec![("a.txt".to_string(), OracleKind::Fixture)];
            runner.config_hash = format!("cfg-hash-{runner_id}");
            let cx = BuildCtx {
                tree: &tree,
                build_dir: &build,
                release: false,
            };
            let manifest = Oracle::freeze(&cx, &runner).unwrap();
            assert_eq!(manifest.runner, runner_id);
            assert_eq!(manifest.config_hash, format!("cfg-hash-{runner_id}"));
            assert_eq!(manifest.invocation, runner.commands);
            assert_eq!(manifest.baseline.test_count, graded.total());
            assert_eq!(manifest.files.len(), 1);
            assert_eq!(manifest.files[0].sha256, sha256_hex(b"hello"));
            // The same invocation re-measured grades to the identical shape.
            let measured = Oracle::measure_tree(&manifest, &tree, &build, &runner).unwrap();
            assert_eq!(measured, graded);
            rm(&tree);
            rm(&build);
        }
    }

    #[test]
    fn grade_halts_when_heldout_dir_missing() {
        let artifact = tmpdir("heldout-missing");
        let build = tmpdir("heldout-missing-build");
        let runner = MockRunner::new("pytest");
        let manifest = empty_manifest("pytest");
        let heldout = HeldoutSuite::new(artifact.join("does-not-exist"));
        let err = Oracle::graded_run(&manifest, &artifact, &build, &heldout, &runner)
            .unwrap_err();
        assert!(
            matches!(err, OracleError::EmptyHeldout(_)),
            "missing held-out must halt, got {err:?}"
        );
        rm(&artifact);
        rm(&build);
    }

    #[test]
    fn grade_halts_when_heldout_dir_empty() {
        let artifact = tmpdir("heldout-empty");
        let build = tmpdir("heldout-empty-build");
        let suite = tmpdir("heldout-suite-empty");
        let runner = MockRunner::new("pytest");
        let manifest = empty_manifest("pytest");
        let heldout = HeldoutSuite::new(suite.clone());
        let err =
            Oracle::graded_run(&manifest, &artifact, &build, &heldout, &runner).unwrap_err();
        assert!(
            matches!(err, OracleError::EmptyHeldout(_)),
            "empty held-out dir must halt, got {err:?}"
        );
        rm(&artifact);
        rm(&build);
        rm(&suite);
    }

    #[test]
    fn grade_halts_when_runner_declares_no_heldout_commands() {
        let artifact = tmpdir("heldout-nocmds");
        let build = tmpdir("heldout-nocmds-build");
        let suite = tmpdir("heldout-suite-nocmds");
        std::fs::write(suite.join("test_keep.py"), b"def test_x(): pass\n").unwrap();
        // Files exist but the runner declares no held-out commands: still a halt,
        // never a vacuous pass. Uses the ctest id variant of the mock.
        let mut runner = MockRunner::new("ctest");
        runner.heldout_cmds = Vec::new();
        let manifest = empty_manifest("ctest");
        let heldout = HeldoutSuite::new(suite.clone());
        let err =
            Oracle::graded_run(&manifest, &artifact, &build, &heldout, &runner).unwrap_err();
        match err {
            OracleError::EmptyHeldout(msg) => {
                assert!(msg.contains("ctest"), "held-out error names runner, got {msg}")
            }
            other => panic!("expected EmptyHeldout, got {other:?}"),
        }
        rm(&artifact);
        rm(&build);
        rm(&suite);
    }

    #[test]
    fn executor_honours_cwd_resolves_program_collects_and_splits_streams() {
        let tree = tmpdir("exec");
        let build = tmpdir("exec-build");
        std::fs::create_dir_all(tree.join("sub")).unwrap();
        std::fs::write(tree.join("out.txt"), b"artifact-bytes").unwrap();
        let cmd = TestCommand {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "pwd; echo hello-out; echo hello-err >&2".to_string(),
            ],
            cwd: Cwd::Rel("sub".to_string()),
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: None,
            collect: vec!["*.txt".to_string()],
        };
        let run = execute_one(&cmd, &tree, &build).unwrap();
        assert_eq!(run.exit_code, 0);
        // cwd comes from TestCommand.cwd: pwd reports the subdir.
        assert!(
            run.stdout.lines().any(|l| l.trim_end().ends_with("sub")),
            "pwd should report the Rel(sub) dir, got {}",
            run.stdout
        );
        // Bare program resolves to an absolute path in the `$` record line.
        let header = run.stdout.lines().next().unwrap_or("");
        assert!(header.starts_with("$ "), "record line, got {header:?}");
        assert!(header.contains('/'), "program resolved absolute, got {header:?}");
        assert!(
            !header.starts_with("$ sh "),
            "bare name must not survive, got {header:?}"
        );
        // stdout/stderr stay separate (body only: the `$` record line echoes the
        // command text itself, which names both streams).
        let body = run.stdout.lines().skip(1).collect::<Vec<_>>().join("\n");
        assert!(body.contains("hello-out"));
        assert!(!body.contains("hello-err"));
        assert!(run.stderr.contains("hello-err"));
        assert!(!run.stderr.contains("hello-out"));
        // Collect globs gather matching artifacts.
        assert_eq!(
            run.artifacts.get("out.txt").map(Vec::as_slice),
            Some(b"artifact-bytes".as_slice())
        );
        rm(&tree);
        rm(&build);
    }

    #[test]
    fn freeze_applies_normalize_for_hash() {
        let tree = tmpdir("norm-freeze");
        let build = tmpdir("norm-freeze-build");
        std::fs::write(tree.join("spec.txt"), b"raw-bytes-v1").unwrap();
        let cx = BuildCtx {
            tree: &tree,
            build_dir: &build,
            release: false,
        };
        // None hashes the raw bytes.
        let mut raw = MockRunner::new("pytest");
        raw.commands = vec![simple_cmd("/bin/true", &[], Cwd::Tree)];
        raw.files = vec![("spec.txt".to_string(), OracleKind::Fixture)];
        let manifest = Oracle::freeze(&cx, &raw).unwrap();
        assert_eq!(manifest.files[0].sha256, sha256_hex(b"raw-bytes-v1"));
        // Some replaces the bytes for hashing.
        let mut norm = MockRunner::new("pytest");
        norm.commands = vec![simple_cmd("/bin/true", &[], Cwd::Tree)];
        norm.files = vec![("spec.txt".to_string(), OracleKind::Fixture)];
        norm.normalize_to = Some(b"canonical".to_vec());
        let manifest = Oracle::freeze(&cx, &norm).unwrap();
        assert_eq!(manifest.files[0].sha256, sha256_hex(b"canonical"));
        rm(&tree);
        rm(&build);
    }

    #[test]
    fn verify_applies_normalize_for_hash() {
        let tree = tmpdir("norm-verify");
        let build = tmpdir("norm-verify-build");
        std::fs::write(tree.join("spec.txt"), b"raw-bytes-v1").unwrap();
        let cx = BuildCtx {
            tree: &tree,
            build_dir: &build,
            release: false,
        };
        let mut raw = MockRunner::new("pytest");
        raw.commands = vec![simple_cmd("/bin/true", &[], Cwd::Tree)];
        raw.files = vec![("spec.txt".to_string(), OracleKind::Fixture)];
        let manifest = Oracle::freeze(&cx, &raw).unwrap();
        // Mutating the file is tamper against raw hashing ...
        std::fs::write(tree.join("spec.txt"), b"totally-different").unwrap();
        let err = Oracle::verify_hashes(&manifest, &tree, &raw).unwrap_err();
        assert!(matches!(err, OracleError::Halt(_)), "got {err:?}");
        // ... but invisible when normalize maps every input to one canonical form.
        let mut norm = MockRunner::new("pytest");
        norm.files = vec![("spec.txt".to_string(), OracleKind::Fixture)];
        norm.normalize_to = Some(b"canonical".to_vec());
        let norm_manifest = Oracle::freeze(&cx, &norm).unwrap();
        std::fs::write(tree.join("spec.txt"), b"yet-other-bytes").unwrap();
        Oracle::verify_hashes(&norm_manifest, &tree, &norm).unwrap();
        assert_eq!(
            Oracle::current_hashes(&norm_manifest, &tree, &norm)[0].sha256,
            sha256_hex(b"canonical")
        );
        rm(&tree);
        rm(&build);
    }
}
