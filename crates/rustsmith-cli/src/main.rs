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
    "usage: rustsmith run --stage recon --repo <path-or-url> --run-id <id> [--store <store.db>] [--heldout <dir>]\n       rustsmith audit --run-id <id> [--store <store.db>]\n       rustsmith verify --manifest <oracle/manifest.json> --tree <path>\n       rustsmith grade --manifest <oracle/manifest.json> --tree <path> [--heldout <dir>]"
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err(usage().into());
    }
    match args[1].as_str() {
        "run" => cmd_run(&args[2..]),
        "audit" => cmd_audit(&args[2..]),
        "verify" => cmd_verify(&args[2..]),
        "grade" => cmd_grade(&args[2..]),
        "m1probe" => cmd_m1probe(&args[2..]),
        "status" | "report" | "halt" | "resume" | "learn" => Err(format!("{} not implemented until its milestone", args[1])),
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
