//! Adapters: per-language frontends + per-repo composite spine (ADR-008).
//!
//! Parsing is per language ([`Frontend`]s), the build/test spine is per repo
//! ([`TestRunner`](rustsmith_core::TestRunner) + [`BuildBridge`] + [`Profiler`]),
//! and stages only ever see the [`CompositeAdapter`]. Python is the one-frontend
//! case: [`PythonFrontend`] + [`PytestRunner`] + [`MaturinBridge`] + [`PyProfiler`].
//!
//! Shared command/result types ([`TestCommand`](rustsmith_core::TestCommand),
//! [`BuildCtx`](rustsmith_core::BuildCtx), [`Workload`](rustsmith_core::Workload),
//! [`AdapterError`](rustsmith_core::AdapterError), …) live in `rustsmith-core`.
//! The legacy [`Adapter`]/[`PythonAdapter`] surface is preserved and now
//! delegates to the new decomposition, so `recon.json`/`dag.json` output is
//! byte-identical to HEAD for the crc/strsimpy fixtures (the UnitId rollout to
//! frozen keys happens atomically in a later track).

use rustsmith_core::{
    AdapterError, BuildCtx, Cwd, GradedResult, Launcher, ObservableSpec, Observation, OracleFile,
    OracleKind, Outcome, RunOutput, TestCommand, TestRunner, Workload,
};
use rustsmith_profile::HotspotBaseline;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepClass {
    Port,
    Bind,
    Keep,
}

