//! `release-prep` / `release-record` / `release-status` (converted projects).
//!
//! `release-prep` turns the final accepted source tree into verified,
//! publishable distributions without uploading anything: it builds the Rust
//! core package, the Python wheels, and the source distribution, verifies a
//! Rust consumer plus the installed Python package, proves the sdist rebuilds
//! (including its Rust core), and retains artifacts, source identity, hashes,
//! and verification results in the output directory alongside a generated
//! release workflow and trusted-publisher setup instructions.
//!
//! Source of truth: `--opt` when present (the optimize work tree holds the
//! final accepted merges; the fork is the mirror baseline), else `--fork`.
//! The recorded `source_sha` is that tree's git HEAD, never a guess.

use std::path::{Path, PathBuf};

fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}
fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name || a.starts_with(&format!("{name}=")))
}

/// Durable write: temp file in the same directory + fsync + rename + dir
/// sync, so a crash never leaves a half-written state file behind. Never
/// logs file contents (paths only, on error).
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{}: no parent dir", path.display()))?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = parent.join(format!(
        ".{}.tmp-{}-{nanos}",
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "state".into()),
        std::process::id()
    ));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    // Sync the temp file before it becomes visible under its real name.
    std::fs::File::open(&tmp)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))?;
    // Sync the directory so the rename itself survives a crash.
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Best-effort canonical form for overlap checks: canonicalize when the
/// path exists, else canonicalize the closest existing ancestor and append
/// the remainder. Falls back to the raw path when nothing resolves.
fn canonical_for_compare(p: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(p) {
        return c;
    }
    let mut cur = p;
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        match std::fs::canonicalize(cur) {
            Ok(c) => {
                let mut out = c;
                for comp in tail.iter().rev() {
                    out.push(comp);
                }
                return out;
            }
            Err(_) => match cur.parent() {
                Some(parent) => {
                    if let Some(name) = cur.file_name() {
                        tail.push(name.to_os_string());
                    }
                    cur = parent;
                }
                None => return p.to_path_buf(),
            },
        }
    }
}

