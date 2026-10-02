//! Release support for rustsmith converted projects.
//!
//! A converted project ships two distributions from one implementation: a
//! reusable Rust core crate (crates.io) plus a Python package calling that
//! same core (PyPI). This crate owns the project-specific release
//! configuration (`release.toml`), its validation against the project tree,
//! per-registry publication state, and the generated release workflow plus
//! trusted-publisher setup instructions.
//!
//! Toolchain work (cargo/maturin/pip/venv) lives in the CLI wiring
//! (`rustsmith-cli/src/release.rs`); everything here is pure and unit-tested.
//! This crate never touches agents, council seats, gates, or the oracle.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Registries tracked per release. `testpypi` is the verified staging step;
/// `pypi` and `crates-io` are the production registries.
pub const KNOWN_REGISTRIES: &[&str] = &["testpypi", "pypi", "crates-io"];

/// SPDX identifiers accepted without further review. Anything else is refused
/// (a wrong license tag on a published port is a legal problem, not a typo).
pub const KNOWN_SPDX: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "MPL-2.0",
    "GPL-3.0-only",
    "LGPL-3.0-only",
];

/// Project-specific release configuration (parsed from `release.toml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseConfig {
    pub project: String,
    pub rust_crate: String,
    pub pypi_dist: String,
    pub python_import: String,
    pub version: String,
    pub requires_python: String,
    pub python_versions: Vec<String>,
    pub platforms: Vec<String>,
    pub github_repo: String,
    pub release_workflow: String,
    pub pypi_environment: String,
    pub testpypi_environment: String,
    pub license_spdx: String,
    pub upstream: String,
    pub upstream_url: String,
    pub upstream_license: String,
    pub upstream_authors: String,
    pub smoke_exprs: Vec<String>,
}

/// Load `release.toml` from a project directory (e.g. `mirror/crc`).
pub fn load_release_config(project_dir: &Path) -> Result<ReleaseConfig, String> {
    let path = project_dir.join("release.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("{}: cannot read release.toml: {e}", project_dir.display()))?;
    parse_release_config(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse_release_config(text: &str) -> Result<ReleaseConfig, String> {
    let v: toml::Value = text.parse().map_err(|e| format!("bad release.toml: {e}"))?;
    let t = v
        .get("release")
        .ok_or("bad release.toml: missing [release] table")?;
    let str_field = |k: &str| -> Result<String, String> {
        t.get(k)
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("bad release.toml: [release] lacks string field `{k}`"))
    };
    let list_field = |k: &str| -> Result<Vec<String>, String> {
        t.get(k)
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("bad release.toml: [release] lacks list field `{k}`"))?
            .iter()
            .map(|x| {
                x.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| format!("bad release.toml: [release] `{k}` must be a string list"))
            })
            .collect()
    };
    Ok(ReleaseConfig {
        project: str_field("project")?,
        rust_crate: str_field("rust_crate")?,
        pypi_dist: str_field("pypi_dist")?,
        python_import: str_field("python_import")?,
        version: str_field("version")?,
        requires_python: str_field("requires_python")?,
        python_versions: list_field("python_versions")?,
        platforms: list_field("platforms")?,
        github_repo: str_field("github_repo")?,
        release_workflow: str_field("release_workflow")?,
        pypi_environment: str_field("pypi_environment")?,
        testpypi_environment: str_field("testpypi_environment")?,
        license_spdx: str_field("license_spdx")?,
        upstream: str_field("upstream")?,
        upstream_url: str_field("upstream_url")?,
        upstream_license: str_field("upstream_license")?,
        upstream_authors: str_field("upstream_authors")?,
        smoke_exprs: list_field("smoke_exprs")?,
    })
}

fn is_dist_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn is_module_name(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// PyPI normalization (PEP 503): runs of `-_.` become `-`, all lowercase.
pub fn normalize_dist(name: &str) -> String {
    let mut out = String::new();
    let mut sep = false;
    for c in name.chars() {
        if c == '-' || c == '_' || c == '.' {
            if !out.is_empty() {
                sep = true;
            }
        } else {
            if sep {
                out.push('-');
                sep = false;
            }
            out.extend(c.to_lowercase());
        }
    }
    out
}

fn is_version(s: &str) -> bool {
    let (nums, suffix) = match s.split_once(|c| c == '-' || c == '+') {
        Some((n, _)) => (n, true),
        None => (s, false),
    };
    let _ = suffix;
    let parts: Vec<&str> = nums.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn is_repo(s: &str) -> bool {
    let parts: Vec<&str> = s.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        })
}

/// Lower bound of a `requires-python` spec (`>=3.9`, `>=3.9,<4.0`, ...).
fn requires_floor(spec: &str) -> Option<(u64, u64)> {
    let ge = spec.split(',').next()?.trim().strip_prefix(">=")?;
    let nums: Vec<&str> = ge.trim().split('.').collect();
    if nums.len() < 2 {
        return None;
    }
    Some((nums[0].parse().ok()?, nums[1].parse().ok()?))
}

fn py_version(s: &str) -> Option<(u64, u64)> {
    let nums: Vec<&str> = s.trim().split('.').collect();
    if nums.len() != 2 {
        return None;
    }
    Some((nums[0].parse().ok()?, nums[1].parse().ok()?))
}