#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub language: String,
    pub build_system: String,
    pub layout: String,
    pub invocation: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CallGraph {
    /// module name -> file path (relative)
    pub modules: HashMap<String, String>,
    /// (dependent, dependency): dependent imports dependency.
    pub edges: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct TestInventory {
    pub test_files: Vec<String>,
    pub config_refs: Vec<String>,
    pub fixture_globs: Vec<String>,
    pub ci_invokers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Attribution {
    pub license: String,
    pub header_text: String,
    pub notice_extra: String,
}

#[derive(Debug, Clone)]
pub struct Unit {
    pub id: String,
    pub module: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UnitDag {
    pub units: Vec<Unit>,
    pub edges: Vec<(String, String)>,
}

impl UnitDag {
    /// Topological leaf-first order (dependencies before dependents).
    /// Error names the cycle path when one exists.
    pub fn leaf_first_order(&self) -> Result<Vec<String>, AdapterError> {
        // Kahn on depends_on edges: edge (dependent -> dependency).
        let mut indeg: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for u in &self.units {
            indeg.entry(u.id.as_str()).or_insert(0);
        }
        for (dependent, dependency) in &self.edges {
            // dependent has one more prerequisite.
            *indeg.entry(dependent.as_str()).or_insert(0) += 1;
            dependents.entry(dependency.as_str()).or_default().push(dependent.as_str());
            indeg.entry(dependency.as_str()).or_insert(0);
        }
        // Deterministic: sort initial queue.
        let mut queue: Vec<&str> = indeg.iter().filter(|(_, &d)| d == 0).map(|(&k, _)| k).collect();
        queue.sort_unstable();
        let mut order = Vec::new();
        while let Some(n) = queue.pop() {
            order.push(n.to_string());
            if let Some(ds) = dependents.get(n) {
                let mut ds = ds.clone();
                ds.sort_unstable();
                for d in ds {
                    let e = indeg.get_mut(d).unwrap();
                    *e -= 1;
                    if *e == 0 {
                        queue.push(d);
                    }
                }
            }
            queue.sort_unstable();
        }
        if order.len() != indeg.len() {
            let cycle: Vec<String> = indeg
                .iter()
                .filter(|(_, &d)| d > 0)
                .map(|(&k, _)| k.to_string())
                .collect();
            return Err(AdapterError::Parse(format!("cycle detected: {}", cycle.join(" -> "))));
        }
        Ok(order)
    }

    pub fn verify_order(&self, order: &[String]) -> bool {
        let pos: HashMap<&str, usize> =
            order.iter().enumerate().map(|(i, s)| (s.as_str(), i)).collect();
        for (dependent, dependency) in &self.edges {
            match (pos.get(dependent.as_str()), pos.get(dependency.as_str())) {
                (Some(&a), Some(&b)) => {
                    if b >= a {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

pub trait Adapter {
    fn language(&self) -> &'static str;
    fn detect(&self, repo: &Path) -> Result<BuildInfo, AdapterError>;
    fn call_graph(&self, repo: &Path) -> Result<CallGraph, AdapterError>;
    fn test_inventory(&self, repo: &Path) -> Result<TestInventory, AdapterError>;
    fn classify_dep(&self, dep: &str) -> DepClass;
    fn license_terms(&self, repo: &Path) -> Result<Attribution, AdapterError>;
}

pub struct PythonAdapter;

impl PythonAdapter {
    fn frontend() -> PythonFrontend {
        PythonFrontend
    }
}

/// Python-ecosystem sniffing shared by [`PythonAdapter`] and the composite:
/// build system from pyproject/setup.py, src- vs flat-layout, split pytest
/// invocation when test/unit + test/integration both exist.
fn python_detect(repo: &Path) -> Result<BuildInfo, AdapterError> {
    let pyproject = repo.join("pyproject.toml");
    let build_system = if pyproject.exists() {
        let t = std::fs::read_to_string(&pyproject)?;
        if t.contains("hatchling") {
            "hatchling"
        } else if t.contains("setuptools") {
            "setuptools"
        } else if t.contains("poetry") {
            "poetry"
        } else {
            "unknown"
        }
        .to_string()
    } else if repo.join("setup.py").exists() {
        "setuptools".into()
    } else {
        "unknown".into()
    };
    let layout = if repo.join("src").is_dir() {
        "src-layout"
    } else {
        "flat-layout"
    }
    .to_string();
    let invocation = if repo.join("test/unit").is_dir() && repo.join("test/integration").is_dir() {
        vec!["pytest test/unit".into(), "pytest test/integration".into()]
    } else {
        vec!["pytest".into()]
    };
    Ok(BuildInfo {
        language: "python".into(),
        build_system,
        layout,
        invocation,
    })
}

/// Python test/config/fixture/CI inventory shared by [`PythonAdapter`] and
/// the composite. bench/ is never oracle.
fn python_test_inventory(repo: &Path) -> Result<TestInventory, AdapterError> {
    let mut test_files = Vec::new();
    let mut config_refs = Vec::new();
    let mut fixture_globs = Vec::new();
    let mut ci_invokers = Vec::new();
    for entry in walkdir::WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| {
            let p = e.path();
            for seg in [".git", "__pycache__", ".pytest_cache", ".venv", "venv", "dist", "target"] {
                if p.components().any(|c| c.as_os_str() == seg) {
                    return false;
                }
            }
            true
        })
    {
        let entry = entry.map_err(|e| AdapterError::Parse(e.to_string()))?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let rel = p.strip_prefix(repo).unwrap().to_string_lossy().replace('\\', "/");
        if rel.contains("bench") {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("test_") || name.ends_with("_test.py") || name == "conftest.py" {
            test_files.push(rel.clone());
        }
        if matches!(rel.as_str(), "pytest.ini" | "tox.ini" | "setup.cfg" | "pyproject.toml" | "tasks.py") {
            config_refs.push(rel.clone());
        }
        if rel.contains("fixture") || rel.contains("data") {
            fixture_globs.push(rel.clone());
        }
        if rel.starts_with(".github/workflows/") {
            ci_invokers.push(rel.clone());
        }
    }
    test_files.sort();
    config_refs.sort();
    fixture_globs.sort();
    ci_invokers.sort();
    Ok(TestInventory {
        test_files,
        config_refs,
        fixture_globs,
        ci_invokers,
    })
}

impl Adapter for PythonAdapter {
    fn language(&self) -> &'static str {
        Self::frontend().id()
    }

    fn detect(&self, repo: &Path) -> Result<BuildInfo, AdapterError> {
        python_detect(repo)
    }

    fn call_graph(&self, repo: &Path) -> Result<CallGraph, AdapterError> {
        python_call_graph(repo)
    }

    fn test_inventory(&self, repo: &Path) -> Result<TestInventory, AdapterError> {
        python_test_inventory(repo)
    }

    fn classify_dep(&self, dep: &str) -> DepClass {
        classify_python_dep(dep)
    }

    fn license_terms(&self, repo: &Path) -> Result<Attribution, AdapterError> {
        Ok(python_license_terms(repo)?.1)
    }
}

/// Shared name-based dependency classification for the Python ecosystem.
/// stdlib -> keep; third-party test-only -> keep; runtime third-party -> port/bind.
fn classify_python_dep(dep: &str) -> DepClass {
    const STDLIB: &[&str] = &[
        "abc", "argparse", "dataclasses", "enum", "functools", "numbers", "sys", "typing",
        "math", "re", "collections", "itertools", "pathlib",
    ];
    if STDLIB.contains(&dep) || dep.starts_with('_') {
        return DepClass::Keep;
    }
    if ["pytest", "invoke", "hatch", "mypy", "pylint", "black", "isort"].contains(&dep) {
        return DepClass::Keep;
    }
    // No known C-extension/dynamic deps in fixtures; default port.
    DepClass::Port
}

/// Python license detection. Returns the matched glob alongside the terms so
/// [`CompositeAdapter::license_terms`] can report one entry per license file.
fn python_license_terms(repo: &Path) -> Result<(String, Attribution), AdapterError> {
    for name in ["LICENSE.txt", "LICENSE", "LICENSE.md", "LICENCE"] {
        let p = repo.join(name);
        if p.exists() {
            let text = std::fs::read_to_string(&p)?;
            let license = if text.contains("BSD") {
                if text.contains("2-Clause") || text.contains("BSD-2") {
                    "BSD-2-Clause"
                } else {
                    "BSD"
                }
            } else if text.contains("Redistribution and use in source and binary forms") {
                // BSD 2-clause text customarily ships without naming "BSD".
                "BSD-2-Clause"
            } else if text.contains("MIT License") {
                "MIT"
            } else if text.contains("Apache") {
                "Apache-2.0"
            } else {
                "unknown"
            }
            .to_string();
            let header: String = text.lines().take(5).collect::<Vec<_>>().join("\n");
            return Ok((
                name.to_string(),
                Attribution {
                    license,
                    header_text: header,
                    notice_extra: String::new(),
                },
            ));
        }
    }
    // Fallback: pyproject license field.
    let pp = repo.join("pyproject.toml");
    if pp.exists() {
        let t = std::fs::read_to_string(&pp)?;
        for line in t.lines() {
            if line.contains("license") && line.contains("BSD") {
                return Ok((
                    "pyproject.toml".to_string(),
                    Attribution {
                        license: "BSD-2-Clause".into(),
                        header_text: String::new(),
                        notice_extra: String::new(),
                    },
                ));
            }
            if line.contains("license") && line.contains("MIT") {
                return Ok((
                    "pyproject.toml".to_string(),
                    Attribution {
                        license: "MIT".into(),
                        header_text: String::new(),
                        notice_extra: String::new(),
                    },
                ));
            }
        }
    }
    Ok((
        "*".to_string(),
        Attribution {
            license: "unknown".into(),
            header_text: String::new(),
            notice_extra: String::new(),
        },
    ))
}

// --- S2: unit identity + frontends (ADR-008 verbatim) ---

/// Canonical unit identity: `<lang>:<repo-rel authoritative pre-generation
/// source>[#<symbol>]`. Fortran symbols are lowercased (the language is
/// case-insensitive); linkage names live in [`Symbol`], not in the ID.
/// Example: `python:src/crc/_crc.py`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnitId(pub String);

impl std::fmt::Display for UnitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Abi {
    C,
    Fortran { bind_c: bool },
    Cxx,
    Python,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub linkage: String,
    pub abi: Abi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitDecl {
    pub id: UnitId,
    pub files: Vec<String>,
    pub generated_from: Option<String>,
    pub exports: Vec<Symbol>,
    pub imports: Vec<Symbol>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fragment {
    pub units: Vec<UnitDecl>,
    /// (dependent, dependency): dependent imports a symbol the dependency exports.
    pub edges: Vec<(UnitId, UnitId)>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug)]
pub struct FragmentCtx<'a> {
    pub repo: &'a Path,
    pub files: &'a [PathBuf],
    pub compile_db: Option<&'a Path>,
    pub compiler_ids: &'a BTreeMap<String, String>,
}

/// ABI/layout rule contributed by a frontend. Repo API rules live in RepoFacts,
/// never here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub text: String,
}

pub trait Frontend: Send + Sync {
    fn id(&self) -> &'static str;
    /// Whether this frontend parses `file`. Extension-based today; C/C++
    /// frontends (later track) also consult the compile DB.
    fn claims(&self, file: &Path, compile_db: Option<&Path>) -> bool;
    fn fragment(&self, cx: &FragmentCtx) -> Result<Fragment, AdapterError>;
    /// ABI/layout rules only; repo API rules live in RepoFacts.
    fn language_rules(&self) -> Vec<Rule>;
}

pub struct PythonFrontend;

impl Frontend for PythonFrontend {
    fn id(&self) -> &'static str {
        "python"
    }

    fn claims(&self, file: &Path, _compile_db: Option<&Path>) -> bool {
        file.extension().map(|x| x == "py").unwrap_or(false)
    }

    fn fragment(&self, cx: &FragmentCtx) -> Result<Fragment, AdapterError> {
        python_fragment(cx)
    }

    fn language_rules(&self) -> Vec<Rule> {
        vec![
            Rule {
                id: "py.module-stem-linkage".into(),
                text: "A module's linkage name is its file stem; flat packages import by stem, src-layout packages by `<pkg>.<stem>`.".into(),
            },
            Rule {
                id: "py.relative-import".into(),
                text: "`from . import x` and `from .mod import y` resolve against sibling stems of the importing module.".into(),
            },
            Rule {
                id: "py.no-stable-abi".into(),
                text: "Python units substitute as whole modules (PyO3 extension shadowing); there is no per-symbol link step.".into(),
            },
        ]
    }
}

/// Deterministic import graph via Python stdlib `ast` (no new Rust deps).
/// Module names are file stems for flat packages and `pkg.stem` for src-layout;
/// edges use stems so strsimpy's `from .shingle_based import` resolves.
fn python_call_graph(repo: &Path) -> Result<CallGraph, AdapterError> {
    // Collect candidate source files (exclude tests, bench, docs) with the
    // legacy walk, then run the frontend over them so behavior stays identical.
    let mut sources: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| {
            let p = e.path();
            for seg in [".git", "__pycache__", ".pytest_cache", ".venv", "venv", "dist", "docs", "target"] {
                if p.components().any(|c| c.as_os_str() == seg) {
                    return false;
                }
            }
            if p.components().any(|c| c.as_os_str() == "test" || c.as_os_str() == "tests" || c.as_os_str() == "bench") {
                return false;
            }
            true
        })
    {
        let entry = entry.map_err(|e| AdapterError::Parse(e.to_string()))?;
        let p = entry.path();
        if p.is_file() && p.extension().map(|x| x == "py").unwrap_or(false) {
            sources.push(p.to_path_buf());
        }
    }
    sources.sort();
    let compiler_ids = BTreeMap::new();
    let cx = FragmentCtx {
        repo,
        files: &sources,
        compile_db: None,
        compiler_ids: &compiler_ids,
    };
    let fragment = PythonFrontend.fragment(&cx)?;
    // Map canonical units back to the legacy stem-keyed shape.
    let mut id_to_stem: HashMap<&str, &str> = HashMap::new();
    let mut modules: HashMap<String, String> = HashMap::new();
    for u in &fragment.units {
        if let Some(stem) = u.exports.first().map(|s| s.linkage.as_str()) {
            id_to_stem.insert(u.id.0.as_str(), stem);
            modules.insert(stem.to_string(), u.files.first().cloned().unwrap_or_default());
        }
    }
    let mut edges = Vec::new();
    for (dependent, dependency) in &fragment.edges {
        if let (Some(a), Some(b)) = (
            id_to_stem.get(dependent.0.as_str()),
            id_to_stem.get(dependency.0.as_str()),
        ) {
            edges.push((a.to_string(), b.to_string()));
        }
    }
    edges.sort();
    edges.dedup();
    Ok(CallGraph { modules, edges })
}

/// Python fragment engine: filter claimed files, parse imports via stdlib
/// `ast`, emit one canonical [`UnitDecl`] per module.
fn python_fragment(cx: &FragmentCtx) -> Result<Fragment, AdapterError> {
    let mut diagnostics = Vec::new();
    // Claimed-file filter. Mirrors the legacy walk: skip test/bench/docs
    // paths and repo-root tooling; `__init__.py` carries no module unit.
    let mut sources: Vec<(&Path, String, String)> = Vec::new();
    for src in cx.files {
        if src.extension().map(|x| x == "py").unwrap_or(false) {
            let rel = src.strip_prefix(cx.repo).unwrap_or(src).to_string_lossy().replace('\\', "/");
            let rel_path = Path::new(&rel);
            if rel_path.components().any(|c| {
                matches!(
                    c.as_os_str().to_str(),
                    Some("test") | Some("tests") | Some("bench") | Some("docs")
                )
            }) {
                continue;
            }
            let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("test_") || name.ends_with("_test.py") || name == "conftest.py" {
                continue;
            }
            if name == "tasks.py" || name == "setup.py" || name == "noxfile.py" {
                continue;
            }
            let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            if stem == "__init__" {
                continue;
            }
            sources.push((src, rel, stem));
        }
    }
    sources.sort_by(|a, b| a.1.cmp(&b.1));
    // Ask Python's ast for imports of each file (accurate, stdlib-only).
    let mut modules: HashMap<String, String> = HashMap::new();
    let mut file_imports: HashMap<String, Vec<String>> = HashMap::new();
    for (src, rel, stem) in &sources {
        if let Some(prev) = modules.insert(stem.clone(), rel.clone()) {
            diagnostics.push(format!(
                "duplicate module stem '{stem}': '{prev}' shadowed by '{rel}'"
            ));
        }
        let script = r#"
import ast, sys, json
path = sys.argv[1]
tree = ast.parse(open(path).read())
mods = []
for node in ast.walk(tree):
    if isinstance(node, ast.Import):
        for a in node.names:
            mods.append(a.name.split('.')[0])
    elif isinstance(node, ast.ImportFrom):
        if node.module:
            mods.append(node.module.split('.')[0])
        for a in node.names:
            # relative `from .x import y` has module x; bare `from . import x` lists x in names
            if node.level and node.level > 0 and not node.module:
                mods.append(a.name.split('.')[0])
print(json.dumps(mods))
"#;
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg(script)
            .arg(src)
            .output()
            .map_err(AdapterError::Io)?;
        if !out.status.success() {
            diagnostics.push(format!(
                "ast parse of '{rel}' failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let mods: Vec<String> = serde_json::from_slice(&out.stdout).unwrap_or_default();
        file_imports.insert(stem.clone(), mods);
    }
    let stems: HashSet<String> = modules.keys().cloned().collect();
    // Resolve imports to stems (last dotted segment covers
    // `from strsimpy.x import` style too).
    let mut resolved: HashMap<String, Vec<String>> = HashMap::new();
    for (stem, imports) in &file_imports {
        let mut seen = HashSet::new();
        let mut deps = Vec::new();
        for imp in imports {
            let base = imp.rsplit('.').next().unwrap_or(imp);
            if stems.contains(base) && base != stem && seen.insert(base.to_string()) {
                deps.push(base.to_string());
            }
        }
        deps.sort();
        resolved.insert(stem.clone(), deps);
    }
    // Emit canonical units in deterministic (stem) order.
    let mut stem_list: Vec<&String> = modules.keys().collect();
    stem_list.sort();
    let mut units = Vec::new();
    let mut edges = Vec::new();
    for stem in stem_list {
        let rel = &modules[stem];
        let id = UnitId(format!("python:{rel}"));
        let imports = resolved
            .get(stem)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|linkage| Symbol { linkage, abi: Abi::Python })
            .collect::<Vec<_>>();
        for imp in &imports {
            let dep_rel = &modules[&imp.linkage];
            edges.push((id.clone(), UnitId(format!("python:{dep_rel}"))));
        }
        units.push(UnitDecl {
            id,
            files: vec![rel.clone()],
            generated_from: None,
            exports: vec![Symbol { linkage: stem.clone(), abi: Abi::Python }],
            imports,
        });
    }
    edges.sort();
    edges.dedup();
    Ok(Fragment { units, edges, diagnostics })
}

/// Representative linkage for scaffolding/substitution: the unit's first
/// export, else the file stem of its authoritative source.
fn unit_stem(unit: &UnitDecl) -> String {
    if let Some(export) = unit.exports.first() {
        return export.linkage.clone();
    }
    if let Some(rel) = unit.files.first() {
        if let Some(stem) = Path::new(rel).file_stem().and_then(|s| s.to_str()) {
            return stem.to_string();
        }
    }
    unit.id.0.clone()
}

// --- Probe (extension census + PROJECT() langs + CTestTestfile presence) ---

/// Halt threshold for [`ProbeReport::unclaimed_share`]: above this,
/// [`select_composite`] refuses to choose a composite instead of silently
/// scheduling a partial DAG. Surfaced to CLI config by a later track.
pub const UNCLAIMED_HALT_THRESHOLD: f64 = 0.2;

/// Extensions that are never source: manifests, docs, lockfiles, caches.
/// Anything else with an extension that no frontend claims lands in
/// `unclaimed`.
const NON_SOURCE_EXTS: &[&str] = &[
    "md", "markdown", "rst", "txt", "toml", "cfg", "ini", "json", "yaml", "yml", "lock", "typed",
    "pyc", "pyo", "log",
    // CMake configure files are build config, never units (Track I: keeps the
    // extension census viable on CMake trees; no Python fixture has `.cmake`).
    "cmake",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeReport {
    /// Frontend ids selected by the census (e.g. `["python"]`).
    pub frontends: Vec<String>,
    /// Repo-rel paths with source-looking extensions no frontend claims.
    pub unclaimed: Vec<String>,
    /// `unclaimed / (claimed + unclaimed)`; `0.0` when no source files exist.
    pub unclaimed_share: f64,
    /// A `CTestTestfile*.cmake` was found: the repo configures a CTest spine.
    pub has_ctest: bool,
    /// Languages declared by `PROJECT(... LANGUAGES ...)` in CMakeLists.txt.
    pub cmake_languages: Vec<String>,
}

/// Walk `repo`, skipping VCS/caches/venvs/build output. Build artifacts must
/// never enter the census (origin rule); generated sources are excluded by
/// living under a build dir.
fn walk_probe_files(repo: &Path) -> Result<Vec<PathBuf>, AdapterError> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(repo).into_iter().filter_entry(|e| {
        let p = e.path();
        for seg in [
            ".git",
            "__pycache__",
            ".pytest_cache",
            ".venv",
            "venv",
            "dist",
            ".eggs",
            "target",
            "build",
            "_build",
        ] {
            if p.components().any(|c| c.as_os_str() == seg) {
                return false;
            }
        }
        true
    }) {
        let entry = entry.map_err(|e| AdapterError::Parse(e.to_string()))?;
        if entry.path().is_file() {
            out.push(entry.path().to_path_buf());
        }
    }
    out.sort();
    Ok(out)
}

/// Languages from `PROJECT(... LANGUAGES a b)`; bare `project(name ...)`
/// defaults to C/CXX per CMake semantics.
fn cmake_project_languages(cmake_lists: &str) -> Vec<String> {
    let mut langs = Vec::new();
    let upper = cmake_lists.to_uppercase();
    let mut search = upper.as_str();
    while let Some(idx) = search.find("PROJECT(") {
        let rest = &search[idx + "PROJECT(".len()..];
        let end = rest.find(')').unwrap_or(rest.len());
        let args = &rest[..end];
        if let Some(li) = args.find("LANGUAGES") {
            for tok in args[li + "LANGUAGES".len()..].split_whitespace() {
                let tok = tok.trim_matches(|c| c == '"' || c == '\'');
                if !tok.is_empty() {
                    langs.push(tok.to_string());
                }
            }
        }
        search = &rest[end.min(rest.len())..];
    }
    if langs.is_empty() && search.contains("PROJECT(") {
        langs.push("C".to_string());
        langs.push("CXX".to_string());
    }
    langs.sort();
    langs.dedup();
    langs
}

pub fn probe(repo: &Path) -> Result<ProbeReport, AdapterError> {
    let mut report = ProbeReport::default();
    let mut claimed = 0usize;
    // One census over every registered frontend. Extension-only claims here
    // (`compile_db: None`): the C/C++ frontend refines membership against the
    // compile DB at fragment time. Python fixtures claim nothing new, so their
    // reports are unchanged.
    let python = PythonFrontend;
    let fortran = FortranFrontend;
    let cxx = CxxFrontend;
    let mut python_claims = 0usize;
    let mut fortran_claims = 0usize;
    let mut cxx_claims = 0usize;
    for path in walk_probe_files(repo)? {
        let rel = path.strip_prefix(repo).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "CMakeLists.txt" {
            if let Ok(text) = std::fs::read_to_string(&path) {
                // First declaration wins: subdirectories re-run CMakeLists
                // without PROJECT(), which must not clobber the root langs.
                let langs = cmake_project_languages(&text);
                if !langs.is_empty() {
                    report.cmake_languages = langs;
                }
            }
            continue;
        }
        if name.starts_with("CTestTestfile") && path.extension().map(|x| x == "cmake").unwrap_or(false)
        {
            report.has_ctest = true;
            continue;
        }
        let Some(ext) = path.extension().and_then(|x| x.to_str()) else {
            continue; // extensionless (LICENSE, Dockerfile, …) is never source.
        };
        if python.claims(&path, None) {
            claimed += 1;
            python_claims += 1;
            continue;
        }
        if fortran.claims(&path, None) {
            claimed += 1;
            fortran_claims += 1;
            continue;
        }
        if cxx.claims(&path, None) {
            claimed += 1;
            cxx_claims += 1;
            continue;
        }
        if NON_SOURCE_EXTS.contains(&ext.to_lowercase().as_str()) {
            continue;
        }
        // No frontend claims this source-looking extension: warn, don't drop.
        report.unclaimed.push(rel);
    }
    if python_claims > 0 {
        report.frontends.push(PythonFrontend.id().to_string());
    }
    if fortran_claims > 0 {
        report.frontends.push(FortranFrontend.id().to_string());
    }
    if cxx_claims > 0 {
        report.frontends.push(CxxFrontend.id().to_string());
    }
    report.frontends.sort();
    report.frontends.dedup();
    report.unclaimed.sort();
    report.unclaimed.dedup();
    let total = claimed + report.unclaimed.len();
    report.unclaimed_share = if total == 0 { 0.0 } else { report.unclaimed.len() as f64 / total as f64 };
    Ok(report)
}

// --- S3: spine (ADR-008 verbatim; BuildCtx/Workload/TestRunner from core) ---

pub trait BuildBridge: Send + Sync {
    /// Ordered, fail-fast configure steps; frozen as `Manifest.prepare`.
    /// Empty when the stack needs no configure phase (Python/maturin).
    fn prepare(&self, cx: &BuildCtx) -> Vec<TestCommand>;
    fn build(&self, cx: &BuildCtx) -> Vec<TestCommand>;
    /// Rebuild with the Rust port compiled in, spliced in place of the unit,
    /// then verify the substitute imports/loads. Ordered, fail-fast.
    fn substitute(
        &self,
        cx: &BuildCtx,
        unit: &UnitDecl,
        rust_lib: &Path,
    ) -> Result<Vec<TestCommand>, AdapterError>;
    /// Port scaffold for `unit`: (path, content) pairs. Replaces the
    /// hand-written per-repo `mirror/<fixture>/template.json`.
    fn scaffold(&self, unit: &UnitDecl) -> Result<Vec<(String, String)>, AdapterError>;
    fn link_deps(&self, cx: &BuildCtx) -> Result<Vec<LinkDep>, AdapterError>;
    /// Build outputs that must survive for grading (wheels, …).
    fn artifacts(&self, cx: &BuildCtx) -> Vec<PathBuf>;
}

pub trait Profiler: Send + Sync {
    /// Untimed setup steps (mesh generation, fixture warm-up, …).
    fn setup(&self, w: &Workload) -> Vec<TestCommand>;
    /// The timed command; the core measures the child via wait4 rusage.
    fn timed(&self, w: &Workload) -> TestCommand;
    /// Hotspot baseline. `perf_available` gates perf-dependent tools only;
    /// the Python path never needs perf (reconciles ADR-003).
    fn hotspots(&self, w: &Workload, perf_available: bool) -> Result<HotspotBaseline, AdapterError>;
}

/// A link dependency discovered from the build. The composite classifies it;
/// frontends never see it (link targets come from the build, not the language).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinkDep {
    pub name: String,
    pub path: Option<String>,
}

/// Grading image requirements: base image, extra packages, and tree paths
/// that must be mounted writable (CTest writes `Testing/` into the build tree;
/// pytest needs none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSpec {
    pub base: String,
    pub packages: Vec<String>,
    pub writable: Vec<String>,
}

/// maturin/PyO3 bridge: `maturin develop` installs the extension into the
/// active environment, the frozen pytest suite grades it in place.
pub struct MaturinBridge;

impl MaturinBridge {
    fn base_command(&self, program: &str, args: Vec<String>) -> TestCommand {
        TestCommand {
            program: program.to_string(),
            args,
            cwd: Cwd::Tree,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: None,
            collect: Vec::new(),
        }
    }

    /// Wheel flow for the optimize stage (`maturin build`, not editable
    /// `develop`): a built wheel installs the same bytes into every venv, so
    /// parent/candidate A/B comparisons cannot alias one tree's `.so` into
    /// another venv's imports. Like [`BuildBridge::build`], the command
    /// carries no venv env: the stage/executor owns the environment, the
    /// bridge only describes the hermetic invocation. Paths are absolute
    /// (tree-anchored manifest, caller-resolved dist dir).
    pub fn build_wheel(&self, cx: &BuildCtx, dist: &Path) -> Vec<TestCommand> {
        vec![self.base_command(
            "maturin",
            vec![
                "build".to_string(),
                "--release".to_string(),
                "--manifest-path".to_string(),
                cx.tree.join("Cargo.toml").to_string_lossy().into_owned(),
                "-o".to_string(),
                dist.to_string_lossy().into_owned(),
            ],
        )]
    }
}

impl BuildBridge for MaturinBridge {
    fn prepare(&self, _cx: &BuildCtx) -> Vec<TestCommand> {
        // No configure phase: pytest runs from the tree and `maturin develop`
        // installs into the active environment at build time.
        Vec::new()
    }

    fn build(&self, cx: &BuildCtx) -> Vec<TestCommand> {
        let mut args = vec!["develop".to_string()];
        if cx.release {
            args.push("--release".to_string());
        }
        vec![self.base_command("maturin", args)]
    }

    fn substitute(
        &self,
        cx: &BuildCtx,
        unit: &UnitDecl,
        rust_lib: &Path,
    ) -> Result<Vec<TestCommand>, AdapterError> {
        if !rust_lib.exists() {
            return Err(AdapterError::Parse(format!(
                "substitute: rust lib '{}' does not exist",
                rust_lib.display()
            )));
        }
        // Reinstall the extension with the port compiled in, then fail fast
        // when the substituted module does not import.
        let mut cmds = self.build(cx);
        let stem = unit_stem(unit);
        let mut check = self.base_command(
            &PytestRunner::python_program(),
            vec!["-c".to_string(), format!("import {stem}")],
        );
        check.timeout_secs = Some(120);
        cmds.push(check);
        Ok(cmds)
    }

    fn scaffold(&self, unit: &UnitDecl) -> Result<Vec<(String, String)>, AdapterError> {
        let stem = unit_stem(unit);
        if stem.is_empty() {
            return Err(AdapterError::Parse(format!(
                "scaffold: unit '{}' has no module name",
                unit.id
            )));
        }
        let lib_rs = format!(
            "// Port scaffold for unit `{id}` (replaces mirror/<fixture>/template.json).\n\
             // Build with `maturin develop`; the frozen pytest suite grades it in place.\n\
             use pyo3::prelude::*;\n\
             \n\
             #[pymodule]\n\
             mod {stem} {{\n\
                 use super::*;\n\
             \n\
                 // TODO(port): expose the original `{stem}` API surface here.\n\
             }}\n",
            id = unit.id,
            stem = stem,
        );
        let cargo_toml = format!(
            "[package]\nname = \"{stem}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
             \n[lib]\nname = \"{stem}\"\ncrate-type = [\"cdylib\"]\n\
             \n[dependencies]\npyo3 = {{ version = \"0.22\", features = [\"extension-module\"] }}\n",
            stem = stem,
        );
        Ok(vec![
            ("src/lib.rs".to_string(), lib_rs),
            ("Cargo.toml".to_string(), cargo_toml),
        ])
    }

    fn link_deps(&self, cx: &BuildCtx) -> Result<Vec<LinkDep>, AdapterError> {
        // Declared `[project] dependencies` from pyproject.toml; names only
        // (version specifiers stripped) for `classify_dep`.
        let pp = cx.tree.join("pyproject.toml");
        if !pp.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&pp)?;
        let parsed: toml::Value =
            text.parse().map_err(|e| AdapterError::Parse(format!("pyproject.toml: {e}")))?;
        let mut deps = Vec::new();
        if let Some(list) = parsed
            .get("project")
            .and_then(|p| p.get("dependencies"))
            .and_then(|d| d.as_array())
        {
            for dep in list {
                if let Some(req) = dep.as_str() {
                    let name = req
                        .split([' ', ';', '<', '>', '=', '!', '[', '~'])
                        .next()
                        .unwrap_or(req)
                        .trim();
                    if !name.is_empty() {
                        deps.push(LinkDep { name: name.to_string(), path: None });
                    }
                }
            }
        }
        deps.sort_by(|a, b| a.name.cmp(&b.name));
        deps.dedup_by(|a, b| a.name == b.name);
        Ok(deps)
    }

    fn artifacts(&self, cx: &BuildCtx) -> Vec<PathBuf> {
        // Wheels materialized under the build dir (`maturin build`); scan by
        // suffix instead of assuming file names.
        let mut out = Vec::new();
        if cx.build_dir.is_dir() {
            for entry in walkdir::WalkDir::new(cx.build_dir)
                .into_iter()
                .filter_entry(|e| {
                    let p = e.path();
                    !p.components().any(|c| {
                        c.as_os_str() == ".git" || c.as_os_str() == "target" || c.as_os_str() == ".cargo"
                    })
                })
            {
                if let Ok(entry) = entry {
                    let p = entry.path();
                    if p.is_file() && p.extension().map(|x| x == "whl").unwrap_or(false) {
                        out.push(p.to_path_buf());
                    }
                }
            }
        }
        out.sort();
        out
    }
}

/// pytest runner: owns the pytest invocation, the `-v` output parser, the
/// oracle file set, and the ADR-002 pyproject normalization.
pub struct PytestRunner;

impl PytestRunner {
    /// Interpreter program. The one `python3` literal the Python spine owns;
    /// absolute resolution happens in the executor at spawn time.
    pub fn python_program() -> String {
        "python3".to_string()
    }

    /// Suite argv groups, mirroring the split invocation (`pytest test/unit`
    /// then `pytest test/integration`) when both dirs exist.
    fn suite_groups(tree: &Path) -> Vec<Vec<String>> {
        if tree.join("test/unit").is_dir() && tree.join("test/integration").is_dir() {
            vec![
                vec!["test/unit".to_string()],
                vec!["test/integration".to_string()],
            ]
        } else {
            vec![Vec::new()]
        }
    }

    fn pytest_command(tree: &Path, extra: &[String], verbose_flags: &[&str]) -> TestCommand {
        let mut args = vec!["-m".to_string(), "pytest".to_string()];
        args.extend(extra.iter().cloned());
        args.extend(verbose_flags.iter().map(|s| s.to_string()));
        let mut env_set = vec![("PY_COLORS".to_string(), "0".to_string())];
        // src-layout packages grade with the tree's src/ on the path. The
        // value is tree-absolute so frozen commands stay hermetic (the legacy
        // spawn additionally inherited ambient PYTHONPATH; frozen commands do
        // not, which is identical whenever the ambient value is unset).
        let src = tree.join("src");
        if src.is_dir() {
            env_set.push(("PYTHONPATH".to_string(), src.to_string_lossy().into_owned()));
        }
        TestCommand {
            program: Self::python_program(),
            args,
            cwd: Cwd::Tree,
            env_set,
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: None,
            collect: Vec::new(),
        }
    }
    /// Rebind tree-graded pytest commands onto a grade venv: `program` becomes
    /// the venv interpreter and the src-layout `PYTHONPATH` is dropped (the
    /// installed extension must win). Commands whose program is not the
    /// runner's interpreter pass through unchanged. Grade/held-out time only,
    /// never frozen: frozen commands stay interpreter-agnostic.
    pub fn bind_venv(cmds: &[TestCommand], venv_python: &Path) -> Vec<TestCommand> {
        cmds.iter()
            .map(|cmd| {
                if cmd.program != Self::python_program() {
                    return cmd.clone();
                }
                let mut rebound = cmd.clone();
                rebound.program = venv_python.to_string_lossy().into_owned();
                rebound.env_set.retain(|(k, _)| k != "PYTHONPATH");
                if !rebound.env_remove.iter().any(|k| k == "PYTHONPATH") {
                    rebound.env_remove.push("PYTHONPATH".to_string());
                }
                rebound
            })
            .collect()
    }




    /// Test-relevant sections of pyproject.toml, normalized (ADR-002, ported
    /// from the oracle which no longer owns it): packaging metadata
    /// ([build-system], [project] name/version) is owned by the Stage-1 port,
    /// while test config ([tool.pytest*], coverage, tox, hypothesis) defines
    /// the suite. Adding a NEW test section changes this text.
    fn pyproject_test_sections(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let text = String::from_utf8_lossy(bytes);
        let parsed: Result<toml::Value, _> = text.parse();
        let v = match parsed {
            Ok(v) => v,
            Err(_) => {
                let mut h = Sha256::new();
                h.update(bytes);
                return format!("unparseable:{:x}", h.finalize());
            }
        };
        let mut picked = BTreeMap::new();
        if let Some(tool) = v.get("tool").and_then(|t| t.as_table()) {
            for (key, val) in tool {
                let k = key.to_lowercase();
                if k.starts_with("pytest") || k.starts_with("coverage") || k.starts_with("tox") || k.starts_with("hypothesis") {
                    picked.insert(key.clone(), val.clone());
                }
            }
        }
        if picked.is_empty() {
            return String::new();
        }
        let table: toml::map::Map<String, toml::Value> = picked.into_iter().collect();
        toml::to_string(&toml::Value::Table(table)).unwrap_or_default()
    }
}

impl TestRunner for PytestRunner {
    fn id(&self) -> &'static str {
        "pytest"
    }

    fn invocation(&self, cx: &BuildCtx) -> Vec<TestCommand> {
        Self::suite_groups(cx.tree)
            .iter()
            .map(|group| Self::pytest_command(cx.tree, group, &["-v", "-rs", "-rxX", "--tb=short"]))
            .collect()
    }

    fn grade(&self, runs: &[RunOutput]) -> Result<GradedResult, AdapterError> {
        // Port of the legacy `parse_pytest_verbose` line rules: per-run
        // stdout+stderr text, `::` node ids only, summary fallback with max
        // merge, synthetic ids for count-only fallbacks. The executor owns the
        // `$ <program> <args>` prefix lines (already inside `stdout`); grade
        // never adds its own.
        let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();
        // Per-run `r_passed`/`r_failed` feed the outcomes map below; legacy
        // counters derive from it via `GradedResult::from_outcomes`.
        let mut skipped_ids: Vec<String> = Vec::new();
        let mut xfailed_ids: Vec<String> = Vec::new();
        let mut deselected_ids: Vec<String> = Vec::new();
        let mut merged = String::new();
        let mut exit_code = 0i32;
        for run in runs {
            let text = format!("{}\n{}\n", run.stdout, run.stderr);
            merged.push_str(&text);
            let mut r_passed = 0u32;
            let mut r_failed = 0u32;
            for line in text.lines() {
                let t = line.trim();
                // Verbose per-test lines: "test_x.py::Test::test_y PASSED".
                if t.contains(" PASSED") || t.contains(" passed") {
                    // Only lines with :: (test node ids); summary lines excluded.
                    if t.contains("::") {
                        r_passed += 1;
                        if let Some(id) = t.split_whitespace().next() {
                            outcomes.insert(id.to_string(), Outcome::Pass);
                        }
                    }
                } else if t.contains(" FAILED") || t.contains(" ERROR") {
                    if t.contains("::") {
                        r_failed += 1;
                        if let Some(id) = t.split_whitespace().next() {
                            outcomes.insert(id.to_string(), Outcome::Fail);
                        }
                    }
                } else if t.contains(" SKIPPED") {
                    if let Some(id) = t.split_whitespace().next() {
                        if id.contains("::") {
                            skipped_ids.push(id.to_string());
                            outcomes.insert(id.to_string(), Outcome::Skip);
                        }
                    }
                } else if t.contains(" XFAIL") {
                    if let Some(id) = t.split_whitespace().next() {
                        if id.contains("::") {
                            xfailed_ids.push(id.to_string());
                            outcomes.insert(id.to_string(), Outcome::XFail);
                        }
                    }
                } else if t.contains(" DSELECTED") || t.contains("deselected") {
                    // Deselected has no Outcome variant: id only.
                    if let Some(id) = t.split_whitespace().next() {
                        if id.contains("::") {
                            deselected_ids.push(id.to_string());
                        }
                    }
                }
                // Short summary info lines ("SKIPPED [1] file:line: reason")
                // duplicate the per-test SKIPPED above; skip to avoid double count.
            }
            // Fallback: summary line "80 passed, 28 subtests passed in 0.18s".
            let (s_passed, s_failed, s_skipped, s_xfail, s_deselect) = summary_counts(&text);
            if r_passed == 0 && r_failed == 0 {
                r_passed = s_passed;
                r_failed = s_failed;
            } else {
                // Verbose undercounts parametrized summaries (e.g. subtests): take max.
                if s_passed > r_passed {
                    r_passed = s_passed;
                }
                if s_failed > r_failed {
                    r_failed = s_failed;
                }
            }
            // Count-only summaries reuse the legacy `skipped[i]`/`xfailed[i]` ids below.
            if skipped_ids.is_empty() && s_skipped > 0 {
                for i in 0..s_skipped {
                    let id = format!("skipped[{i}]");
                    skipped_ids.push(id.clone());
                    outcomes.insert(id, Outcome::Skip);
                }
            }
            if xfailed_ids.is_empty() && s_xfail > 0 {
                for i in 0..s_xfail {
                    let id = format!("xfailed[{i}]");
                    xfailed_ids.push(id.clone());
                    outcomes.insert(id, Outcome::XFail);
                }
            }
            if deselected_ids.is_empty() && s_deselect > 0 {
                for i in 0..s_deselect {
                    deselected_ids.push(format!("deselected[{i}]"));
                }
            }
            // Count-only summaries (no verbose node ids): synthesize stable ids
            // so `from_outcomes` derives the legacy counters.
            let verbose_pass = outcomes.values().filter(|o| **o == Outcome::Pass).count() as u32;
            for i in verbose_pass..r_passed {
                outcomes.insert(format!("summary-passed-{i}"), Outcome::Pass);
            }
            let verbose_fail = outcomes.values().filter(|o| **o == Outcome::Fail).count() as u32;
            for i in verbose_fail..r_failed {
                outcomes.insert(format!("summary-failed-{i}"), Outcome::Fail);
            }
            // Legacy exit-code fold: a failing run sets the code, else first nonzero wins.
            if run.exit_code != 0 && r_failed > 0 {
                exit_code = run.exit_code;
            } else if run.exit_code != 0 && exit_code == 0 {
                exit_code = run.exit_code;
            }
        }
        let mut graded = GradedResult::from_outcomes(exit_code, outcomes, merged);
        deselected_ids.sort();
        deselected_ids.dedup();
        graded.deselected = deselected_ids;
        Ok(graded)
    }

    fn observe(&self, runs: &[RunOutput], specs: &[ObservableSpec]) -> Vec<Observation> {
        // Full-regex extraction via the interpreter the runner owns (no new
        // Rust deps): one `python3 -c re.search` over a JSON payload.
        // Unparseable specs or a missing interpreter yield no observations;
        // the oracle treats a missing observation as a mismatch downstream.
        if runs.is_empty() || specs.is_empty() {
            return Vec::new();
        }
        let payload = serde_json::json!({
            "specs": specs.iter().map(|s| serde_json::json!({
                "test_glob": s.test_glob, "source": s.source, "regex": s.regex,
            })).collect::<Vec<_>>(),
            "runs": runs.iter().map(|r| serde_json::json!({
                "stdout": r.stdout, "stderr": r.stderr,
                "artifacts": r.artifacts.iter().map(|(k, v)| {
                    (k.clone(), String::from_utf8_lossy(v).into_owned())
                }).collect::<BTreeMap<_, _>>(),
            })).collect::<Vec<_>>(),
        });
        let script = r#"
import sys, json, re
payload = json.load(sys.stdin)
out = []
for spec in payload["specs"]:
    try:
        rx = re.compile(spec["regex"])
    except re.error:
        continue
    for run in payload["runs"]:
        src = spec["source"]
        if src == "stdout":
            text = run["stdout"]
        elif src == "stderr":
            text = run["stderr"]
        else:
            text = run["artifacts"].get(src, "")
        m = rx.search(text)
        if m:
            val = m.group(1) if m.lastindex else m.group(0)
            out.append({"test_id": spec["test_glob"], "key": src + ":" + val})
print(json.dumps(out))
"#;
        let mut child = match std::process::Command::new(Self::python_program())
            .arg("-c")
            .arg(script)
            .current_dir(std::env::temp_dir())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return Vec::new(),
        };
        let mut observations = Vec::new();
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(payload.to_string().as_bytes());
        }
        if let Ok(output) = child.wait_with_output() {
            if output.status.success() {
                if let Ok(items) = serde_json::from_slice::<Vec<Observation>>(&output.stdout) {
                    observations = items;
                }
            }
        }
        observations
    }

    fn oracle_files(&self, repo: &Path) -> Result<Vec<OracleFile>, AdapterError> {
        // Same selection as the legacy discovery: tests + configs +
        // fixture/data + CI workflows; bench/ never oracle.
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
            let entry = entry.map_err(|e| AdapterError::Parse(e.to_string()))?;
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
            let is_config = matches!(
                rel.as_str(),
                "pytest.ini" | "tox.ini" | "setup.cfg" | "pyproject.toml"
            );
            let is_fixture_data =
                (rel.contains("fixture") || rel.contains("data")) && (rel.contains("test"));
            let is_ci = rel.starts_with(".github/workflows/");
            if !(is_test || is_config || is_fixture_data || is_ci) {
                continue;
            }
            if rel.contains("bench") {
                continue;
            }
            // Test sources are the frozen spec content; configs/CI/data are
            // fixture material the suite consumes.
            let kind = if is_test { OracleKind::Fixture } else { OracleKind::Spec };
            out.push(OracleFile { path: rel, kind });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out.dedup_by(|a, b| a.path == b.path);
        Ok(out)
    }

    fn normalize_for_hash(&self, rel: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        // `Some` = hash these bytes instead; `None` = hash the raw file.
        // Only pyproject.toml normalizes (ADR-002 test sections, above).
        if rel == "pyproject.toml" {
            Some(Self::pyproject_test_sections(bytes).into_bytes())
        } else {
            None
        }
    }

    fn heldout(&self, suite: &Path, cx: &BuildCtx) -> Vec<TestCommand> {
        vec![Self::pytest_command(
            cx.tree,
            &[suite.to_string_lossy().into_owned()],
            &["-v", "--tb=short"],
        )]
    }

    fn config_hash(&self, cx: &BuildCtx) -> Result<String, AdapterError> {
        // Deterministic FNV-1a64 over the normalized oracle set plus the
        // frozen invocation (MPI ranks ride in `launcher` and hash here when
        // runners set one; the Python runner never does).
        let mut files = self.oracle_files(cx.tree)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut h: u64 = 0xcbf29ce484222325;
        let mut mix = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h ^= 0xff;
            h = h.wrapping_mul(0x100000001b3);
        };
        for f in &files {
            mix(f.path.as_bytes());
            let abs = cx.tree.join(&f.path);
            if let Ok(bytes) = std::fs::read(&abs) {
                let normalized = self.normalize_for_hash(&f.path, &bytes).unwrap_or(bytes);
                mix(&normalized);
            }
        }
        for cmd in self.invocation(cx) {
            mix(cmd.program.as_bytes());
            for arg in &cmd.args {
                mix(arg.as_bytes());
            }
        }
        Ok(format!("{h:016x}"))
    }
}

