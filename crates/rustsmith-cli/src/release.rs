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
    // Staged README receipt: measured outcomes land in the staged README
    // BEFORE anything builds, so the wheel/sdist ship the receipt. The block
    // is generated (never hand-edited); hashes stay in release-manifest.json.
    // The optimize report rides alongside the source tree when prep runs off
    // `--opt`; a fork-only source has no report and receipts the baseline.
    let readme_path = stage.join("README.md");
    let readme_text = std::fs::read_to_string(&readme_path)
        .map_err(|e| format!("{}: staged README missing: {e}", readme_path.display()))?;
    let report_path = source.join("optimize-report.json");
    let perf = match std::fs::read_to_string(&report_path) {
        Ok(t) => {
            let v: serde_json::Value = serde_json::from_str(&t)
                .map_err(|e| format!("{}: unparseable optimize report: {e}", report_path.display()))?;
            rustsmith_release::collect_readme_perf(&v)
        }
        Err(_) => rustsmith_release::ReadmePerf { merged: Vec::new() },
    };
    let block = rustsmith_release::render_readme_perf(&perf);
    std::fs::write(&readme_path, rustsmith_release::inject_readme_perf(&readme_text, &block))
        .map_err(|e| format!("{}: {e}", readme_path.display()))?;
    let dist = out.join("dist");
    std::fs::create_dir_all(&dist).map_err(|e| e.to_string())?;

    let mut verify = serde_json::json!({});
    verify["readme_perf"] = serde_json::json!({
        "passed": true,
        "merged_count": perf.merged.len(),
        "techniques": perf.merged.iter().map(|m| m.technique.clone()).collect::<Vec<_>>(),
    });

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
    // 3. Python wheel + sdist from the staged source. The wheel MUST be a
    // release build (C13 foot-gun: a develop build benchmarked or shipped as
    // "the Rust port" silently measures unoptimized code). The exact argv is
    // retained in verification.json so acceptance can assert `--release`
    // structurally instead of trusting this comment.
    let stage_s = stage.display().to_string();
    let dist_s = dist.display().to_string();
    let wheel_argv = vec![
        "build".to_string(),
        "--release".to_string(),
        "--manifest-path".to_string(),
        format!("{stage_s}/Cargo.toml"),
        "--out".to_string(),
        dist_s.clone(),
    ];
    let r = run_cmd(&stage, "maturin", &wheel_argv, &[]);
    verify["wheel_build"] = serde_json::json!({"passed": r.ok, "tail": tail(&r.log, 6)});
    verify["release_profile"] = serde_json::json!({
        "passed": wheel_argv.contains(&"--release".to_string()),
        "profile": "release",
        "maturin_argv": wheel_argv,
    });
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