/// Validate the configuration values against each other (no tree access).
pub fn validate_names(cfg: &ReleaseConfig) -> Result<(), String> {
    if !is_module_name(&cfg.project) {
        return Err(format!("release.toml: `project` {:?} is not a Python module name", cfg.project));
    }
    if !is_dist_name(&cfg.rust_crate) {
        return Err(format!("release.toml: `rust_crate` {:?} is not a valid crate name", cfg.rust_crate));
    }
    if !is_dist_name(&cfg.pypi_dist) {
        return Err(format!("release.toml: `pypi_dist` {:?} is not a valid distribution name", cfg.pypi_dist));
    }
    if normalize_dist(&cfg.pypi_dist) == normalize_dist(&cfg.project) {
        return Err(format!(
            "release.toml: `pypi_dist` {:?} collides with the original package {:?} (the converted dist must be named `<orig>-rust`, never the upstream name)",
            cfg.pypi_dist, cfg.project
        ));
    }
    if cfg.python_import != cfg.project {
        return Err(format!(
            "release.toml: `python_import` {:?} must equal `project` {:?} (drop-in: users swap the install with zero import changes)",
            cfg.python_import, cfg.project
        ));
    }
    if !is_version(&cfg.version) {
        return Err(format!(
            "release.toml: `version` {:?} is not X.Y.Z (suffixes like `-rc.1` allowed)",
            cfg.version
        ));
    }
    if !KNOWN_SPDX.contains(&cfg.license_spdx.as_str()) {
        return Err(format!(
            "release.toml: `license_spdx` {:?} unknown (known: {})",
            cfg.license_spdx,
            KNOWN_SPDX.join(", ")
        ));
    }
    if !is_repo(&cfg.github_repo) {
        return Err(format!(
            "release.toml: `github_repo` {:?} is not `owner/name`",
            cfg.github_repo
        ));
    }
    if cfg.platforms.is_empty() {
        return Err("release.toml: `platforms` is empty (at least one runner required)".into());
    }
    if cfg.python_versions.is_empty() {
        return Err("release.toml: `python_versions` is empty".into());
    }
    let floor = requires_floor(&cfg.requires_python).ok_or_else(|| {
        format!("release.toml: `requires_python` {:?} needs a `>=X.Y` lower bound", cfg.requires_python)
    })?;
    for pv in &cfg.python_versions {
        let v = py_version(pv).ok_or_else(|| {
            format!("release.toml: `python_versions` entry {pv:?} is not X.Y")
        })?;
        if v < floor {
            return Err(format!(
                "release.toml: Python {pv} is below `requires_python` {:?}",
                cfg.requires_python
            ));
        }
    }
    if cfg.smoke_exprs.is_empty() {
        return Err("release.toml: `smoke_exprs` is empty (at least one installed-package check required)".into());
    }
    Ok(())
}

// --- line-oriented manifest readers (same style as the CLI packaging reads) ---

fn section_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut cur = String::new();
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') && l.ends_with(']') {
            cur = l[1..l.len() - 1].split_whitespace().next().unwrap_or("").into();
            continue;
        }
        if cur == section {
            if let Some(v) = l.strip_prefix(key) {
                let v = v.trim().strip_prefix('=').unwrap_or(v.trim()).trim();
                let v = v.split('#').next().unwrap_or("").trim();
                return Some(v.trim_matches('"').trim_matches('\'').to_string());
            }
        }
    }
    None
}

fn section_contains(text: &str, section: &str, key: &str, want: &str) -> bool {
    let mut cur = String::new();
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') && l.ends_with(']') {
            cur = l[1..l.len() - 1].split_whitespace().next().unwrap_or("").into();
            continue;
        }
        if cur == section && l.starts_with(key) && l.contains(want) {
            return true;
        }
    }
    false
}