/// `pytest` summary line: `80 passed, 28 subtests passed in 0.18s`,
/// `1 failed, 79 passed`, … → (passed, failed, skipped, xfailed, deselected).
fn summary_counts(text: &str) -> (u32, u32, u32, u32, u32) {
    let mut found = (0u32, 0u32, 0u32, 0u32, 0u32);
    for line in text.lines() {
        let t = line.trim();
        if !(t.contains("passed") || t.contains("failed") || t.contains("skipped") || t.contains("xfailed") || t.contains("deselected")) {
            continue;
        }
        for chunk in t.split(',') {
            let mut num: Option<u32> = None;
            let mut kind: Option<&str> = None;
            for tok in chunk.split_whitespace() {
                if let Ok(n) = tok.parse::<u32>() {
                    num = Some(n);
                } else {
                    let k = tok.trim_matches(|c| c == '.' || c == 's');
                    if ["passed", "failed", "skipped", "xfailed", "deselected", "xpass"].contains(&k) {
                        kind = Some(k);
                    }
                }
            }
            if let (Some(n), Some(k)) = (num, kind) {
                match k {
                    "passed" => found.0 = found.0.max(n),
                    "failed" => found.1 = found.1.max(n),
                    "skipped" => found.2 = found.2.max(n),
                    "xfailed" | "xpass" => found.3 = found.3.max(n),
                    "deselected" => found.4 = found.4.max(n),
                    _ => {}
                }
            }
        }
    }
    found
}

/// Python profiler: untimed setup + timed `python -c` harness; hotspot
/// baseline via the profile crate (py-spy preferred, cProfile fallback — no
/// perf dependency per ADR-003). Holds the repo and interpreter paths because
/// [`Profiler`] methods take no [`BuildCtx`].
pub struct PyProfiler {
    pub repo: PathBuf,
    pub python: PathBuf,
}

impl PyProfiler {
    fn cwd(&self) -> Cwd {
        Cwd::Rel(self.repo.to_string_lossy().into_owned())
    }

    fn command(&self, args: Vec<String>) -> TestCommand {
        TestCommand {
            program: self.python.to_string_lossy().into_owned(),
            args,
            cwd: self.cwd(),
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: None,
            collect: Vec::new(),
        }
    }
}

impl Profiler for PyProfiler {
    fn setup(&self, w: &Workload) -> Vec<TestCommand> {
        if w.setup.trim().is_empty() {
            return Vec::new();
        }
        vec![self.command(vec!["-c".to_string(), w.setup.clone()])]
    }

    fn timed(&self, w: &Workload) -> TestCommand {
        // Mirrors the profile crate's timing harness shape: setup once, then
        // `iters` stmt repetitions bracketed by perf_counter; the core reads
        // the printed elapsed seconds and measures CPU via wait4 rusage.
        let harness = format!(
            "import time\n{setup}\n_t0 = time.perf_counter()\nfor _ in range({iters}):\n    {stmt}\n_t1 = time.perf_counter()\nprint(f\"elapsed={{_t1 - _t0:.6f}}\")",
            setup = w.setup,
            iters = w.iters,
            stmt = w.stmt,
        );
        let mut cmd = self.command(vec!["-c".to_string(), harness]);
        cmd.timeout_secs = Some(600);
        cmd
    }

    fn hotspots(&self, w: &Workload, perf_available: bool) -> Result<HotspotBaseline, AdapterError> {
        // The Python path never needs perf; the flag only gates perf-based
        // tools, which this stack does not use.
        let _ = perf_available;
        rustsmith_profile::capture_hotspot_baseline(&self.repo, &[w.stmt.clone()])
            .map_err(|e| AdapterError::Parse(e.to_string()))
    }
}

// --- Composite ---