fn paths_overlap(a: &Path, b: &Path) -> bool {
    let ca = canonical_for_compare(a);
    let cb = canonical_for_compare(b);
    ca == cb || ca.starts_with(&cb) || cb.starts_with(&ca)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

struct Run {
    ok: bool,
    log: String,
}

fn run_cmd(dir: &Path, prog: &str, args: &[String], extra_env: &[(&str, &str)]) -> Run {
    let mut cmd = std::process::Command::new(prog);
    cmd.current_dir(dir).args(args);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    match cmd.output() {
        Err(e) => Run { ok: false, log: format!("spawn {prog}: {e}") },
        Ok(o) => {
            let mut log = format!("$ {prog} {}\n", args.join(" "));
            log.push_str(&String::from_utf8_lossy(&o.stdout));
            log.push_str(&String::from_utf8_lossy(&o.stderr));
            Run { ok: o.status.success(), log }
        }
    }
}

/// Stage copy exclusions: build outputs, venvs, worktrees, VCS, and prior
/// release outputs never enter the staged release source.
fn copy_stage(src: &Path, dst: &Path) -> Result<(), String> {
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if [
            "target",
            ".git",
            ".github",
            ".bundles",
            ".grade-venv",
            ".opt-venv",
            ".parent-venv",
            ".plant-venv",
            ".audit-merged-venv",
            ".audit-rev-venv",
            "__pycache__",
            "orig_src",
            "orig_src_staged",
            "build",
            "dist",
            ".full-build-tmp",
            ".attribution-revert",
        ]
        .contains(&name.as_str())
            || name.starts_with("worktree-")
            || name.starts_with(".cand-")
            || name.starts_with(".audit-fwd-")
            || name.starts_with("verify-venv")
            || name == "verify-venv"
            || name.ends_with(".so")
            || name.ends_with(".pyc")
        {
            continue;
        }
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            std::fs::create_dir_all(&t).map_err(|e| e.to_string())?;
            copy_stage(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn git_head(tree: &Path) -> Result<String, String> {
    let r = run_cmd(tree, "git", &["rev-parse".into(), "HEAD".into()], &[]);
    if !r.ok {
        return Err(format!(
            "{}: no git HEAD (release needs the final accepted commit; got: {})",
            tree.display(),
            r.log.trim()
        ));
    }
    Ok(r.log.lines().last().unwrap_or("").trim().to_string())
}

fn newest(paths: Vec<PathBuf>) -> Option<PathBuf> {
    let mut v = paths;
    v.sort();
    v.into_iter().last()
}

fn tail(log: &str, n: usize) -> String {
    log.lines().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
}

pub fn cmd_release_prep(args: &[String]) -> Result<(), String> {
    let project = PathBuf::from(flag(args, "--project").ok_or("missing --project")?);
    let fork = PathBuf::from(flag(args, "--fork").ok_or("missing --fork")?);
    let recon_out = PathBuf::from(flag(args, "--recon-out").ok_or("missing --recon-out")?);
    let out = PathBuf::from(flag(args, "--out").ok_or("missing --out")?);
    let opt = flag(args, "--opt").map(PathBuf::from);
    if !project.is_dir() {
        return Err(format!("{}: no such project dir", project.display()));
    }
    if !recon_out.is_dir() {
        return Err(format!("{}: no such --recon-out dir", recon_out.display()));
    }
    // Config first: every later step assumes validated names.
    let cfg = rustsmith_release::load_release_config(&project).map_err(|e| e.to_string())?;
    rustsmith_release::validate_names(&cfg).map_err(|e| e.to_string())?;
    rustsmith_release::validate_tree(&cfg, &project).map_err(|e| e.to_string())?;
    // Final accepted source: the optimize tree when it exists, else the fork.
    let (source, source_kind): (PathBuf, &str) = match opt.as_ref() {
        Some(o) if o.is_dir() => (o.clone(), "opt"),
        _ => {
            if !fork.is_dir() {
                return Err(format!("{}: no such --fork dir (and no --opt)", fork.display()));
            }
            (fork.clone(), "fork")
        }
    };
    let source_sha = git_head(&source)?;
    // Never wipe a tree we read from, and never let --out point at store
    // state: canonical overlap with any input tree is always refused (even
    // with --force). Separately, an --out that already holds release state
    // is refused unless --force re-runs prep explicitly.
    {
        let force = has_flag(args, "--force");
        let mut inputs: Vec<&Path> = vec![&project, &fork, &recon_out];
        if let Some(o) = opt.as_ref() {
            inputs.push(o);
        }
        for other in &inputs {
            if paths_overlap(&out, other) {
                return Err(format!(
                    "{}: --out overlaps input {} (refusing to wipe sources)",
                    out.display(),
                    other.display()
                ));
            }
        }
        if out.exists() {
            let state_file = out.join("release-state.json");
            let non_empty = out.is_file()
                || std::fs::read_dir(&out).map(|mut rd| rd.next().is_some()).unwrap_or(true);
            if state_file.is_file() && non_empty && !force {
                return Err(format!(
                    "{}: existing release state (re-run with --force to overwrite)",
                    out.display()
                ));
            }
            if out.is_file() {
                std::fs::remove_file(&out).map_err(|e| e.to_string())?;
            } else {
                std::fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
            }
        }
    }
    // Stage a clean copy; the staged tree (not the live fork) is what builds.
    let stage = out.join("stage");
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    copy_stage(&source, &stage)?;
    // The staged source must carry the same release metadata (drift fails).
    rustsmith_release::validate_tree(&cfg, &stage).map_err(|e| e.to_string())?;
    let dist = out.join("dist");
    std::fs::create_dir_all(&dist).map_err(|e| e.to_string())?;

    let mut verify = serde_json::json!({});
    // 1. Rust core tests: the same core Rust consumers use, standalone.
    let core_manifest = stage.join("crc-core/Cargo.toml");
    let core_manifest_arg = if core_manifest.is_file() {
        core_manifest.display().to_string()
    } else {
        // Generic projects keep the core at the path dep the manifests name;
        // the validated tree guarantees it resolves, so probe the stage.
        probe_core_manifest(&stage)?
    };
    let r = run_cmd(
        &stage,
        "cargo",
        &["test".into(), "--manifest-path".into(), core_manifest_arg.clone()],
        &[],
    );
    verify["rust_core_tests"] =
        serde_json::json!({"passed": r.ok, "manifest": core_manifest_arg, "tail": tail(&r.log, 8)});
    if !r.ok {
        return Err(format!("core tests failed:\n{}", tail(&r.log, 25)));
    }
    // 2. Rust consumer: a downstream crate depending on the core by path,
    // with no Python anywhere. Proves the core is reusable as an rlib.
    let consumer = out.join("consumer");
    std::fs::create_dir_all(consumer.join("src")).map_err(|e| e.to_string())?;
    let core_dep = core_dep_spec(&stage, &cfg)?;
    std::fs::write(
        consumer.join("Cargo.toml"),
        format!(
            "[package]\nname = \"release-consumer\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\n{core_dep}\n\n[workspace]\n"
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(consumer.join("src/main.rs"), "fn main() {}\n").map_err(|e| e.to_string())?;
    let r = run_cmd(&consumer, "cargo", &["build".into(), "--offline".into()], &[]);
    let (consumer_ok, consumer_note) = if r.ok {
        (true, "offline build against the staged core".to_string())
    } else {
        // Offline fails only when the consumer lockfile is cold; retry once
        // with the network (builds may fetch; uploads never happen here).
        let r2 = run_cmd(&consumer, "cargo", &["build".into()], &[]);
        (r2.ok, format!("offline: {} || network: {}", tail(&r.log, 3).replace('\n', " | "), tail(&r2.log, 3).replace('\n', " | ")))
    };
    // A consumer that cannot build even with the network is a hard failure:
    // the core is not independently reusable.
    let consumer_log = if consumer_ok { consumer_note.clone() } else { format!("{consumer_note}\n{}", tail(&r.log, 15)) };
    verify["rust_consumer"] = serde_json::json!({"passed": consumer_ok, "detail": consumer_log});
    if !consumer_ok {
        return Err(format!("rust consumer build failed:\n{consumer_log}"));
    }
    // Core `cargo package` (what crates.io receives) must succeed too.
    let core_dir = core_crate_dir(&stage, &cfg)?;
    let r = run_cmd(
        &core_dir,
        "cargo",
        &[
            "package".into(),
            "--allow-dirty".into(),
            "--no-verify".into(),
        ],
        &[],
    );
    verify["core_package"] = serde_json::json!({"passed": r.ok, "tail": tail(&r.log, 6)});
    if !r.ok {
        return Err(format!("cargo package (core) failed:\n{}", tail(&r.log, 20)));
    }
    // 3. Python wheel + sdist from the staged source.
    let stage_s = stage.display().to_string();
    let dist_s = dist.display().to_string();
    let r = run_cmd(
        &stage,
        "maturin",
        &[
            "build".into(),
            "--release".into(),
            "--manifest-path".into(),
            format!("{stage_s}/Cargo.toml"),
            "--out".into(),
            dist_s.clone(),
        ],
        &[],
    );
    verify["wheel_build"] = serde_json::json!({"passed": r.ok, "tail": tail(&r.log, 6)});
    if !r.ok {
        return Err(format!("maturin build failed:\n{}", tail(&r.log, 25)));
    }
    let r = run_cmd(
        &stage,
        "maturin",
        &[
            "sdist".into(),
            "--manifest-path".into(),
            format!("{stage_s}/Cargo.toml"),
            "--out".into(),
            dist_s.clone(),
        ],
        &[],
    );
    verify["sdist_build"] = serde_json::json!({"passed": r.ok, "tail": tail(&r.log, 6)});
    if !r.ok {
        return Err(format!("maturin sdist failed:\n{}", tail(&r.log, 25)));
    }
    // The .crate file `cargo package` staged under the workspace target dir.
    let dot_crate: Option<PathBuf> = newest(
        walk(&stage.join("target/package"))
            .into_iter()
            .filter(|p| p.extension().map(|x| x == "crate").unwrap_or(false))
            .collect(),
    );
    if let Some(c) = dot_crate {
        let dest = dist.join(c.file_name().unwrap());
        std::fs::copy(&c, &dest).map_err(|e| e.to_string())?;
    }
    let wheel = newest(
        walk(&dist).into_iter().filter(|p| p.extension().map(|x| x == "whl").unwrap_or(false)).collect(),
    )
    .ok_or("maturin build produced no wheel")?;
    let sdist = newest(
        walk(&dist)
            .into_iter()
            .filter(|p| {
                p.file_name().map(|n| n.to_string_lossy().ends_with(".tar.gz")).unwrap_or(false)
            })
            .collect(),
    )
    .ok_or("maturin sdist produced no tarball")?;
    // 4. Installed Python package: fresh venv, wheel from disk, no index.
    let venv = out.join("verify-venv");
    let r = run_cmd(&out, "python3", &["-m".into(), "venv".into(), venv.display().to_string()], &[]);
    if !r.ok {
        return Err(format!("venv creation failed:\n{}", tail(&r.log, 10)));
    }
    let venv_py = venv.join("bin/python");
    let venv_pip = venv.join("bin/pip");
    let r = run_cmd(
        &out,
        &venv_pip.display().to_string(),
        &[
            "install".into(),
            "--no-index".into(),
            "--no-deps".into(),
            wheel.display().to_string(),
        ],
        &[],
    );
    if !r.ok {
        return Err(format!("wheel install failed:\n{}", tail(&r.log, 15)));
    }
    let mut smoke_ok = true;
    let mut smoke_rows = vec![];
    // Distribution identity first: the installed dist name+version must match.
    let r = run_cmd(
        &out,
        &venv_py.display().to_string(),
        &[
            "-c".into(),
            format!("import importlib.metadata as m; assert m.version({:?}) == {:?}, m.version({:?})", cfg.pypi_dist, cfg.version, cfg.pypi_dist),
        ],
        &[],
    );
    smoke_rows.push(serde_json::json!({"check": "dist-version", "passed": r.ok, "tail": tail(&r.log, 3)}));
    smoke_ok &= r.ok;
    for expr in &cfg.smoke_exprs {
        let r = run_cmd(
            &out,
            &venv_py.display().to_string(),
            &["-c".into(), format!("import {imp}; assert {e}, {e:?}", imp = cfg.python_import, e = expr)],
            &[],
        );
        smoke_rows.push(serde_json::json!({"check": expr, "passed": r.ok, "tail": tail(&r.log, 3)}));
        smoke_ok &= r.ok;
    }
    verify["python_install_smoke"] = serde_json::Value::Array(smoke_rows);
    if !smoke_ok {
        return Err(format!("installed-package smoke failed: {}", verify["python_install_smoke"]));
    }
    // 5. sdist completeness: everything needed to rebuild, core included.
    let sdist_tmp = out.join("sdist-check");
    std::fs::create_dir_all(&sdist_tmp).map_err(|e| e.to_string())?;
    let r = run_cmd(
        &sdist_tmp,
        "tar",
        &["xzf".into(), sdist.display().to_string()],
        &[],
    );
    if !r.ok {
        return Err(format!("sdist unpack failed:\n{}", tail(&r.log, 10)));
    }
    let unpacked: Vec<PathBuf> = walk(&sdist_tmp).into_iter().filter(|p| p.is_file()).collect();
    let has = |suffix: &str| unpacked.iter().any(|p| p.display().to_string().ends_with(suffix));
    let need: Vec<(String, bool)> = vec![
        ("Cargo.toml".into(), has("Cargo.toml")),
        ("core Cargo.toml".into(), unpacked.iter().filter(|p| p.file_name().map(|n| n == "Cargo.toml").unwrap_or(false)).count() >= 2),
        ("pyproject.toml".into(), has("pyproject.toml")),
        ("NOTICE".into(), has("NOTICE")),
        (
            format!("{}/__init__.py", cfg.python_import),
            has(&format!("{}/__init__.py", cfg.python_import)),
        ),
    ];
    let missing: Vec<String> =
        need.iter().filter(|(_, ok)| !ok).map(|(n, _)| n.to_string()).collect();
    verify["sdist_contents"] = serde_json::json!({
        "passed": missing.is_empty(),
        "missing": missing,
        "files": unpacked.len(),
    });
    if !missing.is_empty() {
        return Err(format!("sdist is missing rebuild inputs: {missing:?}"));
    }
    // Rebuild the wheel from the unpacked sdist (core must be inside).
    let sdist_root = sdist_build_root(&sdist_tmp)?;
    let r = run_cmd(&sdist_root, "maturin", &["build".into(), "--release".into(), "--out".into(), out.join("sdist-rebuilt").display().to_string()], &[]);
    let rebuilt_ok = r.ok
        && walk(&out.join("sdist-rebuilt"))
            .into_iter()
            .any(|p| p.extension().map(|x| x == "whl").unwrap_or(false));
    verify["sdist_rebuild"] = serde_json::json!({"passed": rebuilt_ok, "tail": tail(&r.log, 6)});
    if !rebuilt_ok {
        return Err(format!("sdist rebuild failed (core missing from sdist?):\n{}", tail(&r.log, 20)));
    }
    // 6. Retain everything: artifacts + identities + hashes + verification.
    let mut artifacts = vec![];
    for f in [&wheel, &sdist]
        .into_iter()
        .chain(walk(&dist).iter().filter(|p| p.extension().map(|x| x == "crate").unwrap_or(false)).collect::<Vec<_>>())
    {
        let bytes = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        artifacts.push(serde_json::json!({
            "file": f.file_name().unwrap().to_string_lossy(),
            "sha256": rustsmith_release::sha256_file(f).map_err(|e| e.to_string())?,
            "bytes": bytes,
        }));
    }
    artifacts.sort_by(|a, b| a["file"].as_str().cmp(&b["file"].as_str()));
    let mut sums = String::new();
    for a in &artifacts {
        sums.push_str(&format!("{}  {}\n", a["sha256"].as_str().unwrap(), a["file"].as_str().unwrap()));
    }
    std::fs::write(out.join("SHA256SUMS"), &sums).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({
        "project": cfg.project,
        "release_version": cfg.version,
        "rust_crate": cfg.rust_crate,
        "pypi_dist": cfg.pypi_dist,
        "python_import": cfg.python_import,
        "source_tree": source_kind,
        "source_sha": source_sha,
        "built_at": now(),
        "artifacts": artifacts,
    });
    std::fs::write(out.join("release-manifest.json"), serde_json::to_string_pretty(&manifest).unwrap())
        .map_err(|e| e.to_string())?;
    verify["passed"] = serde_json::json!(true);
    std::fs::write(out.join("verification.json"), serde_json::to_string_pretty(&verify).unwrap())
        .map_err(|e| e.to_string())?;
    // Per-registry tracking starts here (all pending; record lanes later).
    let mut state = rustsmith_release::initial_state(&cfg.version, &source_sha);
    state.artifacts = artifacts
        .iter()
        .map(|a| rustsmith_release::ArtifactRef {
            file: a["file"].as_str().unwrap().into(),
            sha256: a["sha256"].as_str().unwrap().into(),
            bytes: a["bytes"].as_u64().unwrap(),
        })
        .collect();
    atomic_write(&out.join("release-state.json"), serde_json::to_string_pretty(&state).unwrap().as_bytes())?;
    // Generated workflow + exact setup instructions.
    let wf_dir = out.join(".github/workflows");
    std::fs::create_dir_all(&wf_dir).map_err(|e| e.to_string())?;
    std::fs::write(wf_dir.join("release.yml"), rustsmith_release::render_workflow(&cfg))
        .map_err(|e| e.to_string())?;
    std::fs::write(out.join("TRUSTED_PUBLISHING_SETUP.md"), rustsmith_release::render_setup_instructions(&cfg))
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({
            "project": cfg.project,
            "version": cfg.version,
            "source_tree": source_kind,
            "source_sha": source_sha,
            "artifacts": artifacts,
            "overall": "pending",
        })
    );
    Ok(())
}

/// Core manifest path inside the staged tree (prefers the conventional
/// `crc-core/` layout, else the validated path dependency target).
fn probe_core_manifest(stage: &Path) -> Result<String, String> {
    for entry in walk(stage) {
        if entry.file_name().map(|n| n == "Cargo.toml").unwrap_or(false) && entry != stage.join("Cargo.toml") {
            return Ok(entry.display().to_string());
        }
    }
    Err(format!("{}: staged tree has no core crate manifest", stage.display()))
}

/// `[dependencies]` entry for the consumer crate: the path dep the binding
/// manifests name (same resolver the validator uses, re-read here).
fn core_dep_spec(stage: &Path, cfg: &rustsmith_release::ReleaseConfig) -> Result<String, String> {
    let root = std::fs::read_to_string(stage.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let mut in_deps = false;
    for line in root.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_deps = l == "[dependencies]" || l.starts_with("[dependencies.");
            continue;
        }
        if in_deps && l.contains(&cfg.rust_crate) && l.contains("path") {
            // `name = { path = "dir", ... }` -> consumer depends by absolute path.
            if let Some(rel) = dep_path_value(l) {
                let abs = stage.join(&rel);
                return Ok(format!("\"{}\" = {{ path = \"{}\" }}", cfg.rust_crate, abs.display()));
            }
        }
    }
    Err(format!("staged Cargo.toml names no path dep for {:?}", cfg.rust_crate))
}

fn dep_path_value(line: &str) -> Option<String> {
    let i = line.find("path")?;
    let tail = &line[i + 4..];
    let v = tail.trim().strip_prefix('=')?.trim();
    let q = v.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    Some(v[1..].split(q).next().unwrap_or("").to_string())
}

fn core_crate_dir(stage: &Path, cfg: &rustsmith_release::ReleaseConfig) -> Result<PathBuf, String> {
    let root = std::fs::read_to_string(stage.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let mut in_deps = false;
    for line in root.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_deps = l == "[dependencies]" || l.starts_with("[dependencies.");
            continue;
        }
        if in_deps {
            if let Some(rel) = dep_path_value(l) {
                let dir = stage.join(&rel);
                if let Ok(m) = std::fs::read_to_string(dir.join("Cargo.toml")) {
                    if m.contains(&format!("name = \"{}\"", cfg.rust_crate)) {
                        return Ok(dir);
                    }
                }
            }
        }
    }
    Err(format!("staged tree: core dir for {:?} not found", cfg.rust_crate))
}

fn sdist_build_root(tmp: &Path) -> Result<PathBuf, String> {
    // maturin sdists unpack to `<tmp>/<dist>-<version>/` holding Cargo.toml
    // at top (workspace-root binding) or one level down (subdir binding).
    for entry in walk(tmp) {
        if entry.file_name().map(|n| n == "Cargo.toml").unwrap_or(false) {
            if let Some(parent) = entry.parent() {
                // Prefer the outermost manifest (the binding root).
                if parent == tmp {
                    continue;
                }
                return Ok(parent.to_path_buf());
            }
        }
    }
    // Fallback: first manifest found.
    for entry in walk(tmp) {
        if entry.file_name().map(|n| n == "Cargo.toml").unwrap_or(false) {
            return Ok(entry.parent().unwrap().to_path_buf());
        }
    }
    Err(format!("{}: unpacked sdist has no Cargo.toml", tmp.display()))
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&p) {
            for e in rd.flatten() {
                let q = e.path();
                if q.is_dir() {
                    stack.push(q);
                } else {
                    out.push(q);
                }
            }
        }
    }
    out.sort();
    out
}

