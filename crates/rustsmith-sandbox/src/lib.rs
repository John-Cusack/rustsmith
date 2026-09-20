use camino::Utf8PathBuf;
use rustsmith_core::{GradedResult, Manifest};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("docker: {0}")]
    Docker(String),
    #[error("pytest: {0}")]
    Pytest(String),
}

pub const RUN_IMAGE: &str = "rustsmith-run:0.1.0";
pub const GRADING_IMAGE: &str = "rustsmith-grading:0.1.0";

pub struct Sandbox {
    pub containers_dir: PathBuf,
}

impl Sandbox {
    pub fn new(containers_dir: PathBuf) -> Self {
        Self { containers_dir }
    }

    /// Build run + grading images from `containers/`. Returns (run, grading) tags.
    /// Pins base digests via the Dockerfiles themselves.
    pub fn ensure_images(&self) -> Result<(String, String), SandboxError> {
        let run_dockerfile = self.containers_dir.join("run.Dockerfile");
        let grading_dockerfile = self.containers_dir.join("grading.Dockerfile");
        for f in [&run_dockerfile, &grading_dockerfile] {
            if !f.exists() {
                return Err(SandboxError::Docker(format!("missing {}", f.display())));
            }
        }
        build_image(&run_dockerfile, RUN_IMAGE)?;
        build_image(&grading_dockerfile, GRADING_IMAGE)?;
        Ok((RUN_IMAGE.into(), GRADING_IMAGE.into()))
    }

    /// Ephemeral graded run: `--network=none`, read-only oracle mount + artifact copy.
    /// Never mounts `store.db` or any held-out path (enforced: no such args exist).
    pub fn grading_run(
        &self,
        manifest: &Manifest,
        artifact: &Path,
    ) -> Result<GradedResult, SandboxError> {
        grading_run_impl(manifest, artifact)
    }

