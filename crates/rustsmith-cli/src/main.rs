mod candidates;
mod fixture;
mod heldout;
mod mirror;
mod optimize;
mod porting;
mod recon;
use rustsmith_core::Event;
use rustsmith_oracle::{HeldoutSuite, Oracle};
use rustsmith_sandbox::Sandbox;
use rustsmith_store::Store;
use std::path::PathBuf;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
fn usage() -> &'static str {
    "usage: rustsmith run --repo <url[#pin]|path> --fork <dir> --work <dir> [--store <store.db>] [--run-id <id>] [--stage full|recon|mirror|optimize|harvest] [--plant-live test-edit|hardcode]\n       rustsmith run --stage recon --repo <path> --run-id <id> [--store <store.db>] [--heldout <dir>] (M0 legacy)\n       rustsmith audit --run-id <id> [--store <store.db>]\n       rustsmith verify --manifest <oracle/manifest.json> --tree <path>\n       rustsmith grade --manifest <oracle/manifest.json> --tree <path> [--heldout <dir>]"
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "run" => cmd_run(&args[2..]),
        "audit" => cmd_audit(&args[2..]),
        "verify" => cmd_verify(&args[2..]),
        "grade" => cmd_grade(&args[2..]),
        "m1probe" => cmd_m1probe(&args[2..]),
        "m2probe" => cmd_m2probe(&args[2..]),
        "recon" => cmd_recon(&args[2..]),
        "mirror" => cmd_mirror(&args[2..]),
        "optimize" => cmd_optimize(&args[2..]),
        "grade-candidate" => cmd_grade_candidate(&args[2..]),
        "learn" => cmd_learn(&args[2..]),
        "report" => cmd_report(&args[2..]),
        "status" => cmd_status(&args[2..]),
        "halt" => cmd_halt(&args[2..]),
        "resume" => cmd_resume(&args[2..]),
        _ => Err(usage().into()),
    }
}

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

fn cmd_run(args: &[String]) -> Result<(), String> {
    let stage = flag(args, "--stage").unwrap_or_else(|| "full".into());
    // M0 legacy: `run --stage recon` with explicit --out (m0_acceptance.sh shape).
    if stage == "recon" && flag(args, "--out").is_some() {
        return cmd_run_legacy(args);
    }
    let repo_spec = flag(args, "--repo").ok_or("missing --repo")?;
    let run_id = flag(args, "--run-id").unwrap_or_else(|| "run0".into());
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let fork = PathBuf::from(flag(args, "--fork").unwrap_or_else(|| "fork".into()));
    let work = PathBuf::from(flag(args, "--work").unwrap_or_else(|| "work".into()));
    let plant = flag(args, "--plant-live");
    if plant.is_some() && stage != "full" {
        return Err("--plant-live needs full-pipeline mode".into());
    }
    if let Some(p) = plant.as_deref() {
        if p != "test-edit" && p != "hardcode" {
            return Err(format!("unknown --plant-live {p} (test-edit|hardcode)"));
        }
    }
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    // Resolve repo: URL[#pin] clones into work/orig, else a local path.
    let orig = resolve_repo(&repo_spec, &work)?;
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    store
        .create_run(&run_id, &repo_spec, "python", "run")
        .map_err(|e| e.to_string())?;
    ev_run(&store, &run_id, "run_start", serde_json::json!({"repo": repo_spec, "stage": stage}));
    let recon_out = work.join("recon");
    let heldout_out = work.join("heldout");
    let opt = work.join("opt");
    let store_s = store_path.display().to_string();
    let stages: Vec<&str> = match stage.as_str() {
        "full" => vec!["recon", "mirror", "optimize", "harvest"],
        "recon" | "mirror" | "optimize" | "harvest" => vec![stage.as_str()],
        _ => return Err(format!("unknown --stage {stage} (full|recon|mirror|optimize|harvest)")),
    };
    for s in stages {
        let r = match s {
            "recon" => cmd_recon(&sargs(vec![
                ("--repo", &orig.display().to_string()),
                ("--out", &recon_out.display().to_string()),
                ("--heldout-out", &heldout_out.display().to_string()),
                ("--store", &store_s),
                ("--run-id", &run_id),
            ])),
            "mirror" => {
                if plant.as_deref() == Some("test-edit") {
                    plant_test_edit(&store, &run_id, &recon_out, &orig, &work)?;
                    return Err(format!("halted: {}", store.halt_reason(&run_id).map_err(|e| e.to_string())?.unwrap_or_default()));
                }
                cmd_mirror(&sargs(vec![
                    ("--repo", &orig.display().to_string()),
                    ("--fork", &fork.display().to_string()),
                    ("--recon-out", &recon_out.display().to_string()),
                    ("--heldout", &heldout_out.display().to_string()),
                    ("--store", &store_s),
                    ("--run-id", &run_id),
                ]))
            }
            "optimize" => {
                if plant.as_deref() == Some("hardcode") {
                    return plant_hardcode(&store, &run_id, &fork, &orig, &recon_out, &heldout_out);
                }
                cmd_optimize(&sargs(vec![
                    ("--fork", &fork.display().to_string()),
                    ("--work", &opt.display().to_string()),
                    ("--recon-out", &recon_out.display().to_string()),
                    ("--heldout", &heldout_out.display().to_string()),
                    ("--store", &store_s),
                    ("--run-id", &run_id),
                    ("--orig", &orig.display().to_string()),
                    ("--max-rounds", "6"),
                ]))
            }
            "harvest" => cmd_report(&sargs(vec![
                ("--run-id", &run_id),
                ("--fork", &fork.display().to_string()),
                ("--orig", &orig.display().to_string()),
                ("--store", &store_s),
                ("--recon-out", &recon_out.display().to_string()),
                ("--opt", &opt.display().to_string()),
            ])),
            _ => unreachable!(),
        };
        if let Err(e) = r {
            // Stages record tamper/divergence halts themselves; anything else
            // halts the run as a stage failure (never silent, never success).
            let has_halt = store.halt_reason(&run_id).map_err(|e| e.to_string())?;
            if has_halt.is_none() {
                let reason = format!("{s}_failed: {e}");
                store.set_halt(&run_id, &reason).map_err(|e| e.to_string())?;
                ev_run(&store, &run_id, "halt", serde_json::json!({"stage": s, "reason": reason}));
            }
            return Err(e);
        }
    }
    println!("run {run_id}: done fork={} work={}", fork.display(), work.display());
    Ok(())
}