pub fn cmd_release_record(args: &[String]) -> Result<(), String> {
    let state_path = PathBuf::from(flag(args, "--state").ok_or("missing --state")?);
    let registry = flag(args, "--registry").ok_or("missing --registry (testpypi|pypi|crates-io)")?;
    let result = flag(args, "--result").ok_or("missing --result (success|failed)")?;
    let detail = flag(args, "--detail").unwrap_or_default();
    let text = std::fs::read_to_string(&state_path).map_err(|e| format!("{}: {e}", state_path.display()))?;
    let mut state: rustsmith_release::ReleaseState =
        serde_json::from_str(&text).map_err(|e| format!("{}: bad state: {e}", state_path.display()))?;
    rustsmith_release::record_outcome(&mut state, &registry, &result, &detail, now())
        .map_err(|e| e.to_string())?;
    atomic_write(&state_path, serde_json::to_string_pretty(&state).unwrap().as_bytes())?;
    let overall = rustsmith_release::overall_status(&state);
    println!("{}", serde_json::json!({"registry": registry, "result": result, "overall": overall}));
    Ok(())
}

pub fn cmd_release_status(args: &[String]) -> Result<(), String> {
    let state_path = PathBuf::from(flag(args, "--state").ok_or("missing --state")?);
    let text = std::fs::read_to_string(&state_path).map_err(|e| format!("{}: {e}", state_path.display()))?;
    let state: rustsmith_release::ReleaseState =
        serde_json::from_str(&text).map_err(|e| format!("{}: bad state: {e}", state_path.display()))?;
    let overall = rustsmith_release::overall_status(&state);
    println!("{}", serde_json::json!({"overall": overall, "registries": state.registries, "release_version": state.release_version}));
    if overall == "complete" {
        Ok(())
    } else {
        Err(format!("release {overall} (not complete)"))
    }
}
