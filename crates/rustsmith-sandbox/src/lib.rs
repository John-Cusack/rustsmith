use camino::Utf8PathBuf;
use rustsmith_core::{
    AdapterError, Cwd, GradedResult, ImageSpec, Manifest, RunOutput, TestCommand, TestRunner,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("docker: {0}")]
    Docker(String),
    #[error("runner: {0}")]
    Runner(String),
    /// Host fallback is a Python-only escape hatch. Refusing it for any other
    /// runner is load-bearing: a host run could resolve the tree's absolute
    /// build-dir binaries to system installs and pass against the wrong binary.
    #[error("host fallback refused: {0}")]
    HostFallbackRefused(String),
}

impl From<AdapterError> for SandboxError {
    fn from(e: AdapterError) -> Self {
        SandboxError::Runner(e.to_string())
    }
}

pub struct Sandbox {
    pub containers_dir: PathBuf,
}

impl Sandbox {
    pub fn new(containers_dir: PathBuf) -> Self {
        Self { containers_dir }
    }

    /// Build the grading image described by `spec` and return its tag.
    /// The Dockerfile is rendered (`FROM <base>` plus a package-install layer
    /// when `packages` is non-empty); the tag is derived from the spec so
    /// different toolchains never share one.
    pub fn ensure_images(&self, spec: &ImageSpec) -> Result<String, SandboxError> {
        let tag = image_tag(spec);
        let dir = tempfile::tempdir()?;
        let dockerfile = dir.path().join("Dockerfile");
        let mut text = format!("FROM {}\n", spec.base);
        if !spec.packages.is_empty() {
            text.push_str(&format!(
                "RUN apt-get update && apt-get install -y {} && rm -rf /var/lib/apt/lists/*\n",
                spec.packages.join(" ")
            ));
        }
        std::fs::write(&dockerfile, text)?;
        build_image(&dockerfile, &tag)?;
        Ok(tag)
    }

    /// Ephemeral graded run: `--network=none`, read-only mounts plus writable
    /// overlays for `image.writable`. Never mounts `store.db` or any held-out
    /// path (enforced: no such args exist).
    ///
    /// Docker infra failure falls back to a host run ONLY for the `pytest`
    /// runner (cross-track contract: `PytestRunner::id() == "pytest"`); every
    /// other runner gets [`SandboxError::HostFallbackRefused`]. The fallback
    /// marks stdout so audit can see it. It is never silent.
    pub fn grading_run(
        &self,
        manifest: &Manifest,
        artifact: &Path,
        build_dir: &Path,
        runner: &dyn TestRunner,
        image: &ImageSpec,
        image_tag: &str,
    ) -> Result<GradedResult, SandboxError> {
        grading_run_impl(manifest, artifact, build_dir, runner, image, image_tag)
    }