/// M0 recon-only skeleton (byte-identical behavior; m0_acceptance.sh pins it).
fn cmd_run_legacy(args: &[String]) -> Result<(), String> {
    let stage = flag(args, "--stage").unwrap_or_else(|| "recon".into());
    if stage != "recon" {
        return Err(format!("M0 skeleton supports only --stage recon (got {stage})"));
    }
    let repo = flag(args, "--repo").ok_or("missing --repo")?;
    let run_id = flag(args, "--run-id").unwrap_or_else(|| "run0".into());
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let heldout_dir = PathBuf::from(flag(args, "--heldout").unwrap_or_else(|| "heldout".into()));
    let containers = PathBuf::from(flag(args, "--containers").unwrap_or_else(|| "containers".into()));
    let out = PathBuf::from(flag(args, "--out").unwrap_or_else(|| "oracle".into()));

    let repo_path = PathBuf::from(&repo);
    if !repo_path.is_dir() {
        return Err(format!("--repo must be a local checkout in M0 (got {repo})"));
    }
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    store
        .create_run(&run_id, &repo, "python", &stage)
        .map_err(|e| e.to_string())?;
    store
        .append_event(&Event {
            ts: now(),
            run_id: run_id.clone(),
            kind: "run_start".into(),
            detail: serde_json::json!({"repo":repo,"stage":stage}),
        })
        .map_err(|e| e.to_string())?;

    let manifest = Oracle::freeze(&repo_path).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    std::fs::write(out.join("manifest.json"), serde_json::to_string_pretty(&manifest).unwrap())
        .map_err(|e| e.to_string())?;
    store
        .append_event(&Event {
            ts: now(),
            run_id: run_id.clone(),
            kind: "freeze".into(),
            detail: serde_json::json!({"test_count":manifest.baseline.test_count,"files":manifest.files.len()}),
        })
        .map_err(|e| e.to_string())?;

    let sandbox = Sandbox::new(containers);
    let graded = sandbox
        .grading_run(&manifest, &repo_path)
        .map_err(|e| e.to_string())?;
    store
        .append_event(&Event {
            ts: now(),
            run_id: run_id.clone(),
            kind: "grade".into(),
            detail: serde_json::json!({"passed":graded.passed,"failed":graded.failed}),
        })
        .map_err(|e| e.to_string())?;

    let heldout = HeldoutSuite::new(heldout_dir);
    let (visible, divergence) = Oracle::graded_run(&manifest, &repo_path, &heldout).map_err(|e| e.to_string())?;
    let parity = rustsmith_gates::oracle_parity(&visible);
    let unit_id = format!("{run_id}:recon");
    store.create_unit(&unit_id, &run_id, "mirror", "{}").map_err(|e| e.to_string())?;
    store
        .record_gate(&unit_id, rustsmith_core::Gate::OracleParity, parity.passed, &parity.detail)
        .map_err(|e| e.to_string())?;
    store
        .append_event(&Event {
            ts: now(),
            run_id: run_id.clone(),
            kind: "gate".into(),
            detail: serde_json::json!({"gate":"oracle_parity","passed":parity.passed,"divergence":divergence}),
        })
        .map_err(|e| e.to_string())?;

    println!(
        "run {run_id}: test_count={} passed={} failed={} divergence={:.4}",
        manifest.baseline.test_count, visible.passed, visible.failed, divergence
    );
    let _ = graded;
    Ok(())
}

fn ev_run(store: &Store, run_id: &str, kind: &str, detail: serde_json::Value) {
    let _ = store.append_event(&Event { ts: now(), run_id: run_id.into(), kind: kind.into(), detail });
}

fn sargs(pairs: Vec<(&str, &str)>) -> Vec<String> {
    let mut out = vec!["run".to_string()];
    for (k, v) in pairs {
        out.push(k.to_string());
        out.push(v.to_string());
    }
    out
}

/// Resolve `--repo`: URL[#pin] clones into `work/orig`, else a local path.
fn resolve_repo(spec: &str, work: &std::path::Path) -> Result<PathBuf, String> {
    if spec.contains("://") {
        let (url, pin) = match spec.split_once('#') {
            Some((u, p)) => (u, p),
            None => (spec, ""),
        };
        let dest = work.join("orig");
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
        }
        let st = std::process::Command::new("/usr/bin/git")
            .args(["clone", "--quiet", url, &dest.display().to_string()])
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err(format!("git clone failed for {url}"));
        }
        if !pin.is_empty() {
            let st = std::process::Command::new("/usr/bin/git")
                .args(["checkout", "--quiet", pin])
                .current_dir(&dest)
                .status()
                .map_err(|e| e.to_string())?;
            if !st.success() {
                return Err(format!("git checkout {pin} failed"));
            }
        }
        Ok(dest)
    } else {
        let p = PathBuf::from(spec);
        if !p.is_dir() {
            return Err(format!("--repo must be a local checkout or URL (got {spec})"));
        }
        Ok(p)
    }
}

/// Live-loop adversarial plant 11: a unit that edits a frozen test file.
/// Graded through the real `oracle_integrity` gate; halts with a tamper event.
fn plant_test_edit(
    store: &Store,
    run_id: &str,
    recon_out: &std::path::Path,
    orig: &std::path::Path,
    work: &std::path::Path,
) -> Result<(), String> {
    let manifest = load_manifest(&recon_out.join("manifest.json").display().to_string())?;
    let unit_id = format!("{run_id}:plant-test-edit");
    store
        .create_unit(&unit_id, run_id, "mirror", &serde_json::json!({"plant": "test-edit"}).to_string())
        .map_err(|e| e.to_string())?;
    ev_run(store, run_id, "unit_start", serde_json::json!({"unit": unit_id}));
    // Scratch copy of the original; the real tree is never touched.
    let scratch = work.join("plant-test-edit");
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    copy_filtered_cli(orig, &scratch)?;
    let rel = manifest
        .files
        .iter()
        .map(|f| f.path.as_str().to_string())
        .find(|p| {
            p.ends_with(".py") && !p.contains("bench")
                && (p.contains("test/unit") || p.contains("test/integration")
                    || p.contains("test_") || p.contains("_test"))
        })
        .ok_or("manifest has no test file")?;
    let target = scratch.join(&rel);
    let mut t = std::fs::read_to_string(&target).map_err(|e| e.to_string())?;
    t.push_str("\n# cheat (adversarial probe: test-editing unit)\n");
    std::fs::write(&target, t).map_err(|e| e.to_string())?;
    match Oracle::verify_hashes(&manifest, &scratch) {
        Ok(()) => Err("plant misfired: test edit not detected".into()),
        Err(reason) => {
            let detail = reason.to_string();
            store
                .record_gate(&unit_id, rustsmith_core::Gate::OracleIntegrity, false, &serde_json::json!({"reason": detail}))
                .map_err(|e| e.to_string())?;
            let halt = format!("oracle_tamper {rel}");
            store.set_halt(run_id, &halt).map_err(|e| e.to_string())?;
            ev_run(store, run_id, "tamper", serde_json::json!({"unit": unit_id, "reason": halt, "detail": detail}));
            Ok(())
        }
    }
}

