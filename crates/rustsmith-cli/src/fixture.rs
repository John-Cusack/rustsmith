//! Fixture dispatch + generic mirror template mechanism (M7 slice 3).
//!
//! Only the two pinned fixtures exist (`config/fixture.toml`). The fixture is
//! detected once (recon) and frozen into `recon/fixture.json`; every later
//! stage reads that file and never re-detects (no drift). `mirror` tolerates
//! a missing `fixture.json` (hand-invoked compat) by detecting from the repo.
//!
//! The template directory (`mirror/crc`, `mirror/strsimpy`) carries a
//! `template.json` describing every file the port needs. A port that passes
//! locally but fails in worktrees because a file was not listed is the
//! failure mode this file exists to prevent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureKind {
    Crc,
    Strsimpy,
}

impl FixtureKind {
    pub fn name(&self) -> &'static str {
        match self {
            FixtureKind::Crc => "crc",
            FixtureKind::Strsimpy => "strsimpy",
        }
    }
    pub fn package(&self) -> &'static str {
        self.name()
    }
    /// Original-repo source layout: crc is src-layout (`src/crc/`), strsimpy
    /// is flat (`strsimpy/` at root).
    pub fn src_layout(&self) -> bool {
        match self {
            FixtureKind::Crc => true,
            FixtureKind::Strsimpy => false,
        }
    }
}

/// Detect the fixture from repo layout. Anything else is refused, never guessed.
pub fn detect_fixture(repo: &Path) -> Result<FixtureKind, String> {
    if repo.join("src/crc").is_dir() {
        return Ok(FixtureKind::Crc);
    }
    if repo.join("strsimpy").is_dir() {
        return Ok(FixtureKind::Strsimpy);
    }
    // Fallback: pyproject project name.
    let pp = repo.join("pyproject.toml");
    if pp.exists() {
        let t = std::fs::read_to_string(&pp).map_err(|e| e.to_string())?;
        for line in t.lines() {
            let l = line.trim();
            if l.starts_with("name") && l.contains("crc") {
                return Ok(FixtureKind::Crc);
            }
            if l.starts_with("name") && l.contains("strsimpy") {
                return Ok(FixtureKind::Strsimpy);
            }
        }
    }
    if repo.join("setup.py").exists() {
        let t = std::fs::read_to_string(repo.join("setup.py")).map_err(|e| e.to_string())?;
        if t.contains("strsimpy") {
            return Ok(FixtureKind::Strsimpy);
        }
    }
    Err(format!(
        "unknown fixture for repo {} (only crc + strsimpy pinned)",
        repo.display()
    ))
}

pub fn template_dir(kind: &FixtureKind) -> PathBuf {
    PathBuf::from(format!("mirror/{}", kind.name()))
}

pub fn write_frozen_fixture(out: &Path, kind: &FixtureKind, repo: &Path) -> Result<(), String> {
    std::fs::write(
        out.join("fixture.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "fixture": kind.name(), "package": kind.package(), "src_layout": kind.src_layout(),
            "repo": repo.display().to_string(),
        }))
        .unwrap(),
    )
    .map_err(|e| e.to_string())
}

/// Frozen fixture if recon wrote one, else detect from the repo.
pub fn resolve_fixture(recon_out: &Path, repo: &Path) -> Result<FixtureKind, String> {
    let f = recon_out.join("fixture.json");
    if f.exists() {
        let t = std::fs::read_to_string(&f).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
        match v["fixture"].as_str().unwrap_or("") {
            "crc" => return Ok(FixtureKind::Crc),
            "strsimpy" => return Ok(FixtureKind::Strsimpy),
            other => return Err(format!("bad frozen fixture {other}")),
        }
    }
    detect_fixture(repo)
}

/// Parsed `template.json`. `files` are copied for every unit; `extra_files[id]`
/// additionally for that unit; `delete_on_merge[id]` (default: the unit's own
/// orig source) is removed from the fork in the merge commit.
#[derive(Debug, Clone)]
pub struct TemplateSpec {
    pub package: String,
    pub extension_module: String,
    pub src_layout: bool,
    pub files: Vec<(String, String)>,
    pub extra_files: HashMap<String, Vec<(String, String)>>,
    pub delete_on_merge: HashMap<String, Vec<String>>,
    pub orig_source: HashMap<String, String>,
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
    if let Some(o) = v["orig_source"].as_object() {
        for (k, val) in o {
            if let Some(s) = val.as_str() {
                orig_source.insert(k.clone(), s.to_string());
            }
        }
    }
    Ok(TemplateSpec {
        package: v["package"].as_str().unwrap_or("").to_string(),
        extension_module: v["extension_module"].as_str().unwrap_or("").to_string(),
        src_layout: v["src_layout"].as_bool().unwrap_or(true),
        files: pairs(&v["files"]),
        extra_files,
        delete_on_merge: strmap(&v["delete_on_merge"]),
        orig_source,
    })
}

/// Worker stub task: materialize the unit from the template, commit on its
/// branch. A production run swaps this for a real OMP worker task; grading,
/// gating, and merging are identical.
pub fn unit_task(spec: &TemplateSpec, template: &Path, unit: &str) -> Result<String, String> {
    let mut copies = spec.files.clone();
    if let Some(extra) = spec.extra_files.get(unit) {
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
    cmd.push_str(&format!(
        "&& git add -A && git -c user.email=t@t -c user.name=t commit -qm 'unit {unit}' && echo {unit} > unit_done.txt"
    ));
    Ok(cmd)
}

/// Orig-source + delete list for a unit: template override, else the recon
/// module map (stem -> repo-rel path), else refuse.
pub fn unit_sources(
    spec: &TemplateSpec,
    recon_modules: &HashMap<String, String>,
    unit: &str,
) -> Result<(String, Vec<String>), String> {
    let rel = spec
        .orig_source
        .get(unit)
        .cloned()
        .or_else(|| recon_modules.get(unit).cloned())
        .ok_or_else(|| format!("no orig source for unit {unit}"))?;
    let deletes = spec
        .delete_on_merge
        .get(unit)
        .cloned()
        .unwrap_or_else(|| vec![rel.clone()]);
    Ok((rel, deletes))
}

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