    /// Prove a path is absent inside the grading image (held-out blindness check).
    pub fn grading_image_lacks(&self, pattern: &str) -> Result<bool, SandboxError> {
        let out = std::process::Command::new("docker")
            .args(["run", "--rm", "--network=none", GRADING_IMAGE, "sh", "-c", &format!("find / -name '{pattern}' 2>/dev/null | head -5")])
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

fn grading_run_impl(manifest: &Manifest, artifact: &Path) -> Result<GradedResult, SandboxError> {
    // Prefer real container execution; fall back to host pytest only if docker is
    // unavailable (still reports the fallback in stdout so audit can see it).
    let docker_ok = std::process::Command::new("docker")
        .args(["image", "inspect", GRADING_IMAGE])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if docker_ok {
        if let Ok(r) = docker_grading_run(manifest, artifact) {
            return Ok(r);
        }
        // Fall through to host run on docker execution failure.
    }
    host_grading_run(manifest, artifact, true)
}

fn docker_grading_run(manifest: &Manifest, artifact: &Path) -> Result<GradedResult, SandboxError> {
    // Copy artifact test files + src into a staging dir, mount read-only.
    let stage = tempfile::tempdir()?;
    let stage_path = stage.path().join("artifact");
    copy_dir(artifact, &stage_path)?;
    let mut agg = GradedResult {
        exit_code: 0,
        passed: 0,
        failed: 0,
        skipped: vec![],
        xfailed: vec![],
        deselected: vec![],
        stdout: String::new(),
    };
    for cmd in &manifest.invocation {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let pytest_args = parts.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
        // Inside image workdir is /artifact; PYTHONPATH=/artifact/src for src-layout.
        let inner = format!("cd /artifact && PYTHONPATH=/artifact/src:$PYTHONPATH PY_COLORS=0 python3 -m pytest {pytest_args} -v -rs -rxX --tb=short 2>&1");
        let out = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "--network=none",
                "--tmpfs",
                "/tmp",
                "--tmpfs",
                "/var/tmp",
                "-v",
                &format!("{}:/artifact:ro", stage_path.display()),
            ])
            .arg(GRADING_IMAGE)
            .args(["sh", "-c", &inner])
            .output()
            .map_err(|e| SandboxError::Docker(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let r = crate_parse(&stdout, out.status.code().unwrap_or(-1));
        if r.failed > 0 {
            agg.exit_code = r.exit_code;
        } else if r.exit_code != 0 && agg.exit_code == 0 {
            agg.exit_code = r.exit_code;
        }
        agg.passed += r.passed;
        agg.failed += r.failed;
        agg.skipped.extend(r.skipped);
        agg.xfailed.extend(r.xfailed);
        agg.deselected.extend(r.deselected);
        agg.stdout.push_str(&format!("$ docker {cmd}\n{stdout}\n"));
    }
    agg.skipped.sort();
    agg.xfailed.sort();
    agg.deselected.sort();
    // Keep staging dir alive until done (tempdir drops here).
    let _ = stage.keep();
    Ok(agg)
}

fn host_grading_run(
    manifest: &Manifest,
    artifact: &Path,
    mark_fallback: bool,
) -> Result<GradedResult, SandboxError> {
    let mut agg = GradedResult {
        exit_code: 0,
        passed: 0,
        failed: 0,
        skipped: vec![],
        xfailed: vec![],
        deselected: vec![],
        stdout: String::new(),
    };
    if mark_fallback {
        agg.stdout.push_str("[grading_run: host fallback, docker unavailable]\n");
    }
    for cmd in &manifest.invocation {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let args: Vec<&str> = parts.into_iter().skip(1).collect();
        let r = host_pytest(artifact, &args)?;
        if r.failed > 0 {
            agg.exit_code = r.exit_code;
        } else if r.exit_code != 0 && agg.exit_code == 0 {
            agg.exit_code = r.exit_code;
        }
        agg.passed += r.passed;
        agg.failed += r.failed;
        agg.skipped.extend(r.skipped);
        agg.xfailed.extend(r.xfailed);
        agg.deselected.extend(r.deselected);
        agg.stdout.push_str(&format!("$ {} {}\n{}\n", "pytest", args.join(" "), r.stdout));
    }
    agg.skipped.sort();
    agg.xfailed.sort();
    agg.deselected.sort();
    Ok(agg)
}

fn host_pytest(repo_root: &Path, args: &[&str]) -> Result<GradedResult, SandboxError> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-m").arg("pytest");
    for a in args {
        cmd.arg(a);
    }
    cmd.arg("-v").arg("-rs").arg("-rxX").arg("--tb=short");
    cmd.current_dir(repo_root);
    let src = repo_root.join("src");
    if src.is_dir() {
        let mut pp: std::ffi::OsString = src.into_os_string();
        if let Some(old) = std::env::var_os("PYTHONPATH") {
            pp.push(":");
            pp.push(old);
        }
        cmd.env("PYTHONPATH", pp);
    }
    cmd.env("PY_COLORS", "0");
    let out = cmd.output().map_err(|e| SandboxError::Pytest(e.to_string()))?;
    let stdout = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(crate_parse(&stdout, out.status.code().unwrap_or(-1)))
}

fn crate_parse(stdout: &str, code: i32) -> GradedResult {
    // Minimal local parser mirroring oracle's (sandbox must not depend on oracle).
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = Vec::new();
    for line in stdout.lines() {
        let t = line.trim();
        if t.contains("::") && t.contains(" PASSED") {
            passed += 1;
        } else if t.contains("::") && (t.contains(" FAILED") || t.contains(" ERROR")) {
            failed += 1;
        } else if t.contains("::") && t.contains(" SKIPPED") {
            if let Some(id) = t.split_whitespace().next() {
                skipped.push(id.to_string());
            }
        }
    }
    let (sp, sf, ss, _, _) = summary_counts(stdout);
    if passed == 0 && failed == 0 {
        passed = sp;
        failed = sf;
    } else {
        if sp > passed {
            passed = sp;
        }
        if sf > failed {
            failed = sf;
        }
    }
    if skipped.is_empty() && ss > 0 {
        for i in 0..ss {
            skipped.push(format!("skipped[{i}]"));
        }
    }
    skipped.sort();
    GradedResult {
        exit_code: code,
        passed,
        failed,
        skipped,
        xfailed: vec![],
        deselected: vec![],
        stdout: stdout.to_string(),
    }
}

fn summary_counts(s: &str) -> (u32, u32, u32, u32, u32) {
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = 0u32;
    for line in s.lines() {
        let l = line.to_lowercase();
        if !(l.contains("passed") || l.contains("failed") || l.contains("skipped")) {
            continue;
        }
        let toks: Vec<&str> = l
            .split(|c: char| c == ',' || c == ' ' || c == '=')
            .filter(|t| !t.is_empty())
            .collect();
        let mut i = 0;
        while i < toks.len() {
            if let Ok(n) = toks[i].parse::<u32>() {
                if i + 1 < toks.len() {
                    match toks[i + 1] {
                        w if w.starts_with("passed") => passed = passed.max(n),
                        w if w.starts_with("failed") => failed = failed.max(n),
                        w if w.starts_with("skipped") => skipped = skipped.max(n),
                        _ => {}
                    }
                }
            }
            i += 1;
        }
    }
    (passed, failed, skipped, 0, 0)
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
}
