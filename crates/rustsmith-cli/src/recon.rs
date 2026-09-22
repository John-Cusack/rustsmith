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

    // 0. Package identity from the repo's own packaging metadata, frozen into
    // facts.json below. Later stages read it there; nothing re-detects.
    let package = crate::repo::package_name(repo)?;
    let pkg_content = crate::repo_content::entry(&package)?;
    // 1. Probe + composite selection. Unknown extensions halt above threshold.
    let (composite, probe) = select_composite(repo).map_err(|e| e.to_string())?;
    if probe.unclaimed_share > UNCLAIMED_HALT_THRESHOLD {
        return Err(format!(
            "probe: unclaimed file share {:.3} above threshold {:.3} ({} files, e.g. {})",
            probe.unclaimed_share,
            UNCLAIMED_HALT_THRESHOLD,
            probe.unclaimed.len(),
            probe.unclaimed.first().map(String::as_str).unwrap_or("-"),
        ));
    }
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
    let heldout_tests = crate::heldout::generate_suites_for(&package)?;
    for (name, content) in &heldout_tests {
        std::fs::write(heldout_out.join(name), content).map_err(|e| e.to_string())?;
    }
    // 9b. Generated held-outs (M9 slice-11): control-plane generator seeded
    // from the frozen manifest. Values pinned against the pristine original
    // on the host (never in containers).
    let manifest_json = std::fs::read_to_string(out.join("manifest.json")).map_err(|e| e.to_string())?;
    for (name, content) in crate::heldout::generate_from_manifest(&manifest_json, &package, repo)? {
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
    // reviews facts.json, stages read PORTING.md verbatim.
    let rules = crate::porting::porting_rules_for(&package)?;
    let porting_md = crate::porting::render_porting_md(&package, &composite.languages(), &rules);
    std::fs::write(out.join("PORTING.md"), &porting_md).map_err(|e| e.to_string())?;
    // 11. Unit DAG (leaf-first), frozen with UnitId keys
    // (`<lang>:<repo-rel authoritative source>[#<symbol>]`; the language comes
    // from the composite so no language literal lives here). The `module` stem
    // rides along for audit; readers key on `id` with a stem fallback.
    let dag = dag_from_call_graph(&graph);
    let order = dag.leaf_first_order().map_err(|e| e.to_string())?;
    assert!(dag.verify_order(&order), "DAG order must verify");
    let unit_lang = composite
        .languages()
        .first()
        .cloned()
        .unwrap_or_else(|| build.language.clone());
    let unit_of = |stem: &str| -> String {
        match graph.modules.get(stem) {
            Some(rel) => format!("{unit_lang}:{rel}"),
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
}