/// Enumerate candidate unit sources under `tree`. Build outputs are excluded
/// by origin (the `build_dir` subtree when given, plus conventional build dir
/// names): generated files must never become units. Deviation from the legacy
/// walk, which only knew `target/`: a no-op on the Python fixtures, required
/// before any codegen frontend (`.src` → `.F90`) exists.
fn walk_source_files(tree: &Path, build_dir: Option<&Path>) -> Result<Vec<PathBuf>, AdapterError> {
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(tree)
        .into_iter()
        .filter_entry(|e| {
            let p = e.path();
            if let Some(bd) = build_dir {
                if bd != tree && p.starts_with(bd) {
                    return false;
                }
            }
            for seg in [
                ".git",
                "__pycache__",
                ".pytest_cache",
                ".venv",
                "venv",
                "dist",
                ".eggs",
                "target",
                "build",
                "_build",
                "docs",
                "test",
                "tests",
                "bench",
            ] {
                if p.components().any(|c| c.as_os_str() == seg) {
                    return false;
                }
            }
            true
        })
    {
        let entry = entry.map_err(|e| AdapterError::Parse(e.to_string()))?;
        if entry.path().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

/// Run every frontend over `files`, union the fragments, and resolve imports
/// through a linkage symbol index (adds cross-frontend edges). Shared by
/// [`assemble_call_graph`] and [`CompositeAdapter::partition`]: partition
/// coarsens/marks units between collection and stem mapping, while the legacy
/// path maps immediately so its output is unchanged.
fn collect_fragments(
    tree: &Path,
    files: &[PathBuf],
    frontends: &[Box<dyn Frontend>],
    compile_db: Option<&Path>,
) -> Result<(Vec<UnitDecl>, HashSet<(UnitId, UnitId)>, Vec<String>), AdapterError> {
    let compiler_ids: BTreeMap<String, String> = BTreeMap::new();
    let fcx = FragmentCtx { repo: tree, files, compile_db, compiler_ids: &compiler_ids };
    let mut units: Vec<UnitDecl> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut edges: HashSet<(UnitId, UnitId)> = HashSet::new();
    let mut diagnostics = Vec::new();
    for frontend in frontends {
        let fragment = frontend.fragment(&fcx)?;
        diagnostics.extend(fragment.diagnostics);
        for unit in fragment.units {
            if seen_ids.insert(unit.id.0.clone()) {
                units.push(unit);
            } else {
                diagnostics.push(format!(
                    "duplicate unit id '{}' from frontend '{}': dropped",
                    unit.id,
                    frontend.id()
                ));
            }
        }
        edges.extend(fragment.edges);
    }
    let mut linkage_to_unit: HashMap<&str, &UnitId> = HashMap::new();
    for unit in &units {
        for export in &unit.exports {
            if let Some(prev) = linkage_to_unit.insert(export.linkage.as_str(), &unit.id) {
                if prev != &unit.id {
                    diagnostics.push(format!(
                        "linkage '{}' exported by both '{}' and '{}': first wins",
                        export.linkage, prev, unit.id
                    ));
                }
            }
        }
    }
    for unit in &units {
        for import in &unit.imports {
            if let Some(exporter) = linkage_to_unit.get(import.linkage.as_str()) {
                if *exporter != &unit.id {
                    edges.insert((unit.id.clone(), (*exporter).clone()));
                }
            }
        }
    }
    Ok((units, edges, diagnostics))
}

/// Map canonical unit ids back to module stems for the legacy [`CallGraph`]
/// shape. The UnitId rollout to frozen keys happens atomically in a later
/// track; until then stems keep `dag.json` byte-identical to HEAD.
fn stems_to_call_graph(
    units: &[UnitDecl],
    edges: &HashSet<(UnitId, UnitId)>,
    diagnostics: &mut Vec<String>,
) -> CallGraph {
    let mut id_to_stem: HashMap<&str, &str> = HashMap::new();
    let mut stem_to_rel: HashMap<&str, &str> = HashMap::new();
    for unit in units {
        let stem: &str = if let Some(export) = unit.exports.first() {
            export.linkage.as_str()
        } else if let Some(rel) = unit.files.first() {
            Path::new(rel).file_stem().and_then(|s| s.to_str()).unwrap_or(&unit.id.0)
        } else {
            &unit.id.0
        };
        id_to_stem.insert(unit.id.0.as_str(), stem);
        if let Some(prev_rel) =
            stem_to_rel.insert(stem, unit.files.first().map(|s| s.as_str()).unwrap_or(""))
        {
            if stem_to_rel[stem] != prev_rel && !prev_rel.is_empty() {
                diagnostics.push(format!("stem collision '{stem}': unit '{}' keeps the node", unit.id));
            }
        }
    }
    let mut modules: HashMap<String, String> = HashMap::new();
    for (stem, rel) in &stem_to_rel {
        modules.insert(stem.to_string(), rel.to_string());
    }
    let mut stem_edges: Vec<(String, String)> = Vec::new();
    for (dependent, dependency) in edges {
        match (id_to_stem.get(dependent.0.as_str()), id_to_stem.get(dependency.0.as_str())) {
            (Some(a), Some(b)) => stem_edges.push((a.to_string(), b.to_string())),
            _ => diagnostics
                .push(format!("edge to unknown unit: '{dependent}' -> '{dependency}'")),
        }
    }
    stem_edges.sort();
    stem_edges.dedup();
    CallGraph { modules, edges: stem_edges }
}

/// Run every frontend over `files`, union the fragments, resolve imports
/// through a linkage symbol index (adds cross-frontend edges), and map
/// canonical ids back to module stems for the legacy [`CallGraph`] shape.
/// The UnitId rollout to frozen keys happens atomically in a later track;
/// until then stems keep `dag.json` byte-identical to HEAD.
fn assemble_call_graph(
    tree: &Path,
    files: &[PathBuf],
    frontends: &[Box<dyn Frontend>],
    compile_db: Option<&Path>,
    diagnostics: &mut Vec<String>,
) -> Result<CallGraph, AdapterError> {
    let (units, edges, mut collected) = collect_fragments(tree, files, frontends, compile_db)?;
    diagnostics.append(&mut collected);
    Ok(stems_to_call_graph(&units, &edges, diagnostics))
}

/// Per-repo assembly: the only adapter stages ever touch. Holds the selected
/// frontends plus the single spine (runner + bridge + profiler).
pub struct CompositeAdapter {
    pub frontends: Vec<Box<dyn Frontend>>,
    pub runner: Box<dyn TestRunner>,
    pub bridge: Box<dyn BuildBridge>,
    pub profiler: Box<dyn Profiler>,
}

impl CompositeAdapter {
    /// Language ids of the active frontends (e.g. `["python"]`).
    pub fn languages(&self) -> Vec<String> {
        let mut langs: Vec<String> =
            self.frontends.iter().map(|f| f.id().to_string()).collect();
        langs.sort();
        langs.dedup();
        langs
    }

    /// Runner id for `Manifest.runner` (e.g. `"pytest"`).
    pub fn runner_id(&self) -> String {
        self.runner.id().to_string()
    }

    /// Assemble fragments into a legacy [`UnitDag`]: union units, resolve
    /// imports through a linkage symbol index (adds cross-frontend edges),
    /// map canonical ids back to module stems, order leaf-first with Kahn.
    /// Returns the DAG plus human diagnostics (dup stems, collisions,
    /// skipped files); cycles and threshold violations are errors.
    pub fn partition(&self, cx: &BuildCtx) -> Result<(UnitDag, Vec<String>), AdapterError> {
        let files = walk_source_files(cx.tree, Some(cx.build_dir))?;
        // The compile DB is picked up when the build dir provides one: the
        // C/C++ frontend refines its claims against it (Track I).
        let compile_db_path = cx.build_dir.join("compile_commands.json");
        let compile_db = compile_db_path.is_file().then_some(compile_db_path);
        let (mut units, mut edges, mut diagnostics) =
            collect_fragments(cx.tree, &files, &self.frontends, compile_db.as_deref())?;
        // Track I granularity: non-BIND(C) Fortran has no stable C ABI, so
        // those units merge to file granularity; vendored and uncovered units
        // are marked out of scope. Python-only trees have no such units, so
        // their DAG output is unchanged.
        coarsen_fortran_units(&mut units, &mut edges, &mut diagnostics);
        mark_out_of_scope(&units, &mut diagnostics, cx.build_dir);
        let graph = stems_to_call_graph(&units, &edges, &mut diagnostics);
        let dag = dag_from_call_graph(&graph);
        // 5. Kahn leaf-first; a cycle is an error, not a diagnostic.
        let order = dag.leaf_first_order()?;
        debug_assert!(dag.verify_order(&order));
        diagnostics.sort();
        diagnostics.dedup();
        Ok((dag, diagnostics))
    }

    pub fn classify_dep(&self, dep: &LinkDep) -> DepClass {
        // Single-Python composites keep the name-based ecosystem rules. Any
        // other spine classifies link targets by path origin (Track I): the
        // frontend never sees them (link targets come from the build, not the
        // language).
        if self.languages() == vec!["python".to_string()] {
            return classify_python_dep(&dep.name);
        }
        classify_link_dep(dep)
    }

    pub fn license_terms(&self, repo: &Path) -> Result<Vec<(String, Attribution)>, AdapterError> {
        // Single-Python composites keep the exact HEAD entry. Any other spine
        // reports one entry per license file (Track I).
        if self.languages() == vec!["python".to_string()] {
            return Ok(vec![python_license_terms(repo)?]);
        }
        generic_license_terms(repo, self.frontends.iter().any(|f| f.id() == PythonFrontend.id()))
    }

    pub fn image(&self) -> ImageSpec {
        // Union of frontend + spine toolchain requirements. The package set is
        // derived from the runner id so no pytest literal lives in composite
        // code; no writable mounts on the pytest spine (pytest never writes
        // into the tree). The CTest spine writes `Testing/` into the build
        // tree, so the build dir mounts writable (Track I).
        if self.runner_id() == CtestRunner::RUNNER_ID {
            return ImageSpec {
                base: "gcc:14".to_string(),
                packages: vec![
                    "cmake".to_string(),
                    "gfortran".to_string(),
                    "openmpi-bin".to_string(),
                ],
                writable: vec!["build".to_string()],
            };
        }
        ImageSpec {
            base: "python:3.11-slim".to_string(),
            packages: vec![self.runner_id()],
            writable: Vec::new(),
        }
    }
}
/// Legacy [`Adapter`] surface over the composite, so stages that still read
/// `BuildInfo`/`CallGraph`/`TestInventory` (recon this track) go through
/// [`select_composite`] instead of constructing a language adapter. The
/// single-frontend case reproduces the legacy outputs exactly.
///
/// Name collisions with the ADR-008 spine (`classify_dep`, `license_terms`
/// exist on both) resolve to the inherent spine methods in dot syntax;
/// call the legacy ones with UFCS: `Adapter::classify_dep(&composite, dep)`,
/// `Adapter::license_terms(&composite, repo)`.
impl Adapter for CompositeAdapter {
    fn language(&self) -> &'static str {
        // Single-frontend composites preserve the frozen `build.language` key;
        // multi-language trees report "polyglot" until the frozen key renames
        // to `languages` in the UnitId-rollout track.
        if self.frontends.len() == 1 {
            self.frontends[0].id()
        } else {
            "polyglot"
        }
    }

    fn detect(&self, repo: &Path) -> Result<BuildInfo, AdapterError> {
        if self.languages() == vec!["python".to_string()] {
            return python_detect(repo);
        }
        // CMake/CTest trees: runner-derived invocation; build system is cmake.
        if self.runner_id() == CtestRunner::RUNNER_ID {
            let cx = BuildCtx { tree: repo, build_dir: repo, release: false };
            return Ok(BuildInfo {
                language: self.language().to_string(),
                build_system: "cmake".to_string(),
                layout: "cmake".to_string(),
                invocation: self
                    .runner
                    .invocation(&cx)
                    .iter()
                    .map(|c| format!("{} {}", c.program, c.args.join(" ")))
                    .collect(),
            });
        }
        // Mixed trees: runner-derived invocation; layout/build-system unknown.
        let cx = BuildCtx { tree: repo, build_dir: repo, release: false };
        Ok(BuildInfo {
            language: self.language().to_string(),
            build_system: "mixed".to_string(),
            layout: "mixed".to_string(),
            invocation: self
                .runner
                .invocation(&cx)
                .iter()
                .map(|c| format!("{} {}", c.program, c.args.join(" ")))
                .collect(),
        })
    }

    fn call_graph(&self, repo: &Path) -> Result<CallGraph, AdapterError> {
        let files = walk_source_files(repo, None)?;
        let mut diagnostics = Vec::new();
        let graph = assemble_call_graph(repo, &files, &self.frontends, None, &mut diagnostics)?;
        // The legacy compat path carries no diagnostics channel; partition() keeps them.
        let _ = diagnostics;
        Ok(graph)
    }

    fn test_inventory(&self, repo: &Path) -> Result<TestInventory, AdapterError> {
        // The spine owns the inventory: a ctest-graded tree (even one with
        // Python helpers) inventories through CTest, not pytest.
        if self.runner_id() == CtestRunner::RUNNER_ID {
            return ctest_test_inventory(repo);
        }
        if self.frontends.iter().any(|f| f.id() == PythonFrontend.id()) {
            return python_test_inventory(repo);
        }
        Ok(TestInventory {
            test_files: Vec::new(),
            config_refs: Vec::new(),
            fixture_globs: Vec::new(),
            ci_invokers: Vec::new(),
        })
    }

    fn classify_dep(&self, dep: &str) -> DepClass {
        self.classify_dep(&LinkDep { name: dep.to_string(), path: None })
    }

    fn license_terms(&self, repo: &Path) -> Result<Attribution, AdapterError> {
        Ok(self
            .license_terms(repo)?
            .into_iter()
            .next()
            .map(|(_, attribution)| attribution)
            .unwrap_or(Attribution {
                license: "unknown".into(),
                header_text: String::new(),
                notice_extra: String::new(),
            }))
    }
}


/// Choose the composite for `repo` by probing it: extension census +
/// `PROJECT()` langs + CTestTestfile presence. Halts above
/// [`UNCLAIMED_HALT_THRESHOLD`] instead of scheduling a partial DAG; halts
/// when no frontend claims anything (the CTest spine lands in a later track).
pub fn select_composite(repo: &Path) -> Result<(CompositeAdapter, ProbeReport), AdapterError> {
    let report = probe(repo)?;
    // Halt threshold guards silent partial Python DAGs (every `.py` should be
    // claimed). CTest/polyglot trees carry data/mesh artifacts alongside
    // sources, so their partial DAG is explicit (frozen `unclaimed` +
    // partition diagnostics), never silent: only the single-Python census
    // halts here. Python-only output is unchanged.
    let non_python = report.frontends.iter().any(|f| f != PythonFrontend.id());
    let ctest_spine = non_python || report.has_ctest;
    if report.unclaimed_share > UNCLAIMED_HALT_THRESHOLD && !ctest_spine {
        return Err(AdapterError::Parse(format!(
            "probe: unclaimed share {:.2} above threshold {:.2}: {}",
            report.unclaimed_share,
            UNCLAIMED_HALT_THRESHOLD,
            report.unclaimed.join(", ")
        )));
    }
    let mut frontends: Vec<Box<dyn Frontend>> = Vec::new();
    for name in &report.frontends {
        match name.as_str() {
            "python" => frontends.push(Box::new(PythonFrontend)),
            "fortran" => frontends.push(Box::new(FortranFrontend)),
            "cxx" => frontends.push(Box::new(CxxFrontend)),
            other => {
                return Err(AdapterError::Parse(format!(
                    "probe: no frontend registered for '{other}'"
                )))
            }
        }
    }
    if frontends.is_empty() {
        return Err(AdapterError::Parse(format!(
            "probe: no language claimed (ctest={}, cmake_langs={:?}); the CTest spine lands in a later track",
            report.has_ctest, report.cmake_languages
        )));
    }
    // Spine choice: the single-Python case keeps the exact HEAD stack (pytest
    // + maturin + py-profiler). A CTestTestfile or any non-Python frontend
    // grades through the CMake/CTest spine (Track I).
    let non_python = report.frontends.iter().any(|f| f != PythonFrontend.id());
    let composite = if report.has_ctest || non_python {
        CompositeAdapter {
            frontends,
            runner: Box::new(CtestRunner),
            bridge: Box::new(CmakeBridge),
            profiler: Box::new(PerfProfiler { tree: repo.to_path_buf() }),
        }
    } else {
        CompositeAdapter {
            frontends,
            runner: Box::new(PytestRunner),
            bridge: Box::new(MaturinBridge),
            profiler: Box::new(PyProfiler {
                repo: repo.to_path_buf(),
                // Interpreter literal owned by the runner, not composite code.
                python: PathBuf::from(PytestRunner::python_program()),
            }),
        }
    };
    Ok((composite, report))
}

/// Build a unit DAG from a call graph: one unit per module, depends_on from edges.
pub fn dag_from_call_graph(graph: &CallGraph) -> UnitDag {
    let mut units: Vec<Unit> = graph
        .modules
        .keys()
        .map(|m| Unit {
            id: m.clone(),
            module: m.clone(),
            depends_on: vec![],
        })
        .collect();
    units.sort_by(|a, b| a.id.cmp(&b.id));
    let mut edges = graph.edges.clone();
    edges.sort();
    for (dependent, dependency) in &edges {
        if let Some(u) = units.iter_mut().find(|u| u.id == *dependent) {
            if !u.depends_on.contains(dependency) {
                u.depends_on.push(dependency.clone());
            }
        }
    }
    for u in units.iter_mut() {
        u.depends_on.sort();
    }
    edges.sort();
    UnitDag { units, edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topo_orders_leaves_first_and_detects_cycle() {
        let dag = UnitDag {
            units: vec![
                Unit { id: "a".into(), module: "a".into(), depends_on: vec!["b".into()] },
                Unit { id: "b".into(), module: "b".into(), depends_on: vec![] },
                Unit { id: "c".into(), module: "c".into(), depends_on: vec!["a".into()] },
            ],
            edges: vec![("a".into(), "b".into()), ("c".into(), "a".into())],
        };
        let order = dag.leaf_first_order().unwrap();
        assert!(dag.verify_order(&order));
        assert_eq!(order, vec!["b", "a", "c"]);
        let cyclic = UnitDag {
            units: vec![
                Unit { id: "x".into(), module: "x".into(), depends_on: vec!["y".into()] },
                Unit { id: "y".into(), module: "y".into(), depends_on: vec!["x".into()] },
            ],
            edges: vec![("x".into(), "y".into()), ("y".into(), "x".into())],
        };
        assert!(cyclic.leaf_first_order().is_err());
        assert!(!cyclic.verify_order(&["x".into(), "y".into()]));
    }

    fn write_tmp_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn python_only_tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write_tmp_file(&dir.path().join("pkg/a.py"), "VALUE = 1\n");
        write_tmp_file(&dir.path().join("pkg/b.py"), "import a\n\nassert a.VALUE == 1\n");
        write_tmp_file(
            &dir.path().join("pyproject.toml"),
            "[project]\nname = \"pkg\"\n",
        );
        write_tmp_file(&dir.path().join("README.md"), "# pkg\n");
        dir
    }

    #[test]
    fn probe_python_only_tree_claims_single_frontend() {
        let dir = python_only_tree();
        let report = probe(dir.path()).unwrap();
        assert_eq!(report.frontends, vec!["python"]);
        assert!(report.unclaimed.is_empty());
        assert_eq!(report.unclaimed_share, 0.0);
        assert!(!report.has_ctest);
    }

    #[test]
    fn probe_unknown_extension_is_reported_unclaimed() {
        let dir = tempfile::tempdir().unwrap();
        write_tmp_file(&dir.path().join("pkg/a.py"), "VALUE = 1\n");
        write_tmp_file(&dir.path().join("data/spec.zzz"), "opaque\n");
        write_tmp_file(&dir.path().join("notes.md"), "# notes\n");
        let report = probe(dir.path()).unwrap();
        assert_eq!(report.frontends, vec!["python"]);
        assert_eq!(report.unclaimed, vec!["data/spec.zzz"]);
        assert_eq!(report.unclaimed_share, 0.5);
    }

    #[test]
    fn python_fragment_unit_ids_use_canonical_lang_rel_form() {
        let dir = python_only_tree();
        let files = vec![dir.path().join("pkg/a.py"), dir.path().join("pkg/b.py")];
        let compiler_ids = BTreeMap::new();
        let cx = FragmentCtx {
            repo: dir.path(),
            files: &files,
            compile_db: None,
            compiler_ids: &compiler_ids,
        };
        let fragment = PythonFrontend.fragment(&cx).unwrap();
        let mut ids: Vec<&str> =
            fragment.units.iter().map(|u| u.id.0.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["python:pkg/a.py", "python:pkg/b.py"]);
        for unit in &fragment.units {
            let (lang, rel) = unit.id.0.split_once(':').expect("UnitId holds lang:rel");
            assert_eq!(lang, PythonFrontend.id());
            assert!(!rel.is_empty());
            assert_eq!(unit.files, vec![rel.to_string()]);
        }
        assert!(fragment.edges.contains(&(
            UnitId("python:pkg/b.py".into()),
            UnitId("python:pkg/a.py".into())
        )));
        for (from, to) in &fragment.edges {
            assert!(ids.contains(&from.0.as_str()), "edge from unknown unit {from}");
            assert!(ids.contains(&to.0.as_str()), "edge to unknown unit {to}");
        }
    }

    #[test]
    fn partition_union_is_deterministic_and_leaf_first() {
        let dir = tempfile::tempdir().unwrap();
        write_tmp_file(
            &dir.path().join("src/m.F90"),
            "module m_mod\n  implicit none\ncontains\n  subroutine m_init() bind(c)\n  end subroutine m_init\nend module m_mod\n",
        );
        write_tmp_file(&dir.path().join("lib/u.c"), "void u_init(void) {}\n");
        let build = dir.path().join("build");
        std::fs::create_dir_all(&build).unwrap();
        // Fragment union first: both frontends contribute a canonical unit.
        let files = vec![dir.path().join("src/m.F90"), dir.path().join("lib/u.c")];
        let frontends: Vec<Box<dyn Frontend>> =
            vec![Box::new(FortranFrontend), Box::new(CxxFrontend)];
        let (units, _, _) =
            collect_fragments(dir.path(), &files, &frontends, None).unwrap();
        let mut unit_ids: Vec<&str> = units.iter().map(|u| u.id.0.as_str()).collect();
        unit_ids.sort();
        assert_eq!(unit_ids, vec!["c:lib/u.c", "fortran:src/m.F90"]);
        // Full partition path twice: same DAG, same diagnostics, valid order.
        let composite = CompositeAdapter {
            frontends: vec![Box::new(FortranFrontend), Box::new(CxxFrontend)],
            runner: Box::new(CtestRunner),
            bridge: Box::new(CmakeBridge),
            profiler: Box::new(PerfProfiler { tree: dir.path().to_path_buf() }),
        };
        let cx = BuildCtx { tree: dir.path(), build_dir: &build, release: false };
        let (dag_a, diag_a) = composite.partition(&cx).unwrap();
        let (dag_b, diag_b) = composite.partition(&cx).unwrap();
        assert_eq!(dag_a.units.len(), 2);
        assert_eq!(format!("{dag_a:?}"), format!("{dag_b:?}"));
        assert_eq!(diag_a, diag_b);
        let order = dag_a.leaf_first_order().unwrap();
        assert!(dag_a.verify_order(&order));
    }

    #[test]
    fn select_composite_keeps_pytest_spine_for_python_tree() {
        let dir = python_only_tree();
        let (composite, report) = select_composite(dir.path()).unwrap();
        assert_eq!(report.frontends, vec!["python"]);
        assert_eq!(composite.languages(), vec!["python"]);
        assert_eq!(composite.runner_id(), "pytest");
        let image = composite.image();
        assert_eq!(image.base, "python:3.11-slim");
        assert!(image.writable.is_empty());
        let build = dir.path().join("build");
        std::fs::create_dir_all(&build).unwrap();
        let cx = BuildCtx { tree: dir.path(), build_dir: &build, release: false };
        let invocation = composite.runner.invocation(&cx);
        assert_eq!(invocation.len(), 1);
        assert_eq!(invocation[0].program, "python3");
        assert!(invocation[0].args.contains(&"pytest".to_string()));
        assert_eq!(invocation[0].cwd, Cwd::Tree);
        let heldout = composite.runner.heldout(Path::new("test/unit"), &cx);
        assert_eq!(heldout.len(), 1);
        assert_eq!(heldout[0].cwd, Cwd::Tree);
    }
}
// --- Track I: Fortran + C/C++ frontends and the CMake/CTest spine (ADR-008 step 8) ---
//
// Pinned-tree basis (transcripts, not live checkouts): the test-macro harness
// files at the pin, `ctest -N` MPI-command diffs, and one substitution spike
// establishing the compiler ABI limit (non-`BIND(C)` module procedures have no
// stable C ABI: `__<mod>_MOD_<proc>` mangling plus array descriptors for
// assumed-shape dummies). No pinned checkout or network was available in this
// environment, so every behavior below is pinned by transcript-shaped fixtures
// in `track_i_tests` instead of a live tree. The tri-language repo is test
// data, never a code path: no repo names appear below.
//
// Pure-Rust parsing throughout this section: no new subprocess literals (the
// Python spine keeps its interpreter literals; nothing here adds any) and
// every [`TestCommand`] below sets `cwd`.

/// Fortran source extensions (case-insensitive). Uppercase (`F90`/`F`) means
/// "preprocess first"; `.src` files are authoritative codegen templates whose
/// generated `.F90` under the build dir is build output, never a unit.
const FORTRAN_EXTS: &[&str] = &["f90", "f95", "f03", "f08", "for", "f", "src"];

/// C extensions map to `c:` units, C++ extensions to `cxx:` units. A bare
/// uppercase `C` follows make convention and means C++.
const CXX_C_EXTS: &[&str] = &["c", "h"];
const CXX_CXX_EXTS: &[&str] = &["cc", "cpp", "cxx", "c++", "hh", "hpp", "hxx"];

/// Fortran frontend: claims `F90`/`F`/`src`, parses `USE` after preprocessing.
///
/// The `USE` scan is a pure-Rust fixed/free-form line scan with the same
/// contract as the transcript's fparser2 flow (preprocess `#` directives away
/// first, then resolve `USE` names case-insensitively): no interpreter is
/// spawned, so no new program literal enters the codebase.
pub struct FortranFrontend;

impl Frontend for FortranFrontend {
    fn id(&self) -> &'static str {
        "fortran"
    }

    fn claims(&self, file: &Path, _compile_db: Option<&Path>) -> bool {
        file.extension()
            .and_then(|x| x.to_str())
            .map(|e| FORTRAN_EXTS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    fn fragment(&self, cx: &FragmentCtx) -> Result<Fragment, AdapterError> {
        fortran_fragment(cx)
    }

    fn language_rules(&self) -> Vec<Rule> {
        vec![
            Rule {
                id: "fortran.case-insensitive".into(),
                text: "Fortran is case-insensitive: unit symbols are lowercased; linkage names live in Symbol, not in UnitId.".into(),
            },
            Rule {
                id: "fortran.bind-c-abi".into(),
                text: "Only BIND(C) procedures have a stable C ABI. Non-BIND(C) module procedures use __MOD_MOD_proc mangling with array descriptors; substitute those units only at file granularity with an ABI shim, and partition() coarsens them.".into(),
            },
            Rule {
                id: "fortran.preprocess-then-parse".into(),
                text: "Uppercase extensions (.F90/.F) and .src templates are preprocessed first (# directives stripped); USE edges are parsed after preprocessing.".into(),
            },
            Rule {
                id: "fortran.src-codegen".into(),
                text: ".src files are authoritative codegen templates; the generated .F90 under the build dir is build output and never a unit (origin rule).".into(),
            },
        ]
    }
}

/// A procedure defined in a Fortran scope.
struct FortranProc {
    name: String,
    bind_c: bool,
    bind_name: Option<String>,
}

/// One `MODULE`/`SUBMODULE`/`PROGRAM` scope in a Fortran file.
struct FortranScope {
    name: String,
    uses: Vec<String>,
    procs: Vec<FortranProc>,
}

/// A parsed Fortran file: scopes plus `USE`s outside any scope.
struct FortranFile {
    scopes: Vec<FortranScope>,
    global_uses: Vec<String>,
    directives: usize,
}

/// Quote-aware `!` comment cut. Doubled quotes (`''`, `""`) are escapes, not
/// toggles. Byte indexes stay on ASCII boundaries (`!` and quotes are ASCII).
fn strip_fortran_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_squote = false;
    let mut in_dquote = false;
    let mut i = 0;
    while i < line.len() {
        let c = bytes[i];
        if c == b'\'' && !in_dquote {
            if in_squote && i + 1 < line.len() && bytes[i + 1] == b'\'' {
                i += 2;
                continue;
            }
            in_squote = !in_squote;
        } else if c == b'"' && !in_squote {
            if in_dquote && i + 1 < line.len() && bytes[i + 1] == b'"' {
                i += 2;
                continue;
            }
            in_dquote = !in_dquote;
        } else if c == b'!' && !in_squote && !in_dquote {
            return line[..i].trim_end();
        }
        i += 1;
    }
    line.trim_end()
}

/// Join free-form `&` continuations. Fixed-form continuation (column 6) is not
/// reconstructed: an approximation, documented here; `USE`/scope statements
/// split that way reappear as diagnostics-free missed edges.
fn join_fortran_continuations(lines: Vec<String>, fixed: bool) -> Vec<String> {
    if fixed {
        return lines;
    }
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    for line in lines {
        let chunk = match pending.take() {
            Some(prefix) => format!("{prefix} {}", line.trim()),
            None => line,
        };
        if chunk.trim_end().ends_with('&') {
            let trimmed = chunk.trim_end();
            pending = Some(trimmed[..trimmed.len() - 1].to_string());
        } else {
            out.push(chunk);
        }
    }
    if let Some(rest) = pending {
        out.push(rest);
    }
    out
}

/// First identifier characters of `s` (already lowercased).
fn clean_ident(s: &str) -> String {
    s.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
}

/// Parse `use [attrs ::] module [, only: ...]`; `None` when no module name.
fn parse_use_module(lower: &str) -> Option<String> {
    // Caller guarantees a leading `use` keyword (ASCII; byte indexing safe).
    let rest = lower["use".len()..].trim_start().trim_start_matches(',').trim_start();
    // `use, intrinsic :: iso_c_binding` and `use :: x` resolve after `::`;
    // `use x, only: y` resolves to the first token before any comma/space.
    let after = match rest.find("::") {
        Some(idx) => &rest[idx + 2..],
        None => rest,
    };
    let first = after.trim_start().split([',', ' ', '\t', '(', ':']).next()?.trim();
    let name = clean_ident(first);
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// `Some(None)` = bare `BIND(C)`; `Some(Some(name))` = named; `None` = no
/// `BIND(C)`. ASCII-only scan (`to_ascii_lowercase` keeps byte positions).
fn parse_bind_c(lower: &str, orig: &str) -> Option<Option<String>> {
    let mut search = lower;
    let mut offset = 0;
    while let Some(bi) = search.find("bind") {
        let after = search[bi + 4..].trim_start();
        // Byte offset of `after` within the whole line (ASCII throughout).
        let skipped = offset + bi + 4 + (search[bi + 4..].len() - after.len());
        if after.starts_with('(') {
            let inner = &after[1..];
            let head: String = inner.chars().take_while(|c| *c != ',' && *c != ')').collect();
            if head.trim() == "c" {
                let rest = &inner[head.len()..];
                if let Some(ni) = rest.find("name") {
                    let after_eq = &rest[ni + 4..];
                    if let Some(eq) = after_eq.find('=') {
                        let quoted = after_eq[eq + 1..].trim_start();
                        if let Some(q) = quoted.chars().next() {
                            if q == '\'' || q == '"' {
                                // Same byte range in the original line (ASCII).
                                let base = skipped + 1 + head.len() + ni + 4 + eq + 1
                                    + (after_eq[eq + 1..].len() - quoted.len());
                                let tail = &orig[base + 1..];
                                let name: String =
                                    tail.chars().take_while(|c| *c != q).collect();
                                if !name.is_empty() {
                                    return Some(Some(name));
                                }
                            }
                        }
                    }
                }
                return Some(None);
            }
        }
        let advance = bi + 4;
        offset += advance;
        search = &search[advance..];
    }
    None
}

/// Parse one Fortran file into scopes. Preprocessor directive lines (`#...`)
/// are counted and stripped first: `USE` resolution happens after
/// preprocessing.
fn parse_fortran_file(text: &str, fixed: bool) -> FortranFile {
    let mut logical = Vec::new();
    let mut directives = 0usize;
    for raw in text.lines() {
        if raw.trim_start().starts_with('#') {
            directives += 1;
            continue;
        }
        if fixed {
            let first = raw.chars().next();
            if matches!(first, Some('c') | Some('C') | Some('*')) {
                continue;
            }
        }
        logical.push(raw.to_string());
    }
    let mut file = FortranFile { scopes: Vec::new(), global_uses: Vec::new(), directives };
    // Open-scope stack (indexes into `scopes`): `end module` closes the top
    // without deleting history, so completed scopes still become units while
    // later lines attach to the right scope (or to file-global).
    let mut open: Vec<usize> = Vec::new();
    for orig in join_fortran_continuations(logical, fixed) {
        let line = strip_fortran_comment(&orig);
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        match words[0] {
            // `module procedure` (interface bodies) and `module
            // function/subroutine` (separate module subprograms) are not scope
            // opens; the latter falls through to the procedure scanner below.
            "module" if words.get(1) != Some(&"procedure")
                && !matches!(words.get(1), Some(&"function") | Some(&"subroutine")) => {
                if let Some(name) = words.get(1).map(|s| clean_ident(s)) {
                    if !name.is_empty() {
                        file.scopes.push(FortranScope {
                            name,
                            uses: Vec::new(),
                            procs: Vec::new(),
                        });
                        open.push(file.scopes.len() - 1);
                    }
                }
                continue;
            }
            "submodule" => {
                if let Some(last) = words.last().map(|s| clean_ident(s)) {
                    if !last.is_empty() {
                        file.scopes.push(FortranScope {
                            name: last,
                            uses: Vec::new(),
                            procs: Vec::new(),
                        });
                        open.push(file.scopes.len() - 1);
                    }
                }
                continue;
            }
            "program" => {
                if let Some(name) = words.get(1).map(|s| clean_ident(s)) {
                    if !name.is_empty() {
                        file.scopes.push(FortranScope {
                            name,
                            uses: Vec::new(),
                            procs: Vec::new(),
                        });
                        open.push(file.scopes.len() - 1);
                    }
                }
                continue;
            }
            "end" => {
                // `end module/program/submodule` (or bare `end`) closes the
                // top scope; every other `end ...` line is ignored here so
                // the procedure scanner below never double-counts
                // `end subroutine <name>` as a definition. A bare `end`
                // closing a contained procedure is an accepted
                // approximation (free-form code ends them explicitly).
                if words.len() == 1
                    || matches!(
                        words.get(1),
                        Some(&"module") | Some(&"program") | Some(&"submodule")
                    )
                {
                    open.pop();
                }
                continue;
            }
            "endmodule" | "endprogram" | "endsubmodule" => {
                open.pop();
                continue;
            }
            "use" => {
                if let Some(target) = parse_use_module(&lower) {
                    match open.last().copied() {
                        Some(i) => file.scopes[i].uses.push(target),
                        None => file.global_uses.push(target),
                    }
                }
                continue;
            }
            _ => {}
        }
        // Procedure definition: a `subroutine`/`function` keyword outside an
        // `end` line and outside `module procedure` interface bodies.
        let mut keyword_at: Option<usize> = None;
        for (i, w) in words.iter().enumerate() {
            if *w == "subroutine" || *w == "function" {
                keyword_at = Some(i);
                break;
            }
        }
        if let Some(i) = keyword_at {
            if let Some(raw_name) = words.get(i + 1) {
                let proc = clean_ident(raw_name);
                if !proc.is_empty() {
                    let (bind_c, bind_name) = match parse_bind_c(&lower, t) {
                        Some(named) => (true, named),
                        None => (false, None),
                    };
                    let proc = FortranProc { name: proc, bind_c, bind_name };
                    match open.last().copied() {
                        Some(i) => file.scopes[i].procs.push(proc),
                        None => {
                            // Procedure outside any scope (external subprogram):
                            // model as its own single-procedure scope.
                            file.scopes.push(FortranScope {
                                name: proc.name.clone(),
                                uses: Vec::new(),
                                procs: vec![proc],
                            });
                            open.push(file.scopes.len() - 1);
                        }
                    }
                }
            }
        }
    }
    file
}

/// Fortran fragment engine: one unit per module/program scope (`#<symbol>`
/// when a file holds several), `USE` imports, `BIND(C)`-aware exports.
fn fortran_fragment(cx: &FragmentCtx) -> Result<Fragment, AdapterError> {
    let mut diagnostics = Vec::new();
    let mut sources: Vec<(PathBuf, String)> = Vec::new();
    for src in cx.files {
        if !FortranFrontend.claims(src, None) {
            continue;
        }
        let rel = src.strip_prefix(cx.repo).unwrap_or(src).to_string_lossy().replace('\\', "/");
        if Path::new(&rel).components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some("test") | Some("tests") | Some("bench") | Some("docs")
            )
        }) {
            continue;
        }
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("test_") {
            continue;
        }
        sources.push((src.clone(), rel));
    }
    sources.sort_by(|a, b| a.1.cmp(&b.1));
    let mut units = Vec::new();
    let mut edges = Vec::new();
    for (src, rel) in &sources {
        let ext_lower = src
            .extension()
            .and_then(|x| x.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        let fixed = matches!(ext_lower.as_str(), "f" | "for");
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        // One unit per authoritative source: a checked-in generated `.F90`
        // beside its `.src` template is skipped (the build-dir copy is
        // excluded by origin and never reaches this filter).
        let src_sib = src.parent().unwrap_or(Path::new("")).join(format!("{stem}.src"));
        if ext_lower != "src" && src_sib.is_file() {
            let sib_rel =
                src_sib.strip_prefix(cx.repo).unwrap_or(&src_sib).to_string_lossy().replace('\\', "/");
            diagnostics.push(format!("generated: '{rel}' skipped (authoritative source is '{sib_rel}')"));
            continue;
        }
        let authoritative = rel.clone();
        let generated_from: Option<String> = None;
        let text = match std::fs::read_to_string(src) {
            Ok(text) => text,
            Err(e) => {
                diagnostics.push(format!("unreadable: '{rel}': {e}"));
                continue;
            }
        };
        let parsed = parse_fortran_file(&text, fixed);
        if parsed.directives > 0 {
            diagnostics.push(format!(
                "preprocessed: '{rel}' ({} preprocessor directives stripped before USE scan)",
                parsed.directives
            ));
        }
        if parsed.scopes.is_empty() {
            diagnostics.push(format!(
                "no-scope: '{rel}' declares no module/program; file-level unit"
            ));
            units.push(UnitDecl {
                id: UnitId(format!("fortran:{authoritative}")),
                files: vec![rel.clone()],
                generated_from,
                exports: Vec::new(),
                imports: parsed
                    .global_uses
                    .into_iter()
                    .map(|linkage| Symbol { linkage, abi: Abi::Fortran { bind_c: false } })
                    .collect(),
            });
            continue;
        }
        let multi = parsed.scopes.len() > 1;
        // Same-file scope index for intra-file USE edges.
        let mut scope_ids: HashMap<&str, UnitId> = HashMap::new();
        for scope in &parsed.scopes {
            let id = if multi {
                UnitId(format!("fortran:{authoritative}#{}", scope.name))
            } else {
                UnitId(format!("fortran:{authoritative}"))
            };
            scope_ids.insert(scope.name.as_str(), id);
        }
        for scope in &parsed.scopes {
            let id = scope_ids[scope.name.as_str()].clone();
            let unit_bind_c = scope.procs.iter().all(|p| p.bind_c);
            let mut exports = vec![Symbol {
                linkage: scope.name.clone(),
                abi: Abi::Fortran { bind_c: unit_bind_c },
            }];
            for proc in &scope.procs {
                if proc.bind_c {
                    exports.push(Symbol {
                        linkage: proc.bind_name.clone().unwrap_or_else(|| proc.name.clone()),
                        abi: Abi::Fortran { bind_c: true },
                    });
                } else {
                    // Representative linkage for the gfortran-mangled
                    // procedure symbol (no stable C ABI; see language_rules).
                    exports.push(Symbol {
                        linkage: format!("__{}_MOD_{}", scope.name, proc.name),
                        abi: Abi::Fortran { bind_c: false },
                    });
                }
            }
            let mut imports: Vec<Symbol> = scope
                .uses
                .iter()
                .map(|linkage| Symbol {
                    linkage: linkage.clone(),
                    abi: Abi::Fortran { bind_c: false },
                })
                .collect();
            imports.sort_by(|a, b| a.linkage.cmp(&b.linkage));
            imports.dedup_by(|a, b| a.linkage == b.linkage);
            for import in &imports {
                if let Some(target) = scope_ids.get(import.linkage.as_str()) {
                    if *target != id {
                        edges.push((id.clone(), target.clone()));
                    }
                }
            }
            units.push(UnitDecl {
                id,
                files: vec![rel.clone()],
                generated_from: generated_from.clone(),
                exports,
                imports,
            });
        }
    }
    units.sort_by(|a, b| a.id.cmp(&b.id));
    edges.sort();
    edges.dedup();
    Ok(Fragment { units, edges, diagnostics })
}

/// C/C++ frontend: claims via the compile DB. Extension gate first; when a
/// build dir provides `compile_commands.json`, only files listed there become
/// units (generated sources under the build dir are build output by origin).
pub struct CxxFrontend;

impl Frontend for CxxFrontend {
    fn id(&self) -> &'static str {
        "cxx"
    }

    fn claims(&self, file: &Path, compile_db: Option<&Path>) -> bool {
        let db = compile_db.and_then(compile_db_index);
        cxx_claimed(file, db.as_ref())
    }

    fn fragment(&self, cx: &FragmentCtx) -> Result<Fragment, AdapterError> {
        cxx_fragment(cx)
    }

    fn language_rules(&self) -> Vec<Rule> {
        vec![
            Rule {
                id: "cxx.compile-db-origin".into(),
                text: "C/C++ units come from the compile DB: only files listed in compile_commands.json become units; generated sources under the build dir are build output, never units.".into(),
            },
            Rule {
                id: "cxx.include-linkage".into(),
                text: "Quoted #includes resolve to units (headers attach as <stem>_h); angle includes are system/third-party link targets and surface via BuildBridge::link_deps, never as units.".into(),
            },
            Rule {
                id: "cxx.header-units".into(),
                text: "Headers are units of the including language (DB language wins, else .c/.h map to c:). A header and its source share a stem but never a linkage.".into(),
            },
        ]
    }
}

/// `Some(true)` = C++, `Some(false)` = C, `None` = not a C/C++ file.
fn cxx_ext_kind(file: &Path) -> Option<bool> {
    let ext = file.extension().and_then(|x| x.to_str())?;
    if ext == "C" {
        return Some(true);
    }
    let lower = ext.to_lowercase();
    if CXX_CXX_EXTS.contains(&lower.as_str()) {
        Some(true)
    } else if CXX_C_EXTS.contains(&lower.as_str()) {
        Some(false)
    } else {
        None
    }
}

/// Compile DB index: candidate file path -> compilation language (`C`,
/// `CXX`, ...). Both the raw `file` entry and the `directory`-joined form
/// are indexed; `None` when the DB is missing or unparseable.
fn compile_db_index(db: &Path) -> Option<HashMap<String, String>> {
    let text = std::fs::read_to_string(db).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let mut map = HashMap::new();
    for entry in v.as_array()? {
        let Some(file) = entry.get("file").and_then(|f| f.as_str()) else {
            continue;
        };
        let lang =
            entry.get("language").and_then(|l| l.as_str()).unwrap_or("").to_string();
        map.insert(file.to_string(), lang.clone());
        if let Some(dir) = entry.get("directory").and_then(|d| d.as_str()) {
            if !file.starts_with('/') {
                map.insert(format!("{}/{}", dir.trim_end_matches('/'), file), lang);
            }
        }
    }
    Some(map)
}

