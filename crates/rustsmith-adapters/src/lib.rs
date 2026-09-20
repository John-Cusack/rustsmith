use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(String),
}

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

impl Adapter for PythonAdapter {
    fn language(&self) -> &'static str {
        "python"
    }

    fn detect(&self, repo: &Path) -> Result<BuildInfo, AdapterError> {
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

    fn call_graph(&self, repo: &Path) -> Result<CallGraph, AdapterError> {
        python_call_graph(repo)
    }

    fn test_inventory(&self, repo: &Path) -> Result<TestInventory, AdapterError> {
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

    fn classify_dep(&self, dep: &str) -> DepClass {
        // stdlib -> keep; third-party test-only -> keep; runtime third-party -> port/bind.
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

    fn license_terms(&self, repo: &Path) -> Result<Attribution, AdapterError> {
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
                return Ok(Attribution {
                    license,
                    header_text: header,
                    notice_extra: String::new(),
                });
            }
        }
        // Fallback: pyproject license field.
        let pp = repo.join("pyproject.toml");
        if pp.exists() {
            let t = std::fs::read_to_string(&pp)?;
            for line in t.lines() {
                if line.contains("license") && line.contains("BSD") {
                    return Ok(Attribution {
                        license: "BSD-2-Clause".into(),
                        header_text: String::new(),
                        notice_extra: String::new(),
                    });
                }
                if line.contains("license") && line.contains("MIT") {
                    return Ok(Attribution {
                        license: "MIT".into(),
                        header_text: String::new(),
                        notice_extra: String::new(),
                    });
                }
            }
        }
        Ok(Attribution {
            license: "unknown".into(),
            header_text: String::new(),
            notice_extra: String::new(),
        })
    }
}

/// Deterministic import graph via Python stdlib `ast` (no new Rust deps).
/// Module names are file stems for flat packages and `pkg.stem` for src-layout;
/// edges use stems so strsimpy's `from .shingle_based import` resolves.
fn python_call_graph(repo: &Path) -> Result<CallGraph, AdapterError> {
    // Collect candidate source files (exclude tests, bench, docs).
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
            // Skip test files themselves + repo-root tooling (invoke tasks, setup).
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("test_") || name.ends_with("_test.py") || name == "conftest.py" {
                continue;
            }
            if name == "tasks.py" || name == "setup.py" || name == "noxfile.py" {
                continue;
            }
            sources.push(p.to_path_buf());
        }
    }
    // Ask Python's ast for imports of each file (accurate, stdlib-only).
    let mut modules: HashMap<String, String> = HashMap::new();
    let mut file_imports: HashMap<String, Vec<String>> = HashMap::new();
    for src in &sources {
        let rel = src.strip_prefix(repo).unwrap().to_string_lossy().replace('\\', "/");
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if stem == "__init__" {
            continue;
        }
        modules.insert(stem.clone(), rel.clone());
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
        let mods: Vec<String> = serde_json::from_slice(&out.stdout).unwrap_or_default();
        file_imports.insert(stem, mods);
    }
    let stems: HashSet<String> = modules.keys().cloned().collect();
    let mut edges = Vec::new();
    for (stem, imports) in &file_imports {
        let mut seen = HashSet::new();
        for imp in imports {
            // Resolve: last dotted segment for `from strsimpy.x import` style too.
            let base = imp.rsplit('.').next().unwrap_or(imp);
            if stems.contains(base) && base != stem && seen.insert(base.to_string()) {
                edges.push((stem.clone(), base.to_string()));
            }
        }
    }
    edges.sort();
    edges.dedup();
    Ok(CallGraph { modules, edges })
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
}