/// Validate the configuration against the project tree (manifests agree,
/// shims and attribution exist, the Rust core is independent of Python).
pub fn validate_tree(cfg: &ReleaseConfig, project_dir: &Path) -> Result<(), String> {
    // Root binding crate: version matches, cdylib present, path-dep on the core.
    let root_cargo = std::fs::read_to_string(project_dir.join("Cargo.toml"))
        .map_err(|e| format!("{}: cannot read Cargo.toml: {e}", project_dir.display()))?;
    let root_version = section_value(&root_cargo, "package", "version")
        .ok_or_else(|| format!("{}: Cargo.toml lacks [package] version", project_dir.display()))?;
    if root_version != cfg.version {
        return Err(format!(
            "{}: Cargo.toml version {root_version:?} != release.toml version {:?}",
            project_dir.display(),
            cfg.version
        ));
    }
    if !section_contains(&root_cargo, "lib", "crate-type", "cdylib") {
        return Err(format!(
            "{}: Cargo.toml [lib] crate-type lacks cdylib (binding must build the `python_import` extension)",
            project_dir.display()
        ));
    }
    let lib_name = section_value(&root_cargo, "lib", "name")
        .ok_or_else(|| format!("{}: Cargo.toml lacks [lib] name", project_dir.display()))?;
    // The core crate: found through a path dependency, name+version match,
    // rlib shape, and no Python dependency (the independence prerequisite).
    let core_dir = find_core_dir(&root_cargo, project_dir, &cfg.rust_crate).ok_or_else(|| {
        format!(
            "{}: Cargo.toml has no path dependency resolving to crate {:?} (binding must call the same core Rust consumers use)",
            project_dir.display(),
            cfg.rust_crate
        )
    })?;
    let core_cargo = std::fs::read_to_string(core_dir.join("Cargo.toml"))
        .map_err(|e| format!("{}: cannot read core Cargo.toml: {e}", core_dir.display()))?;
    let core_version = section_value(&core_cargo, "package", "version").unwrap_or_default();
    if core_version != cfg.version {
        return Err(format!(
            "{}: core version {core_version:?} != release.toml version {:?}",
            core_dir.display(),
            cfg.version
        ));
    }
    if section_contains(&core_cargo, "lib", "crate-type", "cdylib") {
        return Err(format!(
            "{}: core crate must not be cdylib (the reusable Rust core builds as a plain library, not an extension)",
            core_dir.display()
        ));
    }
    for py_dep in ["pyo3", "pyo3-ffi", "python3-sys", "pyo3-asyncio"] {
        if core_cargo.contains(py_dep) {
            return Err(format!(
                "{}: core crate depends on {py_dep} (the Rust core must be independent of Python; move bindings to the extension crate)",
                core_dir.display()
            ));
        }
    }
    if !core_dir.join("src/lib.rs").is_file() {
        return Err(format!("{}: core crate lacks src/lib.rs", core_dir.display()));
    }
    // Python packaging: names/version/floor/module all agree with release.toml.
    let pyproject = std::fs::read_to_string(project_dir.join("pyproject.toml"))
        .map_err(|e| format!("{}: cannot read pyproject.toml: {e}", project_dir.display()))?;
    let dist = section_value(&pyproject, "project", "name")
        .ok_or_else(|| format!("{}: pyproject.toml lacks [project] name", project_dir.display()))?;
    if dist != cfg.pypi_dist {
        return Err(format!(
            "{}: pyproject [project] name {dist:?} != release.toml pypi_dist {:?}",
            project_dir.display(),
            cfg.pypi_dist
        ));
    }
    let py_version = section_value(&pyproject, "project", "version").unwrap_or_default();
    if py_version != cfg.version {
        return Err(format!(
            "{}: pyproject version {py_version:?} != release.toml version {:?}",
            project_dir.display(),
            cfg.version
        ));
    }
    let floor = section_value(&pyproject, "project", "requires-python").unwrap_or_default();
    if floor != cfg.requires_python {
        return Err(format!(
            "{}: pyproject requires-python {floor:?} != release.toml {:?}",
            project_dir.display(),
            cfg.requires_python
        ));
    }
    if section_value(&pyproject, "project", "license").is_none()
        && !pyproject.contains("license")
    {
        return Err(format!("{}: pyproject.toml lacks a license field", project_dir.display()));
    }
    let readme = section_value(&pyproject, "project", "readme").unwrap_or_default();
    if !readme.is_empty() && !project_dir.join(&readme).is_file() {
        return Err(format!(
            "{}: pyproject readme {readme:?} is missing from the tree",
            project_dir.display()
        ));
    }
    let module = section_value(&pyproject, "tool.maturin", "module-name").unwrap_or_default();
    let want_prefix = format!("{}.", cfg.python_import);
    if !module.starts_with(&want_prefix) {
        return Err(format!(
            "{}: [tool.maturin] module-name {module:?} must live under the import package {:?} (drop-in keeps the original import path)",
            project_dir.display(),
            cfg.python_import
        ));
    }
    let suffix = module.rsplit('.').next().unwrap_or("").replace('-', "_");
    if suffix != lib_name {
        return Err(format!(
            "{}: [tool.maturin] module-name {module:?} suffix does not match [lib] name {lib_name:?} (maturin builds the extension from that lib target)",
            project_dir.display()
        ));
    }
    if !project_dir.join(&cfg.python_import).join("__init__.py").is_file() {
        return Err(format!(
            "{}: missing `{}/__init__.py` (the import package must exist with the original name)",
            project_dir.display(),
            cfg.python_import
        ));
    }
    let notice = std::fs::read_to_string(project_dir.join("NOTICE")).map_err(|_| {
        format!(
            "{}: missing NOTICE (upstream attribution must ship: {} {})",
            project_dir.display(),
            cfg.upstream,
            cfg.license_spdx
        )
    })?;
    if !notice.contains(&cfg.upstream) {
        return Err(format!(
            "{}: NOTICE lacks upstream attribution {:?}",
            project_dir.display(),
            cfg.upstream
        ));
    }
    if !notice.contains(&cfg.license_spdx) && !notice.contains(&cfg.upstream_license) {
        return Err(format!(
            "{}: NOTICE lacks the license tag ({} / {})",
            project_dir.display(),
            cfg.license_spdx,
            cfg.upstream_license
        ));
    }
    Ok(())
}