/// `release-publish`: autonomous upload lane (no browser, no human clicks).
///
/// Reads the `release-prep` bundle next to `--state` (`dist/`,
/// `verification.json`, `release-manifest.json`), enforces the policy gate
/// (prep battery green; prod lanes additionally need
/// `RUSTSMITH_ALLOW_PROD_PUBLISH=1` plus a fresh TestPyPI success), uploads
/// the SAME retained files by explicit recorded filename (re-hashed first;
/// the `.crate` never goes to a Python index), reconciles per-file remote
/// hashes afterwards, and records the outcome (sticky success). Credentials
/// come from the environment (`TWINE_USERNAME`/`TWINE_PASSWORD`,
/// `CARGO_REGISTRY_TOKEN`) and are never logged. `--dry-run` resolves,
/// gates, and prints the plan without uploading or recording anything.
pub fn cmd_release_publish(args: &[String]) -> Result<(), String> {
    let state_path = PathBuf::from(flag(args, "--state").ok_or("missing --state")?);
    let registry = flag(args, "--registry").ok_or("missing --registry (testpypi|pypi|crates-io)")?;
    let dry = has_flag(args, "--dry-run");
    let out = state_path.parent().ok_or("state has no parent dir")?.to_path_buf();
    let text = std::fs::read_to_string(&state_path).map_err(|e| format!("{}: {e}", state_path.display()))?;
    let state: rustsmith_release::ReleaseState =
        serde_json::from_str(&text).map_err(|e| format!("{}: bad state: {e}", state_path.display()))?;
    let verify: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("verification.json")).map_err(|e| format!("verification.json: {e}"))?,
    )
    .map_err(|e| format!("verification.json: bad json: {e}"))?;
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("release-manifest.json")).map_err(|e| format!("release-manifest.json: {e}"))?,
    )
    .map_err(|e| format!("release-manifest.json: bad json: {e}"))?;
    let prod_allowed = std::env::var("RUSTSMITH_ALLOW_PROD_PUBLISH").as_deref() == Ok("1");
    rustsmith_release::publish_gate(verify.get("passed").and_then(|v| v.as_bool()).unwrap_or(false), &state, &registry, now(), prod_allowed)?;
    // Same-SHA256 promotion: the files on disk must still match the recorded
    // hashes (no rebuild, no drift since prep).
    let dist = out.join("dist");
    let mut local: Vec<(String, String, PathBuf)> = vec![];
    if registry == "crates-io" {
        let stage = PathBuf::from(flag(args, "--stage").ok_or("crates-io lane needs --stage <staged tree>")?);
        let core_manifest = if stage.join("crc-core/Cargo.toml").is_file() {
            stage.join("crc-core/Cargo.toml").display().to_string()
        } else {
            probe_core_manifest(&stage)?
        };
        let plan = serde_json::json!({"registry": registry, "core_manifest": core_manifest, "dry_run": dry});
        if dry {
            println!("{plan}");
            return Ok(());
        }
        if std::env::var("CARGO_REGISTRY_TOKEN").unwrap_or_default().is_empty() {
            return Err("crates-io: missing CARGO_REGISTRY_TOKEN in the environment (nothing uploaded, nothing recorded)".into());
        }
        let r = run_cmd(&stage, "cargo", &["publish".into(), "--manifest-path".into(), core_manifest, "--allow-dirty".into()], &[]);
        return finish_crates_publish(&state_path, &state, &registry, &r, &manifest);
    }
    let names = rustsmith_release::publish_files(&state.artifacts, &registry)?;
    for name in &names {
        let p = dist.join(name);
        let recorded = state.artifacts.iter().find(|a| &a.file == name).map(|a| a.sha256.clone()).unwrap_or_default();
        let actual = rustsmith_release::sha256_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        if actual != recorded {
            return Err(format!("{name}: on-disk sha256 {actual} != recorded {recorded} (re-run prep, never publish drift)"));
        }
        local.push((name.clone(), actual, p));
    }
    let plan = serde_json::json!({"registry": registry, "files": names, "dry_run": dry});
    if dry {
        println!("{plan}");
        return Ok(());
    }
    for var in ["TWINE_USERNAME", "TWINE_PASSWORD"] {
        if std::env::var(var).unwrap_or_default().is_empty() {
            return Err(format!("{registry}: missing {var} in the environment (nothing uploaded, nothing recorded)"));
        }
    }
    let paths: Vec<String> = local.iter().map(|(_, _, p)| p.display().to_string()).collect();
    let mut targs = vec!["upload".into(), "--repository".into(), registry.clone(), "--non-interactive".into()];
    targs.extend(paths);
    let r = run_cmd(&out, "twine", &targs, &[]);
    let dist_name = manifest.get("pypi_dist").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let version = manifest.get("release_version").and_then(|v| v.as_str()).unwrap_or("").to_string();
    // Crash recovery: twine failing does NOT mean unpublished. Reconcile the
    // remote file hashes; a match promotes the attempt to success.
    let reconciled = reconcile_pypi(&registry, &dist_name, &version, &local);
    if !r.ok && reconciled.is_err() {
        record_publish(&state_path, &state, &registry, "failed", &tail(&r.log, 5))?;
        return Err(format!("twine upload failed and remote does not match:\n{}", tail(&r.log, 15)));
    }
    let note = if r.ok { "twine upload ok" } else { "twine errored but remote matches (verified-after-error)" };
    // Post-upload proof: pinned install from the index + smoke vectors.
    match verify_index_install(&out, &registry, &dist_name, &version, &manifest, &verify) {
        Ok(detail) => {
            record_publish(&state_path, &state, &registry, "success", &format!("{note}; {detail}"))?;
            println!("{}", serde_json::json!({"registry": registry, "result": "success", "overall": rustsmith_release::overall_status(&reread(&state_path)?)}));
            Ok(())
        }
        Err(e) => {
            record_publish(&state_path, &state, &registry, "failed", &format!("{note}; index verify failed: {e}"))?;
            Err(format!("uploaded but index verification failed: {e}"))
        }
    }
}

/// Re-read state from disk (for the post-record overall readout).
fn reread(path: &Path) -> Result<rustsmith_release::ReleaseState, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Record one publish attempt (atomic write, sticky success) and print it.
fn record_publish(state_path: &Path, state: &rustsmith_release::ReleaseState, registry: &str, result: &str, detail: &str) -> Result<(), String> {
    let mut owned = state.clone();
    rustsmith_release::record_outcome(&mut owned, registry, result, detail, now()).map_err(|e| e.to_string())?;
    atomic_write(state_path, serde_json::to_string_pretty(&owned).unwrap().as_bytes())?;
    Ok(())
}

/// Per-file remote-hash reconciliation for a Python index: every uploaded
/// file must appear under the release with its recorded SHA-256.
fn reconcile_pypi(registry: &str, dist: &str, version: &str, local: &[(String, String, PathBuf)]) -> Result<(), String> {
    let url = rustsmith_release::pypi_json_url(registry, dist, version)?;
    let r = run_cmd(&std::env::temp_dir(), "curl", &["-sS".into(), "--max-time".into(), "60".into(), url.clone()], &[]);
    if !r.ok {
        return Err(format!("index query failed: {url}"));
    }
    let v: serde_json::Value = serde_json::from_str(&r.log).map_err(|_| format!("index returned no release: {dist}=={version}"))?;
    let urls = v.get("urls").and_then(|u| u.as_array()).ok_or(format!("index has no files for {dist}=={version}"))?;
    for (name, sha, _) in local {
        let ok = urls.iter().any(|u| {
            u.get("filename").and_then(|f| f.as_str()) == Some(name)
                && u.get("digests").and_then(|d| d.get("sha256")).and_then(|s| s.as_str()) == Some(sha)
        });
        if !ok {
            return Err(format!("{name}: remote file or sha256 mismatch on {registry}"));
        }
    }
    Ok(())
}

