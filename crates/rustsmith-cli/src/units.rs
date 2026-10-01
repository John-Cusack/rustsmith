//! Unit addressing + generic mirror template mechanism (Track H).
//!
//! Unit keys are UnitIds (`<lang>:<repo-rel authoritative source>[#<symbol>]`);
//! pre-rollout bare stems still read via the compat shims below. The template
//! directory (`mirror/<package>`) carries a `template.json` describing every
//! file the port needs. A port that passes locally but fails in worktrees
//! because a file was not listed is the failure mode this file exists to
//! prevent. Nothing here names a repo: package identity always arrives as data.
//!
use std::collections::HashMap;
use std::path::Path;

/// Parsed `template.json`. `files` are copied for every unit; `extra_files[id]`
/// additionally for that unit. Per-unit maps are keyed by UnitId
/// (`<lang>:<repo-rel>[#<symbol>]`); pre-rollout stem keys still read via the
/// compat fallback in [`unit_sources`]/[`unit_task`].
///
/// `orig_source`/`delete_on_merge` carry `scaffold()` semantics: the unit's
/// authoritative pre-generation source (the rel inside its UnitId) is replaced
/// by the port scaffold, and `delete_on_merge[id]` (default: that same source)
/// is removed from the fork in the merge commit. Where a bridge is available
/// callers resolve these targets through `bridge.scaffold()` (see
/// [`unit_decl_for_scaffold`]) and use the template maps as compat fallback.
#[derive(Debug, Clone)]
pub struct TemplateSpec {
    pub package: String,
    /// Python extension module (maturin spine). Exactly one of this and
    /// `cmake_target` is set: Python templates name the extension, CMake
    /// templates name the library target the Rust staticlib substitutes into.
    pub extension_module: String,
    /// CMake library target (CTest spine, e.g. `elmersolver`). Empty on the
    /// Python spine; required there would weaken Python validation, so the
    /// loader enforces exactly-one (see below), never a default.
    pub cmake_target: String,
    pub src_layout: bool,
    pub files: Vec<(String, String)>,
    pub extra_files: HashMap<String, Vec<(String, String)>>,
    pub delete_on_merge: HashMap<String, Vec<String>>,
    pub orig_source: HashMap<String, String>,
}

/// True when `s` is a canonical UnitId (`<lang>:<repo-rel>[#<symbol>]`):
/// a non-empty language prefix, a colon, and a non-empty rel. Bare stems
/// (pre-rollout frozen keys) return false. No language names are matched here.
pub fn is_unit_id(s: &str) -> bool {
    let Some(colon) = s.find(':') else {
        return false;
    };
    !s[..colon].is_empty() && colon + 1 < s.len() && !s[..colon].contains('/')
}

/// Authoritative repo-rel source inside a unit key: the UnitId rel part (with
/// any `#symbol` suffix stripped), else the key itself (bare-stem compat).
pub fn unit_rel(unit: &str) -> &str {
    let rel = if is_unit_id(unit) {
        &unit[unit.find(':').unwrap() + 1..]
    } else {
        unit
    };
    match rel.find('#') {
        Some(i) => &rel[..i],
        None => rel,
    }
}

/// Linkage stem of a unit key: file stem of [`unit_rel`] (e.g.
/// `python:src/pkg/mod.py` -> `mod`; bare `mod` -> `mod`).
pub fn unit_stem(unit: &str) -> &str {
    let rel = unit_rel(unit);
    let base = rel.rsplit('/').next().unwrap_or(rel);
    match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    }
}

/// Filesystem/branch-safe name for a unit key (`:` and `/` are illegal in git
/// refs and `/` nests paths): every non-`[A-Za-z0-9_.-]` byte becomes `_`.
pub fn unit_fs_name(unit: &str) -> String {
    unit.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-' {
                b as char
            } else {
                '_'
            }
        })
        .collect()
}

/// Compat lookup in a per-unit map keyed by UnitId (new) or stem (old):
/// exact key first, then the smallest key whose [`unit_stem`] matches.
fn lookup_unit_compat<'a, V>(map: &'a HashMap<String, V>, unit: &str) -> Option<&'a V> {
    if let Some(v) = map.get(unit) {
        return Some(v);
    }
    let want = unit_stem(unit);
    map.iter()
        .filter(|(k, _)| unit_stem(k) == want)
        .min_by(|a, b| a.0.cmp(b.0))
        .map(|(_, v)| v)
}