/// Resolve the core crate directory: a `[dependencies]` path entry whose
/// target manifest names `rust_crate`. Handles both inline tables
/// (`name = { path = "dir", ... }`) and plain tables (`[dependencies.name]`
/// with a `path = "dir"` line). Never guesses beyond the manifests.
fn find_core_dir(root_cargo: &str, project_dir: &Path, rust_crate: &str) -> Option<std::path::PathBuf> {
    let mut in_deps = false;
    for line in root_cargo.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_deps = l == "[dependencies]" || l.starts_with("[dependencies.");
            if let Some(rel) = path_value(l) {
                let dir = project_dir.join(rel);
                if manifest_names(&dir, rust_crate) {
                    return Some(dir);
                }
            }
            continue;
        }
        if in_deps {
            if let Some(rel) = path_value(l) {
                let dir = project_dir.join(rel);
                if manifest_names(&dir, rust_crate) {
                    return Some(dir);
                }
            }
        }
    }
    None
}

/// `path = "dir"` value inside a dependency line or table opener.
fn path_value(line: &str) -> Option<String> {
    let i = line.find("path")?;
    let (head, tail) = (line[..i].to_string(), &line[i + 4..]);
    // `path` must be a key, not part of another word (`xpath`, `"path"`).
    if let Some(c) = head.chars().last() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '"' || c == '\'' {
            return None;
        }
    }
    let v = tail.trim().strip_prefix('=')?.trim();
    let q = v.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    Some(v[1..].split(q).next().unwrap_or("").to_string())
}

/// True when `dir/Cargo.toml` names `rust_crate` under `[package]`.
fn manifest_names(dir: &Path, rust_crate: &str) -> bool {
    std::fs::read_to_string(dir.join("Cargo.toml"))
        .ok()
        .and_then(|m| section_value(&m, "package", "name"))
        .as_deref()
        == Some(rust_crate)
}

/// SHA-256 of a file, hex-encoded (artifact identity in the release manifest).
pub fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::Digest;
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = sha2::Sha256::new();
    h.update(&bytes);
    Ok(hex::encode(h.finalize()))
}

/// Touched files from a unified diff (`+++ b/<path>` lines, sorted, deduped).
/// Used for the `files_touched` record so Stage-2 winners spanning the core
/// and binding crates report exactly what they changed.
pub fn files_of_patch(patch: &str) -> Vec<String> {
    let mut out: Vec<String> = patch
        .lines()
        .filter_map(|l| l.strip_prefix("+++ b/"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "/dev/null")
        .collect();
    out.sort();
    out.dedup();
    out
}

// --- publication state ---

/// One registry attempt (append-only history; retries add rows).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub at: i64,
    pub result: String,
    pub detail: String,
}

/// Per-registry outcome: `pending` | `success` | `failed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryState {
    pub status: String,
    #[serde(default)]
    pub attempts: Vec<Attempt>,
}

/// Release state: artifact identities plus one outcome per registry.
/// `complete` is reported only when every registry reads `success`; a retry
/// after a partial publish flips exactly the registry it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseState {
    pub release_version: String,
    pub source_sha: String,
    #[serde(default)]
    pub artifacts: Vec<ArtifactRef>,
    pub registries: BTreeMap<String, RegistryState>,
}

/// Released artifact identity (file name + SHA-256 + size).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub file: String,
    pub sha256: String,
    pub bytes: u64,
}

pub fn initial_state(version: &str, source_sha: &str) -> ReleaseState {
    let mut registries = BTreeMap::new();
    for r in KNOWN_REGISTRIES {
        registries.insert(
            r.to_string(),
            RegistryState { status: "pending".into(), attempts: vec![] },
        );
    }
    ReleaseState {
        release_version: version.into(),
        source_sha: source_sha.into(),
        artifacts: vec![],
        registries,
    }
}

/// Record one registry outcome. Unknown registries and results are refused;
/// recording never touches the other registries (partial completion stays
/// visible until every registry succeeds).
pub fn record_outcome(
    state: &mut ReleaseState,
    registry: &str,
    result: &str,
    detail: &str,
    now: i64,
) -> Result<(), String> {
    if !KNOWN_REGISTRIES.contains(&registry) {
        return Err(format!("unknown registry {registry:?} (known: {})", KNOWN_REGISTRIES.join(", ")));
    }
    if result != "success" && result != "failed" {
        return Err(format!("unknown result {result:?} (success|failed)"));
    }
    let entry = state.registries.get_mut(registry).expect("known registry");
    entry.status = result.into();
    entry.attempts.push(Attempt { at: now, result: result.into(), detail: detail.into() });
    Ok(())
}

/// Overall status: `complete` only when every registry succeeded.
/// `pending` when nothing was attempted, `partial` once any registry
/// succeeded or failed without completing, `failed` when attempts exist
/// but nothing succeeded yet.
pub fn overall_status(state: &ReleaseState) -> &'static str {
    let mut success = 0;
    let mut attempted = 0;
    for r in KNOWN_REGISTRIES {
        match state.registries.get(*r).map(|s| s.status.as_str()) {
            Some("success") => {
                success += 1;
                attempted += 1;
            }
            Some("failed") => attempted += 1,
            _ => {}
        }
    }
    if success == KNOWN_REGISTRIES.len() {
        "complete"
    } else if attempted == 0 {
        "pending"
    } else if success == 0 {
        "failed"
    } else {
        "partial"
    }
}

// --- generated release workflow + setup instructions ---

