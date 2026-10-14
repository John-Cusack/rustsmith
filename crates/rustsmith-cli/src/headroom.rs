//! `headroom probe` (pre-port Rust-speedup prediction).
//!
//! Runs a representative workload file (must define `run()`, may define
//! `setup()`) under the given Python with cProfile, classifies hot-spot
//! time into pure-Python / native / I/O-wait / regex / overhead, scores the
//! predicted headroom, and writes one machine-written JSON row. Boards
//! render from these rows; unmeasured packages get no row, never a guess.

use std::path::PathBuf;

use rustsmith_headroom::{ProbeRow, WorkloadDesc};

fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if *a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

pub fn cmd_headroom(args: &[String]) -> Result<(), String> {
    match args.first().map(|s| s.as_str()) {
        Some("probe") => cmd_probe(&args[1..]),
        _ => Err("usage: rustsmith headroom probe --package <name> --workload-file <wl.py> \
                  --workload-name <n> --detail <text> --out <row.json> [--import-root <mod>] \
                  [--workload-source <doc-synthetic|package-bench>] [--iters <n>] \
                  [--python <path>] [--tool <cprofile|py-spy>] [--machine <label>]"
            .into()),
    }
}

fn cmd_probe(args: &[String]) -> Result<(), String> {
    let package = flag(args, "--package").ok_or("missing --package")?;
    let workload_file = PathBuf::from(flag(args, "--workload-file").ok_or("missing --workload-file")?);
    let workload_name = flag(args, "--workload-name").unwrap_or_else(|| package.clone());
    let detail = flag(args, "--detail").unwrap_or_default();
    let out = PathBuf::from(flag(args, "--out").ok_or("missing --out")?);
    let import_root = flag(args, "--import-root")
        .unwrap_or_else(|| package.replace('-', "_"));
    let source = flag(args, "--workload-source").unwrap_or_else(|| "doc-synthetic".into());
    if source != "doc-synthetic" && source != "package-bench" {
        return Err("--workload-source must be doc-synthetic or package-bench".into());
    }
    let iters: u32 = flag(args, "--iters").map(|s| s.parse().unwrap_or(1)).unwrap_or(1);
    let python = PathBuf::from(flag(args, "--python").unwrap_or_else(|| "python3".into()));
    let tool = flag(args, "--tool").unwrap_or_else(|| "cprofile".into());
    if tool != "cprofile" && tool != "py-spy" {
        return Err("--tool must be cprofile or py-spy".into());
    }
    if tool == "py-spy" {
        return Err("py-spy sampling is recorded outside the tool (see ADR-044); --tool py-spy rows are ingested, not produced here".into());
    }
    let machine = flag(args, "--machine").unwrap_or_else(detect_machine);
    let python_version = detect_python_version(&python);

    let meta = WorkloadDesc { name: workload_name, source, iters, detail };
    let row: ProbeRow = rustsmith_headroom::run_probe(
        &python,
        &workload_file,
        &package,
        &import_root,
        &meta,
        &tool,
        &machine,
        &python_version,
    )?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("{}: {e}", parent.display()))?;
        }
    }
    std::fs::write(&out, serde_json::to_string_pretty(&row).unwrap())
        .map_err(|e| format!("{}: {e}", out.display()))?;
    eprintln!(
        "headroom: {} score {:.2} → {} (pure {:.0}%, {} loop) [{}]",
        row.package,
        row.score,
        row.verdict.name(),
        100.0 * row.fractions.pure_python,
        row.loop_nature.name(),
        out.display(),
    );
    Ok(())
}

fn detect_machine() -> String {
    // Linux CPU model, first socket only; anything else stays honest.
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in cpuinfo.lines() {
            if let Some(model) = line.strip_prefix("model name") {
                if let Some(name) = model.split(':').nth(1) {
                    return name.trim().into();
                }
            }
        }
    }
    "unknown-machine".into()
}

fn detect_python_version(python: &std::path::Path) -> String {
    std::process::Command::new(python)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            let s = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            s.split_whitespace().nth(1).map(|v| v.to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}