fn pairs(v: &serde_json::Value) -> Vec<(String, String)> {
    v.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            let a = e.as_array()?;
            if a.len() == 2 {
                Some((
                    a[0].as_str().unwrap_or("").to_string(),
                    a[1].as_str().unwrap_or("").to_string(),
                ))
            } else {
                None
            }
        })
        .collect()
}

fn strmap(v: &serde_json::Value) -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    if let Some(o) = v.as_object() {
        for (k, val) in o {
            let list = val
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            m.insert(k.clone(), list);
        }
    }
    m
}

pub fn load_template(dir: &Path) -> Result<TemplateSpec, String> {
    let t = std::fs::read_to_string(dir.join("template.json")).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    let mut extra_files = HashMap::new();
    if let Some(o) = v["extra_files"].as_object() {
        for (k, val) in o {
            extra_files.insert(k.clone(), pairs(val));
        }
    }
    let mut orig_source = HashMap::new();
    let spec = TemplateSpec {
        package: v["package"].as_str().unwrap_or("").to_string(),
        extension_module: v["extension_module"].as_str().unwrap_or("").to_string(),
        cmake_target: v["cmake_target"].as_str().unwrap_or("").to_string(),
        src_layout: v["src_layout"].as_bool().unwrap_or(true),
        files: pairs(&v["files"]),
        extra_files,
        delete_on_merge: strmap(&v["delete_on_merge"]),
        orig_source,
    };
    if spec.package.is_empty() {
        return Err(format!("{}: template.json lacks package", dir.display()));
    }
    // Spine marker: Python templates set `extension_module`, CMake templates
    // set `cmake_target` — exactly one. Neither is defaulted: a missing
    // marker is a misconfigured template, never a guessed spine.
    if spec.extension_module.is_empty() && spec.cmake_target.is_empty() {
        return Err(format!("{}: template.json lacks extension_module (python) or cmake_target (cmake)", dir.display()));
    }
    if !spec.extension_module.is_empty() && !spec.cmake_target.is_empty() {
        return Err(format!("{}: template.json sets both extension_module and cmake_target (exactly one)", dir.display()));
    }
    if spec.files.is_empty() {
        return Err(format!("{}: template.json lists no files", dir.display()));
    }
    // Dual-artifact contract is Python-spine only; CMake templates carry a
    // staticlib scaffold under a different contract.
    if !spec.extension_module.is_empty() {
        validate_dual_artifact(dir, &spec)?;
    }
    Ok(spec)
}

/// Dual-artifact contract (SPEC Stage 1): one template builds both a Rust
/// crate (crates.io) and a Python extension (`-rust` PyPI dist). The
/// `[lib] crate-type` must include `cdylib` and the maturin `module-name`
/// must equal the declared extension module. Line-oriented parsing is enough
/// for template manifests (same style as the optimize `crate_package` read).
fn validate_dual_artifact(dir: &Path, spec: &TemplateSpec) -> Result<(), String> {
    let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let mut section = String::new();
    let mut cdylib = false;
    for line in cargo.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            section = l.to_string();
        }
        if section == "[lib]" && l.starts_with("crate-type") && l.contains("cdylib") {
            cdylib = true;
        }
    }
    if !cdylib {
        return Err(format!(
            "{}: Cargo.toml [lib] crate-type lacks cdylib (dual-artifact contract)",
            dir.display()
        ));
    }
    let pyproject =
        std::fs::read_to_string(dir.join("pyproject.toml")).map_err(|e| e.to_string())?;
    let mut section = String::new();
    let mut module_name = String::new();
    for line in pyproject.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            section = l.to_string();
        }
        if section == "[tool.maturin]" {
            if let Some(v) = l.strip_prefix("module-name") {
                module_name = v
                    .trim()
                    .trim_start_matches('=')
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
            }
        }
    }
    if module_name != spec.extension_module {
        return Err(format!(
            "{}: pyproject [tool.maturin] module-name {module_name:?} != extension_module {:?} (dual-artifact contract)",
            dir.display(),
            spec.extension_module
        ));
    }
    Ok(())
}

