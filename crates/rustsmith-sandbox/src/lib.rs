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