/// Fresh-venv proof that the index serves what was uploaded: pinned
/// `dist==version` install, dist-version assert, then the prep smoke
/// expressions (re-read from `verification.json`, same interpreter shape).
fn verify_index_install(out: &Path, registry: &str, dist: &str, version: &str, manifest: &serde_json::Value, verify: &serde_json::Value) -> Result<String, String> {
    let venv = out.join(".publish-verify-venv");
    let _ = std::fs::remove_dir_all(&venv);
    let r = run_cmd(out, "python3", &["-m".into(), "venv".into(), venv.display().to_string()], &[]);
    if !r.ok {
        return Err(format!("venv creation failed:\n{}", tail(&r.log, 6)));
    }
    let py = venv.join("bin/python");
    let pip = venv.join("bin/pip");
    let (index, extra): (String, Vec<String>) = match registry {
        "testpypi" => ("https://test.pypi.org/simple/".into(), vec!["--extra-index-url".into(), "https://pypi.org/simple/".into()]),
        _ => ("https://pypi.org/simple/".into(), vec![]),
    };
    let mut iargs = vec!["install".into(), "--index-url".into(), index];
    iargs.extend(extra);
    iargs.push(format!("{dist}=={version}"));
    let r = run_cmd(out, &pip.display().to_string(), &iargs, &[]);
    if !r.ok {
        return Err(format!("pinned index install failed:\n{}", tail(&r.log, 8)));
    }
    let imp = manifest.get("python_import").and_then(|v| v.as_str()).unwrap_or("");
    let r = run_cmd(out, &py.display().to_string(), &["-c".into(), format!("import importlib.metadata as m; assert m.version({dist:?}) == {version:?}")], &[]);
    if !r.ok {
        return Err(format!("index dist-version mismatch:\n{}", tail(&r.log, 4)));
    }
    let mut n = 0;
    if let Some(rows) = verify.get("python_install_smoke").and_then(|v| v.as_array()) {
        for row in rows.iter().skip(1) {
            if let Some(expr) = row.get("check").and_then(|c| c.as_str()) {
                let r = run_cmd(out, &py.display().to_string(), &["-c".into(), format!("import {imp}; assert {expr}, {expr:?}")], &[]);
                if !r.ok {
                    return Err(format!("index smoke failed ({expr}):\n{}", tail(&r.log, 4)));
                }
                n += 1;
            }
        }
    }
    Ok(format!("pinned {dist}=={version} install + {n} smoke exprs from {registry}"))
}

/// Terminal handling for the crates.io lane: success records; transport
/// failures query the sparse index first (a timeout after upload does NOT
/// mean unpublished); "already exists" with the version present is an
/// idempotent success, never a duplicate publish.
fn finish_crates_publish(state_path: &Path, state: &rustsmith_release::ReleaseState, registry: &str, r: &Run, manifest: &serde_json::Value) -> Result<(), String> {
    let krate = manifest.get("rust_crate").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let version = manifest.get("release_version").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if r.ok {
        record_publish(state_path, state, registry, "success", "cargo publish ok")?;
        println!("{}", serde_json::json!({"registry": registry, "result": "success", "overall": rustsmith_release::overall_status(&reread(state_path)?)}));
        return Ok(());
    }
    let log = r.log.to_lowercase();
    let ambiguous =
        log.contains("timed out") || log.contains("network") || log.contains("connection") || log.contains("already");
    if ambiguous && crates_version_present(&krate, &version) {
        record_publish(state_path, state, registry, "success", "index shows version (verified-after-upload-error)")?;
        println!("{}", serde_json::json!({"registry": registry, "result": "success", "overall": rustsmith_release::overall_status(&reread(state_path)?)}));
        return Ok(());
    }
    record_publish(state_path, state, registry, "failed", &tail(&r.log, 5))?;
    Err(format!("cargo publish failed:\n{}", tail(&r.log, 15)))
}

/// Sparse-index version check for crash recovery (`cargo` cache layout).
fn crates_version_present(krate: &str, version: &str) -> bool {
    let path = rustsmith_release::crates_index_path(krate);
    let r = run_cmd(
        &std::env::temp_dir(),
        "curl",
        &["-sS".into(), "--max-time".into(), "30".into(), format!("https://index.crates.io/{path}")],
        &[],
    );
    r.ok && r.log.lines().any(|l| l.contains(&format!("\"vers\":\"{version}\"")))
}