/// Worker stub task: materialize the unit from the template, commit on its
/// branch. A production run swaps this for a real OMP worker task; grading,
/// gating, and merging are identical.
pub fn unit_task(spec: &TemplateSpec, template: &Path, unit: &str) -> Result<String, String> {
    let mut copies = spec.files.clone();
    // Compat: per-unit extras keyed by UnitId (new) or stem (old).
    if let Some(extra) = lookup_unit_compat(&spec.extra_files, unit) {
        copies.extend(extra.clone());
    }
    if copies.is_empty() {
        return Err(format!("template lists no files for unit {unit}"));
    }
    let mut dirs: Vec<String> = copies
        .iter()
        .filter_map(|(_, dst)| {
            Path::new(dst)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| p.display().to_string())
        })
        .collect();
    dirs.sort();
    dirs.dedup();
    let mut cmd = format!("mkdir -p {} ", dirs.join(" "));
    let t = template.display().to_string();
    for (src, dst) in &copies {
        cmd.push_str(&format!("&& cp {t}/{src} ./{dst} "));
    }
    // The commit may be empty when a sibling unit already materialized the
    // same template files (single-ext ports); that is not a worker failure.
    // Copy failures still exit nonzero via the && chain above.
    cmd.push_str(&format!(
        "&& git add -A && (git -c user.email=t@t -c user.name=t commit -qm 'unit {unit}' || true) && echo {unit} > unit_done.txt"
    ));
    Ok(cmd)
}
/// Scaffold targets for a unit: the authoritative orig source plus the fork
/// paths removed in the merge commit. Resolution order:
/// 1. template `orig_source` override (UnitId key, else stem-compat key);
/// 2. recon `modules` map (UnitId key, else stem-compat key, else any entry
///    whose value equals the UnitId rel);
/// 3. scaffold fallback: the rel embedded in the UnitId itself (a UnitId is
///    self-describing: `<lang>:<repo-rel>[#<symbol>]`), else refuse.
/// Deletes come from template `delete_on_merge` (same compat keys), defaulting
/// to the resolved orig source — i.e. the scaffold replaces its unit source.
pub fn unit_sources(
    spec: &TemplateSpec,
    recon_modules: &HashMap<String, String>,
    unit: &str,
) -> Result<(String, Vec<String>), String> {
    let rel = lookup_unit_compat(&spec.orig_source, unit)
        .cloned()
        .or_else(|| lookup_unit_compat(recon_modules, unit).cloned())
        .or_else(|| {
            let rel = unit_rel(unit);
            recon_modules
                .values()
                .any(|v| v == rel)
                .then(|| rel.to_string())
        })
        .or_else(|| is_unit_id(unit).then(|| unit_rel(unit).to_string()))
        .ok_or_else(|| format!("no orig source for unit {unit}"))?;
    let deletes = lookup_unit_compat(&spec.delete_on_merge, unit)
        .cloned()
        .unwrap_or_else(|| vec![rel.clone()]);
    Ok((rel, deletes))
}

/// Minimal [`rustsmith_adapters::UnitDecl`] for `bridge.scaffold(unit)`: the id
/// plus its authoritative source file. Exports/imports stay empty so the
/// bridge derives the linkage stem from the file itself; no language is named
/// here, keeping language knowledge in frontends/bridges.
pub fn unit_decl_for_scaffold(unit: &str, rel: &str) -> rustsmith_adapters::UnitDecl {
    rustsmith_adapters::UnitDecl {
        id: rustsmith_adapters::UnitId(unit.to_string()),
        files: vec![rel.to_string()],
        generated_from: None,
        exports: Vec::new(),
        imports: Vec::new(),
    }
}

/// Frozen `recon.json` as written by this track: `modules` + `call_edges`
/// keyed by UnitId, `build.languages` present. Pre-rollout fixtures (bare
/// stems, `build.language`) still read via the compat shims above.
pub fn read_recon_modules(recon_out: &Path) -> HashMap<String, String> {
    let f = recon_out.join("recon.json");
    let t = std::fs::read_to_string(&f).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&t).unwrap_or(serde_json::json!({}));
    let mut m = HashMap::new();
    if let Some(o) = v["modules"].as_object() {
        for (k, val) in o {
            if let Some(s) = val.as_str() {
                m.insert(k.clone(), s.to_string());
            }
        }
    }
    m
}