/// Extension gate plus, when a DB index is given, membership in it (exact
/// absolute match, or an absolute path ending in a relative entry).
fn cxx_claimed(file: &Path, db: Option<&HashMap<String, String>>) -> bool {
    if cxx_ext_kind(file).is_none() {
        return false;
    }
    match db {
        None => true,
        Some(index) => {
            let s = file.to_string_lossy().into_owned();
            index.keys().any(|k| {
                if k.starts_with('/') {
                    k == &s
                } else {
                    s == *k || s.ends_with(&format!("/{k}"))
                }
            })
        }
    }
}

/// Quoted `#include "..."` targets in source order. Angle includes are
/// system/third-party link targets, never units.
fn cxx_quoted_includes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if !t.starts_with('#') {
            continue;
        }
        let after = t[1..].trim_start();
        if !after.starts_with("include") {
            continue;
        }
        let after = after["include".len()..].trim_start();
        if !after.starts_with('"') {
            continue;
        }
        if let Some(target) = after[1..].split('"').next() {
            if !target.is_empty() {
                out.push(target.to_string());
            }
        }
    }
    out
}

fn cxx_is_header(path: &Path) -> bool {
    match path.extension().and_then(|x| x.to_str()) {
        Some("H") => true,
        Some(ext) => {
            let lower = ext.to_lowercase();
            matches!(lower.as_str(), "h" | "hh" | "hpp" | "hxx")
        }
        None => false,
    }
}

/// C/C++ fragment engine: one unit per DB-listed file, `#include` edges.
/// Headers export `<stem>_h` so a header never collides with its source.
fn cxx_fragment(cx: &FragmentCtx) -> Result<Fragment, AdapterError> {
    let mut diagnostics = Vec::new();
    let db_index: Option<HashMap<String, String>> = match cx.compile_db {
        None => None,
        Some(db) => match compile_db_index(db) {
            Some(index) => Some(index),
            None => {
                diagnostics.push(format!(
                    "compile-db: '{}' unreadable; claiming C/C++ by extension",
                    db.display()
                ));
                None
            }
        },
    };
    let mut sources: Vec<(PathBuf, String)> = Vec::new();
    for src in cx.files {
        if !cxx_claimed(src, db_index.as_ref()) {
            continue;
        }
        let rel = src.strip_prefix(cx.repo).unwrap_or(src).to_string_lossy().replace('\\', "/");
        if Path::new(&rel).components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some("test") | Some("tests") | Some("bench") | Some("docs")
            )
        }) {
            continue;
        }
        sources.push((src.clone(), rel));
    }
    sources.sort_by(|a, b| a.1.cmp(&b.1));
    // Resolve quoted includes: same-dir file first, then any claimed file
    // with the same basename (documented heuristic for remapped trees).
    let mut units = Vec::new();
    let mut rel_to_id: HashMap<String, UnitId> = HashMap::new();
    let mut pending_imports: Vec<(UnitId, Vec<String>)> = Vec::new();
    for (src, rel) in &sources {
        let abs_key = src.to_string_lossy().into_owned();
        let lang = match db_index.as_ref().and_then(|db| db.get(&abs_key)) {
            Some(l) if l.to_ascii_uppercase().starts_with("CXX") => "cxx",
            Some(l) if l.to_ascii_uppercase() == "C" => "c",
            _ => {
                if cxx_ext_kind(src) == Some(true) {
                    "cxx"
                } else {
                    "c"
                }
            }
        };
        let id = UnitId(format!("{lang}:{rel}"));
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let header = cxx_is_header(src);
        let export_linkage = if header { format!("{stem}_h") } else { stem };
        let abi = if lang == "cxx" { Abi::Cxx } else { Abi::C };
        let text = match std::fs::read_to_string(src) {
            Ok(text) => text,
            Err(e) => {
                diagnostics.push(format!("unreadable: '{rel}': {e}"));
                continue;
            }
        };
        let mut resolved = Vec::new();
        for inc in cxx_quoted_includes(&text) {
            let same_dir = src.parent().unwrap_or(Path::new("")).join(&inc);
            let hit = if same_dir.is_file() {
                Some(
                    same_dir
                        .strip_prefix(cx.repo)
                        .unwrap_or(&same_dir)
                        .to_string_lossy()
                        .replace('\\', "/"),
                )
            } else {
                sources
                    .iter()
                    .find(|(p, _)| {
                        p.file_name().and_then(|n| n.to_str()) == Some(inc.as_str())
                    })
                    .map(|(_, r)| r.clone())
            };
            match hit {
                Some(target_rel) => {
                    let target_is_header = target_rel
                        .rsplit('.')
                        .next()
                        .map(|e| {
                            e.eq_ignore_ascii_case("h")
                                || e.eq_ignore_ascii_case("hh")
                                || e.eq_ignore_ascii_case("hpp")
                                || e.eq_ignore_ascii_case("hxx")
                        })
                        .unwrap_or(false);
                    let base = target_rel
                        .rsplit('/')
                        .next()
                        .unwrap_or(&target_rel)
                        .rsplit('.')
                        .nth(1)
                        .unwrap_or(&target_rel);
                    resolved.push(if target_is_header {
                        format!("{base}_h")
                    } else {
                        base.to_string()
                    });
                }
                None => diagnostics
                    .push(format!("unresolved-include: '{rel}' includes '{inc}' (no such claimed file)")),
            }
        }
        resolved.sort();
        resolved.dedup();
        rel_to_id.insert(rel.clone(), id.clone());
        pending_imports.push((id.clone(), resolved));
        units.push(UnitDecl {
            id,
            files: vec![rel.clone()],
            generated_from: None,
            exports: vec![Symbol { linkage: export_linkage, abi }],
            imports: Vec::new(),
        });
    }
    // Fill imports and intra-fragment edges now that every id is known. Owned
    // ids: the map must not borrow `units` while it is filled mutably below.
    let mut linkage_to_unit: HashMap<String, UnitId> = HashMap::new();
    for unit in &units {
        for export in &unit.exports {
            linkage_to_unit.entry(export.linkage.clone()).or_insert_with(|| unit.id.clone());
        }
    }
    let mut edges: HashSet<(UnitId, UnitId)> = HashSet::new();
    for unit in &mut units {
        let Some((_, linkages)) = pending_imports.iter().find(|(id, _)| *id == unit.id) else {
            continue;
        };
        let abi = unit.exports.first().map(|e| e.abi).unwrap_or(Abi::C);
        unit.imports =
            linkages.iter().map(|linkage| Symbol { linkage: linkage.clone(), abi }).collect();
        for import in &unit.imports {
            if let Some(target) = linkage_to_unit.get(import.linkage.as_str()) {
                if *target != unit.id {
                    edges.insert((unit.id.clone(), target.clone()));
                }
            }
        }
    }
    let mut edge_list: Vec<(UnitId, UnitId)> = edges.into_iter().collect();
    edge_list.sort();
    units.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Fragment { units, edges: edge_list, diagnostics })
}

// --- Track I: runtime-loaded solvers via LD_DEBUG + the CMake File API ---

/// One source file inside a CMake File API target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileApiSource {
    /// Unit language prefix (`c`, `cxx`, `fortran`).
    pub language: String,
    /// Repo-relative source path.
    pub path: String,
}

/// One CMake File API target: link name plus its repo-rel sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileApiTarget {
    pub name: String,
    pub sources: Vec<FileApiSource>,
}

/// Parse a CMake File API reply dir (`.cmake/api/v1/reply/`): newest
/// `index-*.json`, then each `target-*.json`. Sources outside `repo` are
/// skipped (system/vendored objects are link targets, not units).
pub fn parse_cmake_file_api_reply(
    reply_dir: &Path,
    repo: &Path,
) -> Result<Vec<FileApiTarget>, AdapterError> {
    let mut indices = Vec::new();
    let entries = std::fs::read_dir(reply_dir)?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("index-") && name.ends_with(".json") {
            indices.push(entry.path());
        }
    }
    indices.sort();
    let Some(index_path) = indices.pop() else {
        return Err(AdapterError::Parse(format!(
            "file-api: no index-*.json in '{}'",
            reply_dir.display()
        )));
    };
    let index_text = std::fs::read_to_string(&index_path)?;
    let index: serde_json::Value =
        serde_json::from_str(&index_text).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut targets = Vec::new();
    let objects = index.get("objects").and_then(|o| o.as_array()).cloned().unwrap_or_default();
    for object in &objects {
        if object.get("kind").and_then(|k| k.as_str()) != Some("target") {
            continue;
        }
        let Some(json_file) = object.get("jsonFile").and_then(|j| j.as_str()) else {
            continue;
        };
        let target_text = std::fs::read_to_string(reply_dir.join(json_file))?;
        let target: serde_json::Value =
            serde_json::from_str(&target_text).map_err(|e| AdapterError::Parse(e.to_string()))?;
        let Some(name) = target.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let empty = Vec::new();
        let sources = target.get("sources").and_then(|s| s.as_array()).unwrap_or(&empty);
        let groups = target.get("compileGroups").and_then(|g| g.as_array()).unwrap_or(&empty);
        let mut lang_of: Vec<Option<String>> = vec![None; sources.len()];
        for group in groups {
            let lang = group.get("language").and_then(|l| l.as_str()).unwrap_or("");
            let prefix = if lang.eq_ignore_ascii_case("cxx") {
                Some("cxx".to_string())
            } else if lang.eq_ignore_ascii_case("c") {
                Some("c".to_string())
            } else if lang.eq_ignore_ascii_case("fortran") {
                Some("fortran".to_string())
            } else {
                None
            };
            let indexes = group
                .get("sourceIndexes")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default();
            for idx in indexes {
                if let Some(i) = idx.as_u64().and_then(|n| usize::try_from(n).ok()) {
                    if let Some(slot) = lang_of.get_mut(i) {
                        *slot = prefix.clone();
                    }
                }
            }
        }
        let mut target_sources = Vec::new();
        for (source, lang) in sources.iter().zip(lang_of) {
            let Some(lang) = lang else { continue };
            let Some(path) = source.get("path").and_then(|p| p.as_str()) else {
                continue;
            };
            let rel = match Path::new(path).strip_prefix(repo) {
                Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            target_sources.push(FileApiSource { language: lang, path: rel });
        }
        target_sources.sort_by(|a, b| a.path.cmp(&b.path));
        target_sources.dedup_by(|a, b| a.path == b.path);
        targets.push(FileApiTarget { name: name.to_string(), sources: target_sources });
    }
    targets.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(targets)
}

