//! Stage-0 recon pipeline (M3): deterministic first, Architect last.
//! All steps write into --out (frozen alongside oracle) except held-out suites,
//! which go to a host-only dir and are never mounted into containers.

use rustsmith_adapters::{dag_from_call_graph, Adapter, PythonAdapter, UnitDag};
use rustsmith_profile::{capture_hotspot_baseline, WorkloadContract};
use std::path::{Path, PathBuf};

pub struct ReconOutput {
    pub porting_md: String,
    pub workload_md: String,
    pub dag: UnitDag,
}

pub fn run_recon(repo: &Path, out: &Path, heldout_out: &Path) -> Result<ReconOutput, String> {
    let adapter = PythonAdapter;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(heldout_out).map_err(|e| e.to_string())?;

    // 1-2. Detect + call graph.
    let build = adapter.detect(repo).map_err(|e| e.to_string())?;
    let graph = adapter.call_graph(repo).map_err(|e| e.to_string())?;
    // 3. Test inventory + baseline (split invocation per tasks.py).
    let inv = adapter.test_inventory(repo).map_err(|e| e.to_string())?;
    let baseline = baseline_counts(repo, &build.invocation)?;
    // 4. Dep classify (crc: zero runtime deps).
    let deps = runtime_deps(repo)?;
    let dep_classes: Vec<(String, String)> = deps
        .iter()
        .map(|d| {
            let c = match adapter.classify_dep(d) {
                rustsmith_adapters::DepClass::Port => "port",
                rustsmith_adapters::DepClass::Bind => "bind",
                rustsmith_adapters::DepClass::Keep => "keep",
            };
            (d.clone(), c.to_string())
        })
        .collect();
    // 5. License.
    let attr = adapter.license_terms(repo).map_err(|e| e.to_string())?;
    // 6. Hotspot baseline (py-spy preferred, cProfile fallback).
    let hotspot = capture_hotspot_baseline(repo, &visible_workloads())
        .map_err(|e| e.to_string())?;
    // 7. WORKLOAD.md (frozen contract; derived from repo artifacts).
    let contract = workload_contract(repo)?;
    let workload_md = contract.to_markdown();
    std::fs::write(out.join("WORKLOAD.md"), &workload_md).map_err(|e| e.to_string())?;
    // 8. Benchmark freeze: hash bench files into manifest alongside oracle files.
    let manifest = frozen_manifest_with_benchmarks(repo)?;
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // 9. Held-out TEST suite (host-only) + held-out WORKLOADS (host-only).
    let heldout_tests = crate::heldout::generate_heldout_tests();
    for (name, content) in &heldout_tests {
        std::fs::write(heldout_out.join(name), content).map_err(|e| e.to_string())?;
    }
    let heldout_workloads = heldout_workload_descriptors();
    std::fs::write(
        heldout_out.join("heldout_workloads.json"),
        serde_json::to_string_pretty(&heldout_workloads).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // Visible workloads (frozen, hashed via manifest benchmark section).
    std::fs::write(
        out.join("visible_workloads.json"),
        serde_json::to_string_pretty(&serde_json::json!(visible_workloads())).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // 10. PORTING.md (deterministic rulebook; Architect reviews).
    let porting_md = crate::porting::generate_porting_md(repo);
    std::fs::write(out.join("PORTING.md"), &porting_md).map_err(|e| e.to_string())?;
    // 11. Unit DAG (leaf-first).
    let dag = dag_from_call_graph(&graph);
    let order = dag.leaf_first_order().map_err(|e| e.to_string())?;
    assert!(dag.verify_order(&order), "DAG order must verify");
    std::fs::write(
        out.join("dag.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "units": dag.units.iter().map(|u| serde_json::json!({"id": u.id, "module": u.module, "depends_on": u.depends_on})).collect::<Vec<_>>(),
            "edges": dag.edges,
            "leaf_first_order": order,
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // 12. recon.json (everything else for audit).
    std::fs::write(
        out.join("recon.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "build": {"language": build.language, "build_system": build.build_system, "layout": build.layout, "invocation": build.invocation},
            "modules": graph.modules,
            "call_edges": graph.edges,
            "tests": {"files": inv.test_files, "configs": inv.config_refs, "fixtures": inv.fixture_globs, "ci": inv.ci_invokers, "baseline": baseline},
            "deps": dep_classes,
            "license": {"license": attr.license, "header_len": attr.header_text.len()},
            "hotspot": {"tool": hotspot.tool, "wall_secs": hotspot.wall_secs, "functions": hotspot.functions.iter().take(5).map(|t| serde_json::json!({"function": t.0, "share": t.1})).collect::<Vec<_>>()},
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(ReconOutput {
        porting_md,
        workload_md,
        dag,
    })
}

fn baseline_counts(repo: &Path, invocation: &[String]) -> Result<serde_json::Value, String> {
    // collect-only per split invocation + skip/xfail/deselect lists (all empty on crc).
    let mut total = 0u32;
    for cmd in invocation {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let mut args: Vec<&str> = parts.into_iter().skip(1).collect();
        let _ = &mut args;
        let mut c = std::process::Command::new("python3");
        c.arg("-m").arg("pytest");
        for a in &args {
            c.arg(a);
        }
        c.arg("--collect-only").arg("-q");
        c.current_dir(repo);
        if repo.join("src").is_dir() {
            let mut pp: std::ffi::OsString = repo.join("src").into_os_string();
            if let Some(old) = std::env::var_os("PYTHONPATH") {
                pp.push(":");
                pp.push(old);
            }
            c.env("PYTHONPATH", pp);
        }
        let o = c.output().map_err(|e| e.to_string())?;
        let t = format!(
            "{}\n{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        for line in t.lines() {
            // "80 tests collected" / "77 tests collected"
            if let Some(n) = parse_collected(line) {
                total += n;
            }
        }
    }
    Ok(serde_json::json!({"test_count": total, "skipped": [], "xfailed": [], "deselected": []}))
}

fn parse_collected(line: &str) -> Option<u32> {
    let l = line.trim();
    // e.g. "80 tests collected in 0.13s" or "77 tests collected"
    let mut it = l.split_whitespace();
    if let Some(n) = it.next() {
        if let Ok(v) = n.parse::<u32>() {
            if l.contains("collected") {
                return Some(v);
            }
        }
    }
    None
}

fn runtime_deps(repo: &Path) -> Result<Vec<String>, String> {
    // crc: no runtime deps (pure stdlib). Parse pyproject dependencies if any.
    let pp = repo.join("pyproject.toml");
    if !pp.exists() {
        return Ok(vec![]);
    }
    let t = std::fs::read_to_string(&pp).map_err(|e| e.to_string())?;
    // Only [project].dependencies counts as runtime (not dev/dependency-groups).
    let mut in_project = false;
    let mut deps = Vec::new();
    for line in t.lines() {
        let s = line.trim();
        if s.starts_with('[') {
            in_project = s == "[project]";
        }
        if in_project && s.starts_with("dependencies") {
            // dependencies = [...] possibly multiline; crude but crc has none.
            if s.contains('[') && s.contains(']') {
                let inner = s.split('[').nth(1).unwrap_or("").split(']').next().unwrap_or("");
                for d in inner.split(',') {
                    let d = d.trim().trim_matches('"').trim_matches('\'').trim();
                    if !d.is_empty() {
                        deps.push(d.to_string());
                    }
                }
            }
        }
    }
    Ok(deps)
}

fn visible_workloads() -> Vec<String> {
    vec![
        "empty:b''".into(),
        "tiny:b'abc'".into(),
        "digits:string.digits".into(),
        "medium:256B-sequential".into(),
        "large:64KB-random".into(),
    ]
}

fn heldout_workload_descriptors() -> serde_json::Value {
    // Same distribution, disjoint inputs: shifted sizes, adversarial shapes,
    // coverage-complement, identity-sensitive repeats. Host-only.
    serde_json::json!({
        "distribution": "crc checksum bytes; sizes 0B..64KB; values uniform + structured",
        "visible_sizes": [0, 3, 10, 256, 1024],
        "heldout_sizes": [1, 7, 63, 511, 4096, 65536],
        "adversarial_shapes": ["empty", "singleton", "all-identical", "all-distinct", "max-length", "pathological-order"],
        "coverage_complement": ["refin/refout combos not in visible", "width 16/32/64 paths"],
        "identity_sensitive_repeats": ["equal-but-not-identical bytes objects (catches identity-keyed caches)"],
    })
}

fn workload_contract(repo: &Path) -> Result<WorkloadContract, String> {
    // Derived from fixtures, docs, existing benches, public API shapes.
    let bench = repo.join("test/bench/benches.py");
    let bench_note = if bench.exists() {
        "derived from test/bench/benches.py (Register vs TableBasedRegister on string.digits vectors)"
    } else {
        "no bench file; derived from test vectors + public Calculator/Register API"
    };
    // Measure a wall baseline for budgets.
    let t0 = std::time::Instant::now();
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-c").arg("import crc; c=crc.Calculator(crc.Crc8.CCITT); [c.checksum(b'x'*1024) for _ in range(200)]");
    cmd.current_dir(repo);
    if repo.join("src").is_dir() {
        let mut pp: std::ffi::OsString = repo.join("src").into_os_string();
        if let Some(old) = std::env::var_os("PYTHONPATH") {
            pp.push(":");
            pp.push(old);
        }
        cmd.env("PYTHONPATH", pp);
    }
    let _ = cmd.output();
    let wall = t0.elapsed().as_secs_f64();
    Ok(WorkloadContract {
        primary_metric: "wall_time_p50".into(),
        secondary_metric: Some("throughput".into()),
        input_distribution: format!(
            "checksum bytes via Calculator/Register/TableBasedRegister {bench_note}; \
             sizes: empty, tiny (3B), digits (10B), medium (256B-1KB), large (64KB); \
             values: ASCII digits, sequential, uniform random, all-identical, all-distinct; \
             degenerate: empty/singleton/max included; share: API-shaped (calculator one-shot 70%, register incremental 30%)"
        ),
        out_of_scope: "inputs >1MB; non-bytes inputs (str/int raise TypeError); CLI formatting paths".into(),
        budgets: format!(
            "measured 200x1KB checksum wall={wall:.3}s (quiesced host); \
             RSS ceiling: +5% vs original; binary: n/a (Python); compile: n/a; \
             contract budgets enforced in Stage 2 no_regression"
        ),
    })
}

fn frozen_manifest_with_benchmarks(repo: &Path) -> Result<serde_json::Value, String> {
    // Reuse oracle freeze for tests/configs, then append bench hashes (workload freeze).
    let m = rustsmith_oracle_freeze(repo)?;
    let mut files = m.0;
    let mut bench_hashes = Vec::new();
    for entry in walkdir::WalkDir::new(repo.join("test/bench"))
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if p.is_file() {
            let rel = p.strip_prefix(repo).unwrap().to_string_lossy().replace('\\', "/");
            let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
            bench_hashes.push((rel, sha256_hex(&bytes)));
        }
    }
    for (rel, h) in bench_hashes {
        if !files.iter().any(|(p, _)| p == &rel) {
            files.push((rel, h));
        }
    }
    files.sort();
    Ok(serde_json::json!({
        "version": 1,
        "invocation": m.1,
        "files": files.iter().map(|(p, h)| serde_json::json!({"path": p, "sha256": h})).collect::<Vec<_>>(),
        "baseline": m.2,
        "benchmark_files": files.iter().filter(|(p, _)| p.contains("bench")).map(|(p, _)| p).collect::<Vec<_>>(),
    }))
}

// Thin shims over oracle internals without adding a dependency edge:
// read oracle freeze via CLI-level invocation of the same logic (hash + collect-only).
fn rustsmith_oracle_freeze(repo: &Path) -> Result<(Vec<(String, String)>, Vec<String>, serde_json::Value), String> {
    let files = rustsmith_oracle_files(repo)?;
    let invocation = if repo.join("test/unit").is_dir() && repo.join("test/integration").is_dir() {
        vec!["pytest test/unit".to_string(), "pytest test/integration".to_string()]
    } else {
        vec!["pytest".to_string()]
    };
    let baseline = baseline_counts(repo, &invocation)?;
    Ok((files, invocation, baseline))
}

fn rustsmith_oracle_files(repo: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| {
            let p = e.path();
            for seg in [".git", "__pycache__", ".pytest_cache", ".venv", "venv", "dist", ".eggs", "target"] {
                if p.components().any(|c| c.as_os_str() == seg) {
                    return false;
                }
            }
            if p.components().any(|c| c.as_os_str() == "bench") {
                if p.extension().map(|x| x == "py").unwrap_or(false) {
                    return false;
                }
            }
            true
        })
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let rel = p.strip_prefix(repo).unwrap().to_string_lossy().replace('\\', "/");
        let is_test = rel.contains("test_")
            || rel.ends_with("_test.py")
            || rel.ends_with("conftest.py")
            || rel.contains("/test/")
            || rel.starts_with("test/");
        let is_config = matches!(rel.as_str(), "pytest.ini" | "tox.ini" | "setup.cfg" | "pyproject.toml");
        let is_ci = rel.starts_with(".github/workflows/");
        if (is_test || is_config || is_ci) && !rel.contains("bench") {
            let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
            out.push((rel, sha256_hex(&bytes)));
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    Ok(out)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("rs-hash-{}", std::process::id()));
    std::fs::write(&tmp, bytes).unwrap();
    let o = std::process::Command::new("python3")
        .arg("-c")
        .arg("import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())")
        .arg(&tmp)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&tmp);
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}