/// GitHub Actions release workflow for the converted project.
///
/// Shape (all values from `release.toml`, reusable across projects):
/// build+test the configured platform artifacts -> publish the SAME Python
/// artifacts to TestPyPI -> fresh install check from TestPyPI -> manual
/// `workflow_dispatch` release job for PyPI (trusted publishing) plus the
/// exact `cargo publish` step for the Rust core on crates.io.
pub fn render_workflow(cfg: &ReleaseConfig) -> String {
    let py_list = cfg.python_versions.iter().map(|v| format!("'{v}'")).collect::<Vec<_>>().join(", ");
    let os_list = cfg.platforms.iter().map(|v| format!("'{v}'")).collect::<Vec<_>>().join(", ");
    let smoke: String = cfg
        .smoke_exprs
        .iter()
        .map(|e| format!("          python -c \"import {imp}; assert {e}, {e:?}\"", imp = cfg.python_import, e = e))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"# Release workflow for {dist} (generated by `rustsmith release-prep`).
#
# DO NOT hand-edit here: regenerate from `release.toml` so the workflow stays
# in sync with the release configuration. Publishing order is fixed:
# TestPyPI first (automatic), production PyPI + crates.io only by manual
# dispatch after the TestPyPI install check is green.
#
# Trust model: OpenID Connect trusted publishing on both registries. No
# long-lived tokens live in secrets. First-time setup (accounts, publisher
# identities, environments) is in TRUSTED_PUBLISHING_SETUP.md next to this file.

name: release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:
    inputs:
      publish_production:
        description: "Publish verified artifacts to PyPI and the Rust core to crates.io"
        required: true
        default: "false"

permissions:
  contents: read

jobs:
  build:
    name: build wheels (${{{{ matrix.os }}}} py${{{{ matrix.python-version }}}})
    runs-on: ${{{{ matrix.os }}}}
    strategy:
      fail-fast: true
      matrix:
        os: [{os_list}]
        python-version: [{py_list}]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: ${{{{ matrix.python-version }}}}
      - uses: actions/setup-rust@v1
        with:
          rust-version: stable
      - name: build wheel
        uses: PyO3/maturin-action@v1
        with:
          command: build
          args: --release --out dist
      - name: core tests (same core Rust consumers use)
        run: cargo test --release --manifest-path crc-core/Cargo.toml
        if: matrix.os == 'ubuntu-22.04' && matrix.python-version == '3.12'
      - name: upload wheel (exact file the publish jobs reuse)
        uses: actions/upload-artifact@v4
        with:
          name: wheels-${{{{ matrix.os }}}}-py${{{{ matrix.python-version }}}}
          path: dist/*.whl
          if-no-files-found: error

  sdist:
    name: build sdist (includes the Rust core)
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - name: build sdist
        uses: PyO3/maturin-action@v1
        with:
          command: sdist
          args: --out dist
      - name: sdist carries the core (rebuild proof runs in release-prep)
        run: tar tzf dist/*.tar.gz | grep -q 'crc-core/Cargo.toml'
      - uses: actions/upload-artifact@v4
        with:
          name: sdist
          path: dist/*.tar.gz
          if-no-files-found: error

  test-wheels:
    name: test wheel (fresh install, no network build)
    needs: [build]
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/download-artifact@v4
        with:
          pattern: wheels-*
          merge-multiple: true
          path: dist
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - name: install one wheel from disk (same file that ships)
        run: pip install dist/*.whl
      - name: smoke (installed package, pinned vectors)
{smoke}
      - name: upload tested wheels (publish consumes THESE files)
        uses: actions/upload-artifact@v4
        with:
          name: tested-dist
          path: dist/*

  publish-testpypi:
    name: publish to TestPyPI
    needs: [test-wheels, sdist]
    runs-on: ubuntu-22.04
    environment: {testpypi_env}
    permissions:
      id-token: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: tested-dist
          path: dist
      - uses: actions/download-artifact@v4
        with:
          name: sdist
          path: dist
      - name: publish the tested artifacts to TestPyPI (no rebuild)
        uses: pypa/gh-action-pypi-publish@release/v1
        with:
          repository-url: https://test.pypi.org/legacy/
          packages-dir: dist/
          verify-metadata: true
          skip-existing: true

  verify-testpypi:
    name: verify install from TestPyPI
    needs: [publish-testpypi]
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - name: install from TestPyPI (proves the index, not the local file)
        run: pip install --index-url https://test.pypi.org/simple/ --extra-index-url https://pypi.org/simple/ {dist}
      - name: smoke (index-installed package)
{smoke}

  release:
    name: production release (PyPI + crates.io, manual only)
    needs: [verify-testpypi]
    if: github.event_name == 'workflow_dispatch' && inputs.publish_production == 'true'
    runs-on: ubuntu-22.04
    environment: {pypi_env}
    permissions:
      id-token: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: tested-dist
          path: dist
      - uses: actions/download-artifact@v4
        with:
          name: sdist
          path: dist
      - name: publish the SAME tested artifacts to PyPI (no rebuild)
        uses: pypa/gh-action-pypi-publish@release/v1
        with:
          packages-dir: dist/
          verify-metadata: true
          skip-existing: false
      - uses: actions/setup-rust@v1
        with:
          rust-version: stable
      - name: publish the Rust core to crates.io
        # Trusted publishing for crates.io (or `cargo login` as fallback, see
        # TRUSTED_PUBLISHING_SETUP.md). Core first: the binding depends on it.
        run: cargo publish --manifest-path crc-core/Cargo.toml
      - name: record outcomes per registry
        # Manual fallback uses the same state file `release-prep` seeds:
        #   rustsmith release-record --state dist/release-state.json \
        #     --registry pypi --result success --detail "workflow run <id>"
        # Retry one registry at a time; `release-status` reports complete only
        # when testpypi, pypi, AND crates-io all read success.
        run: echo "record each registry outcome with release-record (see setup doc)"
"#,
        dist = cfg.pypi_dist,
        os_list = os_list,
        py_list = py_list,
        smoke = smoke,
        testpypi_env = cfg.testpypi_environment,
        pypi_env = cfg.pypi_environment,
    )
}

/// Exact first-time publisher setup for the converted project (trusted
/// publishing on both registries; no tokens unless noted as fallback).
pub fn render_setup_instructions(cfg: &ReleaseConfig) -> String {
    format!(
        r#"# Trusted publisher setup for {dist} / {core} (generated, project-specific)
#
# Generated by `rustsmith release-prep` from `release.toml`. Follow once, in
# order. Nothing here uploads anything; the workflow does that after setup.

## 0. Prerequisites

- A GitHub account owning (or able to create) `{repo}`.
- A PyPI account (production) and a TestPyPI account
  (https://test.pypi.org/account/register/).
- A crates.io account with a verified email.
- The prepared tree from `release-prep --out <dist>` (wheels, sdist,
  `release.yml`, this file). Verify it locally first:
  `rustsmith release-status --state <dist>/release-state.json` reads
  `pending` before anything is published.

## 1. Create the publishing repository

```sh
# In the prepared tree (NOT in the rustsmith checkout):
git init -b main
git add -A
git commit -m "release {dist} {version} (rustsmith-prepared)"
gh repo create {repo} --public --source . --push
mkdir -p .github/workflows
cp <dist>/.github/workflows/release.yml .github/workflows/release.yml
git add .github/workflows/release.yml && git commit -m "add release workflow" && git push
```

`release.toml` pins this identity: repo `{repo}`, workflow
`{workflow}`. Renaming either means re-registering the publishers below.

## 2. Check the names are free (before registering anything)

```sh
cargo search {core}          # expect: no existing crate with this exact name
pip index versions {dist}    # expect: no such distribution (PyPI)
pip index versions --index-url https://test.pypi.org/simple/ {dist}
```

If a name is taken, change it in `release.toml` (+ manifests) and re-run
`release-prep`. Never publish under a squatted or confusingly close name.

## 3. TestPyPI trusted publisher (automatic lane)

PyPI project page for `{dist}` does not exist yet: trusted publishing for a
pending project is registered on TestPyPI under account settings, not under
a project:

1. Log in to https://test.pypi.org/manage/account/publishing/.
2. "Add a new pending publisher" with exactly:
   - PyPI project name: `{dist}`
   - Owner: `{owner}`
   - Repository name: `{reponame}`
   - Workflow name: `{workflow_file}`
   - Environment name: `{testpypi_env}`
3. Push a tag (`git tag v{version} && git push --tags`) or run the workflow
   manually with `publish_production: false`. The `publish-testpypi` job
   publishes, `verify-testpypi` installs from the index and runs the smoke
   vectors. Merging nothing until that job is green.

## 4. PyPI trusted publisher (production lane, manual only)

1. Log in to https://pypi.org/manage/account/publishing/.
2. Same pending-publisher form with environment `{pypi_env}` (project
   `{dist}`, owner `{owner}`, repo `{reponame}`, workflow `{workflow_file}`).
3. In the GitHub repo: Settings -> Environments -> New environment named
   `{pypi_env}` with required reviewers (at least one human) and no
   deployment branches restriction beyond tags.
4. Dispatch: Actions -> release -> Run workflow -> `publish_production: true`.
   The `release` job publishes the SAME `tested-dist` artifacts (downloaded,
   never rebuilt) to PyPI.

## 5. crates.io (Rust core `{core}`, manual)

1. `cargo login` is the fallback only. Prefer trusted publishing:
   crates.io -> Account Settings -> Trusted Publishing -> add GitHub
   (`{repo}`, workflow `{workflow_file}`, environment `{pypi_env}`).
2. The `release` job runs `cargo publish --manifest-path crc-core/Cargo.toml`.
   If trusted publishing is unavailable, run locally instead:
   ```sh
   cargo publish --manifest-path crc-core/Cargo.toml
   ```
   Publish the CORE first (the binding depends on it by version).

## 6. Record outcomes per registry (partial completion is explicit)

`release-prep` seeds `<dist>/release-state.json` with all three registries
`pending`. After each lane, record exactly that registry:

```sh
rustsmith release-record --state <dist>/release-state.json \
  --registry testpypi --result success --detail "workflow run <id>"
rustsmith release-record --state <dist>/release-state.json \
  --registry pypi --result success --detail "workflow run <id>"
rustsmith release-record --state <dist>/release-state.json \
  --registry crates-io --result success --detail "crates.io release page"
rustsmith release-status --state <dist>/release-state.json
```

`release-status` exits 0 and prints `complete` ONLY when testpypi, pypi,
AND crates-io all read `success`. One success out of three prints `partial`
(exit 1): retry the remaining registries, never re-report the finished ones.
A failure prints `failed`/`partial` with the attempt history; re-running the
same `--registry` after fixing the cause appends a new attempt (retries are
per-registry, never "publish everything again").

## 7. License and attribution check (every release)

- `NOTICE` names upstream `{upstream}` ({upstream_url}) and the
  `{license}` terms; every source file carries the SPDX tag.
- `cargo package --list --manifest-path crc-core/Cargo.toml` shows NOTICE
  (or the license file) in the packaged files.
- The PyPI project page renders README.md with the upstream link intact.
"#,
        dist = cfg.pypi_dist,
        core = cfg.rust_crate,
        version = cfg.version,
        repo = cfg.github_repo,
        owner = cfg.github_repo.split('/').next().unwrap_or("?"),
        reponame = cfg.github_repo.split('/').nth(1).unwrap_or("?"),
        workflow = cfg.release_workflow,
        workflow_file = cfg.release_workflow.rsplit('/').next().unwrap_or("release.yml"),
        testpypi_env = cfg.testpypi_environment,
        pypi_env = cfg.pypi_environment,
        license = cfg.license_spdx,
        upstream = cfg.upstream,
        upstream_url = cfg.upstream_url,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> ReleaseConfig {
        ReleaseConfig {
            project: "crc".into(),
            rust_crate: "crc-rust-core".into(),
            pypi_dist: "crc-rust".into(),
            python_import: "crc".into(),
            version: "0.1.0".into(),
            requires_python: ">=3.9".into(),
            python_versions: vec!["3.9".into(), "3.12".into()],
            platforms: vec!["ubuntu-22.04".into()],
            github_repo: "John-Cusack/crc-rust".into(),
            release_workflow: ".github/workflows/release.yml".into(),
            pypi_environment: "pypi".into(),
            testpypi_environment: "testpypi".into(),
            license_spdx: "BSD-2-Clause".into(),
            upstream: "Nicoretti/crc".into(),
            upstream_url: "https://github.com/Nicoretti/crc".into(),
            upstream_license: "BSD-2-Clause".into(),
            upstream_authors: "Nicola Coretti".into(),
            smoke_exprs: vec!["crc.Calculator(crc.Crc8.CCITT).verify(b'1', 0)".into()],
        }
    }

    #[test]
    fn names_happy() {
        assert!(validate_names(&example()).is_ok());
    }

    #[test]
    fn names_reject_colliding_dist() {
        let mut c = example();
        c.pypi_dist = "crc".into();
        let e = validate_names(&c).unwrap_err();
        assert!(e.contains("collides"), "{e}");
    }

    #[test]
    fn names_reject_renamed_import() {
        let mut c = example();
        c.python_import = "crc_rs".into();
        let e = validate_names(&c).unwrap_err();
        assert!(e.contains("must equal"), "{e}");
    }

    #[test]
    fn names_reject_bad_version_and_license() {
        let mut c = example();
        c.version = "eight".into();
        assert!(validate_names(&c).unwrap_err().contains("X.Y.Z"));
        c.version = "0.1.0".into();
        c.license_spdx = "WTFPL".into();
        assert!(validate_names(&c).unwrap_err().contains("unknown"));
    }

    #[test]
    fn names_reject_python_below_floor() {
        let mut c = example();
        c.python_versions = vec!["3.8".into()];
        assert!(validate_names(&c).unwrap_err().contains("below"));
    }

    #[test]
    fn parse_missing_field_names_it() {
        let e = parse_release_config("[release]\nproject = \"crc\"\n").unwrap_err();
        assert!(e.contains("rust_crate"), "{e}");
    }

    #[test]
    fn state_partial_never_complete() {
        let mut s = initial_state("0.1.0", "abc");
        assert_eq!(overall_status(&s), "pending");
        record_outcome(&mut s, "pypi", "success", "manual", 1).unwrap();
        assert_eq!(overall_status(&s), "partial");
        record_outcome(&mut s, "testpypi", "success", "w", 2).unwrap();
        assert_eq!(overall_status(&s), "partial");
        assert!(record_outcome(&mut s, "nope", "success", "", 3).is_err());
        assert!(record_outcome(&mut s, "pypi", "maybe", "", 3).is_err());
        record_outcome(&mut s, "crates-io", "success", "c", 4).unwrap();
        assert_eq!(overall_status(&s), "complete");
    }

    #[test]
    fn state_failed_then_retried() {
        let mut s = initial_state("0.1.0", "abc");
        record_outcome(&mut s, "pypi", "failed", "403", 1).unwrap();
        assert_eq!(overall_status(&s), "failed");
        record_outcome(&mut s, "pypi", "success", "retry", 2).unwrap();
        assert_eq!(overall_status(&s), "partial");
        assert_eq!(s.registries["pypi"].attempts.len(), 2);
    }

    #[test]
    fn workflow_uses_same_artifacts_and_matrix() {
        let w = render_workflow(&example());
        assert!(w.contains("crc-rust"), "dist name");
        assert!(w.contains("test.pypi.org/legacy/"), "testpypi");
        assert!(w.contains("download-artifact"), "artifact reuse");
        assert!(w.contains("no rebuild"), "same-file guarantee");
        assert!(w.contains("ubuntu-22.04"), "platforms");
        assert!(w.contains("id-token: write"), "trusted publishing");
        assert!(w.contains("cargo publish --manifest-path crc-core/Cargo.toml"), "core publish");
        assert!(w.contains("crc-core/Cargo.toml"), "core in sdist job");
        assert!(!w.contains("${ matrix"), "escaped template braces: {w}");
    }

    #[test]
    fn setup_names_exact_identities() {
        let t = render_setup_instructions(&example());
        for needle in [
            "John-Cusack/crc-rust",
            "crc-rust",
            "crc-rust-core",
            "testpypi",
            "release-record",
            "release-status",
            "partial",
        ] {
            assert!(t.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn patch_files_parsed() {
        let patch = "diff --git a/src/lib.rs b/src/lib.rs\n+++ b/src/lib.rs\n@@\n\
             diff --git a/crc-core/src/lib.rs b/crc-core/src/lib.rs\n+++ b/crc-core/src/lib.rs\n@@\n";
        assert_eq!(files_of_patch(patch), vec!["crc-core/src/lib.rs", "src/lib.rs"]);
        assert!(files_of_patch("").is_empty());
    }

    #[test]
    fn load_and_tree_validate_example_layout() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("crc-core/src")).unwrap();
        std::fs::create_dir_all(p.join("crc")).unwrap();
        std::fs::write(
            p.join("release.toml"),
            r#"[release]
project = "crc"
rust_crate = "crc-rust-core"
pypi_dist = "crc-rust"
python_import = "crc"
version = "0.1.0"
requires_python = ">=3.9"
python_versions = ["3.9", "3.12"]
platforms = ["ubuntu-22.04"]
github_repo = "John-Cusack/crc-rust"
release_workflow = ".github/workflows/release.yml"
pypi_environment = "pypi"
testpypi_environment = "testpypi"
license_spdx = "BSD-2-Clause"
upstream = "Nicoretti/crc"
upstream_url = "https://github.com/Nicoretti/crc"
upstream_license = "BSD-2-Clause"
upstream_authors = "Nicola Coretti"
smoke_exprs = ["True"]
"#,
        )
        .unwrap();
        std::fs::write(
            p.join("Cargo.toml"),
            "[package]\nname = \"crc-rust\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [lib]\nname = \"_crc\"\ncrate-type = [\"cdylib\"]\n\n\
             [dependencies]\ncrc-rust-core = { path = \"crc-core\", version = \"0.1.0\" }\n\n[workspace]\n",
        )
        .unwrap();
        std::fs::write(
            p.join("crc-core/Cargo.toml"),
            "[package]\nname = \"crc-rust-core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [lib]\nname = \"crc_core\"\ncrate-type = [\"rlib\"]\n",
        )
        .unwrap();
        std::fs::write(p.join("crc-core/src/lib.rs"), "// core\n").unwrap();
        std::fs::write(
            p.join("pyproject.toml"),
            "[project]\nname = \"crc-rust\"\nversion = \"0.1.0\"\nrequires-python = \">=3.9\"\n\
             readme = \"README.md\"\nlicense = \"BSD-2-Clause\"\n\n\
             [tool.maturin]\nmodule-name = \"crc._crc\"\nbindings = \"pyo3\"\n",
        )
        .unwrap();
        std::fs::write(p.join("README.md"), "# x\n").unwrap();
        std::fs::write(p.join("crc/__init__.py"), "# shim\n").unwrap();
        std::fs::write(p.join("NOTICE"), "Upstream Nicoretti/crc BSD-2-Clause\n").unwrap();
        let cfg = load_release_config(p).unwrap();
        assert!(validate_names(&cfg).is_ok());
        assert!(validate_tree(&cfg, p).is_ok());
        // Version drift fails with the field named.
        std::fs::write(
            p.join("pyproject.toml"),
            std::fs::read_to_string(p.join("pyproject.toml")).unwrap().replace("0.1.0", "9.9.9"),
        )
        .unwrap();
        let e = validate_tree(&cfg, p).unwrap_err();
        assert!(e.contains("pyproject version"), "{e}");
    }

    #[test]
    fn tree_rejects_python_tied_core() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("crc-core/src")).unwrap();
        std::fs::write(
            p.join("crc-core/Cargo.toml"),
            "[package]\nname = \"crc-rust-core\"\nversion = \"0.1.0\"\n\n[dependencies]\npyo3 = \"0.23\"\n",
        )
        .unwrap();
        std::fs::write(p.join("crc-core/src/lib.rs"), "").unwrap();
        std::fs::write(
            p.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"_x\"\ncrate-type = [\"cdylib\"]\n\n\
             [dependencies]\ncrc-rust-core = { path = \"crc-core\" }\n",
        )
        .unwrap();
        let mut cfg = example();
        cfg.project = "x".into();
        cfg.python_import = "x".into();
        let e = validate_tree(&cfg, p).unwrap_err();
        assert!(e.contains("independent of Python"), "{e}");
    }
}