/// Parse `LD_DEBUG=files` output into loaded `.so` paths (sorted, deduped).
/// The baseline run sets `LD_DEBUG=files` on its test command; this maps the
/// `file=` lines back to paths. Non-`.so` mappings are ignored.
pub fn parse_ld_debug_files(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(fi) = line.find("file=") else { continue };
        let rest = &line[fi + "file=".len()..];
        let end = rest
            .find(|c| c == ' ' || c == '\t' || c == ';' || c == ']')
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim().trim_matches('"');
        if candidate.contains(".so") {
            out.push(candidate.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Map loaded `.so` paths to units through File API targets: `lib<Name>.so*`
/// (version suffixes stripped) matches target `Name`, whose sources become
/// unit ids. Unmapped objects (system runtimes) are dropped.
pub fn map_loaded_objects_to_units(
    so_paths: &[String],
    targets: &[FileApiTarget],
) -> Vec<UnitId> {
    let mut out = Vec::new();
    for so in so_paths {
        let base = so.rsplit('/').next().unwrap_or(so);
        let Some(so_pos) = base.find(".so") else { continue };
        let mut stem = &base[..so_pos];
        stem = stem.strip_prefix("lib").unwrap_or(stem);
        let stem_lower = stem.to_lowercase();
        for target in targets {
            if target.name == stem || target.name.to_lowercase() == stem_lower {
                for source in &target.sources {
                    out.push(UnitId(format!("{}:{}", source.language, source.path)));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

// --- Track I: CTest runner ---

/// CTest runner: owns the `ctest` invocation (the `-L quick` subset from the
/// pinned transcript), the text verdict parser, the CMake oracle file set,
/// and the MPI-rank/config-feature hashing. SUT-written verdict files are a
/// cross-check only: grading reads ctest's own verdicts, while [`TestRunner::observe`]
/// extracts frozen-tolerance observables from the output.
pub struct CtestRunner;

impl CtestRunner {
    pub const RUNNER_ID: &str = "ctest";

    /// The one `ctest` literal the CTest spine owns; absolute resolution
    /// happens in the executor at spawn time.
    pub fn ctest_program() -> String {
        "ctest".to_string()
    }

    fn command(
        program: String,
        args: Vec<String>,
        timeout_secs: Option<u32>,
        collect: Vec<String>,
    ) -> TestCommand {
        TestCommand {
            program,
            args,
            cwd: Cwd::BuildDir,
            env_set: vec![("CTEST_OUTPUT_ON_FAILURE".to_string(), "1".to_string())],
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs,
            collect,
        }
    }

    /// Parse `ctest -N [-V]` output into `(test name, launcher)` pairs. Tests
    /// whose `Test command:` line wraps an MPI launcher (`mpiexec`/`mpirun`
    /// `-n <ranks>`) record the ranks as [`Launcher`] data so `config_hash`
    /// pins them; other tests get `None`.
    pub fn test_launchers(ctest_n_output: &str) -> Vec<(String, Option<Launcher>)> {
        let mut out = Vec::new();
        let mut current: Option<String> = None;
        let mut launcher: Option<Launcher> = None;
        for line in ctest_n_output.lines() {
            let t = line.trim();
            if let Some(idx) = t.find("Test #") {
                if let Some((_, right)) = t[idx..].split_once(':') {
                    if let Some(name) = current.take() {
                        out.push((name, launcher.take()));
                    }
                    let name = right
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_matches('.')
                        .to_string();
                    current = if name.is_empty() { None } else { Some(name) };
                    continue;
                }
            }
            if let Some(rest) = t.strip_prefix("Test command:") {
                launcher = parse_test_command(rest);
            }
        }
        if let Some(name) = current.take() {
            out.push((name, launcher.take()));
        }
        out
    }

    /// Parse `add_test(...)` entries in `CTestTestfile*.cmake` text into
    /// `(test name, MPI ranks)` pairs. Feeds `config_hash` without spawning:
    /// the test set depends on configure (MPI, optional libraries), so the
    /// recorded set plus the found-feature set must hash, with a mismatch
    /// halting downstream rather than failing.
    pub fn add_tests_in_text(text: &str) -> Vec<(String, Option<u16>)> {
        parse_add_tests(text)
    }

    /// Found-feature set from `CMakeCache.txt` (`WITH_*`, `ENABLE_*`,
    /// `USE_*`, `*_FOUND`, build type). Sorted, deduped, hashed into
    /// `config_hash`.
    pub fn cache_features_in_dir(build_dir: &Path) -> Vec<String> {
        cmake_cache_features(build_dir)
    }
}

/// Parse an MPI-wrapping test command into [`Launcher`] data. `None` for
/// direct (non-MPI) commands.
fn parse_test_command(cmd: &str) -> Option<Launcher> {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        let base = token.rsplit('/').next().unwrap_or(token);
        if !(base.eq_ignore_ascii_case("mpiexec") || base == "mpirun" || base == "srun") {
            continue;
        }
        let mut j = i + 1;
        while j < tokens.len() {
            if tokens[j] == "-n" || tokens[j] == "-np" {
                if let Some(next) = tokens.get(j + 1) {
                    if let Ok(np) = next.parse::<u16>() {
                        if np > 0 {
                            return Some(Launcher {
                                program: (*token).to_string(),
                                np_flag: tokens[j].to_string(),
                                np,
                                extra: Vec::new(),
                            });
                        }
                    }
                }
            }
            j += 1;
        }
    }
    None
}

/// Scan `add_test(name ...)` entries with balanced parens; quoted tokens in
/// order, MPI ranks from an `mpiexec -n K` command tail.
fn parse_add_tests(text: &str) -> Vec<(String, Option<u16>)> {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(pos) = lower[i..].find("add_test") {
        let start = i + pos;
        // Word boundary before the macro name.
        if start > 0 {
            let prev = bytes[start - 1] as char;
            if prev.is_ascii_alphanumeric() || prev == '_' {
                i = start + 8;
                continue;
            }
        }
        let Some(open_rel) = text[start..].find('(') else { break };
        let mut depth = 0;
        let mut close = None;
        for (k, c) in text[start + open_rel..].char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    close = Some(start + open_rel + k);
                    break;
                }
            }
        }
        let Some(end) = close else { break };
        let inner = &text[start + open_rel + 1..end];
        // Tokens are quoted strings or bare words (`add_test(name "cmd")`
        // leaves the name unquoted in real CTestTestfiles).
        let mut tokens = Vec::new();
        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '"' || c == '\'' {
                let mut token = String::new();
                for d in chars.by_ref() {
                    if d == c {
                        break;
                    }
                    token.push(d);
                }
                tokens.push(token);
            } else if !c.is_whitespace() {
                let mut token = String::from(c);
                while let Some(&d) = chars.peek() {
                    if d.is_whitespace() || d == '(' || d == ')' {
                        break;
                    }
                    token.push(d);
                    chars.next();
                }
                tokens.push(token);
            }
        }
        if let Some(name) = tokens.first() {
            let mut ranks: Option<u16> = None;
            for (k, token) in tokens.iter().enumerate() {
                let base = token.rsplit('/').next().unwrap_or(token);
                if base.eq_ignore_ascii_case("mpiexec") || base == "mpirun" || base == "srun" {
                    for window in tokens[k + 1..].windows(2) {
                        if window[0] == "-n" || window[0] == "-np" {
                            if let Ok(np) = window[1].parse::<u16>() {
                                if np > 0 {
                                    ranks = Some(np);
                                }
                                break;
                            }
                        }
                    }
                }
            }
            out.push((name.clone(), ranks));
        }
        i = end + 1;
    }
    out
}

/// All `add_test` entries in every `CTestTestfile*.cmake` under `build_dir`.
fn ctest_add_tests_in_dir(build_dir: &Path) -> Vec<(String, Option<u16>)> {
    let mut out = Vec::new();
    if !build_dir.is_dir() {
        return out;
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(build_dir).into_iter().filter_entry(|e| {
        !e.path().components().any(|c| c.as_os_str() == ".git")
    }) {
        let Ok(entry) = entry else { continue };
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("CTestTestfile") && p.extension().map(|x| x == "cmake").unwrap_or(false)
        {
            files.push(p.to_path_buf());
        }
    }
    files.sort();
    for file in files {
        if let Ok(text) = std::fs::read_to_string(&file) {
            out.extend(parse_add_tests(&text));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

fn cmake_cache_features(build_dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(build_dir.join("CMakeCache.txt")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let name = key.split(':').next().unwrap_or(key);
        if name.starts_with("WITH_")
            || name.starts_with("ENABLE_")
            || name.starts_with("USE_")
            || name.ends_with("_FOUND")
            || name == "CMAKE_BUILD_TYPE"
            || name == "BUILD_SHARED_LIBS"
        {
            out.push(format!("{name}={value}"));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Grade one ctest output text into `outcomes`: per-test `Test #N:` verdict
/// lines (the LAST verb on the line wins, so names containing verb words are
/// safe), then the `The following tests FAILED:` section, which wins ties.
fn ctest_grade_text(text: &str, outcomes: &mut BTreeMap<String, Outcome>) {
    let mut in_failed_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("The following tests FAILED") {
            in_failed_section = true;
            continue;
        }
        if in_failed_section {
            if let Some((_, rest)) = t.split_once(" - ") {
                if let Some((name, verdict)) = rest.rsplit_once(" (") {
                    let verdict = verdict.trim_end_matches(')');
                    let outcome = match verdict {
                        "Passed" => Some(Outcome::Pass),
                        "Failed" => Some(Outcome::Fail),
                        "Timeout" => Some(Outcome::Timeout),
                        "Not Run" => Some(Outcome::NotRun),
                        _ => None,
                    };
                    if let Some(outcome) = outcome {
                        outcomes.insert(name.trim().to_string(), outcome);
                    }
                }
            }
            continue;
        }
        let Some(idx) = t.find("Test #") else { continue };
        let Some((_, right)) = t[idx..].split_once(':') else { continue };
        let right = right.trim();
        let name = right.split_whitespace().next().unwrap_or("").trim_matches('.');
        if name.is_empty() {
            continue;
        }
        let verbs =
            [("Passed", Outcome::Pass), ("Failed", Outcome::Fail), ("Timeout", Outcome::Timeout), ("Not Run", Outcome::NotRun), ("Disabled", Outcome::Skip)];
        let mut best: Option<(usize, Outcome)> = None;
        for (verb, outcome) in verbs {
            if let Some(pos) = right.rfind(verb) {
                if best.map_or(true, |(p, _)| pos >= p) {
                    best = Some((pos, outcome));
                }
            }
        }
        if let Some((_, outcome)) = best {
            outcomes.insert(name.to_string(), outcome);
        }
    }
}

/// ctest summary line (`75% tests passed, 1 tests failed out of 4`) counts.
fn ctest_summary_counts(text: &str) -> (u32, u32, u32, u32) {
    let mut found = (0u32, 0u32, 0u32, 0u32);
    for line in text.lines() {
        let t = line.trim();
        if !(t.contains("tests passed")
            || t.contains("tests failed")
            || t.contains("test passed")
            || t.contains("test failed"))
        {
            continue;
        }
        // Adjacent `<n> [tests] <verdict>` pairs only: `100%` is a percent,
        // never a count, and the `out of <total>` tail carries no verdict.
        for chunk in t.split(',') {
            let toks: Vec<&str> = chunk.split_whitespace().collect();
            let mut k = 0;
            while k < toks.len() {
                let Ok(n) = toks[k].parse::<u32>() else {
                    k += 1;
                    continue;
                };
                let mut next = k + 1;
                if next < toks.len() && (toks[next] == "test" || toks[next] == "tests") {
                    next += 1;
                }
                if next < toks.len() {
                    let word = toks[next]
                        .trim_matches(|c| c == '.' || c == ',')
                        .to_ascii_lowercase();
                    let word = if word == "not"
                        && toks.get(next + 1).map(|s| s.trim_matches(|c: char| c == '.' || c == ',').to_ascii_lowercase()).as_deref()
                            == Some("run")
                    {
                        "notrun".to_string()
                    } else {
                        word
                    };
                    match word.as_str() {
                        "passed" => found.0 = found.0.max(n),
                        "failed" => found.1 = found.1.max(n),
                        "timeout" => found.2 = found.2.max(n),
                        "notrun" | "disabled" => found.3 = found.3.max(n),
                        _ => {}
                    }
                }
                k += 1;
            }
        }
    }
    found
}

/// Letter/space runs (`len >= 3`) of a spec regex: the literal anchors a
/// candidate output line must contain. Digits and regex operators never form
/// anchors, so character classes like `[0-9.eE+-]` cannot poison matching.
fn literal_anchors(regex: &str) -> Vec<String> {
    let mut anchors = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, anchors: &mut Vec<String>| {
        let anchor = cur.trim().to_string();
        // Length 3+: two-letter runs fall out of character classes and
        // `\s` escapes (`eE`, `s`) and must never gate matching.
        if anchor.len() >= 3 {
            anchors.push(anchor);
        }
        cur.clear();
    };
    for c in regex.chars() {
        if c.is_ascii_alphabetic() || c == '_' || c == ' ' {
            cur.push(c);
        } else {
            flush(&mut cur, &mut anchors);
        }
    }
    flush(&mut cur, &mut anchors);
    anchors
}

/// All float literals on a line in order (`(value, raw token)`).
fn scan_floats(line: &str) -> Vec<(f64, String)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let next_is_digit =
            i + 1 < bytes.len() && (bytes[i + 1] as char).is_ascii_digit();
        let starts = c.is_ascii_digit() || ((c == '+' || c == '-' || c == '.') && next_is_digit);
        if !starts {
            i += 1;
            continue;
        }
        // A sign/dot glued to a word (`test-1`) is not a number start.
        if (c == '+' || c == '-' || c == '.')
            && i > 0
            && (bytes[i - 1] as char).is_ascii_alphanumeric()
        {
            i += 1;
            continue;
        }
        let mut j = i;
        if bytes[j] as char == '+' || bytes[j] as char == '-' {
            j += 1;
        }
        let mut any = false;
        while j < bytes.len() && (bytes[j] as char).is_ascii_digit() {
            j += 1;
            any = true;
        }
        if j < bytes.len() && bytes[j] as char == '.' {
            j += 1;
            while j < bytes.len() && (bytes[j] as char).is_ascii_digit() {
                j += 1;
                any = true;
            }
        }
        if j < bytes.len() && ((bytes[j] as char) == 'e' || (bytes[j] as char) == 'E') {
            let mut k = j + 1;
            if k < bytes.len() && ((bytes[k] as char) == '+' || (bytes[k] as char) == '-') {
                k += 1;
            }
            let mut exp_digits = 0;
            while k < bytes.len() && (bytes[k] as char).is_ascii_digit() {
                k += 1;
                exp_digits += 1;
            }
            if exp_digits > 0 {
                j = k;
            }
        }
        if any {
            if let Ok(value) = line[i..j].parse::<f64>() {
                out.push((value, line[i..j].to_string()));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Artifact glob match (`*` only): exact match without a star, otherwise
/// ordered prefix/infix/suffix matching.
fn artifact_glob_match(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == name;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = name;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if let Some(stripped) = rest.strip_prefix(part) {
                rest = stripped;
            } else {
                return false;
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else if let Some(pos) = rest.find(part) {
            rest = &rest[pos + part.len()..];
        } else {
            return false;
        }
    }
    true
}

/// Source text for one observable spec: `stdout`/`stderr`, `*` (everything),
/// or an artifact glob over collected files.
fn ctest_source_text(run: &RunOutput, source: &str) -> String {
    if source == "stdout" {
        return run.stdout.clone();
    }
    if source == "stderr" {
        return run.stderr.clone();
    }
    if source == "*" {
        let mut text = run.stdout.clone();
        text.push('\n');
        for (_, bytes) in &run.artifacts {
            text.push_str(&String::from_utf8_lossy(bytes));
            text.push('\n');
        }
        return text;
    }
    let mut hits = Vec::new();
    for (key, bytes) in &run.artifacts {
        if artifact_glob_match(source, key) {
            hits.push(String::from_utf8_lossy(bytes).into_owned());
        }
    }
    hits.join("\n")
}

impl TestRunner for CtestRunner {
    fn id(&self) -> &'static str {
        Self::RUNNER_ID
    }

    fn invocation(&self, _cx: &BuildCtx) -> Vec<TestCommand> {
        // The pinned transcript grades the `quick` label subset; full-suite
        // runs are heldout-shaped (see `heldout`). ctest runs in the build
        // dir and collects the machine-readable log plus any JUnit output.
        vec![Self::command(
            Self::ctest_program(),
            vec!["--output-on-failure".to_string(), "-L".to_string(), "quick".to_string()],
            Some(1800),
            vec![
                "Testing/Temporary/LastTest.log".to_string(),
                "Testing/**/*.xml".to_string(),
            ],
        )]
    }

    fn grade(&self, runs: &[RunOutput]) -> Result<GradedResult, AdapterError> {
        let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();
        let mut merged = String::new();
        let mut exit_code = 0i32;
        for run in runs {
            let text = format!("{}\n{}\n", run.stdout, run.stderr);
            merged.push_str(&text);
            ctest_grade_text(&text, &mut outcomes);
            // Summary fallback/top-up, mirroring the pytest runner: count-only
            // output synthesizes stable ids so `from_outcomes` derives counts.
            let (s_passed, s_failed, s_timeout, s_notrun) = ctest_summary_counts(&text);
            let count =
                |m: &BTreeMap<String, Outcome>, o: Outcome| m.values().filter(|v| **v == o).count() as u32;
            for i in count(&outcomes, Outcome::Pass)..s_passed {
                outcomes.insert(format!("summary-ctest-passed-{i}"), Outcome::Pass);
            }
            for i in count(&outcomes, Outcome::Fail)..s_failed {
                outcomes.insert(format!("summary-ctest-failed-{i}"), Outcome::Fail);
            }
            for i in count(&outcomes, Outcome::Timeout)..s_timeout {
                outcomes.insert(format!("summary-ctest-timeout-{i}"), Outcome::Timeout);
            }
            for i in count(&outcomes, Outcome::NotRun)..s_notrun {
                outcomes.insert(format!("summary-ctest-notrun-{i}"), Outcome::NotRun);
            }
            if run.exit_code != 0 && exit_code == 0 {
                exit_code = run.exit_code;
            }
        }
        Ok(GradedResult::from_outcomes(exit_code, outcomes, merged))
    }

    fn observe(&self, runs: &[RunOutput], specs: &[ObservableSpec]) -> Vec<Observation> {
        // Pure-Rust extraction (no interpreter spawn): literal anchors from
        // each spec regex select candidate lines, the last float on the line
        // is the value. Full-regex features (groups, classes) are honored
        // only through their literal anchors; the oracle compares against
        // frozen tolerances downstream.
        let mut out = Vec::new();
        for spec in specs {
            let anchors = literal_anchors(&spec.regex);
            if anchors.is_empty() {
                continue;
            }
            for run in runs {
                let text = ctest_source_text(run, &spec.source);
                for line in text.lines() {
                    if anchors.iter().all(|a| line.contains(a)) {
                        // Numeric validation only: the value rides in the key
                        // (`<source>:<raw>`, the frozen pytest shape); the
                        // oracle compares it against frozen tolerances.
                        if let Some((_, raw)) = scan_floats(line).pop() {
                            out.push(Observation {
                                test_id: spec.test_glob.clone(),
                                key: format!("{}:{raw}", spec.source),
                            });
                        }
                    }
                }
            }
        }
        out
    }

    fn oracle_files(&self, repo: &Path) -> Result<Vec<OracleFile>, AdapterError> {
        // CMake/CTest oracle set: configure files (Spec), generated-tree
        // CTestTestfiles excluded by origin (build dirs never walked);
        // test-dir sources compiled against SUT units are Harness, other
        // test-dir data is Fixture. SUT sources outside test dirs are never
        // oracle.
        let mut non_test_symbols: HashSet<String> = HashSet::new();
        let mut candidates: Vec<(PathBuf, String)> = Vec::new();
        for entry in walkdir::WalkDir::new(repo).into_iter().filter_entry(|e| {
            let p = e.path();
            for seg in [".git", "__pycache__", "target", "build", "_build", ".venv", "venv", "Testing"] {
                if p.components().any(|c| c.as_os_str() == seg) {
                    return false;
                }
            }
            true
        }) {
            let entry = entry.map_err(|e| AdapterError::Parse(e.to_string()))?;
            if entry.path().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(repo)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");
                candidates.push((entry.path().to_path_buf(), rel));
            }
        }
        candidates.sort_by(|a, b| a.1.cmp(&b.1));
        // First pass: linkable symbols provided by non-test sources.
        for (abs, rel) in &candidates {
            if is_ctest_test_dir(rel) {
                continue;
            }
            let is_fortran = FortranFrontend.claims(abs, None);
            let is_cxx = CxxFrontend.claims(abs, None);
            if !is_fortran && !is_cxx {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(abs) else { continue };
            if is_fortran {
                let ext = abs
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                let parsed = parse_fortran_file(&text, matches!(ext.as_str(), "f" | "for"));
                for scope in parsed.scopes {
                    non_test_symbols.insert(scope.name);
                }
            } else {
                if let Some(stem) = abs.file_stem().and_then(|s| s.to_str()) {
                    non_test_symbols.insert(stem.to_ascii_lowercase());
                    non_test_symbols.insert(format!("{}_h", stem.to_ascii_lowercase()));
                }
            }
        }
        let mut out = Vec::new();
        for (abs, rel) in &candidates {
            let name = abs.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if rel.starts_with(".github/workflows/") {
                out.push(OracleFile { path: rel.clone(), kind: OracleKind::Spec });
                continue;
            }
            if name == "CMakeLists.txt"
                || (name.starts_with("CTestTestfile")
                    && abs.extension().map(|x| x == "cmake").unwrap_or(false))
                || abs.extension().and_then(|x| x.to_str()).map(|e| e.eq_ignore_ascii_case("cmake")).unwrap_or(false)
            {
                out.push(OracleFile { path: rel.clone(), kind: OracleKind::Spec });
                continue;
            }
            if !is_ctest_test_dir(rel) {
                continue;
            }
            let is_source = FortranFrontend.claims(abs, None) || CxxFrontend.claims(abs, None);
            if !is_source {
                out.push(OracleFile { path: rel.clone(), kind: OracleKind::Fixture });
                continue;
            }
            // Test-local source: Harness when it compiles against SUT units
            // (`USE` of a provided module / include of a provided header),
            // else Fixture.
            let Ok(text) = std::fs::read_to_string(abs) else { continue };
            let mut harness = false;
            if FortranFrontend.claims(abs, None) {
                let ext = abs
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                let parsed = parse_fortran_file(&text, matches!(ext.as_str(), "f" | "for"));
                let uses = parsed
                    .scopes
                    .iter()
                    .flat_map(|s| s.uses.iter())
                    .chain(parsed.global_uses.iter());
                for target in uses {
                    if non_test_symbols.contains(target) {
                        harness = true;
                        break;
                    }
                }
            } else {
                for inc in cxx_quoted_includes(&text) {
                    let base = inc
                        .rsplit('/')
                        .next()
                        .unwrap_or(&inc)
                        .rsplit('.')
                        .nth(1)
                        .unwrap_or(&inc)
                        .to_ascii_lowercase();
                    if non_test_symbols.contains(&base)
                        || non_test_symbols.contains(&format!("{base}_h"))
                    {
                        harness = true;
                        break;
                    }
                }
            }
            out.push(OracleFile {
                path: rel.clone(),
                kind: if harness { OracleKind::Harness } else { OracleKind::Fixture },
            });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out.dedup_by(|a, b| a.path == b.path);
        Ok(out)
    }

    fn normalize_for_hash(&self, rel: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        // Generated `CTestTestfile`s embed absolute build paths; only
        // source-tree cmake files hash, and only whitespace-normalized
        // (line endings, trailing space) so configure noise never perturbs
        // the frozen hash.
        let is_cmake = rel == "CMakeLists.txt"
            || rel.starts_with("CTestTestfile")
            || rel.ends_with(".cmake");
        if !is_cmake {
            return None;
        }
        let text = String::from_utf8_lossy(bytes);
        let normalized =
            text.lines().map(|line| line.trim_end()).collect::<Vec<_>>().join("\n");
        Some(normalized.into_bytes())
    }

    fn heldout(&self, suite: &Path, _cx: &BuildCtx) -> Vec<TestCommand> {
        // Held-outs are `ctest -R` selections run in the build dir. An empty
        // held-out set must halt at the oracle (never pass silently); the
        // runner only describes the command.
        let pattern = suite
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| suite.to_string_lossy().into_owned());
        vec![Self::command(
            Self::ctest_program(),
            vec!["--output-on-failure".to_string(), "-R".to_string(), pattern],
            Some(1800),
            vec!["Testing/Temporary/LastTest.log".to_string()],
        )]
    }

    fn config_hash(&self, cx: &BuildCtx) -> Result<String, AdapterError> {
        // Deterministic FNV-1a64 over the normalized oracle set, the frozen
        // invocation, the configured test set with MPI ranks, and the
        // found-feature set. MPI ranks and configure features ride here so a
        // host/grading-image mismatch halts instead of grading the wrong set.
        let mut files = self.oracle_files(cx.tree)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut h: u64 = 0xcbf29ce484222325;
        let mut mix = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h ^= 0xff;
            h = h.wrapping_mul(0x100000001b3);
        };
        for f in &files {
            mix(f.path.as_bytes());
            let abs = cx.tree.join(&f.path);
            if let Ok(bytes) = std::fs::read(&abs) {
                let normalized = self.normalize_for_hash(&f.path, &bytes).unwrap_or(bytes);
                mix(&normalized);
            }
        }
        for cmd in self.invocation(cx) {
            mix(cmd.program.as_bytes());
            for arg in &cmd.args {
                mix(arg.as_bytes());
            }
        }
        for (test, ranks) in ctest_add_tests_in_dir(cx.build_dir) {
            mix(test.as_bytes());
            mix(&ranks.unwrap_or(1).to_le_bytes());
        }
        for feature in cmake_cache_features(cx.build_dir) {
            mix(feature.as_bytes());
        }
        Ok(format!("{h:016x}"))
    }
}

/// Test-dir membership for oracle classification: `test`/`tests` segments or
/// a directory that configures CTest (holds a `CTestTestfile`).
fn is_ctest_test_dir(rel: &str) -> bool {
    Path::new(rel).components().any(|c| {
        matches!(c.as_os_str().to_str(), Some("test") | Some("tests") | Some("Testing"))
    })
}

/// CTest inventory for non-Python composites: configured test files, cmake
/// config refs, the CTestTestfile fixture glob, and CI invokers.
fn ctest_test_inventory(repo: &Path) -> Result<TestInventory, AdapterError> {
    let mut test_files = Vec::new();
    let mut config_refs = Vec::new();
    let mut ci_invokers = Vec::new();
    for entry in walkdir::WalkDir::new(repo).into_iter().filter_entry(|e| {
        let p = e.path();
        for seg in [".git", "__pycache__", "target", "build", "_build", ".venv", "venv", "Testing"] {
            if p.components().any(|c| c.as_os_str() == seg) {
                return false;
            }
        }
        true
    }) {
        let entry = entry.map_err(|e| AdapterError::Parse(e.to_string()))?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let rel = p.strip_prefix(repo).unwrap_or(p).to_string_lossy().replace('\\', "/");
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if rel.starts_with(".github/workflows/") {
            ci_invokers.push(rel);
            continue;
        }
        if name.starts_with("CTestTestfile")
            && p.extension().map(|x| x == "cmake").unwrap_or(false)
        {
            test_files.push(rel);
            continue;
        }
        if name == "CMakeLists.txt" {
            if is_ctest_test_dir(&rel) {
                test_files.push(rel);
            } else {
                config_refs.push(rel);
            }
            continue;
        }
        if p.extension().and_then(|x| x.to_str()).map(|e| e.eq_ignore_ascii_case("cmake")).unwrap_or(false) {
            config_refs.push(rel);
        }
    }
    test_files.sort();
    test_files.dedup();
    config_refs.sort();
    config_refs.dedup();
    ci_invokers.sort();
    ci_invokers.dedup();
    Ok(TestInventory {
        test_files,
        config_refs,
        fixture_globs: vec!["**/CTestTestfile*.cmake".to_string()],
        ci_invokers,
    })
}

// --- Track I: CMake bridge ---

/// CMake bridge: `cmake` configure, `cmake --build`, and the link-substitution
/// flow. The Rust port builds as a staticlib exporting the original linkage
/// names; `substitute` archives it in place of the unit objects, rebuilds the
/// affected target, and reruns the unit's test. Only `BIND(C)`-clean units
/// substitute one at a time (the gfortran ABI limit from the spike).
pub struct CmakeBridge;

impl CmakeBridge {
    /// The one `cmake` literal the CMake spine owns; absolute resolution
    /// happens in the executor at spawn time.
    pub fn cmake_program() -> String {
        "cmake".to_string()
    }

    /// Build target standing in for `unit`: the file stem of its
    /// authoritative source.
    fn target_for_unit(unit: &UnitDecl) -> String {
        unit.files
            .first()
            .and_then(|f| Path::new(f).file_stem())
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| unit_stem(unit))
    }

    /// Reject units whose splicing would cross the gfortran ABI boundary.
    fn check_substitutable(unit: &UnitDecl) -> Result<(), String> {
        let mut bad: Vec<String> = unit
            .exports
            .iter()
            .filter(|e| matches!(e.abi, Abi::Fortran { bind_c: false }))
            .map(|e| e.linkage.clone())
            .collect();
        bad.sort();
        bad.dedup();
        if bad.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "unit '{}' exports non-BIND(C) Fortran ({}): no stable C ABI (__<mod>_MOD_<proc> mangling, array descriptors for assumed-shape dummies); coarsen to file granularity with an ABI shim",
                unit.id,
                bad.join(", ")
            ))
        }
    }
}

impl BuildBridge for CmakeBridge {
    fn prepare(&self, cx: &BuildCtx) -> Vec<TestCommand> {
        // Ordered, fail-fast configure; frozen as `Manifest.prepare`.
        let build_type = if cx.release { "Release" } else { "Debug" };
        vec![TestCommand {
            program: Self::cmake_program(),
            args: vec![
                "-S".to_string(),
                cx.tree.to_string_lossy().into_owned(),
                "-B".to_string(),
                cx.build_dir.to_string_lossy().into_owned(),
                format!("-DCMAKE_BUILD_TYPE={build_type}"),
            ],
            cwd: Cwd::Tree,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: Some(1200),
            collect: Vec::new(),
        }]
    }

    fn build(&self, cx: &BuildCtx) -> Vec<TestCommand> {
        let mut args = vec![
            "--build".to_string(),
            cx.build_dir.to_string_lossy().into_owned(),
            "--parallel".to_string(),
        ];
        if cx.release {
            args.push("--config".to_string());
            args.push("Release".to_string());
        }
        vec![TestCommand {
            program: Self::cmake_program(),
            args,
            cwd: Cwd::Tree,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: Some(3600),
            collect: Vec::new(),
        }]
    }

    fn substitute(
        &self,
        _cx: &BuildCtx,
        unit: &UnitDecl,
        rust_lib: &Path,
    ) -> Result<Vec<TestCommand>, AdapterError> {
        if !rust_lib.exists() {
            return Err(AdapterError::Parse(format!(
                "substitute: rust lib '{}' does not exist",
                rust_lib.display()
            )));
        }
        if let Err(why) = Self::check_substitutable(unit) {
            return Err(AdapterError::Parse(format!("substitute: {why}")));
        }
        // Archive splice, target rebuild, unit-test verify. Ordered, fail-fast.
        let stem = Self::target_for_unit(unit);
        let archive = format!("lib{stem}_rs.a");
        Ok(vec![
            TestCommand {
                program: "ar".to_string(),
                args: vec![
                    "rcs".to_string(),
                    archive,
                    rust_lib.to_string_lossy().into_owned(),
                ],
                cwd: Cwd::BuildDir,
                env_set: Vec::new(),
                env_remove: Vec::new(),
                launcher: None,
                timeout_secs: Some(600),
                collect: Vec::new(),
            },
            TestCommand {
                program: Self::cmake_program(),
                args: vec![
                    "--build".to_string(),
                    ".".to_string(),
                    "--target".to_string(),
                    stem.clone(),
                ],
                cwd: Cwd::BuildDir,
                env_set: Vec::new(),
                env_remove: Vec::new(),
                launcher: None,
                timeout_secs: Some(3600),
                collect: Vec::new(),
            },
            TestCommand {
                program: CtestRunner::ctest_program(),
                args: vec![
                    "--output-on-failure".to_string(),
                    "-R".to_string(),
                    stem,
                ],
                cwd: Cwd::BuildDir,
                env_set: vec![("CTEST_OUTPUT_ON_FAILURE".to_string(), "1".to_string())],
                env_remove: Vec::new(),
                launcher: None,
                timeout_secs: Some(600),
                collect: vec!["Testing/Temporary/LastTest.log".to_string()],
            },
        ])
    }

    fn scaffold(&self, unit: &UnitDecl) -> Result<Vec<(String, String)>, AdapterError> {
        let stem = Self::target_for_unit(unit);
        let crate_name = sanitize_crate_name(&stem);
        if crate_name.is_empty() {
            return Err(AdapterError::Parse(format!(
                "scaffold: unit '{}' has no crate name",
                unit.id
            )));
        }
        let mut linkages: Vec<String> = unit.exports.iter().map(|e| e.linkage.clone()).collect();
        if linkages.is_empty() {
            linkages.push(stem.clone());
        }
        linkages.sort();
        linkages.dedup();
        let has_unstable_abi = unit
            .exports
            .iter()
            .any(|e| matches!(e.abi, Abi::Fortran { bind_c: false }));
        let abi_note = if has_unstable_abi {
            "// NOTE: this unit exports non-BIND(C) Fortran (no stable C ABI): port at\n// file granularity behind an ABI shim (see partition diagnostics).\n"
        } else {
            ""
        };
        let mut fns = String::new();
        for linkage in &linkages {
            let fn_name = format!("port_{}", sanitize_crate_name(linkage));
            fns.push_str(&format!(
                "#[export_name = \"{linkage}\"]\npub extern \"C\" fn {fn_name}() {{\n    unimplemented!(\"port `{id}` ({linkage})\")\n}}\n\n",
                linkage = linkage,
                fn_name = fn_name,
                id = unit.id,
            ));
        }
        let lib_rs = format!(
            "// Port scaffold for unit `{id}` (CMake staticlib flow; replaces\n// mirror/<fixture>/template.json).\n//\n// Build as a staticlib exporting the original linkage names, then\n// `CmakeBridge::substitute` splices the archive in place of the unit\n// objects and reruns the unit's ctest.\n{abi_note}{fns}",
            id = unit.id,
            abi_note = abi_note,
            fns = fns,
        );
        let cargo_toml = format!(
            "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\nname = \"{crate_name}\"\ncrate-type = [\"staticlib\"]\n",
            crate_name = crate_name,
        );
        Ok(vec![
            ("src/lib.rs".to_string(), lib_rs),
            ("Cargo.toml".to_string(), cargo_toml),
        ])
    }

    fn link_deps(&self, cx: &BuildCtx) -> Result<Vec<LinkDep>, AdapterError> {
        // Link targets from the build: `link.txt` command lines (`-l<name>`
        // and absolute archives) plus `CMakeCache.txt` library entries. Names
        // only for `-l`; paths kept so `classify_dep` can sort vendored from
        // system targets by origin.
        let mut deps: Vec<LinkDep> = Vec::new();
        if cx.build_dir.is_dir() {
            let mut link_files = Vec::new();
            for entry in walkdir::WalkDir::new(cx.build_dir).into_iter().filter_entry(|e| {
                !e.path().components().any(|c| c.as_os_str() == ".git")
            }) {
                let Ok(entry) = entry else { continue };
                if entry.path().is_file() && entry.path().file_name().and_then(|n| n.to_str()) == Some("link.txt") {
                    link_files.push(entry.path().to_path_buf());
                }
            }
            link_files.sort();
            for file in link_files {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    for token in text.split_whitespace() {
                        if let Some(name) = token.strip_prefix("-l") {
                            if !name.is_empty() {
                                deps.push(LinkDep { name: name.to_string(), path: None });
                            }
                        } else if token.starts_with('/')
                            && (token.contains(".so") || token.ends_with(".a") || token.contains(".dylib"))
                        {
                            let base = token.rsplit('/').next().unwrap_or(token);
                            let end = base
                                .find(".so")
                                .or_else(|| base.find(".dylib"))
                                .or_else(|| base.find(".a"))
                                .unwrap_or(base.len());
                            let mut stem = base[..end].to_string();
                            if let Some(stripped) = stem.strip_prefix("lib") {
                                stem = stripped.to_string();
                            }
                            if !stem.is_empty() {
                                deps.push(LinkDep {
                                    name: stem,
                                    path: Some(token.to_string()),
                                });
                            }
                        }
                    }
                }
            }
            let cache = cx.build_dir.join("CMakeCache.txt");
            if let Ok(text) = std::fs::read_to_string(&cache) {
                for line in text.lines() {
                    let line = line.trim();
                    let Some((key, value)) = line.split_once('=') else { continue };
                    let name = key.split(':').next().unwrap_or(key);
                    if !(name.contains("LIBRARY") || name.contains("LIB")) || value.is_empty() {
                        continue;
                    }
                    if value.ends_with("-NOTFOUND") || !value.starts_with('/') {
                        continue;
                    }
                    let base = value.rsplit('/').next().unwrap_or(value);
                    let end = base
                        .find(".so")
                        .or_else(|| base.find(".dylib"))
                        .or_else(|| base.find(".a"))
                        .unwrap_or(base.len());
                    let mut stem = base[..end].to_string();
                    if let Some(stripped) = stem.strip_prefix("lib") {
                        stem = stripped.to_string();
                    }
                    if !stem.is_empty() {
                        deps.push(LinkDep { name: stem, path: Some(value.to_string()) });
                    }
                }
            }
        }
        deps.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
        deps.dedup_by(|a, b| a.name == b.name && a.path == b.path);
        Ok(deps)
    }

    fn artifacts(&self, cx: &BuildCtx) -> Vec<PathBuf> {
        // Built libraries that must survive for grading, by suffix.
        let mut out = Vec::new();
        if cx.build_dir.is_dir() {
            for entry in walkdir::WalkDir::new(cx.build_dir).into_iter().filter_entry(|e| {
                !e.path().components().any(|c| c.as_os_str() == ".git")
            }) {
                let Ok(entry) = entry else { continue };
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.contains(".so") || name.ends_with(".a") {
                    out.push(p.to_path_buf());
                }
            }
        }
        out.sort();
        out
    }
}

/// Lowercase-alphanumeric crate identifier (`_` for anything else; `_`
/// prefix when leading with a digit).
fn sanitize_crate_name(s: &str) -> String {
    let mut ident: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if ident.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        ident.insert(0, '_');
    }
    ident.trim_end_matches('_').to_string()
}

// --- Track I: perf profiler ---

/// `perf`-shaped profiler for compiled trees. `setup`/`timed` describe plain
/// shell workloads (the core measures the child via wait4 rusage);
/// `hotspots` needs a working `perf` and honors `perf_available`, per the
/// ADR-003 reconciliation (no perf dependency: the Python path never needs
/// it, this one reports absence as an error, never silent data).
pub struct PerfProfiler {
    pub tree: PathBuf,
}

impl Profiler for PerfProfiler {
    fn setup(&self, w: &Workload) -> Vec<TestCommand> {
        if w.setup.trim().is_empty() {
            return Vec::new();
        }
        vec![TestCommand {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), w.setup.clone()],
            cwd: Cwd::Rel(self.tree.to_string_lossy().into_owned()),
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: Some(600),
            collect: Vec::new(),
        }]
    }

    fn timed(&self, w: &Workload) -> TestCommand {
        TestCommand {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), w.stmt.clone()],
            cwd: Cwd::Rel(self.tree.to_string_lossy().into_owned()),
            env_set: Vec::new(),
            env_remove: Vec::new(),
            launcher: None,
            timeout_secs: Some(600),
            collect: Vec::new(),
        }
    }

    fn hotspots(&self, w: &Workload, perf_available: bool) -> Result<HotspotBaseline, AdapterError> {
        if !perf_available {
            return Err(AdapterError::Parse(
                "hotspots: perf unavailable (perf_event_paranoid blocks perf; ADR-003): PerfProfiler has no fallback sampler".into(),
            ));
        }
        let probe = std::process::Command::new("perf")
            .arg("--version")
            .current_dir(std::env::temp_dir())
            .output()
            .map_err(AdapterError::Io)?;
        if !probe.status.success() {
            return Err(AdapterError::Parse(
                "hotspots: perf binary missing or failing despite perf_available".into(),
            ));
        }
        let started = std::time::Instant::now();
        let out = std::process::Command::new("perf")
            .args(["stat", "-e", "task-clock", "sh", "-c", &w.stmt])
            .current_dir(std::env::temp_dir())
            .output()
            .map_err(AdapterError::Io)?;
        let wall_secs = started.elapsed().as_secs_f64();
        if !out.status.success() {
            return Err(AdapterError::Parse(format!(
                "hotspots: perf stat failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        // No symbolization pipeline: functions stay empty until a DWARF unwind
        // reader lands; the wall time is measured, never fabricated.
        Ok(HotspotBaseline { tool: "perf".into(), functions: Vec::new(), wall_secs })
    }
}

// --- Track I: composite helpers (classify/license/coarsen/out-of-scope) ---

/// Link-target classification by path origin. Frontends never see link
/// targets (they come from the build, not the language): vendored sources
/// and project-built absolutes port, system paths and bare `-l` names keep.
fn classify_link_dep(dep: &LinkDep) -> DepClass {
    const VENDORED_SEGS: &[&str] =
        &["third_party", "thirdparty", "external", "vendor", "contrib", "submodules"];
    if let Some(path) = &dep.path {
        let lower = path.to_ascii_lowercase();
        if lower.split('/').any(|seg| VENDORED_SEGS.contains(&seg)) {
            return DepClass::Port;
        }
        if lower.starts_with("/usr/") || lower.starts_with("/lib/") || lower == "/lib" {
            return DepClass::Keep;
        }
        return DepClass::Port;
    }
    DepClass::Keep
}

/// One [`Attribution`] per license file: top-level `LICENSE*`/`LICENCE*`/
/// `COPYING*`/`NOTICE*` plus any `*license*` directory (the installed-tree
/// `license_texts` shape). Optionally keeps the Python entry for mixed
/// trees; falls back to `unknown` when nothing is found.
fn generic_license_terms(
    repo: &Path,
    include_python: bool,
) -> Result<Vec<(String, Attribution)>, AdapterError> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(repo).into_iter().filter_entry(|e| {
        let p = e.path();
        !p.components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some(".git" | "target" | "build" | "_build" | ".venv" | "venv")
            )
        })
    }) {
        let entry = entry.map_err(|e| AdapterError::Parse(e.to_string()))?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let lower = name.to_ascii_lowercase();
        let file_hit = lower.starts_with("license")
            || lower.starts_with("licence")
            || lower.starts_with("copying")
            || lower.starts_with("notice");
        let dir_hit = p
            .parent()
            .and_then(|d| d.file_name())
            .and_then(|d| d.to_str())
            .map(|d| d.to_ascii_lowercase().contains("license"))
            .unwrap_or(false);
        if !file_hit && !dir_hit {
            continue;
        }
        let rel = p.strip_prefix(repo).unwrap_or(p).to_string_lossy().replace('\\', "/");
        // License sniffing is best-effort: repos carry legacy encodings
        // (e.g. ISO-8859) and unreadable files must never halt recon. Read
        // bytes and sniff lossily; skip files that cannot be read at all.
        let Ok(bytes) = std::fs::read(p) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        out.push((
            rel,
            Attribution {
                license: sniff_license_text(&text),
                header_text: text.lines().take(5).collect::<Vec<_>>().join("\n"),
                notice_extra: String::new(),
            },
        ));
    }
    if include_python {
        let (glob, attribution) = python_license_terms(repo)?;
        if glob != "*" {
            out.push((glob, attribution));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    if out.is_empty() {
        out.push((
            "*".to_string(),
            Attribution {
                license: "unknown".into(),
                header_text: String::new(),
                notice_extra: String::new(),
            },
        ));
    }
    Ok(out)
}

/// License family sniff shared by the generic scan (the Python helper keeps
/// its own copy so its frozen behavior never changes).
fn sniff_license_text(text: &str) -> String {
    if text.contains("BSD") {
        if text.contains("2-Clause") || text.contains("BSD-2") {
            "BSD-2-Clause"
        } else {
            "BSD"
        }
    } else if text.contains("Redistribution and use in source and binary forms") {
        "BSD-2-Clause"
    } else if text.contains("MIT License") {
        "MIT"
    } else if text.contains("Apache") {
        "Apache-2.0"
    } else {
        "unknown"
    }
    .to_string()
}

/// Whether a Fortran unit is `BIND(C)`-clean: every exported procedure
/// carries `BIND(C)` (vacuously true with no procedures).
fn fortran_unit_bind_c(unit: &UnitDecl) -> bool {
    unit.exports.iter().all(|e| !matches!(e.abi, Abi::Fortran { bind_c: false }))
}

/// Coarsen non-`BIND(C)` Fortran units to file granularity: multi-symbol
/// files merge their `#`-suffixed units into one file unit (edges rewired,
/// self-edges dropped); single-file units keep their shape but gain a
/// diagnostic. Python/C/C++ units are untouched.
fn coarsen_fortran_units(
    units: &mut Vec<UnitDecl>,
    edges: &mut HashSet<(UnitId, UnitId)>,
    diagnostics: &mut Vec<String>,
) {
    for unit in units.iter() {
        if unit.id.0.starts_with("fortran:") && !unit.id.0.contains('#') && !fortran_unit_bind_c(unit) {
            diagnostics.push(format!(
                "coarsen: '{}' is non-BIND(C) Fortran; substitutes only at file granularity with an ABI shim",
                unit.id
            ));
        }
    }
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, unit) in units.iter().enumerate() {
        if !unit.id.0.starts_with("fortran:") {
            continue;
        }
        if let Some((file, _)) = unit.id.0.split_once('#') {
            groups.entry(file.to_string()).or_default().push(i);
        }
    }
    let mut merged_groups: Vec<(String, Vec<UnitId>)> = Vec::new();
    let mut merged_units: Vec<UnitDecl> = Vec::new();
    let mut remove: HashSet<usize> = HashSet::new();
    for (file, members) in &groups {
        let mut unstable: Vec<String> = members
            .iter()
            .flat_map(|i| units[*i].exports.iter())
            .filter(|e| matches!(e.abi, Abi::Fortran { bind_c: false }))
            .map(|e| e.linkage.clone())
            .collect();
        unstable.sort();
        unstable.dedup();
        if unstable.is_empty() {
            continue;
        }
        let rel = file.strip_prefix("fortran:").unwrap_or(file).to_string();
        let mut file_unit = UnitDecl {
            id: UnitId(file.clone()),
            files: vec![rel.clone()],
            generated_from: None,
            exports: Vec::new(),
            imports: Vec::new(),
        };
        let mut generated: Vec<Option<String>> =
            members.iter().map(|i| units[*i].generated_from.clone()).collect();
        generated.sort();
        generated.dedup();
        if generated.len() == 1 {
            file_unit.generated_from = generated.into_iter().next().unwrap();
        }
        for i in members {
            file_unit.exports.extend(units[*i].exports.iter().cloned());
            file_unit.imports.extend(units[*i].imports.iter().cloned());
            remove.insert(*i);
        }
        file_unit.exports.sort_by(|a, b| a.linkage.cmp(&b.linkage));
        file_unit.exports.dedup_by(|a, b| a.linkage == b.linkage && a.abi == b.abi);
        file_unit.imports.sort_by(|a, b| a.linkage.cmp(&b.linkage));
        file_unit.imports.dedup_by(|a, b| a.linkage == b.linkage && a.abi == b.abi);
        diagnostics.push(format!(
            "coarsen: '{file}' merges {} Fortran units: non-BIND(C) ({}) has no stable C ABI; partition at file granularity",
            members.len(),
            unstable.join(", ")
        ));
        merged_groups.push((file.clone(), members.iter().map(|i| units[*i].id.clone()).collect()));
        merged_units.push(file_unit);
    }
    if remove.is_empty() {
        return;
    }
    let mut kept = Vec::new();
    for (i, unit) in units.drain(..).enumerate() {
        if !remove.contains(&i) {
            kept.push(unit);
        }
    }
    kept.extend(merged_units);
    *units = kept;
    let remap: HashMap<&str, &str> =
        merged_groups.iter().flat_map(|(file, ids)| ids.iter().map(|id| (id.0.as_str(), file.as_str()))).collect();
    let rewired: HashSet<(UnitId, UnitId)> = edges
        .drain()
        .filter_map(|(a, b)| {
            let a2 = remap.get(a.0.as_str()).map(|s| UnitId((*s).to_string())).unwrap_or(a);
            let b2 = remap.get(b.0.as_str()).map(|s| UnitId((*s).to_string())).unwrap_or(b);
            if a2 == b2 {
                None
            } else {
                Some((a2, b2))
            }
        })
        .collect();
    *edges = rewired;
}