/// Frozen build languages: `build.languages` (new) with `build.language`
/// (pre-rollout) as compat fallback. Empty when recon.json is missing.
pub fn read_recon_build_languages(recon_out: &Path) -> Vec<String> {
    let f = recon_out.join("recon.json");
    let t = std::fs::read_to_string(&f).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&t).unwrap_or(serde_json::json!({}));
    if let Some(arr) = v["build"]["languages"].as_array() {
        let langs: Vec<String> = arr.iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
        if !langs.is_empty() {
            return langs;
        }
    }
    if let Some(s) = v["build"]["language"].as_str() {
        if !s.is_empty() {
            return vec![s.to_string()];
        }
    }
    Vec::new()
}

/// New-spec check for a frozen `dag.json` value: every unit `id`, `depends_on`
/// entry, edge endpoint, and `leaf_first_order` entry parses as a UnitId.
pub fn validate_dag_json(v: &serde_json::Value) -> Result<(), String> {
    let units = v["units"].as_array().ok_or("dag.json lacks units")?;
    if units.is_empty() {
        return Err("dag.json lists no units".into());
    }
    let mut ids = std::collections::HashSet::new();
    for u in units {
        let id = u["id"].as_str().ok_or("dag unit lacks id")?;
        if !is_unit_id(id) {
            return Err(format!("dag unit id is not a UnitId: {id}"));
        }
        ids.insert(id.to_string());
        for d in u["depends_on"].as_array().cloned().unwrap_or_default() {
            let d = d.as_str().ok_or("dag depends_on entry is not a string")?;
            if !is_unit_id(d) {
                return Err(format!("dag depends_on entry is not a UnitId: {d}"));
            }
        }
    }
    for e in v["edges"].as_array().cloned().unwrap_or_default() {
        let pair = e.as_array().cloned().unwrap_or_default();
        if pair.len() != 2 {
            return Err("dag edge is not a pair".into());
        }
        for end in &pair {
            let s = end.as_str().ok_or("dag edge endpoint is not a string")?;
            if !is_unit_id(s) {
                return Err(format!("dag edge endpoint is not a UnitId: {s}"));
            }
            if !ids.contains(s) {
                return Err(format!("dag edge references unknown unit: {s}"));
            }
        }
    }
    let order: Vec<String> = serde_json::from_value(v["leaf_first_order"].clone())
        .map_err(|_| "dag.json lacks leaf_first_order")?;
    for id in &order {
        if !is_unit_id(id) {
            return Err(format!("dag order entry is not a UnitId: {id}"));
        }
        if !ids.contains(id) {
            return Err(format!("dag order references unknown unit: {id}"));
        }
    }
    Ok(())
}