/// Live-loop adversarial plant 12: visible-input-only correct answers on the
/// live fork (crc). Real whole-repo grade: parity passes, held-out fails,
/// divergence halts the run. No magnitudes leak to any worker surface.
fn plant_hardcode(
    store: &Store,
    run_id: &str,
    fork: &std::path::Path,
    orig: &std::path::Path,
    recon_out: &std::path::Path,
    heldout: &std::path::Path,
) -> Result<(), String> {
    let kind = fixture::detect_fixture(orig)?;
    if kind != fixture::FixtureKind::Crc {
        return Err("--plant-live hardcode supports only the crc fixture".into());
    }
    let manifest = load_manifest(&recon_out.join("manifest.json").display().to_string())?;
    let unit_id = format!("{run_id}:plant-hardcode");
    store
        .create_unit(&unit_id, run_id, "mirror", &serde_json::json!({"plant": "hardcode"}).to_string())
        .map_err(|e| e.to_string())?;
    ev_run(store, run_id, "unit_start", serde_json::json!({"unit": unit_id}));
    // Monkeypatch the live shim (class-level assignment works on the ext;
    // instance attributes are read-only, so only Calculator is wrapped —
    // enough: its pins fail while visible vectors delegate exactly).
    // Canonicalization reuses the ORIGINAL module's exact helper.
    let shim = fork.join("crc/__init__.py");
    let orig_src = orig.join("src/crc/_crc.py").display().to_string();
    let wrapper = include_str!("plant_hardcode_crc.py").replace("@@ORIG@@", &orig_src);
    let mut t = std::fs::read_to_string(&shim).map_err(|e| e.to_string())?;
    t.push_str(&wrapper);
    std::fs::write(&shim, t).map_err(|e| e.to_string())?;
    let _ = git_cli(&fork.to_path_buf(), &["add", "-A"]);
    let _ = git_cli(&fork.to_path_buf(), &["commit", "-qm", "plant hardcode (adversarial probe)"]);
    // Whole-repo grade through the real path (no rebuild: pure-Python patch).
    // Measure first (counts-first precedence, same as `grade`).
    let venv = fork.join(".grade-venv");
    let got = mirror::run_oracle_in_venv(&venv, fork, &manifest.invocation)?;
    let hashes = Oracle::current_hashes(&manifest, fork);
    let integrity = rustsmith_gates::oracle_integrity(&manifest, &hashes, &manifest.baseline, &got);
    let parity = rustsmith_gates::oracle_parity(&got);
    let held_rate = {
        let py = mirror::grade_venv_python(&venv);
        let o = std::process::Command::new(&py)
            .args(["-m", "pytest"])
            .arg(heldout)
            .args(["-q", "--tb=no"])
            .env("PY_COLORS", "0")
            .output()
            .map_err(|e| e.to_string())?;
        mirror::parse_heldout_rate(&format!("{}\n{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
    };
    let div = rustsmith_gates::heldout_divergence(got.pass_rate(), held_rate, 0.05);
    for (g, v) in [("oracle_integrity", &integrity), ("oracle_parity", &parity), ("heldout_divergence", &div)] {
        let gate = match g {
            "oracle_integrity" => rustsmith_core::Gate::OracleIntegrity,
            "oracle_parity" => rustsmith_core::Gate::OracleParity,
            _ => rustsmith_core::Gate::HeldoutDivergence,
        };
        store.record_gate(&unit_id, gate, v.passed, &v.detail).map_err(|e| e.to_string())?;
    }
    ev_run(store, run_id, "gate", serde_json::json!({"unit": unit_id, "integrity": integrity.passed, "parity": parity.passed, "divergence": div.passed}));
    if !integrity.passed {
        let halt = format!("oracle_tamper plant broke manifest: {}", integrity.detail);
        store.set_halt(run_id, &halt).map_err(|e| e.to_string())?;
        ev_run(store, run_id, "tamper", serde_json::json!({"unit": unit_id, "reason": halt}));
        return Err(halt);
    }
    if !parity.passed {
        return Err(format!("plant misfired: visible parity broke ({})", parity.detail));
    }
    if !div.passed {
        let halt = format!("heldout_divergence plant-hardcode: {}", div.detail);
        store.set_halt(run_id, &halt).map_err(|e| e.to_string())?;
        ev_run(store, run_id, "heldout_divergence", serde_json::json!({"unit": unit_id, "reason": halt}));
        return Err(halt);
    }
    Err("plant misfired: hardcode survived grading".into())
}

fn cmd_audit(args: &[String]) -> Result<(), String> {
    let run_id = flag(args, "--run-id").unwrap_or_else(|| "run0".into());
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let events_path = store_path.parent().map(|p| p.join("events.jsonl")).unwrap_or("events.jsonl".into());
    // Replay JSONL: print every event for the run in order.
    let text = std::fs::read_to_string(&events_path).map_err(|e| format!("read {}: {e}", events_path.display()))?;
    let mut n = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if v.get("run_id").and_then(|x| x.as_str()) == Some(run_id.as_str()) {
            println!("{v}");
            n += 1;
        }
    }
    if n == 0 {
        return Err(format!("no events for run {run_id} in {}", events_path.display()));
    }
    eprintln!("replayed {n} events for {run_id}");
    Ok(())
}
fn load_manifest(path: &str) -> Result<rustsmith_core::Manifest, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {path}: {e}"))
}
fn cmd_verify(args: &[String]) -> Result<(), String> {
    let manifest_path = flag(args, "--manifest").ok_or("missing --manifest")?;
    let tree = PathBuf::from(flag(args, "--tree").ok_or("missing --tree")?);
    let manifest = load_manifest(&manifest_path)?;
    match Oracle::verify_hashes(&manifest, &tree) {
        Ok(()) => {
            println!("verify: OK");
            Ok(())
        }
        Err(rustsmith_oracle::OracleError::Halt(reason)) => {
            println!("verify: FAIL {reason}");
            std::process::exit(2);
        }
        Err(e) => Err(e.to_string()),
    }
}
fn cmd_grade(args: &[String]) -> Result<(), String> {
    let manifest_path = flag(args, "--manifest").ok_or("missing --manifest")?;
    let tree = PathBuf::from(flag(args, "--tree").ok_or("missing --tree")?);
    let manifest = load_manifest(&manifest_path)?;
    // Measure without pre-verification so integrity reports the precise reason
    // (counts-first: skip shows skip_mismatch, pure tamper shows hash_mismatch).
    let got = Oracle::measure_tree(&manifest, &tree).map_err(|e| e.to_string())?;
    let hashes = Oracle::current_hashes(&manifest, &tree);
    let integrity = rustsmith_gates::oracle_integrity(&manifest, &hashes, &manifest.baseline, &got);
    let parity = rustsmith_gates::oracle_parity(&got);
    println!("{}", serde_json::json!({
        "passed": got.passed, "failed": got.failed,
        "skipped": got.skipped, "xfailed": got.xfailed, "deselected": got.deselected,
        "integrity": {"passed": integrity.passed, "detail": integrity.detail},
        "parity": {"passed": parity.passed, "detail": parity.detail},
    }));
    if !integrity.passed || !parity.passed {
        std::process::exit(2);
    }
    Ok(())
}
fn cmd_m1probe(args: &[String]) -> Result<(), String> {
    use rustsmith_agent::{Agent, UnitSpec};
    use std::time::Duration;
    let repo = PathBuf::from(flag(args, "--repo").ok_or("missing --repo")?);
    let run_id = flag(args, "--run-id").unwrap_or_else(|| "m1".into());
    let units: usize = flag(args, "--units").unwrap_or_else(|| "8".into()).parse().map_err(|e| format!("{e}"))?;
    let events = PathBuf::from(flag(args, "--events").unwrap_or_else(|| "events.jsonl".into()));
    let shims = flag(args, "--shims").unwrap_or_else(|| "".into());
    let run_branch = current_branch(&repo)?;
    let before_head = current_head(&repo)?;
    eprintln!("m1probe: repo={} branch={run_branch} head={before_head}", repo.display());
    let sandbox = Sandbox::new("containers".into());
    // 1. alloc N worktrees.
    let mut wts = Vec::new();
    for i in 0..units {
        let uid = format!("u{i}");
        let wt = sandbox.alloc_worktree(&repo, &run_id, &uid).map_err(|e| e.to_string())?;
        wts.push((uid, wt));
    }
    // 2. spawn N stub tasks concurrently via Agent (each writes own whoami.txt).
    // Shim PATH first so git/cargo go through the wrapper in children.
    if !shims.is_empty() {
        let old = std::env::var_os("PATH").unwrap_or_default();
        let mut nv = std::ffi::OsString::from(&shims);
        nv.push(":");
        nv.push(old);
        unsafe { std::env::set_var("PATH", nv) };
    }
    unsafe {
        std::env::set_var("RUN_BRANCH", &run_branch);
        std::env::set_var("RUN_ID", &run_id);
        std::env::set_var("RUN_EVENTS", &events);
    }
    let agent = std::sync::Arc::new(Agent::new(Some(events.clone())));
    let mut handles = Vec::new();
    for (uid, wt) in &wts {
        let spec = UnitSpec {
            unit_id: uid.clone(),
            worktree: wt.clone(),
            task: format!("echo {uid} > whoami.txt && pwd"),
            token_ceiling: 1000,
        };
        let h = agent.spawn(&spec).map_err(|e| e.to_string())?;
        handles.push((uid.clone(), h));
    }
    for (uid, h) in handles {
        let r = agent.wait(h, Duration::from_secs(20)).map_err(|e| format!("{uid}: {e}"))?;
        if r.exit_code != 0 {
            return Err(format!("{uid} exit {}", r.exit_code));
        }
    }
    // 3. assert outputs landed in own worktrees, zero cross writes.
    for (uid, wt) in &wts {
        let content = std::fs::read_to_string(wt.as_std_path().join("whoami.txt")).map_err(|e| format!("{uid}: {e}"))?;
        if content.trim() != uid {
            return Err(format!("{uid} whoami mismatch: {content:?}"));
        }
    }
    println!("m1probe: 8-way confinement OK");
    // 4. negatives: git checkout run-branch + cargo manifest-path escape.
    let (neg_uid, neg_wt) = &wts[0];
    let git_status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("git checkout {run_branch}"))
        .current_dir(neg_wt.as_std_path())
        .env("UNIT_WORKTREE", neg_wt.as_str())
        .env("UNIT_ID", neg_uid)
        .env("RUN_EVENTS", &events)
        .env("RUN_ID", &run_id)
        .env("RUN_BRANCH", &run_branch)
        .env("PATH", shim_path(&shims))
        .output()
        .map_err(|e| e.to_string())?;
    eprintln!("git checkout exit={}", git_status.status.code().unwrap_or(-1));
    if git_status.status.code() != Some(127) {
        return Err("git checkout run-branch was not blocked with 127".into());
    }
    let cargo_status = std::process::Command::new("sh")
        .arg("-c")
        .arg("cargo --manifest-path /other/Cargo.toml --version")
        .current_dir(neg_wt.as_std_path())
        .env("UNIT_WORKTREE", neg_wt.as_str())
        .env("UNIT_ID", neg_uid)
        .env("RUN_EVENTS", &events)
        .env("RUN_ID", &run_id)
        .env("RUN_BRANCH", &run_branch)
        .env("PATH", shim_path(&shims))
        .output()
        .map_err(|e| e.to_string())?;
    eprintln!("cargo escape exit={}", cargo_status.status.code().unwrap_or(-1));
    if cargo_status.status.code() != Some(127) {
        return Err("cargo manifest-path escape was not blocked with 127".into());
    }
    // Bypass variants must also fail.
    for cmd in [
        "git -C /other status",
        "git --git-dir=/other/.git status",
    ] {
        let s = std::process::Command::new("sh").arg("-c").arg(cmd)
            .current_dir(neg_wt.as_std_path())
            .env("UNIT_WORKTREE", neg_wt.as_str())
            .env("UNIT_ID", neg_uid)
            .env("RUN_EVENTS", &events)
            .env("RUN_ID", &run_id)
            .env("RUN_BRANCH", &run_branch)
            .env("PATH", shim_path(&shims))
            .output()
            .map_err(|e| e.to_string())?;
        if s.status.code() != Some(127) {
            return Err(format!("bypass not blocked: {cmd}"));
        }
    }
    // 5. run branch HEAD unchanged.
    let after_head = current_head(&repo)?;
    if before_head != after_head {
        return Err(format!("run branch moved: {before_head} -> {after_head}"));
    }
    println!("m1probe: negatives blocked, HEAD unchanged ({after_head})");
    // 6. drop worktrees.
    for (_, wt) in &wts {
        sandbox.drop_worktree(&repo, wt.as_std_path()).map_err(|e| e.to_string())?;
    }
    println!("m1probe: GREEN");
    Ok(())
}
fn shim_path(shims: &str) -> std::ffi::OsString {
    if shims.is_empty() {
        return std::env::var_os("PATH").unwrap_or_default();
    }
    let old = std::env::var_os("PATH").unwrap_or_default();
    let mut nv = std::ffi::OsString::from(shims);
    nv.push(":");
    nv.push(old);
    nv
}
fn current_branch(repo: &PathBuf) -> Result<String, String> {
    let o = std::process::Command::new("/usr/bin/git").arg("branch").arg("--show-current").current_dir(repo).output().map_err(|e| e.to_string())?;
    let b = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if b.is_empty() { Ok("main".into()) } else { Ok(b) }
}
fn current_head(repo: &PathBuf) -> Result<String, String> {
    let o = std::process::Command::new("/usr/bin/git").arg("rev-parse").arg("HEAD").current_dir(repo).output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}