/// Mark out-of-scope units via diagnostics (the frozen [`UnitDag`] shape is
/// unchanged): vendored sources (link targets, not ported) and units with no
/// baseline coverage. Coverage comes from the baseline run's
/// `.rustsmith-coverage.json` (`{unit_id: samples}`) in the build dir;
/// absent file means no coverage marks.
fn mark_out_of_scope(units: &[UnitDecl], diagnostics: &mut Vec<String>, build_dir: &Path) {
    const VENDORED_SEGS: &[&str] =
        &["third_party", "thirdparty", "external", "vendor", "contrib", "submodules"];
    for unit in units {
        let vendored = unit.files.iter().any(|f| {
            f.split('/').any(|seg| VENDORED_SEGS.contains(&seg.to_ascii_lowercase().as_str()))
        });
        if vendored {
            diagnostics.push(format!(
                "out-of-scope: '{}' is vendored (link target, not ported; see link_deps)",
                unit.id
            ));
        }
    }
    let coverage_path = build_dir.join(".rustsmith-coverage.json");
    if !coverage_path.is_file() {
        return;
    }
    let Ok(text) = std::fs::read_to_string(&coverage_path) else { return };
    let Ok(counts) = serde_json::from_str::<BTreeMap<String, u64>>(&text) else {
        diagnostics.push(
            "coverage: '.rustsmith-coverage.json' unparseable; skipping coverage marks".to_string(),
        );
        return;
    };
    for unit in units {
        if counts.get(&unit.id.0).copied().unwrap_or(0) == 0 {
            diagnostics.push(format!(
                "out-of-scope: '{}' has no baseline coverage",
                unit.id
            ));
        }
    }
}

#[cfg(test)]
mod track_i_tests {
    //! Track I proofs against transcript-shaped fixtures (no pinned checkout
    //! or network in this environment): claims, fragment, partition,
    //! substitute, and scaffold paths for the Fortran/C/C++ frontends and the
    //! CMake/CTest spine, plus Python-fixture parity.
    use super::*;

    struct MockTree {
        _dir: tempfile::TempDir,
        tree: PathBuf,
        build: PathBuf,
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    /// Stand-in for the pinned tri-language tree: preprocessed Fortran,
    /// `.src` codegen, multi-module files, C, C++, one Python helper, CMake
    /// configure files, a configured build dir, and a SUT-compiled test.
    fn mock_tree() -> MockTree {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        let build = tree.join("build");
        write_file(
            &tree.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.20)\nproject(MockSolver LANGUAGES C CXX Fortran)\n",
        );
        write_file(
            &tree.join("CTestTestfile.cmake"),
            "subdirs(\"tests\")\nadd_test(solver_unit \"/b/solver_test\")\nadd_test(mpi_solver \"/usr/bin/mpiexec\" \"-n\" \"4\" \"/b/mpi_test\")\n",
        );
        write_file(
            &tree.join("cmake/test_macros.cmake"),
            "# transcript-shaped test macros at the pin\nmacro(mock_add_test name)\n  add_test(${name} ${ARGN})\nendmacro()\n",
        );
        write_file(
            &tree.join("cmake/runtest.cmake"),
            "# transcript-shaped runner at the pin: a SUT-written verdict is a cross-check only\nif(EXISTS \"TEST.PASSED\")\n  message(STATUS \"solver verdict present\")\nendif()\n",
        );
        write_file(&tree.join("LICENSE"), "MIT License\n\nPermission is hereby granted.\n");
        write_file(
            &tree.join("src/solver.F90"),
            "#include \"defs.h\"\n#ifdef USE_MPI\n#define SOLVER_MPI 1\n#endif\nmodule solver_mod\n  use iso_c_binding\n  implicit none\ncontains\n  subroutine init_solver(n) bind(c, name=\"solver_init\")\n    integer(c_int), value :: n\n  end subroutine init_solver\n  subroutine step_solver(a)\n    real(8) :: a(:)\n  end subroutine step_solver\nend module solver_mod\n",
        );
        write_file(
            &tree.join("src/gen.src"),
            "module gen_mod\n  implicit none\ncontains\n  subroutine gen_init() bind(c)\n  end subroutine gen_init\nend module gen_mod\n",
        );
        write_file(
            &tree.join("src/gen.F90"),
            "module gen_mod\n  implicit none\ncontains\n  subroutine gen_init() bind(c)\n  end subroutine gen_init\nend module gen_mod\n",
        );
        write_file(
            &tree.join("src/multi.F90"),
            "module alpha_mod\n  implicit none\ncontains\n  function alpha_val() bind(c) result(r)\n    real(8) :: r\n  end function alpha_val\nend module alpha_mod\nmodule beta_mod\n  use alpha_mod\n  use gen_mod, only: gen_init\n  implicit none\ncontains\n  subroutine beta_step(x)\n    real(8) :: x\n  end subroutine beta_step\nend module beta_mod\n",
        );
        write_file(&tree.join("lib/util.h"), "void util_init(void);\n");
        write_file(
            &tree.join("lib/util.c"),
            "#include \"util.h\"\nvoid util_init(void) {}\n",
        );
        write_file(
            &tree.join("app/main.cpp"),
            "#include \"util.h\"\n#include \"missing.h\"\nint main() { return 0; }\n",
        );
        write_file(&tree.join("tools/helper.py"), "VALUE = 1\n");
        write_file(&tree.join("tools/check.py"), "import helper\n\nassert helper.VALUE == 1\n");
        write_file(&tree.join("third_party/vendored.f90"), "module vend_mod\nend module vend_mod\n");
        write_file(&tree.join("tests/CMakeLists.txt"), "add_test(solver_unit solver_test)\n");
        write_file(
            &tree.join("tests/test_solver.F90"),
            "program test_solver\n  use solver_mod\nend program test_solver\n",
        );
        let abs = |rel: &str| tree.join(rel).to_string_lossy().replace('\\', "/");
        write_file(
            &build.join("compile_commands.json"),
            &format!(
                "[{{\"directory\": \"{b}\", \"file\": \"{u}\", \"language\": \"C\"}},{{\"directory\": \"{b}\", \"file\": \"{h}\", \"language\": \"C\"}},{{\"directory\": \"{b}\", \"file\": \"{m}\", \"language\": \"CXX\"}}]",
                b = build.to_string_lossy(),
                u = abs("lib/util.c"),
                h = abs("lib/util.h"),
                m = abs("app/main.cpp"),
            ),
        );
        write_file(
            &build.join("CMakeCache.txt"),
            "WITH_MPI:BOOL=ON\nCMAKE_BUILD_TYPE:STRING=Release\nUMFPACK_LIBRARY:FILEPATH=/usr/lib/x86_64-linux-gnu/libumfpack.so\nVENDORED_LIB:FILEPATH=/app/third_party/lib/libspecial.a\nCMAKE_HOME_DIRECTORY:PATH=/app/tree\n",
        );
        write_file(
            &build.join("CTestTestfile.cmake"),
            "subdirs(\"tests\")\nadd_test(solver_unit \"/b/solver_test\")\nadd_test(mpi_solver \"/usr/bin/mpiexec\" \"-n\" \"4\" \"/b/mpi_test\")\n",
        );
        let reply = build.join(".cmake/api/v1/reply");
        write_file(
            &reply.join("index-001.json"),
            "{\"objects\": [{\"kind\": \"target\", \"jsonFile\": \"target-solver.json\"}, {\"kind\": \"target\", \"jsonFile\": \"target-util.json\"}]}",
        );
        write_file(
            &reply.join("target-solver.json"),
            &format!(
                "{{\"name\": \"solver\", \"sources\": [{{\"path\": \"{s}\", \"compileGroupIndex\": 0}}, {{\"path\": \"{m}\", \"compileGroupIndex\": 0}}], \"compileGroups\": [{{\"language\": \"Fortran\", \"sourceIndexes\": [0, 1]}}]}}",
                s = abs("src/solver.F90"),
                m = abs("src/multi.F90"),
            ),
        );
        write_file(
            &reply.join("target-util.json"),
            &format!(
                "{{\"name\": \"util\", \"sources\": [{{\"path\": \"{u}\", \"compileGroupIndex\": 0}}], \"compileGroups\": [{{\"language\": \"C\", \"sourceIndexes\": [0]}}]}}",
                u = abs("lib/util.c"),
            ),
        );
        write_file(
            &build.join("CMakeFiles/util.dir/link.txt"),
            "/usr/bin/cc -shared -o libutil.so util.c.o -lm /usr/lib/x86_64-linux-gnu/libmpi.so\n",
        );
        write_file(&build.join("libutil.a"), "");
        write_file(&build.join("libsolver.so"), "");
        let coverage = serde_json::json!({
            "fortran:src/solver.F90": 10,
            "fortran:src/gen.src": 5,
            "fortran:src/multi.F90": 7,
            "fortran:third_party/vendored.f90": 3,
            "c:lib/util.c": 4,
            "c:lib/util.h": 4,
            "python:tools/helper.py": 2,
            "python:tools/check.py": 2,
        });
        write_file(
            &build.join(".rustsmith-coverage.json"),
            &serde_json::to_string(&coverage).unwrap(),
        );
        MockTree { _dir: dir, tree, build }
    }

