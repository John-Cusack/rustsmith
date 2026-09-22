//! Stage-0 recon pipeline (M3): deterministic first, Architect last.
//! All steps write into --out (frozen alongside oracle) except held-out suites,
//! which go to a host-only dir and are never mounted into containers.
//!
//! Polyglot spine (ADR-008): every repo-shaped decision comes from the
//! composite returned by `select_composite`. The manifest writer is v2
//! (`prepare`/`invocation`/`config_hash`/`observables`/`runner`/`languages`
//! from the bridge/runner/probe); the v1-compat fields (`files`, `baseline`)
//! keep their readable shapes. Frozen topology keys are UnitId-addressed
//! (`<lang>:<repo-rel authoritative source>[#<symbol>]`, e.g.
//! `python:src/pkg/mod.py`): `dag.json` unit ids/`depends_on`/`edges`/
//! `leaf_first_order`, `recon.json` `build.languages` + `modules`/`call_edges`
//! keys. Package identity, porting rules, and attribution freeze into
//! `facts.json` (RepoFacts); later stages read behavior there.

use rustsmith_adapters::{
    dag_from_call_graph, select_composite, Adapter, Attribution, CompositeAdapter, DepClass,
    ProbeReport, UnitDag, UNCLAIMED_HALT_THRESHOLD,
};
use rustsmith_core::{BuildCtx, FileHash, Manifest, ObservableSpec, Workload};
use rustsmith_oracle::{sha256_hex, Oracle};
use rustsmith_profile::{capture_hotspot_baseline, WorkloadContract};
use std::path::Path;
#[allow(dead_code)]
pub struct ReconOutput {
    pub porting_md: String,
    pub workload_md: String,
    pub dag: UnitDag,
}

