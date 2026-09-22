//! Repo-derived metadata + `facts.json` access (Track H).
//!
//! Package identity, layout, template selection, upstream, and attribution all
//! come from the repo itself or the frozen `recon/facts.json` — never from a
//! hardcoded fixture switch. Nothing in this file names a repo.

use std::path::{Path, PathBuf};

/// Discover the package name from the repo's own packaging metadata:
/// `pyproject.toml [project] name`, else `setup.py`/`setup.cfg` `name`.
/// Refused when unknown, never guessed.
pub fn package_name(repo: &Path) -> Result<String, String> {
    let pp = repo.join("pyproject.toml");
    if pp.is_file() {
        if let Ok(text) = std::fs::read_to_string(&pp) {
            if let Some(name) = pyproject_name(&text) {
                return Ok(name);
            }
        }
    }
    for cfg in [repo.join("setup.py"), repo.join("setup.cfg")] {
        if cfg.is_file() {
            if let Ok(text) = std::fs::read_to_string(&cfg) {
                if let Some(name) = setup_name(&text) {
                    return Ok(name);
                }
            }
        }
    }
    Err(format!(
        "unknown package for repo {} (no [project] name in pyproject.toml, no name in setup.py/cfg)",
        repo.display()
    ))
}

/// `[project] name` under the `[project]` section (line-oriented; enough for
/// packaging manifests without a toml dependency on this path).
fn pyproject_name(text: &str) -> Option<String> {
    let mut section = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].trim().to_lowercase();
            continue;
        }
        if section == "project" {
            let line = t.split('#').next().unwrap_or("").trim();
            if let Some(rest) = line.strip_prefix("name") {
                let rest = rest.trim().strip_prefix('=')?.trim();
                let name = rest.trim_matches(|c| c == '"' || c == '\'').trim();
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// `name="..."` / `name='...'` in setup.py/setup.cfg packaging metadata,
/// either one-per-line or inside a `setup(...)` call.
fn setup_name(text: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.split('#').next().unwrap_or("").trim();
        // Candidate starts: line start or right after `(` / `,`.
        for start in std::iter::once(0).chain(t.match_indices(['(', ',']).map(|(i, _)| i + 1)) {
            let cand = t[start..].trim_start();
            let Some(rest) = cand.strip_prefix("name") else {
                continue;
            };
            // Word boundary: `filename=` / `names=` must not match.
            if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let Some(rest) = rest.trim_start().strip_prefix('=') else {
                continue;
            };
            let first = rest.split(',').next().unwrap_or("").trim();
            let name = first.trim_matches(|c| c == '"' || c == '\'').trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}
/// `src/<package>` (with an `__init__` marker); else flat (`<package>/`).
pub fn is_src_layout(repo: &Path, package: &str) -> bool {
    let src_pkg = repo.join("src").join(package);
    src_pkg.is_dir()
        && (src_pkg.join("__init__.py").is_file() || src_pkg.join("__init__.pyi").is_file())
}

/// Resolve the mirror template dir: an explicit `--template` wins; otherwise
/// `mirror/<package>` (the package name discovered from the repo, never a
/// hardcoded fixture switch). The caller canonicalizes (tasks run with
/// cwd=worktree, so the path must be absolute).
pub fn resolve_template(repo: &Path, explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(t) = explicit {
        return Ok(t);
    }
    let package = package_name(repo)?;
    let dir = PathBuf::from("mirror").join(&package);
    if !dir.join("template.json").is_file() {
        return Err(format!(
            "no mirror template for package '{package}' (looked for {})",
            dir.join("template.json").display()
        ));
    }
    Ok(dir)
}

/// Read a JSON field out of the frozen `recon/facts.json` probe section.
/// Every post-recon stage reads behavior here, never from fixture switches.
fn facts_probe(recon_out: &Path) -> Result<serde_json::Value, String> {
    let t = std::fs::read_to_string(recon_out.join("facts.json"))
        .map_err(|e| format!("read recon/facts.json: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&t).map_err(|e| format!("parse recon/facts.json: {e}"))?;
    Ok(v["probe"].clone())
}

/// Frozen package identity (`probe.package`, written by recon from the repo).
pub fn facts_package(recon_out: &Path) -> Result<String, String> {
    let probe = facts_probe(recon_out)?;
    probe["package"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "recon/facts.json lacks probe.package".to_string())
}


/// Upstream `owner/repo` from the repo's own git remote, else the package
/// name. Pure repo data: no registry of known upstreams lives here.
pub fn upstream_of(repo: &Path, package: &str) -> String {
    let out = std::process::Command::new("/usr/bin/git")
        .args(["remote", "get-url", "origin"])
        .current_dir(repo)
        .output();
    if let Ok(out) = out {
        if out.status.success() {
            let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(upstream) = shorten_remote(&url) {
                return upstream;
            }
        }
    }
    package.to_string()
}

/// `https://host/owner/repo(.git)` / `host:owner/repo(.git)` -> `owner/repo`.
fn shorten_remote(url: &str) -> Option<String> {
    let url = url.strip_suffix(".git").unwrap_or(url);
    let path = url
        .rsplit_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url);
    // Drop a leading host (first segment) for both `/` and scp-like `:` shapes.
    let mut segs: Vec<&str> = path.split(['/', ':']).filter(|s| !s.is_empty()).collect();
    if segs.len() < 3 {
        return None;
    }
    let repo = segs.pop()?;
    let owner = segs.pop()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyproject_project_name_parses() {
        let text = "[build-system]\nrequires = []\n[project]\nname = \"demo-pkg\"\nversion = \"1\"";
        assert_eq!(pyproject_name(text).as_deref(), Some("demo-pkg"));
        assert_eq!(pyproject_name("[project]\nversion=\"1\""), None);
    }

    #[test]
    fn setup_name_parses() {
        assert_eq!(
            setup_name("setup(name='demo2', version='1')").as_deref(),
            Some("demo2")
        );
        assert_eq!(setup_name("x = 1"), None);
    }

    #[test]
    fn remote_shortens_both_shapes() {
        assert_eq!(
            shorten_remote("https://github.com/acme/widgets.git").as_deref(),
            Some("acme/widgets")
        );
        assert_eq!(
            shorten_remote("git@github.com:acme/widgets.git").as_deref(),
            Some("acme/widgets")
        );
        assert_eq!(shorten_remote("widgets"), None);
    }
}
