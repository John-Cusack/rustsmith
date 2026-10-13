//! `candidates fetch|classify` (rewrite-candidate finder).
//!
//! `fetch` downloads the pinned download-stats snapshot (hugovk
//! top-pypi-packages) and stamps source + date. `classify` takes the
//! snapshot's top `--limit` packages and classifies EACH into exactly one
//! path — (a) adopt/wrap, (b) mirror, (c) upstream — writing a Markdown
//! candidate list plus a same-stem `.json` of the classifications.
//! `--offline` classifies from snapshot + curated table with zero network
//! (deterministic; used by the acceptance test).

use std::path::PathBuf;

fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}
fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name || a.starts_with(&format!("{name}=")))
}

pub fn cmd_candidates(args: &[String]) -> Result<(), String> {
    match args.first().map(|s| s.as_str()) {
        Some("fetch") => cmd_fetch(&args[1..]),
        Some("classify") => cmd_classify(&args[1..]),
        _ => Err("usage: rustsmith candidates fetch --out <snapshot.json> [--url <url>]\n       rustsmith candidates classify --input <snapshot.json> --out <candidates.md> [--limit <n>] [--offline]".into()),
    }
}

fn cmd_fetch(args: &[String]) -> Result<(), String> {
    let out = PathBuf::from(flag(args, "--out").ok_or("missing --out")?);
    let url = flag(args, "--url").unwrap_or_else(|| {
        rustsmith_candidate_finder::DEFAULT_SNAPSHOT_URL.to_string()
    });
    eprintln!("fetching download snapshot from {url}");
    let snap = fetch_snapshot_text(&url)?;
    // Validate shape before writing so --out never holds a bad snapshot.
    let parsed = rustsmith_candidate_finder::parse_snapshot(&snap)?;
    std::fs::write(&out, &snap).map_err(|e| format!("{}: {e}", out.display()))?;
    eprintln!(
        "wrote {} ({} rows, last_update {})",
        out.display(),
        parsed.rows.len(),
        parsed.last_update
    );
    Ok(())
}

/// Fetch the raw snapshot text via curl (keeps the upstream bytes, stamp and
/// all, so the file stays a faithful pinned snapshot).
fn fetch_snapshot_text(url: &str) -> Result<String, String> {
    let out = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "-m",
            "120",
            "-H",
            "User-Agent: rustsmith-candidate-finder/0.1.0 (research tooling)",
            url,
        ])
        .output()
        .map_err(|e| format!("curl {url}: cannot run curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl {url}: failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("{url}: not UTF-8: {e}"))
}

fn cmd_classify(args: &[String]) -> Result<(), String> {
    use rustsmith_candidate_finder as f;
    let input = PathBuf::from(flag(args, "--input").ok_or("missing --input")?);
    let out_md = PathBuf::from(flag(args, "--out").ok_or("missing --out")?);
    let limit: usize = flag(args, "--limit").map(|s| s.parse().unwrap_or(30)).unwrap_or(30);
    let offline = has_flag(args, "--offline");
    let snapshot = f::load_snapshot(&input)?;
    let mut rows = Vec::new();
    for (i, row) in snapshot.rows.iter().take(limit).enumerate() {
        let rank = i + 1;
        eprintln!("[{rank}/{limit}] {} ...", row.project);
        let signals = f::gather(&row.project, offline);
        rows.push(f::classify(rank, &row.project, row.download_count, &signals));
    }
    let md = f::render_markdown(&snapshot, &rows, limit, offline);
    if let Some(parent) = out_md.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&out_md, &md).map_err(|e| format!("{}: {e}", out_md.display()))?;
    let out_json = out_md.with_extension("json");
    let json_rows: Vec<serde_json::Value> = rows
        .iter()
        .map(|c| {
            serde_json::json!({
                "rank": c.rank,
                "project": c.project,
                "downloads": c.downloads,
                "dependents": c.dependents,
                "path": c.path.tag(),
                "alternative": c.alternative,
                "payoff": c.payoff,
                "why": c.why,
                "evidence": c.evidence,
            })
        })
        .collect();
    std::fs::write(&out_json, serde_json::to_string_pretty(&json_rows).unwrap())
        .map_err(|e| format!("{}: {e}", out_json.display()))?;
    let counts = |p| rows.iter().filter(|c| c.path == p).count();
    eprintln!(
        "wrote {} (+ {}) — adopt {}, mirror {}, upstream {}",
        out_md.display(),
        out_json.display(),
        counts(f::Path::Adopt),
        counts(f::Path::Mirror),
        counts(f::Path::Upstream),
    );
    Ok(())
}