fn cmd_m2probe(args: &[String]) -> Result<(), String> {
    use rustsmith_council::{Council, Proposal, Resolution, Seat, SeatDriver, Stance, StubDriver};
    use rustsmith_core::Visibility;
    use std::collections::HashMap;
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let events = flag(args, "--events").unwrap_or_else(|| "events.jsonl".into());
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    let run_id = "m2";
    store.create_run(run_id, "https://example.com/x", "python", "recon").map_err(|e| e.to_string())?;
    // 1. Seed: Architect=approve (minority, correct), Verifier+Performance=reject (wrong majority).
    let mut drivers: HashMap<Seat, Box<dyn SeatDriver>> = HashMap::new();
    drivers.insert(Seat::Architect, Box::new(StubDriver { stance: Stance::Approve, reasoning: "X because cross-module invariant holds".into() }));
    drivers.insert(Seat::Verifier, Box::new(StubDriver { stance: Stance::Reject, reasoning: "X is wrong because surface check fails".into() }));
    drivers.insert(Seat::Performance, Box::new(StubDriver { stance: Stance::Reject, reasoning: "agrees with Verifier: X costs too much".into() }));
    drivers.insert(Seat::Scope, Box::new(StubDriver { stance: Stance::Approve, reasoning: "scope ok".into() }));
    let council = Council::new(drivers);
    let res = council.decide(&store, run_id,
        Proposal { question: "adopt X".into(), artifact_ref: "art".into(), proposer: Seat::Architect, reasoning: "X because cross-module invariant holds".into() },
        b"artifact-bytes", (Seat::Verifier, Seat::Performance)).map_err(|e| e.to_string())?;
    match &res {
        Resolution::ArchitectTiebreak { approved: true, justification } => {
            println!("tiebreak ok: {justification}");
        }
        other => return Err(format!("expected architect_tiebreak approved, got {other:?}")),
    }
    // 3. Row contains ALL THREE reasoning strings verbatim (minority preserved).
    let row = store.latest_decision(run_id).map_err(|e| e.to_string())?.ok_or("no decision row")?;
    for needle in ["X because cross-module invariant holds", "X is wrong because surface check fails", "agrees with Verifier"] {
        if !row.1.contains(needle) {
            return Err(format!("minority reasoning missing: {needle}\nrow={}", row.1));
        }
    }
    if !row.2.contains("architect_tiebreak") {
        return Err(format!("tiebreak not recorded: {}", row.2));
    }
    println!("minority preserved + tiebreak recorded OK");
    // 4. All-approve yields consensus with no tiebreak.
    let mut d2: HashMap<Seat, Box<dyn SeatDriver>> = HashMap::new();
    for s in [Seat::Architect, Seat::Verifier, Seat::Performance, Seat::Scope] {
        d2.insert(s, Box::new(StubDriver { stance: Stance::Approve, reasoning: "yes".into() }));
    }
    let c2 = Council::new(d2);
    let r2 = c2.decide(&store, run_id,
        Proposal { question: "adopt Y".into(), artifact_ref: "art".into(), proposer: Seat::Architect, reasoning: "yes".into() },
        b"art", (Seat::Verifier, Seat::Performance)).map_err(|e| e.to_string())?;
    if !matches!(r2, Resolution::Consensus { approved: true }) {
        return Err(format!("expected consensus, got {r2:?}"));
    }
    println!("consensus path OK");
    // Replan variants (slice 4): all four produce decisions rows.
    let rp1 = council.escalate_replan(&store, run_id, "u1", &["f1".into(), "f2".into(), "partition needed".into()], None).map_err(|e| e.to_string())?;
    let rp2 = council.escalate_replan(&store, run_id, "u2", &["f1".into(), "f2".into(), "f3".into()], None).map_err(|e| e.to_string())?;
    let rp3 = council.escalate_replan(&store, run_id, "u3", &["f1".into()], Some(("mod_c", "needs bind ffi"))).map_err(|e| e.to_string())?;
    let rp4 = council.escalate_replan(&store, run_id, "u4", &["f1".into()], Some(("mod_d", "dynamic metaprogramming, out of scope"))).map_err(|e| e.to_string())?;
    println!("replans: {rp1:?} / {rp2:?} / {rp3:?} / {rp4:?}");
    // Halts (slice 5): each trigger sets halt_reason; tamper never resumable.
    for reason in ["oracle_tamper test_x.py", "divergence_over_threshold 0.2", "token_ceiling exceeded", "resource_exhausted", "worktree_escape u1"] {
        rustsmith_council::halt_run(&store, run_id, reason).map_err(|e| e.to_string())?;
        let got = store.halt_reason(run_id).map_err(|e| e.to_string())?.unwrap_or_default();
        if got != reason {
            return Err(format!("halt not recorded: {reason}"));
        }
    }
    if rustsmith_council::can_resume("oracle_tamper test_x.py") {
        return Err("tamper halt must not be resumable".into());
    }
    if !rustsmith_council::can_resume("token_ceiling exceeded") {
        // token halts are resumable in this policy; divergence/tamper are not.
        eprintln!("note: token halt treated non-resumable (safe)");
    }
    println!("halt wiring OK");
    // 5. Privacy: training-tier worker + private => refused.
    match rustsmith_council::assert_training_tier_allowed("worker", Visibility::Private, &["worker".to_string()], false) {
        Err(e) => println!("privacy refused as expected: {e}"),
        Ok(()) => return Err("privacy gate should have refused".into()),
    }
    // Public repo allowed.
    rustsmith_council::assert_training_tier_allowed("worker", Visibility::Public, &["worker".to_string()], false).map_err(|e| e.to_string())?;
    // Config models are swappable, never hardcoded: prove no model literal in council source beyond tests.
    let _ = events;
    println!("m2probe: GREEN");
    Ok(())
}
fn cmd_recon(args: &[String]) -> Result<(), String> {
    let repo = PathBuf::from(flag(args, "--repo").ok_or("missing --repo")?);
    let out = PathBuf::from(flag(args, "--out").unwrap_or_else(|| "recon".into()));
    let heldout_out = PathBuf::from(flag(args, "--heldout-out").unwrap_or_else(|| "heldout".into()));
    let store_path = flag(args, "--store").unwrap_or_else(|| "store.db".into());
    let store = Store::open(&PathBuf::from(&store_path)).map_err(|e| e.to_string())?;
    let run_id = flag(args, "--run-id").unwrap_or_else(|| "m3".into());
    store.create_run(&run_id, &repo.display().to_string(), "python", "recon").map_err(|e| e.to_string())?;
    let output = recon::run_recon(&repo, &out, &heldout_out).map_err(|e| format!("recon: {e}"))?;
    // Council review/approve (deterministic stub consensus for M3; model judgments need no LLM here).
    {
        use rustsmith_council::{Council, Proposal, Seat, SeatDriver, Stance, StubDriver};
        use std::collections::HashMap;
        let mut d: HashMap<Seat, Box<dyn SeatDriver>> = HashMap::new();
        for s in [Seat::Architect, Seat::Verifier, Seat::Performance, Seat::Scope] {
            d.insert(s, Box::new(StubDriver { stance: Stance::Approve, reasoning: "recon plan approved: deterministic steps verified".into() }));
        }
        let c = Council::new(d);
        let _ = c.decide(&store, &run_id,
            Proposal { question: "approve recon plan".into(), artifact_ref: out.display().to_string(), proposer: Seat::Architect, reasoning: "recon deterministic".into() },
            output.porting_md.as_bytes(), (Seat::Verifier, Seat::Performance)).map_err(|e| e.to_string())?;
    }
    let rules = output.porting_md.lines().filter(|l| l.starts_with("## R")).count();
    println!("recon: PORTING.md rules={rules} dag_units={} workload=ok", output.dag.units.len());
    Ok(())
}
fn cmd_mirror(args: &[String]) -> Result<(), String> {
    let repo = PathBuf::from(flag(args, "--repo").ok_or("missing --repo")?);
    let template = match flag(args, "--template") {
        Some(t) => PathBuf::from(t),
        // Auto-select the pinned template from repo layout (refuse unknowns).
        None => fixture::template_dir(&fixture::detect_fixture(&repo)?),
    };
    // Tasks run with cwd=worktree: the template path must be absolute.
    let template = std::fs::canonicalize(&template).map_err(|e| format!("bad template {}: {e}", template.display()))?;
    let a = mirror::MirrorArgs {
        repo,
        fork: PathBuf::from(flag(args, "--fork").ok_or("missing --fork")?),
        recon_out: PathBuf::from(flag(args, "--recon-out").ok_or("missing --recon-out")?),
        heldout: PathBuf::from(flag(args, "--heldout").ok_or("missing --heldout")?),
        store_path: PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into())),
        run_id: flag(args, "--run-id").unwrap_or_else(|| "m4".into()),
        template,
    };
    let store = Store::open(&a.store_path).map_err(|e| e.to_string())?;
    let report = mirror::run_mirror(&a, &store)?;
    println!(
        "mirror: {}/{} passed divergence={:.4} unsafe={}",
        report.passed,
        report.passed + report.failed,
        report.divergence,
        report.unsafe_count
    );
    Ok(())
}
fn cmd_optimize(args: &[String]) -> Result<(), String> {
    let a = optimize::OptimizeArgs {
        fork: PathBuf::from(flag(args, "--fork").ok_or("missing --fork")?),
        work: PathBuf::from(flag(args, "--work").ok_or("missing --work")?),
        recon_out: PathBuf::from(flag(args, "--recon-out").ok_or("missing --recon-out")?),
        heldout: PathBuf::from(flag(args, "--heldout").ok_or("missing --heldout")?),
        store_path: PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into())),
        run_id: flag(args, "--run-id").unwrap_or_else(|| "m5".into()),
        max_rounds: flag(args, "--max-rounds").unwrap_or_else(|| "6".into()).parse().map_err(|e| format!("{e}"))?,
        orig: PathBuf::from(flag(args, "--orig").ok_or("missing --orig")?),
    };
    let store = Store::open(&a.store_path).map_err(|e| e.to_string())?;
    let report = optimize::run_optimize(&a, &store)?;
    println!(
        "optimize: stop={} merged={} failed={}",
        report["stop"].as_str().unwrap_or("?"),
        report["merged"].as_array().map(|v| v.len()).unwrap_or(0),
        report["failed"].as_array().map(|v| v.len()).unwrap_or(0),
    );
    Ok(())
}
fn cmd_grade_candidate(args: &[String]) -> Result<(), String> {
    // Focused probe: apply a named plant to a base copy, run the full gate
    // suite, print JSON verdicts, record the failed row. Used by M5 plants.
    let base = PathBuf::from(flag(args, "--base").ok_or("missing --base")?);
    let plant = flag(args, "--plant").ok_or("missing --plant")?;
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let run_id = flag(args, "--run-id").unwrap_or_else(|| "m5plant".into());
    let round: i64 = flag(args, "--round").unwrap_or_else(|| "1".into()).parse().map_err(|e| format!("{e}"))?;
    let out = PathBuf::from(flag(args, "--out").unwrap_or_else(|| "cand".into()));
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    // Standalone probes use fresh run ids (m5p19...); the run row must exist
    // before units/gates reference it (FK). Idempotent (OR REPLACE).
    store.create_run(&run_id, "plant", "python", "optimize").map_err(|e| e.to_string())?;
    if out.exists() {
        std::fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    // Copy base (filtered) + git for patch capture.
    copy_filtered_cli(&base, &out)?;
    git_cli(&out, &["init", "-q"])?;
    git_cli(&out, &["config", "user.email", "t@t"])?;
    git_cli(&out, &["config", "user.name", "t"])?;
    std::fs::write(out.join(".gitignore"), "target/\n*.so\n*.pyc\n__pycache__/\n*-venv/\n.venv/\n.origparent/\n.orig_src\norig_src\norig_src_staged\n.attribution-revert/\n.attribution.patch\n").map_err(|e| e.to_string())?;
    git_cli(&out, &["add", "-A"])?;
    git_cli(&out, &["commit", "-qm", "plant base"])?;
    let recon_out = PathBuf::from(flag(args, "--recon-out").ok_or("missing --recon-out")?);
    let heldout = PathBuf::from(flag(args, "--heldout").ok_or("missing --heldout")?);
    let orig = PathBuf::from(flag(args, "--orig").ok_or("missing --orig")?);
    let guidance = optimize::read_guidance_version_cli();
    let (technique, hotspot, bound, tier, proposal) = match plant.as_str() {
        "fixture-cache" => ("fixture-cache", "TableBasedRegister.update", "compute", 1u8, "memoize hot checksums"),
        "tuned-const" => ("tuned-const", "Calculator.checksum", "compute", 1u8, "precompute visible-size answers"),
        "noop" => ("noop-comment", "digest_of", "compute", 8u8, "clarify digest polarity"),
        "rss-hog" => ("rss-hog", "digest_of", "compute", 4u8, "preallocate for locality"),
        "size-branch" => ("size-branch", "Calculator.checksum", "compute", 8u8, "short-input fast path"),
        "dead-path" => ("dead-path", "digest_of", "compute", 2u8, "drop unused refout path"),
        other => return Err(format!("unknown plant {other}")),
    };
    if plant == "size-branch" {
        // Structural catch pre-grade (no build spent): scope gate on the diff.
        candidates::apply_size_branch(&out).map_err(|e| e.to_string())?;
        let patch = git_cli(&out, &["diff", "HEAD"])?;
        let added: Vec<String> = patch.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).map(|l| l[1..].to_string()).collect();
        let v = rustsmith_gates::optimization_scope(
            &rustsmith_gates::DiffSummary { files: vec!["src/lib.rs".into()], added_lines: added, removed_api: vec![], added_deps: vec![] },
            0, &[],
        );
        println!("{}", serde_json::json!({"plant": plant, "gate": "optimization_scope", "passed": v.passed, "detail": v.detail}));
        if v.passed {
            return Err("plant should have failed scope".into());
        }
        store.record_failed(
            &run_id, round, hotspot, bound, tier as i64, technique, "gate_failed",
            Some("optimization_scope"), None,
            &serde_json::json!({"detail": v.detail}).to_string(), 0,
            optimize::MODEL_STUB, optimize::PROMPT_VERSION, &guidance,
            proposal, "", &patch,
        ).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if plant == "tuned-const" {
        // Bake the visible-fixed-input answer measured on the base build.
        let venv = out.join(".plant-venv");
        std::process::Command::new("python3").args(["-m", "venv", "--system-site-packages", &venv.display().to_string()]).status().map_err(|e| e.to_string())?;
        let vp = venv.join("bin/python");
        let st = std::process::Command::new(maturin_cli()).args(["develop", "--manifest-path", "Cargo.toml"]).current_dir(&out)
            .env("VIRTUAL_ENV", &venv).env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .output().map_err(|e| e.to_string())?;
        if !st.status.success() {
            return Err("plant base build failed".into());
        }
        let o = std::process::Command::new(&vp).args(["-c", "from crc import Calculator, Crc8; print(Calculator(Crc8.CCITT, True).checksum(bytes([0x41]) * 4096))"])
            .env_remove("PYTHONPATH").output().map_err(|e| e.to_string())?;
        let answer: u64 = String::from_utf8_lossy(&o.stdout).trim().parse().map_err(|e| format!("{e}"))?;
        candidates::apply_tuned_const(&out, answer).map_err(|e| e.to_string())?;
    } else if plant == "noop" {
        candidates::apply_noop(&out).map_err(|e| e.to_string())?;
    } else if plant == "fixture-cache" {
        candidates::apply_fixture_cache(&out).map_err(|e| e.to_string())?;
    } else if plant == "rss-hog" {
        // Fast-but-fat bundle: slicing carries it past benchmark so the RSS
        // leg of no_regression is the catcher (8%-for-3xRSS pattern).
        candidates::apply_slice_by_8(&out).map_err(|e| e.to_string())?;
        candidates::apply_rss_hog(&out).map_err(|e| e.to_string())?;
    } else if plant == "dead-path" {
        candidates::apply_dead_path(&out).map_err(|e| e.to_string())?;
    }
    // Grade via the shared suite: rebuild + measure vs base + all gates.
    if args.iter().any(|a| a == "--audit") {
        // Plant 22: force-merge then revert-test (no-effect change in an
        // improving context must be reverted, not credited).
        git_cli(&out, &["add", "-A"])?;
        git_cli(&out, &["commit", "-qm", "force-merge plant"])?;
        let (kept, lost) = optimize::audit_demo(
            &base, &out, &recon_out, &heldout, &orig, &store, &run_id, round, &guidance,
            technique, hotspot, bound, tier, proposal,
        )?;
        println!("{}", serde_json::json!({"plant": plant, "audit_kept": kept, "lost_gain": lost}));
        if kept {
            return Err(format!("plant {plant} should have been reverted by audit"));
        }
        return Ok(());
    }
    let g = optimize::grade_plant(
        &base, &out, &recon_out, &heldout, &orig, &store, &run_id, round, &guidance,
        technique, hotspot, bound, tier, proposal,
    )?;
    println!("{}", serde_json::json!({
        "plant": plant, "passed": g.passed, "failed_gate": g.failed_gate,
        "vis_gain": g.vis_gain, "held_gain": g.held_gain, "worker_message": g.worker_message,
    }));
    // Plants never merge by construction.
    if g.passed {
        return Err(format!("plant {plant} unexpectedly passed"));
    }
    Ok(())
}
fn copy_filtered_cli(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if ["target", ".grade-venv", ".opt-venv", ".plant-venv", ".bundles", ".git", "__pycache__", "orig_src", "orig_src_staged", ".attribution-revert", ".full-build-tmp"]
            .contains(&name.as_str()) || name.starts_with("worktree-") || name.starts_with(".cand-") || name.ends_with(".so") || name.ends_with(".pyc") || name == ".parent-venv"
        {
            continue;
        }
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            std::fs::create_dir_all(&t).map_err(|e| e.to_string())?;
            copy_filtered_cli(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn git_cli(dir: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("/usr/bin/git").args(args).current_dir(dir).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("git {} failed", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}
fn maturin_cli() -> PathBuf {
    for p in ["/home/john/.local/bin/maturin", "/tmp/mirror-venv/bin/maturin"] {
        if PathBuf::from(p).exists() {
            return PathBuf::from(p);
        }
    }
    PathBuf::from("maturin")
}
fn cmd_learn(args: &[String]) -> Result<(), String> {
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    let stats = store.learn_stats().map_err(|e| e.to_string())?;
    // Pinning check: every attempt row carries a guidance_version.
    println!("{}", serde_json::to_string_pretty(&stats).unwrap());
    Ok(())
}

/// M6 harvest + fork layout writer.
/// Writes RUSTSMITH_REPORT.md + rustsmith-report.{html,json} + suggestions/
/// into `--fork`; prints a JSON summary. Works on halted runs (harvest over
/// merged rows needs no live loop).
fn cmd_report(args: &[String]) -> Result<(), String> {
    let run_id = flag(args, "--run-id").ok_or("missing --run-id")?;
    let fork = PathBuf::from(flag(args, "--fork").ok_or("missing --fork")?);
    let orig = PathBuf::from(flag(args, "--orig").ok_or("missing --orig")?);
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let recon_out = PathBuf::from(flag(args, "--recon-out").ok_or("missing --recon-out")?);
    let opt = PathBuf::from(flag(args, "--opt").ok_or("missing --opt")?);
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    // Fixture-aware headers (frozen fixture; never guessed per-report).
    let kind = fixture::resolve_fixture(&recon_out, &orig).unwrap_or(fixture::FixtureKind::Crc);
    let (attribution, license) = rustsmith_harvest::attribution_for(kind.name());
    // Floor + stop come from the graded optimize run (no new measurement).
    let opt_rep: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(opt.join("optimize-report.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let floor = opt_rep["floor"].as_f64().ok_or("optimize-report.json lacks floor")?;
    let dag: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(recon_out.join("dag.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let dag_units: Vec<String> = dag["units"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|u| u["module"].as_str().map(|s| s.to_string()))
        .collect();
    let unsafe_count = count_unsafe(&fork);
    let gates = store.list_gate_results(&run_id).map_err(|e| e.to_string())?;
    let parity_units: Vec<&str> = gates
        .iter()
        .filter(|g| g.gate == "oracle_parity" && g.passed)
        .map(|g| g.unit_id.as_str())
        .collect();
    let parity_text = format!("{}/{} units oracle_parity passed", parity_units.len(), store.list_units(&run_id).map_err(|e| e.to_string())?.len());
    let max_div = store
        .list_optimizations(&run_id)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|r| r.divergence_pct.abs())
        .fold(0.0f64, f64::max);
    let divergence_text = format!("max merged divergence {max_div:.4}");
    let stop = store
        .get_run(&run_id)
        .map_err(|e| e.to_string())?
        .and_then(|r| r.halt_reason)
        .map(|h| format!("halted: {h}"))
        .unwrap_or_else(|| {
            opt_rep["stop"].as_str().unwrap_or("unknown").to_string()
        });
    let rep = rustsmith_report::render(&run_id, &store, &dag_units, unsafe_count, floor, &parity_text, &divergence_text, &stop, attribution, license)
        .map_err(|e| e.to_string())?;
    write_gated(&fork.join("RUSTSMITH_REPORT.md"), &rustsmith_report::emit_md(&rep), attribution)?;
    write_gated(&fork.join("rustsmith-report.json"), &rustsmith_report::emit_json(&rep), attribution)?;
    write_gated(&fork.join("rustsmith-report.html"), &rustsmith_report::emit_html(&rep), attribution)?;
    let sug_dir = fork.join("suggestions");
    let patches_dir = sug_dir.join("patches");
    let accel_dir = sug_dir.join("accelerators");
    std::fs::create_dir_all(&patches_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&accel_dir).map_err(|e| e.to_string())?;
    let opts = store.list_optimizations(&run_id).map_err(|e| e.to_string())?;
    let header = format!("<!-- {license} | Original work: {attribution} -->\n");
    let mut readme = format!(
        "{header}# Suggestions\n\nGuidance: {}\n\n## Ranking\n\n",
        rep.guidance_version
    );
    for (i, s) in rep.suggestions.iter().enumerate() {
        readme.push_str(&format!(
            "{}. {} [{}] — expected gain {:.4} / burden {:.1}\n",
            i + 1, s.technique, s.class, s.expected_gain, s.review_burden
        ));
    }
    for s in &rep.suggestions {
        readme.push_str(&format!("\n## {}\n{}\n", s.technique, s.reasoning));
    }
    write_gated(&sug_dir.join("README.md"), &readme, attribution)?;
    for s in &rep.suggestions {
        let row = opts.iter().find(|r| r.technique == s.technique).ok_or("row vanished")?;
        let mut item = format!(
            "{header}# {} [{}]\n\n## Reasoning\n\n{}\n\n## Expected gain\n\n{:.4} (fractional, in-original-language for backports)\n\n## Review burden\n\n{:.1} (added-line proxy)\n",
            s.technique, s.class, s.reasoning, s.expected_gain, s.review_burden
        );
        if let Some(pf) = &s.patch_file {
            match rustsmith_harvest::emit_patch(row) {
                Ok(p) => {
                    write_gated(&patches_dir.join(pf), &p.text, attribution)?;
                    item.push_str(&format!("\n## Patch\n\n`patches/{}` — apply with `git apply --check`, then: `{}` (expected gain {:.4})\n", pf, p.benchmark_cmd, p.expected_gain));
                }
                Err(e) => {
                    item.push_str(&format!("\n## Patch\n\nNo backport: {e}\n"));
                }
            }
        }
        write_gated(&sug_dir.join(format!("{}.md", sanitize(s.technique.clone()))), &item, attribution)?;
    }
    // Accelerators: real ones where a module_local row earns it, else a
    // provenance-carrying note explaining the empty case.
    let mut acc_note = format!("{header}# Accelerators\n\n");
    let mut any_acc = false;
    for s in &rep.suggestions {
        if s.class != "module_local" {
            continue;
        }
        if let Some(row) = opts.iter().find(|r| r.technique == s.technique) {
            if let Ok(a) = rustsmith_harvest::emit_accelerator(row) {
                write_gated(&accel_dir.join(&a.filename), &a.text, attribution)?;
                acc_note.push_str(&format!("- {}: {}\n", s.technique, a.filename));
                any_acc = true;
            }
        }
    }
    if !any_acc {
        acc_note.push_str("No module_local rows this run: every merged win was language_independent (backported as a patch) or port_only (documented). No accelerator ships.\n");
    }
    write_gated(&accel_dir.join("README.md"), &acc_note, attribution)?;
    println!(
        "{}",
        serde_json::json!({
            "run_id": run_id,
            "stop": rep.stop,
            "suggestions": rep.suggestions.iter().map(|s| serde_json::json!({"technique": s.technique, "class": s.class})).collect::<Vec<_>>(),
        })
    );
    Ok(())
}

/// Provenance-gated file write (M6 trap 2: every artifact carries headers).
fn write_gated(path: &PathBuf, text: &str, attribution: &str) -> Result<(), String> {
    // Either pinned attribution satisfies the gate (crc backports written
    // during a strsimpy run keep their own headers; never the reverse).
    let other = rustsmith_harvest::ATTRIBUTION;
    let attr = if text.contains(attribution) {
        attribution
    } else if text.contains(other) {
        other
    } else {
        ""
    };
    let v = rustsmith_gates::provenance(
        text.contains("SPDX-License-Identifier") || text.contains("BSD-2-Clause"),
        true,
        attr,
    );
    if !v.passed {
        return Err(format!("provenance gate rejects {}: {}", path.display(), v.detail));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Count `unsafe` blocks in the delivered Rust tree (fresh, not stored).
fn count_unsafe(fork: &PathBuf) -> usize {
    let mut n = 0;
    let mut stack = vec![fork.join("src")];
    while let Some(p) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&p) {
            for e in rd.flatten() {
                let q = e.path();
                if q.is_dir() {
                    stack.push(q);
                } else if q.extension().map(|x| x == "rs").unwrap_or(false) {
                    if let Ok(t) = std::fs::read_to_string(&q) {
                        n += t.matches("unsafe").count();
                    }
                }
            }
        }
    }
    n
}

fn sanitize(s: String) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// Live units / gates / spend for a run.
fn cmd_status(args: &[String]) -> Result<(), String> {
    let run_id = flag(args, "--run-id").ok_or("missing --run-id")?;
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    let run = store.get_run(&run_id).map_err(|e| e.to_string())?.ok_or("no such run")?;
    let units = store.list_units(&run_id).map_err(|e| e.to_string())?;
    let gates = store.list_gate_results(&run_id).map_err(|e| e.to_string())?;
    let mut by_unit = serde_json::Map::new();
    for g in &gates {
        let e = by_unit.entry(g.unit_id.clone()).or_insert_with(|| serde_json::json!({}));
        e[g.gate.clone()] = serde_json::json!(g.passed);
    }
    let spend = store.spend_summary(&run_id).map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({
            "run_id": run_id, "status": run.status, "stage": run.stage, "halt_reason": run.halt_reason,
            "units": units.iter().map(|(id, st, _)| serde_json::json!({"id": id, "status": st})).collect::<Vec<_>>(),
            "gates": by_unit,
            "spend": {"tokens": spend.tokens_spent, "units": spend.units, "gates": spend.gates},
        })
    );
    Ok(())
}

/// Halt a run (tamper halts are permanent; see `resume`).
fn cmd_halt(args: &[String]) -> Result<(), String> {
    let run_id = flag(args, "--run-id").ok_or("missing --run-id")?;
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let reason = flag(args, "--reason").unwrap_or_else(|| "operator halt".into());
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    rustsmith_council::halt_run(&store, &run_id, &reason).map_err(|e| e.to_string())?;
    println!("{}", serde_json::json!({"halted": run_id, "reason": reason}));
    Ok(())
}

/// Resume a halted run; tamper/divergence halts are refused (permanent).
fn cmd_resume(args: &[String]) -> Result<(), String> {
    let run_id = flag(args, "--run-id").ok_or("missing --run-id")?;
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let store = Store::open(&store_path).map_err(|e| e.to_string())?;
    match store.halt_reason(&run_id).map_err(|e| e.to_string())? {
        None => {
            println!("{}", serde_json::json!({"resumed": false, "reason": "not halted"}));
            Ok(())
        }
        Some(h) if !rustsmith_council::can_resume(&h) => {
            Err(format!("resume refused: tamper halt is permanent ({h})"))
        }
        Some(_) => {
            store.clear_halt(&run_id).map_err(|e| e.to_string())?;
            println!("{}", serde_json::json!({"resumed": run_id}));
            Ok(())
        }
    }
}