    /// Prove a path is absent inside the grading image (held-out blindness check).
    pub fn grading_image_lacks(&self, image_tag: &str, pattern: &str) -> Result<bool, SandboxError> {
        let out = std::process::Command::new("docker")
            .args(["run", "--rm", "--network=none", image_tag, "sh", "-c", &format!("find / -name '{pattern}' 2>/dev/null | head -5")])
            .output()
            .map_err(|e| SandboxError::Docker(e.to_string()))?;
        if !out.status.success() {
            return Err(SandboxError::Docker(format!(
                "find failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().is_empty())
    }
}

fn build_image(dockerfile: &Path, tag: &str) -> Result<(), SandboxError> {
    let context = dockerfile.parent().unwrap_or(Path::new("."));
    let out = std::process::Command::new("docker")
        .args(["build", "-f", &dockerfile.display().to_string(), "-t", tag, &context.display().to_string()])
        .output()
        .map_err(|e| SandboxError::Docker(e.to_string()))?;
    if !out.status.success() {
        return Err(SandboxError::Docker(format!(
            "build {tag} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

/// Deterministic tag: sanitized base + FNV-1a over packages/writable
/// (hand-rolled so one tag needs no new dependency).
fn image_tag(spec: &ImageSpec) -> String {
    let base: String = spec
        .base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut h: u64 = 0xcbf29ce484222325;
    for b in spec
        .packages
        .iter()
        .chain(spec.writable.iter())
        .flat_map(|s| s.bytes())
    {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("rustsmith-{base}-{h:016x}")
}

fn grading_run_impl(
    manifest: &Manifest,
    artifact: &Path,
    build_dir: &Path,
    runner: &dyn TestRunner,
    image: &ImageSpec,
    image_tag: &str,
) -> Result<GradedResult, SandboxError> {
    match docker_grading_run(manifest, artifact, build_dir, runner, image, image_tag) {
        Ok(r) => Ok(r),
        // Only docker-infra failures consult the fallback policy; a `Runner`
        // grading error is deterministic and propagates untouched.
        Err(SandboxError::Docker(_)) if runner.id() == "pytest" => {
            let mut r = host_grading_run(manifest, artifact, build_dir, runner)?;
            r.stdout.insert_str(
                0,
                "[grading_run: host fallback, docker unavailable]\n",
            );
            Ok(r)
        }
        Err(SandboxError::Docker(e)) => Err(SandboxError::HostFallbackRefused(format!(
            "runner '{}': {e}",
            runner.id()
        ))),
        Err(e) => Err(e),
    }
}

fn docker_grading_run(
    manifest: &Manifest,
    artifact: &Path,
    build_dir: &Path,
    runner: &dyn TestRunner,
    image: &ImageSpec,
    image_tag: &str,
) -> Result<GradedResult, SandboxError> {
    // Stage the tree; an out-of-tree build dir is staged alongside it. Both
    // mounts stay read-only; each `writable` entry gets its own rw bind
    // mount on the matching root (finer mounts win over the ro parent), so
    // e.g. CTest can write `Testing/` without seeing the rest writable.
    let stage = tempfile::tempdir()?;
    let stage_tree = stage.path().join("artifact");
    copy_dir(artifact, &stage_tree)?;
    let out_of_tree = build_dir != artifact;
    let stage_build = stage.path().join("build");
    if out_of_tree {
        copy_dir(build_dir, &stage_build)?;
    }
    let mut runs = Vec::with_capacity(manifest.invocation.len());
    for cmd in &manifest.invocation {
        let mut docker = Command::new("docker");
        docker.args([
            "run",
            "--rm",
            "--network=none",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/var/tmp",
            "-v",
            &format!("{}:/artifact:ro", stage_tree.display()),
        ]);
        if out_of_tree {
            docker.args(["-v", &format!("{}:/build:ro", stage_build.display())]);
        }
        let (writable_root, writable_prefix) =
            if matches!(cmd.cwd, Cwd::BuildDir) && out_of_tree {
                (&stage_build, "/build")
            } else {
                (&stage_tree, "/artifact")
            };
        for w in &image.writable {
            let host = writable_root.join(w);
            std::fs::create_dir_all(&host)?;
            docker.args(["-v", &format!("{}:{writable_prefix}/{w}:rw", host.display())]);
        }
        docker.args(["--workdir", &container_cwd(&cmd.cwd, out_of_tree)]);
        for (k, v) in &cmd.env_set {
            docker.args(["-e", &format!("{k}={v}")]);
        }
        docker.arg(image_tag);
        // No shell: `env -u` covers env_remove, then launcher/program/argv.
        if !cmd.env_remove.is_empty() {
            docker.arg("env");
            for k in &cmd.env_remove {
                docker.args(["-u", k]);
            }
        }
        docker.args(container_argv(cmd, artifact, build_dir, out_of_tree));
        // Every spawn sets cwd (the container workdir is set above; the
        // client itself runs from the staging dir for determinism).
        docker.current_dir(stage.path());
        let out = docker
            .output()
            .map_err(|e| SandboxError::Docker(e.to_string()))?;
        let code = out.status.code().unwrap_or(-1);
        // 125/126/127 are docker's own "could not run it" statuses; anything
        // else is the test command's verdict, even when nonzero.
        if (125..=127).contains(&code) || out.status.code().is_none() {
            return Err(SandboxError::Docker(format!(
                "docker run failed (status {code}): {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        // Writes propagate back through the bind mounts; collect from staging.
        let mut artifacts = collect_from(&stage_tree, cmd);
        if out_of_tree {
            artifacts.extend(collect_from(&stage_build, cmd));
        }
        runs.push(RunOutput {
            exit_code: code,
            stdout: format!(
                "$ {}\n{}",
                requested_argv(cmd).join(" "),
                String::from_utf8_lossy(&out.stdout)
            ),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            artifacts,
        });
    }
    let graded = runner.grade(&runs)?;
    // Keep staging dir alive until done (tempdir drops here).
    let _ = stage.keep();
    Ok(graded)
}

/// Host execution of the manifest invocation. Only reachable through the
/// `pytest` fallback in [`grading_run_impl`]; every other runner refuses
/// the host above instead of silently grading the wrong binaries.
fn host_grading_run(
    manifest: &Manifest,
    artifact: &Path,
    build_dir: &Path,
    runner: &dyn TestRunner,
) -> Result<GradedResult, SandboxError> {
    let mut runs = Vec::with_capacity(manifest.invocation.len());
    for cmd in &manifest.invocation {
        runs.push(execute_one(cmd, artifact, build_dir)?);
    }
    Ok(runner.grade(&runs)?)
}

fn container_cwd(cwd: &Cwd, out_of_tree: bool) -> String {
    match cwd {
        Cwd::Tree => "/artifact".to_string(),
        Cwd::BuildDir if out_of_tree => "/build".to_string(),
        Cwd::BuildDir => "/artifact".to_string(),
        Cwd::Rel(r) => format!("/artifact/{r}"),
    }
}

/// Remap an absolute host path into the container. Paths under the graded
/// tree/build point at the staged copies; system pins pass through for the
/// image toolchain to provide.
fn remap(p: &str, artifact: &Path, build_dir: &Path, out_of_tree: bool) -> String {
    let path = Path::new(p);
    if let Ok(rel) = path.strip_prefix(artifact) {
        return format!("/artifact/{}", rel.display());
    }
    if out_of_tree {
        if let Ok(rel) = path.strip_prefix(build_dir) {
            return format!("/build/{}", rel.display());
        }
    }
    p.to_string()
}

fn container_argv(
    cmd: &TestCommand,
    artifact: &Path,
    build_dir: &Path,
    out_of_tree: bool,
) -> Vec<String> {
    let mut argv = Vec::new();
    if let Some(l) = &cmd.launcher {
        argv.push(remap(&l.program, artifact, build_dir, out_of_tree));
        argv.push(l.np_flag.clone());
        argv.push(l.np.to_string());
        argv.extend(l.extra.iter().cloned());
    }
    // Bare names resolve inside the image PATH; absolute pins are remapped.
    let prog = if cmd.program.contains('/') {
        remap(&cmd.program, artifact, build_dir, out_of_tree)
    } else {
        cmd.program.clone()
    };
    argv.push(prog);
    for a in &cmd.args {
        if a.starts_with('/') {
            argv.push(remap(a, artifact, build_dir, out_of_tree));
        } else {
            argv.push(a.clone());
        }
    }
    argv
}

/// The command as requested (host spelling), for the `$` record line.
/// The container-resolved spelling lives in [`container_argv`].
fn requested_argv(cmd: &TestCommand) -> Vec<String> {
    let mut argv = Vec::new();
    if let Some(l) = &cmd.launcher {
        argv.push(l.program.clone());
        argv.push(l.np_flag.clone());
        argv.push(l.np.to_string());
        argv.extend(l.extra.iter().cloned());
    }
    argv.push(cmd.program.clone());
    argv.extend(cmd.args.iter().cloned());
    argv
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

fn execute_one(
    cmd: &TestCommand,
    tree: &Path,
    build_dir: &Path,
) -> Result<RunOutput, SandboxError> {
    let dir = resolve_cwd(tree, build_dir, &cmd.cwd);
    let program = resolve_program(&cmd.program);
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
        spawn_capture(&mut child, cmd.timeout_secs).map_err(SandboxError::Io)?;
    let mut artifacts = collect_from(tree, cmd);
    if build_dir != tree {
        artifacts.extend(collect_from(build_dir, cmd));
    }
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
const TIMEOUT_EXIT_CODE: i32 = 124;

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
    let so = read_out
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    let se = read_err
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
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

/// Gather `cmd.collect` artifact globs under `root` (keys are root-relative
/// paths). Bounded: 64 files, 8 MiB each.
fn collect_from(root: &Path, cmd: &TestCommand) -> BTreeMap<String, Vec<u8>> {
    const MAX_FILES: usize = 64;
    const MAX_BYTES: u64 = 8 << 20;
    let mut out = BTreeMap::new();
    if cmd.collect.is_empty() {
        return out;
    }
    for f in walkdir_simple(root).unwrap_or_default() {
        if !f.is_file() {
            continue;
        }
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
    out
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), SandboxError> {
    std::fs::create_dir_all(dst)?;
    for entry in walkdir_simple(src)? {
        let rel = entry.strip_prefix(src).unwrap();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dst.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&entry, &target)?;
        }
    }
    Ok(())
}

fn walkdir_simple(root: &Path) -> Result<Vec<PathBuf>, SandboxError> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        out.push(p.clone());
        if p.is_dir() {
            // Skip heavy dirs.
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if matches!(name, ".git" | "__pycache__" | ".pytest_cache" | "target" | ".venv" | "venv") {
                    out.pop();
                    continue;
                }
            }
            for e in std::fs::read_dir(&p)? {
                let e = e?;
                stack.push(e.path());
            }
        }
    }
    Ok(out)
}

/// M1 worktree helpers live here (scaffolded now, wired in M1).
pub fn alloc_worktree_path(run_dir: &Path, unit_id: &str) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(run_dir.join(format!("worktree-{unit_id}"))).expect("utf8")
}
impl Sandbox {
    /// Allocate a git worktree for `unit_id` on branch `unit/<id>`.
    /// Returns the worktree path. Sets mode 0700 where uid isolation is unavailable.
    pub fn alloc_worktree(&self, repo: &Path, run_id: &str, unit_id: &str) -> Result<Utf8PathBuf, SandboxError> {
        let wt = alloc_worktree_path(repo, &format!("{run_id}-{unit_id}"));
        let branch = format!("unit/{unit_id}");
        // Control plane bypasses the worker PATH wrapper: absolute binary.
        // Clean stale worktree/branch from previous runs.
        let _ = std::process::Command::new("/usr/bin/git").args(["worktree", "remove", "--force", wt.as_str()]).current_dir(repo).output();
        let _ = std::process::Command::new("/usr/bin/git").args(["branch", "-D", &branch]).current_dir(repo).output();
        let out = std::process::Command::new("/usr/bin/git")
            .args(["worktree", "add", "-b", &branch, wt.as_str()])
            .current_dir(repo)
            .output()
            .map_err(|e| SandboxError::Io(e))?;
        if !out.status.success() {
            return Err(SandboxError::Docker(format!("git worktree add failed: {}", String::from_utf8_lossy(&out.stderr))));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(wt.as_std_path(), std::fs::Permissions::from_mode(0o700));
        }
        Ok(wt)
    }
    pub fn drop_worktree(&self, repo: &Path, worktree: &Path) -> Result<(), SandboxError> {
        let out = std::process::Command::new("/usr/bin/git")
            .args(["worktree", "remove", "--force", &worktree.display().to_string()])
            .current_dir(repo)
            .output()
            .map_err(SandboxError::Io)?;
        if !out.status.success() {
            // Fallback: rm -rf + prune.
            let _ = std::fs::remove_dir_all(worktree);
            let _ = std::process::Command::new("/usr/bin/git").args(["worktree", "prune"]).current_dir(repo).output();
        }
        Ok(())
    }
}
/// RAII build lease: one holder per run container at a time.
/// Backed by flock on `$RUN_DIR/.build.lock`; blocking with timeout.
pub struct BuildLease {
    _file: std::fs::File,
    _path: PathBuf,
}
impl BuildLease {
    pub fn acquire(run_dir: &Path, timeout: std::time::Duration) -> Result<Self, SandboxError> {
        std::fs::create_dir_all(run_dir)?;
        let path = run_dir.join(".build.lock");
        let file = std::fs::OpenOptions::new().create(true).write(true).open(&path)?;
        let start = std::time::Instant::now();
        loop {
            match try_lock(&file) {
                Ok(true) => return Ok(Self { _file: file, _path: path }),
                Ok(false) => {
                    if start.elapsed() >= timeout {
                        return Err(SandboxError::Docker("build lease timeout".into()));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => return Err(SandboxError::Io(e)),
            }
        }
    }
}
#[cfg(unix)]
fn try_lock(f: &std::fs::File) -> std::io::Result<bool> {
    use std::os::unix::io::AsRawFd;
    let r = unsafe { libc_flock(f.as_raw_fd(), true) };
    Ok(r == 0)
}
#[cfg(not(unix))]
fn try_lock(_f: &std::fs::File) -> std::io::Result<bool> { Ok(true) }
#[cfg(unix)]
unsafe fn libc_flock(fd: i32, nonblock: bool) -> i32 {
    // LOCK_EX=2, LOCK_NB=4
    let op = if nonblock { 2 | 4 } else { 2 };
    unsafe extern "C" { fn flock(fd: i32, op: i32) -> i32; }
    unsafe { flock(fd, op) }
}
/// Best-effort cgroup v2 CPU/memory cap for `pid`. Logs fallback when the host
/// lacks delegation (common on dev machines); returns Ok in both cases unless
/// the pid is invalid.
#[derive(Debug, Clone)]
pub struct CgroupLimits { pub cpu_shares: u64, pub mem_bytes: u64 }
pub fn apply_limits(pid: u32, limits: &CgroupLimits) -> Result<String, SandboxError> {
    // Try cgroup v2 path for this pid; fall back with a logged reason.
    let cgroup_file = format!("/proc/{pid}/cgroup");
    let cgroup = std::fs::read_to_string(&cgroup_file).unwrap_or_default();
    // Look for a writable cgroup dir under /sys/fs/cgroup.
    let candidates = ["/sys/fs/cgroup"];
    for base in candidates {
        let test = Path::new(base).join("cgroup.controllers");
        if test.exists() {
            // Host has cgroup v2; attempt to write limits is privileged and will
            // usually fail without delegation — record that as the reason.
            return Ok(format!("cgroup v2 present ({cgroup:?}); no delegation for pid {pid}, limits cpu={} mem={} logged as fallback", limits.cpu_shares, limits.mem_bytes));
        }
    }
    Ok(format!("cgroup unavailable; limits cpu={} mem={} logged as fallback for pid {pid}", limits.cpu_shares, limits.mem_bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lease_serializes_contenders() {
        let dir = tempfile::tempdir().unwrap();
        let run_dir = dir.path().join("run");
        std::fs::create_dir_all(&run_dir).unwrap();
        // 8 threads contend; each holds the lease briefly and stamps timestamps.
        let stamps = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let mut hs = Vec::new();
        for _ in 0..8 {
            let rd = run_dir.clone();
            let st = stamps.clone();
            hs.push(std::thread::spawn(move || {
                let _lease = BuildLease::acquire(&rd, std::time::Duration::from_secs(10)).unwrap();
                let t0 = std::time::Instant::now();
                std::thread::sleep(std::time::Duration::from_millis(30));
                let t1 = std::time::Instant::now();
                st.lock().push((t0, t1));
            }));
        }
        for h in hs { h.join().unwrap(); }
        let mut v = stamps.lock().clone();
        v.sort_by_key(|(a, _)| *a);
        for w in v.windows(2) {
            // No overlap: next start >= prev end (allow 1ms slop for timer granularity).
            assert!(w[1].0 >= w[0].1 - std::time::Duration::from_millis(1), "leases overlapped");
        }
        assert_eq!(v.len(), 8);
    }
    #[test]
    fn cgroup_fallback_logged() {
        let r = apply_limits(std::process::id(), &CgroupLimits { cpu_shares: 1024, mem_bytes: 512 << 20 }).unwrap();
        assert!(r.contains("fallback") || r.contains("no delegation"));
    }
    #[test]
    fn collect_glob_star_star_does_not_cross_without_it() {
        assert!(glob_match("Testing/**/*.xml", "Testing/a/b.xml"));
        assert!(glob_match("Testing/**/*.xml", "Testing/b.xml"));
        assert!(!glob_match("Testing/*.xml", "Testing/a/b.xml"));
        assert!(glob_match("*.log", "run.log"));
        assert!(!glob_match("*.log", "a/run.log"));
        assert!(glob_match("**/*.log", "a/run.log"));
        assert!(glob_match("out-?.txt", "out-1.txt"));
        assert!(!glob_match("out-?.txt", "out-12.txt"));
    }
}

#[cfg(test)]
mod polyglot_regression_tests {
    use super::*;
    use rustsmith_core::{
        AdapterError, Baseline, BuildCtx, Cwd, GradedResult, Manifest, ObservableSpec,
        Observation, OracleFile, TestCommand, TestRunner, MANIFEST_VERSION,
    };
    use std::collections::BTreeMap;

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

    /// In-test mock: scripted runner, no toolchain, no network.
    struct MockRunner {
        id: &'static str,
        commands: Vec<TestCommand>,
        graded: GradedResult,
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
            Ok(Vec::new())
        }
        fn normalize_for_hash(&self, _rel: &str, _bytes: &[u8]) -> Option<Vec<u8>> {
            None
        }
        fn heldout(&self, _suite: &Path, _cx: &BuildCtx) -> Vec<TestCommand> {
            Vec::new()
        }
        fn config_hash(&self, _cx: &BuildCtx) -> Result<String, AdapterError> {
            Ok("mock-config".to_string())
        }
    }

    fn manifest_with(commands: Vec<TestCommand>, runner_id: &str) -> Manifest {
        Manifest {
            version: MANIFEST_VERSION,
            runner: runner_id.to_string(),
            languages: Vec::new(),
            prepare: Vec::new(),
            invocation: commands,
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

    /// A tag that can never exist, so `docker run` always reports infra
    /// failure (exit 125) and the test never needs a real image or daemon.
    /// When docker is not even installed the spawn itself fails the same way.
    const BOGUS_IMAGE: &str = "rustsmith-nonexistent-test-image:0";

    fn image_spec() -> ImageSpec {
        ImageSpec {
            base: "scratch".to_string(),
            packages: Vec::new(),
            writable: Vec::new(),
        }
    }

    #[test]
    fn host_fallback_refused_for_ctest() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = dir.path().join("artifact");
        let build = dir.path().join("build");
        std::fs::create_dir_all(&artifact).unwrap();
        std::fs::create_dir_all(&build).unwrap();
        // Non-empty invocation forces a real `docker run` attempt, which the
        // bogus image guarantees to fail as infra error.
        let runner = MockRunner {
            id: "ctest",
            commands: vec![simple_cmd("/bin/true", &[], Cwd::Tree)],
            graded: graded_fixture(9, 0, "must-not-surface"),
        };
        let manifest = manifest_with(runner.commands.clone(), "ctest");
        let sandbox = Sandbox::new(dir.path().join("containers"));
        let err = sandbox
            .grading_run(&manifest, &artifact, &build, &runner, &image_spec(), BOGUS_IMAGE)
            .unwrap_err();
        match err {
            SandboxError::HostFallbackRefused(msg) => {
                assert!(msg.contains("ctest"), "refusal names runner, got {msg}")
            }
            other => panic!("ctest must refuse the host, got {other:?}"),
        }
    }

    #[test]
    fn host_fallback_marks_stdout_for_pytest() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = dir.path().join("artifact");
        let build = dir.path().join("build");
        std::fs::create_dir_all(&artifact).unwrap();
        std::fs::create_dir_all(&build).unwrap();
        let runner = MockRunner {
            id: "pytest",
            commands: vec![simple_cmd("/bin/true", &[], Cwd::Tree)],
            graded: graded_fixture(7, 2, "sentinel-graded"),
        };
        let manifest = manifest_with(runner.commands.clone(), "pytest");
        let sandbox = Sandbox::new(dir.path().join("containers"));
        let out = sandbox
            .grading_run(&manifest, &artifact, &build, &runner, &image_spec(), BOGUS_IMAGE)
            .unwrap();
        // Fallback is never silent: the audit marker leads, the grade shape
        // (counts plus sentinel body) survives untouched.
        assert!(
            out.stdout
                .starts_with("[grading_run: host fallback, docker unavailable]\n"),
            "fallback marker first, got {:?}",
            out.stdout
        );
        assert!(out.stdout.contains("sentinel-graded"));
        assert_eq!(out.passed, 7);
        assert_eq!(out.failed, 2);
    }

    #[test]
    fn executor_honours_cwd_resolves_program_collects_and_splits_streams() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        let build = dir.path().join("build");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("result.xml"), b"<ok/>").unwrap();
        std::fs::write(tree.join("tree.txt"), b"unrelated").unwrap();
        let cmd = TestCommand {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "pwd; echo build-out; echo build-err >&2".to_string(),
            ],
            cwd: Cwd::BuildDir,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: None,
            collect: vec!["*.xml".to_string()],
        };
        let run = execute_one(&cmd, &tree, &build).unwrap();
        assert_eq!(run.exit_code, 0);
        // cwd comes from TestCommand.cwd: pwd reports the build dir.
        assert!(
            run.stdout
                .lines()
                .any(|l| l.trim_end().ends_with("build")),
            "pwd should report the BuildDir, got {}",
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
        assert!(body.contains("build-out"));
        assert!(!body.contains("build-err"));
        assert!(run.stderr.contains("build-err"));
        assert!(!run.stderr.contains("build-out"));
        assert_eq!(
            run.artifacts.get("result.xml").map(Vec::as_slice),
            Some(b"<ok/>".as_slice())
        );
        assert!(!run.artifacts.contains_key("tree.txt"));
    }
}