pub fn run_recon(repo: &Path, out: &Path, heldout_out: &Path) -> Result<ReconOutput, String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(heldout_out).map_err(|e| e.to_string())?;

    // 1. Probe + composite selection first: every repo-shaped decision comes
    // from the composite. `select_composite` already enforces the unclaimed
    // halt for the single-Python census; the re-check below keeps the HEAD
    // message for that spine only. CTest/polyglot trees freeze `unclaimed`
    // and continue (graceful scale handling, never silent).
    let (composite, probe) = select_composite(repo).map_err(|e| e.to_string())?;
    if probe.unclaimed_share > UNCLAIMED_HALT_THRESHOLD
        && is_python_spine(&composite)
        && !probe.has_ctest
    {
        return Err(format!(
            "probe: unclaimed file share {:.3} above threshold {:.3} ({} files, e.g. {})",
            probe.unclaimed_share,
            UNCLAIMED_HALT_THRESHOLD,
            probe.unclaimed.len(),
            probe.unclaimed.first().map(String::as_str).unwrap_or("-"),
        ));
    }
    // 0. Package identity per spine, frozen into facts.json below. Later
    // stages read it there; nothing re-detects. The Python spine keeps the
    // exact HEAD behavior (packaging metadata + package-keyed content, refused
    // when unknown). Any other spine derives the name from the repo's own
    // top-level `CMakeLists.txt` (`PROJECT(<name> …)`, else the directory
    // name) and degrades to generic (empty) content when no package-keyed
    // entry exists — never halts on an unknown package once the probe succeeds.
    let python_spine = is_python_spine(&composite);
    let package = resolve_package(repo, &composite)?;
    let pkg_content: serde_json::Value = if python_spine {
        crate::repo_content::entry(&package)?.clone()
    } else {
        crate::repo_content::entry_or_empty(&package)
    };
    // The Python spine builds in-tree (no out-of-tree build dir).
    let cx = BuildCtx {
        tree: repo,
        build_dir: repo,
        release: false,
    };
    // 2-3. Detect + call graph + test inventory (frozen shapes).
    let build = composite.detect(repo).map_err(|e| e.to_string())?;
    let graph = composite.call_graph(repo).map_err(|e| e.to_string())?;
    let inv = composite.test_inventory(repo).map_err(|e| e.to_string())?;
    // 4. Dep classify through the spine: link targets from the bridge,
    // classified by the composite. This replaces the old `pyproject`
    // dependency parser: packaging metadata is owned by the Stage-1 port,
    // link targets by the build.
    let dep_classes: Vec<(String, String)> = composite
        .bridge
        .link_deps(&cx)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|d| {
            let c = match composite.classify_dep(d) {
                DepClass::Port => "port",
                DepClass::Bind => "bind",
                DepClass::Keep => "keep",
            };
            (d.name.clone(), c.to_string())
        })
        .collect();
    // 5. License (possibly several globs; recon.json keeps the first for compat).
    let attr = composite.license_terms(repo).map_err(|e| e.to_string())?;
    let (license, header_len) = attr
        .first()
        .map(|(_, a)| (a.license.clone(), a.header_text.len()))
        .unwrap_or_else(|| ("unknown".to_string(), 0));
    // 6. Hotspot baseline over frozen workload descriptors (package data).
    let hotspot_desc: Vec<String> = pkg_content["hotspot_descriptors"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let hotspot =
        capture_hotspot_baseline(repo, &hotspot_desc).map_err(|e| e.to_string())?;
    // 7. WORKLOAD.md (frozen contract from package data).
    let wc = &pkg_content["workload_contract"];
    let contract = WorkloadContract {
        primary_metric: wc["primary"].as_str().unwrap_or("").to_string(),
        secondary_metric: wc["secondary"].as_str().map(str::to_string),
        input_distribution: wc["distribution"].as_str().unwrap_or("").to_string(),
        out_of_scope: wc["out_of_scope"].as_str().unwrap_or("").to_string(),
        budgets: wc["budgets"].as_str().unwrap_or("").to_string(),
    };
    let workload_md = contract.to_markdown();
    std::fs::write(out.join("WORKLOAD.md"), &workload_md).map_err(|e| e.to_string())?;
    // 8. Manifest v2 freeze (baseline comes from runner.grade outcomes inside
    // the freeze; the old collect-only counter and its parser are deleted).
    let observables = probe_observables();
    let manifest = frozen_manifest_with_benchmarks(&cx, &composite, &observables)?;
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // 9. Held-out TEST suite (host-only) + held-out WORKLOADS (host-only).
    // Python spine: package-keyed suites (refused when unknown, HEAD behavior).
    // Any other spine: unknown packages degrade to no suites (never halt once
    // the probe succeeds; the frozen differential probes still come from the
    // runner below).
    let heldout_tests: Vec<(String, String)> = if python_spine {
        crate::heldout::generate_suites_for(&package)?
    } else {
        crate::heldout::generate_suites_for(&package).unwrap_or_default()
    };
    for (name, content) in &heldout_tests {
        std::fs::write(heldout_out.join(name), content).map_err(|e| e.to_string())?;
    }
    // 9b. Generated held-outs (M9 slice-11): control-plane generator seeded
    // from the frozen manifest. Values pinned against the pristine original
    // on the host (never in containers). Generic spine degrades to none.
    let manifest_json = std::fs::read_to_string(out.join("manifest.json")).map_err(|e| e.to_string())?;
    let generated: Vec<(String, String)> = if python_spine {
        crate::heldout::generate_from_manifest(&manifest_json, &package, repo)?
    } else {
        crate::heldout::generate_from_manifest(&manifest_json, &package, repo).unwrap_or_default()
    };
    for (name, content) in &generated {
        std::fs::write(heldout_out.join(name), content).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        heldout_out.join("heldout_workloads.json"),
        serde_json::to_string_pretty(&pkg_content["heldout_descriptors"]).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // Visible workloads (frozen, hashed via manifest benchmark section).
    std::fs::write(
        out.join("visible_workloads.json"),
        serde_json::to_string_pretty(&serde_json::json!(hotspot_desc)).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // 10. PORTING.md: RepoFacts rules seeded from package data; the Architect
    // reviews facts.json, stages read PORTING.md verbatim. Generic spine seeds
    // no rules (the frontend language rules still apply downstream).
    let rules: Vec<crate::porting::PortingRule> = if python_spine {
        crate::porting::porting_rules_for(&package)?
    } else {
        crate::porting::porting_rules_for(&package).unwrap_or_default()
    };
    let porting_md = crate::porting::render_porting_md(&package, &composite.languages(), &rules);
    std::fs::write(out.join("PORTING.md"), &porting_md).map_err(|e| e.to_string())?;
    // 11. Unit DAG (leaf-first), frozen with UnitId keys
    // (`<lang>:<repo-rel authoritative source>[#<symbol>]`; the language comes
    // from the composite so no language literal lives here). The `module` stem
    // rides along for audit; readers key on `id` with a stem fallback.
    // Python spine: cycles are errors (HEAD behavior). Any other spine:
    // large compiled trees carry circular USE/include approximations, so
    // cycles break deterministically (lexicographically smallest feedback
    // edge first) and the frozen DAG stays acyclic (graceful scale handling).
    let dag = dag_from_call_graph(&graph);
    let (dag, order) = if python_spine {
        let order = dag.leaf_first_order().map_err(|e| e.to_string())?;
        assert!(dag.verify_order(&order), "DAG order must verify");
        (dag, order)
    } else {
        let (acyclic, order, _dropped) = leaf_first_order_forgiving(&dag);
        assert!(acyclic.verify_order(&order), "forgiving DAG order must verify");
        (acyclic, order)
    };
    // Fallback language for stems the graph carries no language for (cannot
    // happen on either spine, but frozen ids must always be well-formed).
    let unit_lang = composite
        .languages()
        .first()
        .cloned()
        .unwrap_or_else(|| build.language.clone());
    // Unit language comes from the fragment unit the stem came from
    // (`module_langs`); the composite-first language is only a fallback.
    // Single-language trees map every stem to the fallback, so Python-spine
    // output is byte-identical.
    let unit_of = |stem: &str| -> String {
        match graph.modules.get(stem) {
            Some(rel) => {
                let lang = graph
                    .module_langs
                    .get(stem)
                    .map(String::as_str)
                    .unwrap_or(unit_lang.as_str());
                format!("{lang}:{rel}")
            }
            None => stem.to_string(),
        }
    };
    let mut dag_units: Vec<(String, String, Vec<String>)> = dag
        .units
        .iter()
        .map(|u| {
            let mut deps: Vec<String> = u.depends_on.iter().map(|d| unit_of(d)).collect();
            deps.sort();
            deps.dedup();
            (unit_of(&u.id), u.module.clone(), deps)
        })
        .collect();
    dag_units.sort_by(|a, b| a.0.cmp(&b.0));
    let mut dag_edges: Vec<(String, String)> = dag
        .edges
        .iter()
        .map(|(a, b)| (unit_of(a), unit_of(b)))
        .collect();
    dag_edges.sort();
    dag_edges.dedup();
    let dag_order: Vec<String> = order.iter().map(|s| unit_of(s)).collect();
    std::fs::write(
        out.join("dag.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "units": dag_units.iter().map(|(id, module, depends_on)| serde_json::json!({"id": id, "module": module, "depends_on": depends_on})).collect::<Vec<_>>(),
            "edges": dag_edges,
            "leaf_first_order": dag_order,
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // 12. recon.json (everything else for audit). Frozen with UnitId keys:
    // `build.languages` (list; `build.language` stays as a compat alias),
    // `modules`/`call_edges` keyed by UnitId. Readers accept the pre-rollout
    // shapes via the `units` compat shims.
    let recon_modules: std::collections::BTreeMap<String, String> = graph
        .modules
        .iter()
        .map(|(stem, rel)| (unit_of(stem), rel.clone()))
        .collect();
    let mut recon_edges: Vec<(String, String)> = graph
        .edges
        .iter()
        .map(|(a, b)| (unit_of(a), unit_of(b)))
        .collect();
    recon_edges.sort();
    recon_edges.dedup();
    std::fs::write(
        out.join("recon.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "build": {"languages": composite.languages(), "language": build.language, "build_system": build.build_system, "layout": build.layout, "invocation": build.invocation},
            "modules": recon_modules,
            "call_edges": recon_edges,
            "tests": {"files": inv.test_files, "configs": inv.config_refs, "fixtures": inv.fixture_globs, "ci": inv.ci_invokers, "baseline": serde_json::to_value(&manifest.baseline).unwrap()},
            "deps": dep_classes,
            "license": {"license": license, "header_len": header_len},
            "hotspot": {"tool": hotspot.tool, "wall_secs": hotspot.wall_secs, "functions": hotspot.functions.iter().take(5).map(|t| serde_json::json!({"function": t.0, "share": t.1})).collect::<Vec<_>>()},
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    // 13. facts.json: deterministic probe section + seeded rules section.
    // Written after the held-out suite exists so the runner-owned held-out
    // commands freeze as differential probes. Later stages read behavior
    // here, never from fixture switches.
    let facts = probe_facts(
        &composite,
        &probe,
        &dag,
        &attr,
        &package,
        &pkg_content,
        &rules,
        heldout_out,
        &cx,
        &observables,
    );
    std::fs::write(
        out.join("facts.json"),
        serde_json::to_string_pretty(&facts).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(ReconOutput {
        porting_md,
        workload_md,
        dag,
    })
}
/// Single-Python composite predicate (mirrors the adapter spine helpers):
/// exactly `["python"]` keeps the HEAD Python behavior; anything else rides
/// the CMake/CTest spine with generic content.
fn is_python_spine(composite: &CompositeAdapter) -> bool {
    composite.languages() == vec!["python".to_string()]
}

/// Package identity per spine, derived from the repo (never a fixture switch):
/// Python packaging for the Python spine (refused when unknown), CMake
/// `PROJECT(<name> …)` (else the directory name, else `unknown`) for any
/// other spine. Never halts on an unknown package once the probe succeeds.
fn resolve_package(repo: &Path, composite: &CompositeAdapter) -> Result<String, String> {
    if is_python_spine(composite) {
        return crate::repo::package_name(repo);
    }
    if let Some(name) = crate::repo::cmake_project_name(repo) {
        if !name.is_empty() {
            return Ok(name);
        }
    }
    Ok(repo
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "unknown".to_string()))
}

/// Deterministic forgiving leaf-first order for large compiled trees: Kahn's
/// algorithm, breaking cycles by dropping the lexicographically smallest
/// remaining feedback edge whenever the queue stalls. Returns the acyclic
/// DAG (dropped edges removed from units/edges), the verifying order, and
/// the dropped edges (sorted, for audit). Python callers never use this
/// (cycles stay errors there); non-Python recon freezes the acyclic result.
fn leaf_first_order_forgiving(
    dag: &UnitDag,
) -> (UnitDag, Vec<String>, Vec<(String, String)>) {
    use std::collections::{BTreeMap, BTreeSet};
    // Working copies: depends_on per unit + edge set.
    let mut depends: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for u in &dag.units {
        depends.entry(u.id.clone()).or_default();
    }
    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    for (dependent, dependency) in &dag.edges {
        depends.entry(dependent.clone()).or_default();
        depends.entry(dependency.clone()).or_default();
        let is_new = depends
            .get_mut(dependent)
            .map(|set| set.insert(dependency.clone()))
            .unwrap_or(false);
        if is_new {
            edges.insert((dependent.clone(), dependency.clone()));
        }
    }
    // Dependents index rebuilt after each drop (small, deterministic).
    let mut dropped: Vec<(String, String)> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut remaining: BTreeSet<String> = depends.keys().cloned().collect();
    while !remaining.is_empty() {
        // Kahn pass: repeatedly emit zero-indegree nodes (lexicographically
        // largest first via pop, matching `UnitDag::leaf_first_order`).
        loop {
            let mut ready: Vec<String> = remaining
                .iter()
                .filter(|id| depends[*id].is_empty())
                .cloned()
                .collect();
            if ready.is_empty() {
                break;
            }
            ready.sort();
            let n = ready.pop().unwrap();
            order.push(n.clone());
            remaining.remove(&n);
            for deps in depends.values_mut() {
                deps.remove(&n);
            }
        }
        if remaining.is_empty() {
            break;
        }
        // Stalled on a cycle: drop the smallest remaining edge whose
        // dependent is still pending (deterministic feedback arc).
        let victim = edges
            .iter()
            .filter(|(dependent, _)| remaining.contains(dependent))
            .cloned()
            .next();
        match victim {
            Some((dependent, dependency)) => {
                edges.remove(&(dependent.clone(), dependency.clone()));
                if let Some(deps) = depends.get_mut(&dependent) {
                    deps.remove(&dependency);
                }
                dropped.push((dependent, dependency));
            }
            None => {
                // No edges left but nodes remain (isolated): emit smallest.
                let n = remaining.iter().next().cloned().unwrap();
                order.push(n.clone());
                remaining.remove(&n);
            }
        }
    }
    dropped.sort();
    dropped.dedup();
    // Rebuild the acyclic DAG in the frozen `UnitDag` shape.
    let mut units: Vec<rustsmith_adapters::Unit> = dag
        .units
        .iter()
        .map(|u| {
            let deps: Vec<String> = u
                .depends_on
                .iter()
                .filter(|d| edges.contains(&(u.id.clone(), (*d).clone())))
                .cloned()
                .collect();
            rustsmith_adapters::Unit {
                id: u.id.clone(),
                module: u.module.clone(),
                depends_on: deps,
            }
        })
        .collect();
    units.sort_by(|a, b| a.id.cmp(&b.id));
    for u in units.iter_mut() {
        u.depends_on.sort();
    }
    let mut edge_list: Vec<(String, String)> = edges.into_iter().collect();
    edge_list.sort();
    let acyclic = UnitDag { units, edges: edge_list };
    debug_assert!(acyclic.verify_order(&order));
    (acyclic, order, dropped)
}

/// Norm-diff observable skeleton. xUnit repos grade pass/fail, so the probe
/// declares no specs; tolerance-bearing specs for norm-diff repos arrive with
/// the Architect's rules (later track) and freeze into the manifest then.
fn probe_observables() -> Vec<ObservableSpec> {
    Vec::new()
}

/// Deterministic `recon/facts.json`: probe section (workload skeletons,
/// runner-owned differential probes, observable skeleton, API surface,
/// package identity, attribution) plus the seeded `rules` section (RepoFacts
/// porting rules; the Architect refines workloads there in a later track).
fn probe_facts(
    composite: &CompositeAdapter,
    probe: &ProbeReport,
    dag: &UnitDag,
    attribution: &[(String, Attribution)],
    package: &str,
    pkg_content: &serde_json::Value,
    rules: &[crate::porting::PortingRule],
    heldout_suite: &Path,
    cx: &BuildCtx,
    observables: &[ObservableSpec],
) -> serde_json::Value {
    // Runner-owned held-out commands: the frozen differential probes.
    let differential_probes = composite.runner.heldout(heldout_suite, cx);
    // API surface: sorted module stems from the unit DAG.
    let mut api_surface: Vec<String> = dag.units.iter().map(|u| u.module.clone()).collect();
    api_surface.sort();
    api_surface.dedup();
    // Workload skeletons: names plus input shapes from the frozen visible
    // descriptors. `setup` stays empty: executable snippets are authored with
    // the rules, not probed. `iters` is the schema placeholder (refined later).
    let descriptors: Vec<String> = pkg_content["hotspot_descriptors"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let workloads: Vec<Workload> = descriptors
        .into_iter()
        .map(|descriptor| Workload {
            name: descriptor.clone(),
            setup: String::new(),
            stmt: descriptor,
            iters: 1,
        })
        .collect();
    // Held-out workload skeletons, same shape, from the host-only descriptors.
    let heldout_desc = &pkg_content["heldout_descriptors"];
    let distribution = heldout_desc
        .get("distribution")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut heldout_workloads = Vec::new();
    for key in [
        "adversarial_shapes",
        "coverage_complement",
        "identity_sensitive_repeats",
    ] {
        if let Some(arr) = heldout_desc.get(key).and_then(|v| v.as_array()) {
            for shape in arr.iter().filter_map(|v| v.as_str()) {
                heldout_workloads.push(Workload {
                    name: format!("heldout:{shape}"),
                    setup: String::new(),
                    stmt: distribution.clone(),
                    iters: 1,
                });
            }
        }
    }
    // Attribution with report-ready strings: upstream from the repo's own git
    // remote (else the package name), SPDX tag from the detected license id.
    // No registry of known upstreams lives here.
    let upstream = crate::repo::upstream_of(cx.tree, package);
    let attribution_json: Vec<serde_json::Value> = attribution
        .iter()
        .map(|(glob, a)| {
            serde_json::json!({
                "glob": glob,
                "license": a.license,
                "header_len": a.header_text.len(),
                "notice_extra": a.notice_extra,
                "upstream": upstream,
                "spdx": format!("SPDX-License-Identifier: {}", a.license),
            })
        })
        .collect();
    serde_json::json!({
        "probe": {
            "frontends": probe.frontends,
            "unclaimed": probe.unclaimed,
            "unclaimed_share": probe.unclaimed_share,
            "has_ctest": probe.has_ctest,
            "cmake_languages": probe.cmake_languages,
            "package": package,
            "workloads": workloads,
            "heldout_workloads": heldout_workloads,
            "differential_probes": differential_probes,
            "observables": observables,
            "api_surface": api_surface,
            "attribution": attribution_json,
        },
        "rules": { "porting_rules": crate::porting::rules_to_facts(rules) },
    })
}


fn frozen_manifest_with_benchmarks(
    cx: &BuildCtx,
    composite: &CompositeAdapter,
    observables: &[ObservableSpec],
) -> Result<Manifest, String> {
    // Canonical freeze through the runner: file membership from
    // runner.oracle_files, ADR-002 normalization via
    // runner.normalize_for_hash, baseline from runner.grade outcomes over the
    // executed invocation.
    let mut m = Oracle::freeze(cx, &*composite.runner).map_err(|e| e.to_string())?;
    // Benchmark freeze: hash bench files alongside oracle files (whole bytes;
    // bench workloads are not oracle files so runner normalization does not
    // apply). Skips files the oracle already froze.
    for entry in walkdir::WalkDir::new(cx.tree.join("test/bench"))
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if p.is_file() {
            let rel = p.strip_prefix(cx.tree).unwrap().to_string_lossy().replace('\\', "/");
            let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
            if !m.files.iter().any(|f| f.path.as_str() == rel) {
                m.files.push(FileHash {
                    path: rel.into(),
                    sha256: sha256_hex(&bytes),
                });
            }
        }
    }
    m.files.sort_by(|a, b| a.path.cmp(&b.path));
    // Composite-owned v2 fields (the oracle freeze leaves these empty for this
    // writer): ordered prepare commands from the bridge, language list, and
    // the frozen observable specs.
    m.prepare = composite.bridge.prepare(cx);
    m.languages = composite.languages();
    m.observables = observables.to_vec();
    Ok(m)
}

#[cfg(test)]
mod recon_regression_tests {
    use super::*;
    use rustsmith_adapters::Unit;
    use rustsmith_core::{
        AdapterError, Cwd, GradedResult, Observation, OracleFile, RunOutput, TestCommand,
        TestRunner, parse_manifest_json, MANIFEST_VERSION,
    };
    use std::collections::BTreeMap;

    /// Mock runner: empty oracle set + empty invocation, so `Oracle::freeze`
    /// executes zero host commands (no subprocess); grading sees no runs.
    struct MockRunner;
    impl TestRunner for MockRunner {
        fn id(&self) -> &'static str {
            "mocktest"
        }
        fn invocation(&self, _cx: &BuildCtx) -> Vec<TestCommand> {
            Vec::new()
        }
        fn grade(&self, _runs: &[RunOutput]) -> Result<GradedResult, AdapterError> {
            Ok(GradedResult::from_outcomes(0, BTreeMap::new(), String::new()))
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
        fn heldout(&self, suite: &Path, _cx: &BuildCtx) -> Vec<TestCommand> {
            vec![TestCommand {
                program: "mocktest".to_string(),
                args: vec![suite.to_string_lossy().into_owned()],
                cwd: Cwd::Tree,
                env_set: Vec::new(),
                env_remove: Vec::new(),
                launcher: None,
                timeout_secs: None,
                collect: Vec::new(),
            }]
        }
        fn config_hash(&self, _cx: &BuildCtx) -> Result<String, AdapterError> {
            Ok("mock-config-hash".to_string())
        }
    }

    fn mock_prepare() -> Vec<TestCommand> {
        vec![TestCommand {
            program: "mock-configure".to_string(),
            args: vec!["--prefix".to_string(), "/tmp/prefix".to_string()],
            cwd: Cwd::BuildDir,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: None,
            collect: Vec::new(),
        }]
    }

    fn tol_spec() -> ObservableSpec {
        ObservableSpec {
            test_glob: "*".to_string(),
            source: "stdout".to_string(),
            regex: "([0-9.]+)".to_string(),
            rel_tol: 0.01,
            abs_tol: 0.0,
        }
    }

    /// Manifest v2 writer shape: `Oracle::freeze` through a mock runner (zero
    /// host commands) plus the composite-owned fill (`prepare`/`languages`/
    /// `observables`, mirroring `frozen_manifest_with_benchmarks`) must carry
    /// every v2 field and round-trip through `parse_manifest_json`.
    #[test]
    fn manifest_v2_writer_shape_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let cx = BuildCtx {
            tree: dir.path(),
            build_dir: dir.path(),
            release: false,
        };
        let runner = MockRunner;
        let mut m = Oracle::freeze(&cx, &runner).unwrap();
        // Composite-owned v2 fill, verbatim shape of the recon writer.
        m.prepare = mock_prepare();
        m.languages = vec!["python".to_string()];
        m.observables = vec![tol_spec()];
        assert_eq!(m.version, MANIFEST_VERSION);
        assert_eq!(m.runner, "mocktest");
        assert_eq!(m.config_hash, "mock-config-hash");
        let text = serde_json::to_string_pretty(&m).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        for key in [
            "prepare",
            "invocation",
            "config_hash",
            "observables",
            "runner",
            "languages",
            "files",
            "baseline",
        ] {
            assert!(v.get(key).is_some(), "manifest v2 lacks `{key}`");
        }
        let back = parse_manifest_json(&text).unwrap();
        assert_eq!(m, back, "v2 manifest must survive a freeze/parse round-trip");
        // Every frozen command carries an explicit cwd (no ambient-relative spawn).
        for cmd in m.prepare.iter().chain(m.invocation.iter()) {
            assert!(!cmd.program.is_empty(), "frozen command needs a program");
            assert!(
                matches!(cmd.cwd, Cwd::Tree | Cwd::BuildDir | Cwd::Rel(_)),
                "frozen command must set cwd"
            );
        }
    }

    /// `facts.json` probe section is deterministic: identical inputs freeze to
    /// byte-identical JSON carrying frontends/unclaimed/api_surface/attribution.
    #[test]
    fn probe_facts_deterministic_and_complete() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pkgmod.py"), "VALUE = 1\n").unwrap();
        let (composite, probe) = select_composite(dir.path()).unwrap();
        let heldout_dir = tempfile::tempdir().unwrap();
        let suite = heldout_dir.path().join("heldout.py");
        let cx = BuildCtx {
            tree: dir.path(),
            build_dir: dir.path(),
            release: false,
        };
        let dag = UnitDag {
            units: vec![
                Unit {
                    id: "a".to_string(),
                    module: "pkgmod".to_string(),
                    depends_on: Vec::new(),
                },
                Unit {
                    id: "b".to_string(),
                    module: "pkgmod".to_string(),
                    depends_on: vec!["a".to_string()],
                },
            ],
            edges: vec![("b".to_string(), "a".to_string())],
        };
        let attribution = vec![(
            "pkgmod.py".to_string(),
            Attribution {
                license: "MIT".to_string(),
                header_text: "copyright".to_string(),
                notice_extra: String::new(),
            },
        )];
        let pkg_content = serde_json::json!({
            "hotspot_descriptors": ["stmt_a"],
            "heldout_descriptors": {
                "distribution": "dist",
                "adversarial_shapes": ["shape_a"],
                "coverage_complement": [],
                "identity_sensitive_repeats": []
            }
        });
        let rules: Vec<crate::porting::PortingRule> = Vec::new();
        // No git remote in the temp tree, so `upstream_of` falls back to the
        // package name deterministically (read-only probe, instant fallback).
        let package = "pkg-fixture";
        let observational: Vec<ObservableSpec> = Vec::new();
        let a = probe_facts(
            &composite,
            &probe,
            &dag,
            &attribution,
            package,
            &pkg_content,
            &rules,
            &suite,
            &cx,
            &observational,
        );
        let b = probe_facts(
            &composite,
            &probe,
            &dag,
            &attribution,
            package,
            &pkg_content,
            &rules,
            &suite,
            &cx,
            &observational,
        );
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap(),
            "same probe input must freeze byte-identical facts"
        );
        let probe_json = &a["probe"];
        assert_eq!(probe_json["frontends"], serde_json::json!(["python"]));
        assert!(probe_json["unclaimed"].is_array(), "unclaimed must freeze");
        assert!(
            probe_json["api_surface"]
                .as_array()
                .is_some_and(|s| s.iter().any(|m| m == "pkgmod")),
            "api_surface must list dag modules"
        );
        let attr = probe_json["attribution"]
            .as_array()
            .and_then(|xs| xs.first())
            .expect("attribution must freeze");
        assert_eq!(attr["upstream"], serde_json::json!(package));
        assert!(
            attr["spdx"].as_str().is_some_and(|s| s.contains("MIT")),
            "attribution must carry the SPDX tag"
        );
        assert_eq!(probe_json["package"], serde_json::json!(package));
        assert!(
            probe_json["differential_probes"].is_array(),
            "runner-owned differential probes must freeze"
        );
    }

    /// Probe-first ordering: a CMake-only repo (no Python packaging) probes to
    /// the fortran/cxx composite before any package resolution. The legacy
    /// `package_name` still refuses it, but the per-spine resolver derives
    /// `PROJECT(<name> …)` from the repo itself.
    #[test]
    fn probe_first_resolves_cmake_identity_without_python_packaging() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.16)\nPROJECT(ProbeFirst Fortran C CXX)\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("mod.F90"),
            "module probe_mod\nimplicit none\ncontains\nsubroutine probe_init() bind(c)\nend subroutine\nend module\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("util.c"), "int probe_fn(void){return 0;}\n").unwrap();
        // Probe succeeds where the Python-only identity refuses.
        let (composite, probe) = select_composite(dir.path()).unwrap();
        assert!(!is_python_spine(&composite));
        assert!(composite.languages().contains(&"fortran".to_string()));
        assert!(composite.languages().contains(&"cxx".to_string()));
        assert!(crate::repo::package_name(dir.path()).is_err());
        assert_eq!(resolve_package(dir.path(), &composite).as_deref(), Ok("ProbeFirst"));
        assert_eq!(probe.cmake_languages, Vec::<String>::new());
    }

    /// Generic content fallback: unknown packages degrade to empty/defaults
    /// (never halt) once the probe succeeds. The Python spine keeps refusing
    /// unknowns; the generic spine below is what non-Python recon freezes.
    #[test]
    fn generic_content_fallback_degrades_for_unknown_packages() {
        let package = "no-such-package-for-fallback-probe";
        assert!(crate::repo_content::entry(package).is_err());
        let empty = crate::repo_content::entry_or_empty(package);
        assert_eq!(empty["hotspot_descriptors"], serde_json::json!([]));
        // Workload contract degrades to empty strings via `unwrap_or("")`.
        let wc = &empty["workload_contract"];
        assert_eq!(wc["primary"].as_str().unwrap_or(""), "");
        assert_eq!(wc["distribution"].as_str().unwrap_or(""), "");
        // Held-out descriptors degrade to no workloads.
        assert!(empty["heldout_descriptors"].as_object().is_some());
        // Package-keyed generators still refuse unknowns (Python parity), but
        // the generic spine degrades to none instead of halting.
        assert!(crate::heldout::generate_suites_for(package).is_err());
        assert!(crate::heldout::generate_suites_for(package).unwrap_or_default().is_empty());
        assert!(crate::porting::porting_rules_for(package).is_err());
        assert!(crate::porting::porting_rules_for(package).unwrap_or_default().is_empty());
    }

    /// Forgiving order breaks cycles deterministically for large compiled
    /// trees while the strict order (Python spine) still refuses them.
    #[test]
    fn forgiving_order_breaks_cycles_deterministically() {
        use rustsmith_adapters::Unit;
        let dag = UnitDag {
            units: vec![
                Unit { id: "a".to_string(), module: "a".to_string(), depends_on: vec!["b".to_string()] },
                Unit { id: "b".to_string(), module: "b".to_string(), depends_on: vec!["a".to_string()] },
                Unit { id: "c".to_string(), module: "c".to_string(), depends_on: vec![] },
            ],
            edges: vec![("a".to_string(), "b".to_string()), ("b".to_string(), "a".to_string())],
        };
        assert!(dag.leaf_first_order().is_err(), "strict order must refuse cycles");
        let (acyclic, order, dropped) = leaf_first_order_forgiving(&dag);
        assert!(!dropped.is_empty(), "a feedback edge must drop");
        assert!(acyclic.verify_order(&order), "forgiving order must verify");
        assert_eq!(order.len(), 3, "all units still scheduled");
        // Deterministic: same input breaks the same edge.
        let (_, _, dropped2) = leaf_first_order_forgiving(&dag);
        assert_eq!(dropped, dropped2);
        // Acyclic input passes through untouched.
        let clean = UnitDag {
            units: vec![
                Unit { id: "a".to_string(), module: "a".to_string(), depends_on: vec!["b".to_string()] },
                Unit { id: "b".to_string(), module: "b".to_string(), depends_on: vec![] },
            ],
            edges: vec![("a".to_string(), "b".to_string())],
        };
        let (acyclic_clean, order_clean, dropped_clean) = leaf_first_order_forgiving(&clean);
        assert!(dropped_clean.is_empty());
        assert_eq!(order_clean, clean.leaf_first_order().unwrap());
        assert_eq!(acyclic_clean.edges, clean.edges);
    }
}