/// New-spec check for a frozen `recon.json` value: `build.languages`
/// non-empty, `modules` keys and `call_edges` endpoints UnitIds whose rels
/// match their values.
pub fn validate_recon_json(v: &serde_json::Value) -> Result<(), String> {
    let langs = v["build"]["languages"].as_array().ok_or("recon.json lacks build.languages")?;
    if langs.is_empty() || langs.iter().any(|l| !l.is_string()) {
        return Err("recon.json build.languages is empty or non-string".into());
    }
    let mods = v["modules"].as_object().ok_or("recon.json lacks modules")?;
    if mods.is_empty() {
        return Err("recon.json lists no modules".into());
    }
    for (k, val) in mods {
        if !is_unit_id(k) {
            return Err(format!("recon module key is not a UnitId: {k}"));
        }
        let rel = val.as_str().ok_or("recon module value is not a string")?;
        if unit_rel(k) != rel {
            return Err(format!("recon module {k} maps to mismatched rel {rel}"));
        }
    }
    for e in v["call_edges"].as_array().cloned().unwrap_or_default() {
        let pair = e.as_array().cloned().unwrap_or_default();
        if pair.len() != 2 {
            return Err("recon call edge is not a pair".into());
        }
        for end in &pair {
            let s = end.as_str().ok_or("recon call edge endpoint is not a string")?;
            if !is_unit_id(s) {
                return Err(format!("recon call edge endpoint is not a UnitId: {s}"));
            }
            if !mods.contains_key(s) {
                return Err(format!("recon call edge references unknown unit: {s}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    fn base_spec(
        extra_files: HashMap<String, Vec<(String, String)>>,
        orig_source: HashMap<String, String>,
    ) -> TemplateSpec {
        TemplateSpec {
            package: "pkg".to_string(),
            extension_module: "_pkg".to_string(),
            cmake_target: String::new(),
            src_layout: true,
            files: vec![pair("Cargo.toml", "Cargo.toml")],
            extra_files,
            delete_on_merge: HashMap::new(),
            orig_source,
        }
    }

    #[test]
    fn validate_dag_json_accepts_unit_ids_rejects_stems() {
        let new = serde_json::json!({
            "units": [
                {"id": "python:src/pkg/a.py", "depends_on": []},
                {"id": "python:src/pkg/b.py", "depends_on": ["python:src/pkg/a.py"]}
            ],
            "edges": [["python:src/pkg/b.py", "python:src/pkg/a.py"]],
            "leaf_first_order": ["python:src/pkg/a.py", "python:src/pkg/b.py"]
        });
        assert!(validate_dag_json(&new).is_ok());
        // Pre-rollout stem-only keys are rejected, not silently accepted.
        let old = serde_json::json!({
            "units": [
                {"id": "a", "depends_on": []},
                {"id": "b", "depends_on": ["a"]}
            ],
            "edges": [["b", "a"]],
            "leaf_first_order": ["a", "b"]
        });
        let err = validate_dag_json(&old).unwrap_err();
        assert!(err.contains("not a UnitId"), "unexpected: {err}");
        // A stem sneaking into depends_on of an otherwise new-shape dag.
        let mixed = serde_json::json!({
            "units": [
                {"id": "python:src/pkg/a.py", "depends_on": []},
                {"id": "python:src/pkg/b.py", "depends_on": ["a"]}
            ],
            "edges": [["python:src/pkg/b.py", "python:src/pkg/a.py"]],
            "leaf_first_order": ["python:src/pkg/a.py", "python:src/pkg/b.py"]
        });
        let err = validate_dag_json(&mixed).unwrap_err();
        assert!(err.contains("not a UnitId"), "unexpected: {err}");
    }

    #[test]
    fn validate_recon_json_requires_build_languages() {
        let new = serde_json::json!({
            "build": {"languages": ["python"]},
            "modules": {
                "python:src/pkg/a.py": "src/pkg/a.py",
                "python:src/pkg/b.py": "src/pkg/b.py"
            },
            "call_edges": [["python:src/pkg/b.py", "python:src/pkg/a.py"]]
        });
        assert!(validate_recon_json(&new).is_ok());
        // Pre-rollout `build.language` (singular) no longer validates.
        let legacy_lang = serde_json::json!({
            "build": {"language": "python"},
            "modules": {"a": "src/pkg/a.py"},
            "call_edges": []
        });
        let err = validate_recon_json(&legacy_lang).unwrap_err();
        assert!(err.contains("build.languages"), "unexpected: {err}");
        // Plural languages present but a stem key remains: still rejected.
        let legacy_keys = serde_json::json!({
            "build": {"languages": ["python"]},
            "modules": {"a": "src/pkg/a.py"},
            "call_edges": []
        });
        let err = validate_recon_json(&legacy_keys).unwrap_err();
        assert!(err.contains("not a UnitId"), "unexpected: {err}");
        // A UnitId whose rel disagrees with its module value is corrupt.
        let skewed = serde_json::json!({
            "build": {"languages": ["python"]},
            "modules": {"python:src/pkg/a.py": "src/pkg/other.py"},
            "call_edges": []
        });
        assert!(validate_recon_json(&skewed).is_err());
    }

    #[test]
    fn per_unit_maps_resolve_stem_and_unit_id_keys() {
        let template = Path::new("template");
        // Old stem-keyed extras resolve from a UnitId query.
        let old_keyed = base_spec(
            HashMap::from([(
                "a".to_string(),
                vec![pair("extra/a_test.py", "extra/a_test.py")],
            )]),
            HashMap::new(),
        );
        let cmd = unit_task(&old_keyed, template, "python:src/pkg/a.py").unwrap();
        assert!(cmd.contains("extra/a_test.py"), "stem-keyed extra missing: {cmd}");
        // New UnitId keys resolve exactly.
        let new_keyed = base_spec(
            HashMap::from([(
                "python:src/pkg/a.py".to_string(),
                vec![pair("extra/a_new.py", "extra/a_new.py")],
            )]),
            HashMap::new(),
        );
        let cmd = unit_task(&new_keyed, template, "python:src/pkg/a.py").unwrap();
        assert!(cmd.contains("extra/a_new.py"), "UnitId-keyed extra missing: {cmd}");
        // Old stem-keyed recon modules resolve a UnitId query ...
        let empty = base_spec(HashMap::new(), HashMap::new());
        let recon_old: HashMap<String, String> =
            HashMap::from([("a".to_string(), "src/pkg/a.py".to_string())]);
        let (rel, _) = unit_sources(&empty, &recon_old, "python:src/pkg/a.py").unwrap();
        assert_eq!(rel, "src/pkg/a.py");
        // ... and new UnitId-keyed recon resolves a bare-stem query.
        let recon_new: HashMap<String, String> =
            HashMap::from([("python:src/pkg/a.py".to_string(), "src/pkg/a.py".to_string())]);
        let (rel, _) = unit_sources(&empty, &recon_new, "a").unwrap();
        assert_eq!(rel, "src/pkg/a.py");
    }

    #[test]
    fn migrated_template_keys_resolve_both_directions() {
        // Migrated template held in memory (no mirror checkout is read): new
        // UnitId keys for ported units, one legacy stem key awaiting migration.
        let spec = TemplateSpec {
            package: "crc".to_string(),
            extension_module: "_crc".to_string(),
            cmake_target: String::new(),
            src_layout: true,
            files: vec![pair("Cargo.toml", "Cargo.toml")],
            extra_files: HashMap::from([
                (
                    "python:src/pkg/_crc.py".to_string(),
                    vec![pair("extra/crc_helper.py", "extra/crc_helper.py")],
                ),
                (
                    "shingle".to_string(),
                    vec![pair("extra/shingle_helper.py", "extra/shingle_helper.py")],
                ),
            ]),
            delete_on_merge: HashMap::new(),
            orig_source: HashMap::from([(
                "python:src/pkg/_crc.py".to_string(),
                "src/pkg/_crc.py".to_string(),
            )]),
        };
        let template = Path::new("template");
        // Migrated unit: UnitId-keyed extra plus the orig-source override.
        let cmd = unit_task(&spec, template, "python:src/pkg/_crc.py").unwrap();
        assert!(cmd.contains("extra/crc_helper.py"), "migrated extra missing: {cmd}");
        let (rel, deletes) = unit_sources(&spec, &HashMap::new(), "python:src/pkg/_crc.py").unwrap();
        assert_eq!(rel, "src/pkg/_crc.py");
        assert_eq!(deletes, vec!["src/pkg/_crc.py".to_string()]);
        // Legacy stem-keyed unit resolves from a UnitId query, falling back to
        // the self-describing rel inside the UnitId itself.
        let cmd = unit_task(&spec, template, "python:src/pkg/shingle.py").unwrap();
        assert!(cmd.contains("extra/shingle_helper.py"), "legacy extra missing: {cmd}");
        let (rel, _) = unit_sources(&spec, &HashMap::new(), "python:src/pkg/shingle.py").unwrap();
        assert_eq!(rel, "src/pkg/shingle.py");
        // Reverse: a bare-stem query still finds the migrated UnitId key.
        let cmd = unit_task(&spec, template, "_crc").unwrap();
        assert!(cmd.contains("extra/crc_helper.py"), "migrated key missed: {cmd}");
    }

    fn write_template(dir: &std::path::Path, body: &str) {
        std::fs::write(dir.join("template.json"), body).unwrap();
        // Minimal manifests so Python-spined fixtures also exercise the
        // dual-artifact contract (CMake fixtures skip it by marker).
        if let Some(rest) = body.split("\"extension_module\":\"").nth(1) {
            let module = rest.split('"').next().unwrap_or("m");
            std::fs::write(
                dir.join("Cargo.toml"),
                "[package]\nname = \"pkg\"\n[lib]\ncrate-type = [\"cdylib\"]\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("pyproject.toml"),
                format!("[tool.maturin]\nmodule-name = \"{module}\"\n"),
            )
            .unwrap();
        }
    }

    #[test]
    fn template_spine_marker_is_exactly_one() {
        let dir = tempfile::tempdir().unwrap();
        // Python spine: extension_module only.
        write_template(
            dir.path(),
            r#"{"package":"p","extension_module":"m","files":[["a","a"]]}"#,
        );
        let spec = load_template(dir.path()).unwrap();
        assert_eq!(spec.extension_module, "m");
        assert!(spec.cmake_target.is_empty());
        // CMake spine: cmake_target only.
        write_template(
            dir.path(),
            r#"{"package":"q","cmake_target":"t","src_layout":false,"files":[["a","a"]]}"#,
        );
        let spec = load_template(dir.path()).unwrap();
        assert_eq!(spec.cmake_target, "t");
        assert!(!spec.src_layout);
        // Neither marker: refused, never a guessed spine.
        write_template(
            dir.path(),
            r#"{"package":"q","files":[["a","a"]]}"#,
        );
        let err = load_template(dir.path()).unwrap_err();
        assert!(err.contains("extension_module (python) or cmake_target (cmake)"), "unexpected: {err}");
        // Both markers: refused as misconfiguration.
        write_template(
            dir.path(),
            r#"{"package":"q","extension_module":"m","cmake_target":"t","files":[["a","a"]]}"#,
        );
        let err = load_template(dir.path()).unwrap_err();
        assert!(err.contains("exactly one"), "unexpected: {err}");
    }

    #[test]
    fn elmer_template_loads_cmake_spine() {
        // The shipped `mirror/Elmer` template (workspace-anchored, not cwd):
        // CMake spine marker, flat layout, base files present.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../mirror/Elmer");
        let spec = load_template(&dir).unwrap();
        assert_eq!(spec.package, "Elmer");
        assert_eq!(spec.cmake_target, "elmersolver");
        assert!(spec.extension_module.is_empty());
        assert!(!spec.src_layout);
        for (src, _) in &spec.files {
            assert!(dir.join(src).is_file(), "template base file missing: {src}");
        }
    }

    #[test]
    fn load_template_enforces_dual_artifact_contract() {
        // Shipped templates satisfy the contract (module name == extension
        // module, cdylib crate): checked against the real mirror/ data.
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut checked = 0;
        for pkg in crate::repo_content::packages() {
            let tpl = std::fs::read_to_string(root.join("../../mirror").join(&pkg).join("template.json"));
            let Ok(text) = tpl else { continue };
            if !text.contains("extension_module") {
                continue;
            }
            let spec = load_template(&root.join("../../mirror").join(&pkg));
            assert!(spec.is_ok(), "template rejected for package {pkg}");
            checked += 1;
        }
        assert!(checked > 0, "no Python-spined template exercised");
        // Missing cdylib fails.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("template.json"),
            r#"{"package":"pkg","extension_module":"pkg._pkg","files":[["a","a"]]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"pkg\"\n[lib]\nname = \"_pkg\"\ncrate-type = [\"rlib\"]\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.maturin]\nmodule-name = \"pkg._pkg\"\n",
        )
        .unwrap();
        let err = load_template(dir.path()).unwrap_err();
        assert!(err.contains("cdylib"), "unexpected: {err}");
        // Module-name mismatch fails.
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"pkg\"\n[lib]\nname = \"_pkg\"\ncrate-type = [\"cdylib\"]\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.maturin]\nmodule-name = \"pkg._other\"\n",
        )
        .unwrap();
        let err = load_template(dir.path()).unwrap_err();
        assert!(err.contains("module-name"), "unexpected: {err}");
    }
}