    fn claimed_fortran_files(mock: &MockTree) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&mock.tree).into_iter().filter_entry(|e| {
            !e.path().components().any(|c| c.as_os_str() == "build")
        }) {
            let entry = entry.unwrap();
            if entry.path().is_file() && FortranFrontend.claims(entry.path(), None) {
                files.push(entry.path().to_path_buf());
            }
        }
        files.sort();
        files
    }

    #[test]
    fn fortran_claims_f90_f_src_not_others() {
        for ext in ["F90", "f90", "F", "f", "src", "f95", "for"] {
            assert!(
                FortranFrontend.claims(Path::new(&format!("solver.{ext}")), None),
                "claims .{ext}"
            );
        }
        for name in ["solver.py", "solver.cpp", "solver.c", "CMakeLists.txt", "run.cmake"] {
            assert!(!FortranFrontend.claims(Path::new(name), None), "rejects {name}");
        }
    }

    #[test]
    fn fortran_fragment_modules_uses_bindc_codegen() {
        let mock = mock_tree();
        let files = claimed_fortran_files(&mock);
        let compiler_ids = BTreeMap::new();
        let cx =
            FragmentCtx { repo: &mock.tree, files: &files, compile_db: None, compiler_ids: &compiler_ids };
        let fragment = FortranFrontend.fragment(&cx).unwrap();
        let ids: Vec<&str> = fragment.units.iter().map(|u| u.id.0.as_str()).collect();
        // One unit per authoritative source: the checked-in generated copy is
        // skipped, multi-module files split with `#`, tests never units.
        assert!(ids.contains(&"fortran:src/solver.F90"));
        assert!(ids.contains(&"fortran:src/gen.src"));
        assert!(!ids.iter().any(|id| id.contains("gen.F90")));
        assert!(ids.contains(&"fortran:src/multi.F90#alpha_mod"));
        assert!(ids.contains(&"fortran:src/multi.F90#beta_mod"));
        assert!(ids.contains(&"fortran:third_party/vendored.f90"));
        assert!(!ids.iter().any(|id| id.contains("test_solver")));
        assert_eq!(ids.len(), 5);
        // USE edges at fragment scope (same file); cross-file USEs are
        // imports resolved by the composite linkage index.
        assert!(fragment.edges.contains(&(
            UnitId("fortran:src/multi.F90#beta_mod".into()),
            UnitId("fortran:src/multi.F90#alpha_mod".into())
        )));
        let beta = fragment
            .units
            .iter()
            .find(|u| u.id.0 == "fortran:src/multi.F90#beta_mod")
            .unwrap();
        let imports: Vec<&str> =
            beta.imports.iter().map(|s| s.linkage.as_str()).collect();
        assert!(imports.contains(&"alpha_mod"));
        assert!(imports.contains(&"gen_mod"));
        // BIND(C): named linkage kept, plain procedures mangled representatives.
        let solver = fragment
            .units
            .iter()
            .find(|u| u.id.0 == "fortran:src/solver.F90")
            .unwrap();
        let linkages: Vec<&str> =
            solver.exports.iter().map(|s| s.linkage.as_str()).collect();
        assert!(linkages.contains(&"solver_mod"));
        assert!(linkages.contains(&"solver_init"));
        assert!(linkages.contains(&"__solver_mod_MOD_step_solver"));
        assert!(solver.imports.iter().any(|s| s.linkage == "iso_c_binding"));
        // Preprocess + codegen diagnostics.
        assert!(fragment.diagnostics.iter().any(|d| d.contains("preprocessed")
            && d.contains("src/solver.F90")
            && d.contains('4')));
        assert!(fragment.diagnostics.iter().any(|d| d.contains("generated")
            && d.contains("src/gen.F90")
            && d.contains("src/gen.src")));
    }

    #[test]
    fn fortran_line_parsers_edge_cases() {
        assert_eq!(
            parse_use_module("use, intrinsic :: iso_c_binding"),
            Some("iso_c_binding".into())
        );
        assert_eq!(parse_use_module("use foo, only: bar"), Some("foo".into()));
        assert_eq!(parse_use_module("use :: foo"), Some("foo".into()));
        // `!` inside a string literal is not a comment.
        assert_eq!(
            strip_fortran_comment("  character(len=*) :: s = \"a!b\""),
            "  character(len=*) :: s = \"a!b\""
        );
        assert_eq!(strip_fortran_comment("  x = 1 ! trailing"), "  x = 1");
    }

    #[test]
    fn cxx_claims_by_extension_then_compile_db() {
        let mock = mock_tree();
        let db = mock.build.join("compile_commands.json");
        let main = mock.tree.join("app/main.cpp");
        let util_c = mock.tree.join("lib/util.c");
        assert!(CxxFrontend.claims(&main, None));
        assert!(CxxFrontend.claims(&util_c, None));
        assert!(!CxxFrontend.claims(&mock.tree.join("src/solver.F90"), None));
        assert!(!CxxFrontend.claims(&mock.tree.join("tools/helper.py"), None));
        // Listed files stay claimed; an unlisted C++ file drops out once the
        // DB exists; an unreadable DB falls back to extension claims.
        assert!(CxxFrontend.claims(&main, Some(&db)));
        assert!(!CxxFrontend.claims(&mock.tree.join("app/other.cpp"), Some(&db)));
        assert!(CxxFrontend.claims(
            &mock.tree.join("app/other.cpp"),
            Some(&mock.tree.join("nope.json"))
        ));
    }

    #[test]
    fn cxx_fragment_includes_edges_and_unresolved() {
        let mock = mock_tree();
        let db = mock.build.join("compile_commands.json");
        let files = vec![
            mock.tree.join("lib/util.c"),
            mock.tree.join("lib/util.h"),
            mock.tree.join("app/main.cpp"),
        ];
        let compiler_ids = BTreeMap::new();
        let cx = FragmentCtx {
            repo: &mock.tree,
            files: &files,
            compile_db: Some(&db),
            compiler_ids: &compiler_ids,
        };
        let fragment = CxxFrontend.fragment(&cx).unwrap();
        let ids: Vec<&str> = fragment.units.iter().map(|u| u.id.0.as_str()).collect();
        assert!(ids.contains(&"c:lib/util.c"));
        assert!(ids.contains(&"c:lib/util.h"));
        assert!(ids.contains(&"cxx:app/main.cpp"));
        // Headers export `<stem>_h`; both sources attach to the header unit.
        assert!(fragment.edges.contains(&(
            UnitId("c:lib/util.c".into()),
            UnitId("c:lib/util.h".into())
        )));
        assert!(fragment.edges.contains(&(
            UnitId("cxx:app/main.cpp".into()),
            UnitId("c:lib/util.h".into())
        )));
        assert!(fragment
            .diagnostics
            .iter()
            .any(|d| d.contains("unresolved-include") && d.contains("missing.h")));
    }

    #[test]
    fn probe_yields_three_frontends_on_mock_tree() {
        let mock = mock_tree();
        let report = probe(&mock.tree).unwrap();
        assert_eq!(report.frontends, vec!["cxx", "fortran", "python"]);
        assert!(report.has_ctest);
        assert_eq!(report.cmake_languages, vec!["C", "CXX", "FORTRAN"]);
        assert_eq!(report.unclaimed_share, 0.0);
        assert!(report.unclaimed.is_empty());
    }

    #[test]
    fn probe_stays_single_frontend_on_python_fixture() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("pkg/a.py"), "VALUE = 1\n");
        write_file(&dir.path().join("pyproject.toml"), "[project]\nname = \"a\"\n");
        let report = probe(dir.path()).unwrap();
        assert_eq!(report.frontends, vec!["python"]);
        assert_eq!(report.unclaimed_share, 0.0);
    }
    #[test]
    fn select_composite_picks_ctest_spine_for_mixed_tree() {
        let mock = mock_tree();
        let (composite, _) = select_composite(&mock.tree).unwrap();
        assert_eq!(composite.languages(), vec!["cxx", "fortran", "python"]);
        assert_eq!(composite.runner_id(), "ctest");
        let image = composite.image();
        assert_eq!(image.base, "gcc:14");
        assert!(!image.writable.is_empty());
    }

    #[test]
    fn select_composite_keeps_pytest_spine_for_python_tree() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("pkg/a.py"), "VALUE = 1\n");
        let (composite, _) = select_composite(dir.path()).unwrap();
        assert_eq!(composite.languages(), vec!["python"]);
        assert_eq!(composite.runner_id(), "pytest");
        let image = composite.image();
        assert_eq!(image.base, "python:3.11-slim");
        assert!(image.writable.is_empty());
    }

    #[test]
    fn ctest_invocation_is_quick_subset_in_build_dir() {
        let mock = mock_tree();
        let cx =
            BuildCtx { tree: &mock.tree, build_dir: &mock.build, release: false };
        let cmds = CtestRunner.invocation(&cx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].program, "ctest");
        assert!(cmds[0].args.contains(&"-L".to_string()));
        assert!(cmds[0].args.contains(&"quick".to_string()));
        assert_eq!(cmds[0].cwd, Cwd::BuildDir);
        assert!(!cmds[0].collect.is_empty());
    }

    #[test]
    fn ctest_launchers_record_mpi_ranks() {
        let output = "Test project /app/build\n  Test #1: solver_unit\n  Test command: /app/build/solver_test\n  Test #2: mpi_solver\n  Test command: /usr/bin/mpiexec -n 4 /app/build/mpi_test\n  Test #3: serial_check\n";
        let launchers = CtestRunner::test_launchers(output);
        assert_eq!(launchers.len(), 3);
        assert_eq!(launchers[0], ("solver_unit".to_string(), None));
        assert_eq!(launchers[2], ("serial_check".to_string(), None));
        let (name, launcher) = &launchers[1];
        assert_eq!(name, "mpi_solver");
        let launcher = launcher.as_ref().unwrap();
        assert_eq!(launcher.program, "/usr/bin/mpiexec");
        assert_eq!(launcher.np_flag, "-n");
        assert_eq!(launcher.np, 4);
    }

    #[test]
    fn ctest_grade_parses_transcript_verdicts() {
        let stdout = "Test project /app/build\n    Start 1: solver_unit\n1/4 Test #1: solver_unit ............ Passed    0.12 sec\n    Start 2: mpi_solver\n2/4 Test #2: mpi_solver ............. Failed    1.03 sec\n    Start 3: slow_case\n3/4 Test #3: slow_case .............. Timeout  300.00 sec\n    Start 4: skipped_case\n4/4 Test #4: skipped_case ........... Not Run   0.00 sec\n75% tests passed, 1 tests failed out of 4\nThe following tests FAILED:\n\t  2 - mpi_solver (Failed)\n";
        let runs = vec![RunOutput {
            exit_code: 1,
            stdout: stdout.to_string(),
            stderr: String::new(),
            artifacts: BTreeMap::new(),
        }];
        let graded = CtestRunner.grade(&runs).unwrap();
        assert_eq!(graded.count(Outcome::Pass), 1);
        assert_eq!(graded.count(Outcome::Fail), 1);
        assert_eq!(graded.count(Outcome::Timeout), 1);
        assert_eq!(graded.count(Outcome::NotRun), 1);
        assert_eq!(graded.passed, 1);
        assert_eq!(graded.failed, 2);
        assert_eq!(graded.exit_code, 1);
        assert!(graded.outcomes.contains_key("solver_unit"));
    }

    #[test]
    fn ctest_grade_summary_only_counts_without_phantoms() {
        // `100%` is a percent, never a count; `out of 3` carries no verdict.
        let only_percent = vec![RunOutput {
            exit_code: 0,
            stdout: "100% tests passed, 0 tests failed out of 3\n".to_string(),
            stderr: String::new(),
            artifacts: BTreeMap::new(),
        }];
        let graded = CtestRunner.grade(&only_percent).unwrap();
        assert_eq!(graded.total_outcomes(), 0);
        let count_only = vec![RunOutput {
            exit_code: 0,
            stdout: "3 tests passed, 0 tests failed\n".to_string(),
            stderr: String::new(),
            artifacts: BTreeMap::new(),
        }];
        let graded = CtestRunner.grade(&count_only).unwrap();
        assert_eq!(graded.passed, 3);
        assert_eq!(graded.total_outcomes(), 3);
    }

    #[test]
    fn ctest_observe_extracts_norm_observable() {
        let runs = vec![RunOutput {
            exit_code: 0,
            stdout: "solver_unit: Reference Norm = 1.234e-05\nsolver_unit: TEST.PASSED\n".to_string(),
            stderr: String::new(),
            artifacts: BTreeMap::new(),
        }];
        let specs = vec![ObservableSpec {
            test_glob: "solver_*".to_string(),
            source: "stdout".to_string(),
            regex: "Reference Norm\\s*=\\s*([0-9.eE+-]+)".to_string(),
            rel_tol: 1e-6,
            abs_tol: 1e-12,
        }];
        let observations = CtestRunner.observe(&runs, &specs);
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].test_id, "solver_*");
        assert_eq!(observations[0].key, "stdout:1.234e-05");
    }

    #[test]
    fn ctest_oracle_files_kinds_and_harness() {
        let mock = mock_tree();
        let files = CtestRunner.oracle_files(&mock.tree).unwrap();
        let kind_of = |suffix: &str| {
            files.iter().find(|f| f.path.ends_with(suffix)).map(|f| f.kind)
        };
        assert_eq!(kind_of("CMakeLists.txt"), Some(OracleKind::Spec));
        assert_eq!(kind_of("CTestTestfile.cmake"), Some(OracleKind::Spec));
        assert_eq!(kind_of("cmake/test_macros.cmake"), Some(OracleKind::Spec));
        assert_eq!(kind_of("cmake/runtest.cmake"), Some(OracleKind::Spec));
        // Test-local source compiled against SUT units is Harness.
        assert_eq!(kind_of("tests/test_solver.F90"), Some(OracleKind::Harness));
        assert_eq!(kind_of("tests/CMakeLists.txt"), Some(OracleKind::Spec));
        // SUT sources and build outputs are never oracle.
        assert!(files.iter().all(|f| !f.path.ends_with("src/solver.F90")));
        assert!(files.iter().all(|f| !f.path.starts_with("build/")));
    }

    #[test]
    fn ctest_normalize_and_config_hash() {
        let mock = mock_tree();
        let cx =
            BuildCtx { tree: &mock.tree, build_dir: &mock.build, release: false };
        assert_eq!(
            CtestRunner.normalize_for_hash("CMakeLists.txt", b"a  \r\nb\r\n"),
            Some(b"a\nb".to_vec())
        );
        assert_eq!(CtestRunner.normalize_for_hash("tools/helper.py", b"a"), None);
        let first = CtestRunner.config_hash(&cx).unwrap();
        assert_eq!(first.len(), 16);
        assert_eq!(first, CtestRunner.config_hash(&cx).unwrap());
        // A differently configured build (no MPI) hashes differently: the
        // test set depends on configure, so a mismatch halts downstream.
        let build2 = mock.tree.join("build2");
        write_file(
            &build2.join("CMakeCache.txt"),
            "WITH_MPI:BOOL=OFF\nCMAKE_BUILD_TYPE:STRING=Release\n",
        );
        write_file(
            &build2.join("CTestTestfile.cmake"),
            "add_test(solver_unit \"/b/solver_test\")\n",
        );
        let cx2 =
            BuildCtx { tree: &mock.tree, build_dir: &build2, release: false };
        assert_ne!(first, CtestRunner.config_hash(&cx2).unwrap());
        // MPI ranks come from the CTestTestfile text without spawning.
        let text = std::fs::read_to_string(mock.build.join("CTestTestfile.cmake")).unwrap();
        assert_eq!(
            CtestRunner::add_tests_in_text(&text),
            vec![("solver_unit".to_string(), None), ("mpi_solver".to_string(), Some(4)),]
        );
        assert_eq!(
            CtestRunner::cache_features_in_dir(&mock.build),
            // Found-feature set only; library paths are link_deps, not features.
            vec!["CMAKE_BUILD_TYPE=Release".to_string(), "WITH_MPI=ON".to_string(),]
        );
    }

    #[test]
    fn cmake_prepare_build_and_substitution_spike() {
        let mock = mock_tree();
        let (composite, _) = select_composite(&mock.tree).unwrap();
        let cx =
            BuildCtx { tree: &mock.tree, build_dir: &mock.build, release: false };
        let prepare = composite.bridge.prepare(&cx);
        assert_eq!(prepare.len(), 1);
        assert_eq!(prepare[0].program, "cmake");
        assert!(prepare[0].args.contains(&"-S".to_string()));
        assert!(prepare[0].args.contains(&"-B".to_string()));
        assert!(prepare[0].args.iter().any(|a| a.contains("CMAKE_BUILD_TYPE=Debug")));
        assert_eq!(prepare[0].cwd, Cwd::Tree);
        let build = composite.bridge.build(&cx);
        assert_eq!(build[0].args[0], "--build");
        // The spike: one BIND(C)-clean unit rebuilds and reruns its test.
        let bind_c_unit = UnitDecl {
            id: UnitId("fortran:src/gen.src".into()),
            files: vec!["src/gen.src".into()],
            generated_from: None,
            exports: vec![
                Symbol { linkage: "gen_mod".into(), abi: Abi::Fortran { bind_c: true } },
                Symbol { linkage: "gen_init".into(), abi: Abi::Fortran { bind_c: true } },
            ],
            imports: Vec::new(),
        };
        let rust_lib = mock.build.join("librs_gen.a");
        std::fs::write(&rust_lib, b"mock archive").unwrap();
        let cmds = composite.bridge.substitute(&cx, &bind_c_unit, &rust_lib).unwrap();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0].program, "ar");
        assert!(cmds[0].args.contains(&"libgen_rs.a".to_string()));
        assert_eq!(cmds[1].args, vec!["--build", ".", "--target", "gen"]);
        assert_eq!(cmds[1].cwd, Cwd::BuildDir);
        assert_eq!(cmds[2].program, "ctest");
        assert_eq!(cmds[2].args, vec!["--output-on-failure", "-R", "gen"]);
        // The ABI limit: non-BIND(C) units refuse one-at-a-time substitution
        // instead of linking a corrupt archive.
        let mixed_unit = UnitDecl {
            id: UnitId("fortran:src/solver.F90".into()),
            files: vec!["src/solver.F90".into()],
            generated_from: None,
            exports: vec![Symbol {
                linkage: "__solver_mod_MOD_step_solver".into(),
                abi: Abi::Fortran { bind_c: false },
            }],
            imports: Vec::new(),
        };
        let err = composite.bridge.substitute(&cx, &mixed_unit, &rust_lib).unwrap_err();
        assert!(err.to_string().contains("BIND(C)"), "unexpected: {err}");
        let err = composite
            .bridge
            .substitute(&cx, &bind_c_unit, &mock.build.join("missing.a"))
            .unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn cmake_scaffold_is_staticlib_with_linkage_names() {
        let bridge = CmakeBridge;
        let unit = UnitDecl {
            id: UnitId("fortran:src/gen.src".into()),
            files: vec!["src/gen.src".into()],
            generated_from: None,
            exports: vec![
                Symbol { linkage: "gen_mod".into(), abi: Abi::Fortran { bind_c: true } },
                Symbol { linkage: "gen_init".into(), abi: Abi::Fortran { bind_c: true } },
            ],
            imports: Vec::new(),
        };
        let scaffold = bridge.scaffold(&unit).unwrap();
        assert_eq!(scaffold.len(), 2);
        let cargo = scaffold.iter().find(|(p, _)| p == "Cargo.toml").unwrap();
        assert!(cargo.1.contains("staticlib"));
        let lib = scaffold.iter().find(|(p, _)| p == "src/lib.rs").unwrap();
        assert!(lib.1.contains("export_name = \"gen_init\""));
        assert!(lib.1.contains("fn port_gen_init"));
        // Non-BIND(C) scaffolds carry the file-granularity note.
        let mixed = UnitDecl {
            id: UnitId("fortran:src/solver.F90".into()),
            files: vec!["src/solver.F90".into()],
            generated_from: None,
            exports: vec![Symbol {
                linkage: "__solver_mod_MOD_step_solver".into(),
                abi: Abi::Fortran { bind_c: false },
            }],
            imports: Vec::new(),
        };
        let lib = bridge
            .scaffold(&mixed)
            .unwrap()
            .into_iter()
            .find(|(p, _)| p == "src/lib.rs")
            .unwrap();
        assert!(lib.1.contains("ABI shim"));
    }

    #[test]
    fn cmake_link_deps_artifacts_and_classify() {
        let mock = mock_tree();
        let bridge = CmakeBridge;
        let cx =
            BuildCtx { tree: &mock.tree, build_dir: &mock.build, release: false };
        let deps = bridge.link_deps(&cx).unwrap();
        assert!(deps.iter().any(|d| d.name == "m" && d.path.is_none()));
        assert!(deps.iter().any(|d| d.name == "mpi" && d.path.is_some()));
        assert!(deps.iter().any(|d| d.name == "umfpack"));
        let artifacts = bridge.artifacts(&cx);
        let names: Vec<String> = artifacts
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"libutil.a".to_string()));
        assert!(names.contains(&"libsolver.so".to_string()));
        let (composite, _) = select_composite(&mock.tree).unwrap();
        assert_eq!(
            composite.classify_dep(&LinkDep {
                name: "mpi".into(),
                path: Some("/usr/lib/x86_64-linux-gnu/libmpi.so".into()),
            }),
            DepClass::Keep
        );
        assert_eq!(
            composite.classify_dep(&LinkDep {
                name: "util".into(),
                path: Some(mock.build.join("libutil.a").to_string_lossy().into_owned()),
            }),
            DepClass::Port
        );
        assert_eq!(
            composite.classify_dep(&LinkDep { name: "m".into(), path: None }),
            DepClass::Keep
        );
        assert_eq!(
            composite.classify_dep(&LinkDep {
                name: "special".into(),
                path: Some("/app/third_party/lib/libspecial.a".into()),
            }),
            DepClass::Port
        );
    }

    #[test]
    fn ld_debug_maps_to_units_through_file_api() {
        let mock = mock_tree();
        let reply = mock.build.join(".cmake/api/v1/reply");
        let targets = parse_cmake_file_api_reply(&reply, &mock.tree).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].name, "solver");
        // LD_DEBUG=files transcript from the baseline run.
        let text = "  9871:\tfile=libgfortran.so.5 [0];  needed by /b/solver_test\n  9871:\tfile=/app/build/lib/libsolver.so [0];  generated\n  9871:\tfile=/app/build/lib/libutil.so.1 [0];  generated\n";
        let sos = parse_ld_debug_files(text);
        assert_eq!(sos.len(), 3);
        let units = map_loaded_objects_to_units(&sos, &targets);
        // System runtimes drop out; both solver sources and the util source map.
        assert_eq!(
            units,
            vec![
                UnitId("c:lib/util.c".into()),
                UnitId("fortran:src/multi.F90".into()),
                UnitId("fortran:src/solver.F90".into()),
            ]
        );
    }

    #[test]
    fn partition_coarsens_and_marks_out_of_scope() {
        let mock = mock_tree();
        let (composite, _) = select_composite(&mock.tree).unwrap();
        let cx =
            BuildCtx { tree: &mock.tree, build_dir: &mock.build, release: false };
        let (dag, diagnostics) = composite.partition(&cx).unwrap();
        // Nine stems: fortran singles + merged multi + c + cxx + python pair.
        assert_eq!(dag.units.len(), 9);
        assert!(diagnostics.iter().any(|d| d
            .contains("coarsen: 'fortran:src/multi.F90' merges 2 Fortran units")));
        assert!(diagnostics.iter().any(|d| d
            .contains("coarsen: 'fortran:src/solver.F90' is non-BIND(C) Fortran")));
        assert!(diagnostics.iter().any(|d| d
            .contains("out-of-scope: 'fortran:third_party/vendored.f90' is vendored")));
        assert!(diagnostics.iter().any(|d| d
            .contains("out-of-scope: 'cxx:app/main.cpp' has no baseline coverage")));
        assert!(!diagnostics
            .iter()
            .any(|d| d.contains("gen.src' has no baseline coverage")));
        // The merged unit keeps the cross-file USE edge to the generated unit.
        assert!(dag.edges.iter().any(|(_, b)| b == "gen_mod"));
        let order = dag.leaf_first_order().unwrap();
        assert!(dag.verify_order(&order));
    }

    #[test]
    fn ctest_heldout_and_test_inventory() {
        let mock = mock_tree();
        let cx =
            BuildCtx { tree: &mock.tree, build_dir: &mock.build, release: false };
        let cmds = CtestRunner.heldout(Path::new("tests/solver_unit"), &cx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].program, "ctest");
        assert_eq!(cmds[0].args, vec!["--output-on-failure", "-R", "solver_unit"]);
        assert_eq!(cmds[0].cwd, Cwd::BuildDir);
        let (composite, _) = select_composite(&mock.tree).unwrap();
        let inventory = composite.test_inventory(&mock.tree).unwrap();
        assert!(inventory.test_files.iter().any(|f| f.ends_with("CTestTestfile.cmake")));
        assert!(inventory.config_refs.iter().any(|f| f.ends_with("CMakeLists.txt")));
        assert_eq!(inventory.fixture_globs, vec!["**/CTestTestfile*.cmake"]);
    }

    #[test]
    fn perf_profiler_shapes_and_honest_hotspots() {
        let mock = mock_tree();
        let profiler = PerfProfiler { tree: mock.tree.clone() };
        let empty = Workload {
            name: "w".into(),
            setup: String::new(),
            stmt: "exit 0".into(),
            iters: 1,
        };
        assert!(profiler.setup(&empty).is_empty());
        let loaded = Workload {
            name: "w".into(),
            setup: "echo warm".into(),
            stmt: "exit 0".into(),
            iters: 3,
        };
        let setup = profiler.setup(&loaded);
        assert_eq!(setup.len(), 1);
        assert_eq!(setup[0].program, "sh");
        let timed = profiler.timed(&loaded);
        assert_eq!(timed.program, "sh");
        assert_eq!(timed.args, vec!["-c".to_string(), "exit 0".to_string()]);
        // perf unavailable: honest error, never fabricated hotspots.
        assert!(profiler.hotspots(&loaded, false).is_err());
        // Claimed-available: either real perf data or an honest error when
        // the binary cannot run here (blocked perf events, missing binary).
        match profiler.hotspots(&loaded, true) {
            Ok(baseline) => assert_eq!(baseline.tool, "perf"),
            Err(_) => {}
        }
    }

    #[test]
    fn frontend_rules_are_abi_only() {
        assert_eq!(FortranFrontend.language_rules().len(), 4);
        assert_eq!(CxxFrontend.language_rules().len(), 3);
        assert!(CxxFrontend
            .language_rules()
            .iter()
            .any(|r| r.id == "cxx.compile-db-origin"));
    }

    #[test]
    fn composite_license_terms_report_per_file() {
        let mock = mock_tree();
        let (composite, _) = select_composite(&mock.tree).unwrap();
        let terms = composite.license_terms(&mock.tree).unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].0, "LICENSE");
        assert_eq!(terms[0].1.license, "MIT");
    }

    #[test]
    fn scan_floats_and_anchors() {
        assert_eq!(literal_anchors("norm\\s*=\\s*([0-9.eE+-]+)"), vec!["norm".to_string()]);
        let floats = scan_floats("x = -1.5e-3 y");
        assert_eq!(floats.len(), 1);
        assert!((floats[0].0 - -0.0015).abs() < 1e-12);
        assert_eq!(floats[0].1, "-1.5e-3");
    }
}
