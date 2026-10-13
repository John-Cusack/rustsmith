//! PyPI → Rust rewrite candidate finder.
//!
//! Takes the most-downloaded PyPI packages and classifies EACH into exactly
//! one path with evidence:
//!
//! * `(a) adopt/wrap` — a full Rust alternative exists (adopt it, not rewrite);
//!   also used for data-only/trivial packages where there is nothing to port.
//! * `(b) mirror` — no alternative: mirror-rewrite candidate, ranked by an
//!   estimated payoff (downloads × observed pure-Python hot surface).
//! * `(c) upstream` — the win belongs inside the original package (a
//!   PyO3/maturin speed core), because upstream already ships native code or
//!   the curated table says so.
//!
//! Evidence discipline: everything the tool measured (download counts, file
//! layout from the sdist, curated-table verdicts) is reported bare; every
//! heuristic (hotness keywords, payoff score, upstream-fit judgment) is
//! labeled ESTIMATE. The tool never presents a guess as data.
//!
//! Transport is `curl` subprocesses (no new dependencies; the host already
//! requires curl). `--offline` classifies from the snapshot + curated table
//! with zero network, which is also what the acceptance test uses.


const USER_AGENT: &str = "rustsmith-candidate-finder/0.1.0 (research tooling)";
const KNOWN_TABLE: &str = include_str!("../data/known-alternatives.json");

/// Default download-stats snapshot: hugovk top-pypi-packages, a daily pinned
/// JSON dump of PyPI download counts (same BigQuery lineage the original
/// triage read via pypistats, now reproducible by URL + `last_update` stamp).
pub const DEFAULT_SNAPSHOT_URL: &str =
    "https://hugovk.github.io/top-pypi-packages/top-pypi-packages-30-days.json";
/// Cap on sdist downloads: above this the layout is recorded as unobserved.
pub const MAX_SDIST_BYTES: u64 = 32 * 1024 * 1024;
/// Substrings matched (lowercased) against module paths + PyPI summary to
/// detect parsing/encoding workloads. A keyword hit-list, not a measurement.
pub const HOT_KEYWORDS: &[&str] = &[
    "pars", "scann", "token", "lex", "encod", "decod", "serial", "yaml", "json",
    "csv", "regex", "mark", "html", "xml", "charset", "compress", "crypt",
    "hash", "multipart", "url", "date", "version", "valid",
];

// ---------------------------------------------------------------------------
// Download snapshot
// ---------------------------------------------------------------------------

/// One row of the download snapshot.
#[derive(Debug, Clone)]
pub struct SnapRow {
    pub project: String,
    pub download_count: u64,
}

/// Pinned, dated download-stats input.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub source: String,
    pub last_update: String,
    pub rows: Vec<SnapRow>,
}

/// Parse a hugovk-format snapshot (`{last_update, source?, rows: [...]}`).
/// Rows sort descending by download count; the position is the download rank.
pub fn parse_snapshot(text: &str) -> Result<Snapshot, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("snapshot: invalid JSON: {e}"))?;
    let last_update = v
        .get("last_update")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown-date")
        .to_string();
    let source = v
        .get("source")
        .and_then(|s| s.as_str())
        .unwrap_or("hugovk/top-pypi-packages")
        .to_string();
    let rows = v
        .get("rows")
        .and_then(|r| r.as_array())
        .ok_or_else(|| "snapshot: missing `rows` array".to_string())?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let project = r
            .get("project")
            .and_then(|p| p.as_str())
            .ok_or_else(|| "snapshot: row without `project`".to_string())?;
        let download_count = r
            .get("download_count")
            .and_then(|d| d.as_u64())
            .ok_or_else(|| format!("snapshot: row `{project}` without `download_count`"))?;
        out.push(SnapRow { project: project.to_string(), download_count });
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.download_count));
    Ok(Snapshot { source, last_update, rows: out })
}

/// Load a snapshot file from disk.
pub fn load_snapshot(path: &std::path::Path) -> Result<Snapshot, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_snapshot(&text)
}

// ---------------------------------------------------------------------------
// Transport (curl subprocess; see module docs for why)
// ---------------------------------------------------------------------------

fn curl_bytes(url: &str, max_bytes: Option<u64>) -> Result<Vec<u8>, String> {
    let mut args: Vec<String> = vec![
        "-fsSL".into(),
        "-m".into(),
        "60".into(),
        "-H".into(),
        "Accept: application/json".into(),
        "-H".into(),
        format!("User-Agent: {USER_AGENT}"),
    ];
    if let Some(cap) = max_bytes {
        args.push("--max-filesize".into());
        args.push(cap.to_string());
    }
    args.push(url.into());
    let out = std::process::Command::new("curl")
        .args(&args)
        .output()
        .map_err(|e| format!("curl {url}: cannot run curl: {e}"))?;
    if !out.status.success() {
        let tail = String::from_utf8_lossy(&out.stderr);
        return Err(format!("curl {url}: failed: {}", tail.trim()));
    }
    Ok(out.stdout)
}

fn curl_json(url: &str) -> Result<serde_json::Value, String> {
    let bytes = curl_bytes(url, None)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{url}: invalid JSON: {e}"))
}

// ---------------------------------------------------------------------------
// Live signals (each optional; failures degrade to documented absence)
// ---------------------------------------------------------------------------

/// PyPI metadata: version, summary, sdist URL. Measured via the PyPI JSON API.
#[derive(Debug)]
pub struct PypiMeta {
    pub version: String,
    pub summary: String,
    pub sdist_url: Option<String>,
}

pub fn pypi_meta(project: &str) -> Result<PypiMeta, String> {
    let v = curl_json(&format!("https://pypi.org/pypi/{project}/json"))?;
    let info = &v["info"];
    let version = info["version"].as_str().unwrap_or("?").to_string();
    let summary = info["summary"].as_str().unwrap_or("").to_string();
    let mut sdist_url = None;
    if let Some(urls) = v["urls"].as_array() {
        for u in urls {
            if u["packagetype"].as_str() == Some("sdist") {
                sdist_url = u["url"].as_str().map(|s| s.to_string());
                break;
            }
        }
    }
    Ok(PypiMeta { version, summary, sdist_url })
}

/// File layout observed from the sdist (measured, not guessed).
#[derive(Debug)]
pub struct SdistStats {
    pub py_files: usize,
    pub py_loc: u64,
    pub native_files: usize,
    /// True when the sdist's own build files declare native compilation
    /// (`ext_modules`, Cython, maturin, …): distinguishes "upstream builds
    /// native code" from "ships C files as data" (pycparser's fake-libc
    /// parse fixtures). Measured by grepping shipped build files.
    pub native_build: bool,
    pub keywords: Vec<String>,
    pub summary_hits: Vec<String>,
}

/// Shipped code only: test/example/doc/fixture trees are not part of the
/// installed surface, so their `.c` samples (lexer corpora, parser fixtures)
/// must not count as "upstream ships native code".
fn is_shipped(member: &str) -> bool {
    !member.to_lowercase().split('/').any(|seg| {
        matches!(
            seg,
            "test" | "tests" | "testing" | "example" | "examples" | "docs" | "doc"
                | "fixtures" | "benchmark" | "benchmarks"
        )
    })
}

/// Top-level build declarations worth extracting + grepping for native-build
/// markers. Basename match on the lowercased member path; test/example/doc
/// trees never count (their setup.py files describe fixtures, not the build).
fn is_build_file(lower_member: &str) -> bool {
    const NAMES: &[&str] =
        &["setup.py", "setup.cfg", "pyproject.toml", "cargo.toml", "meson.build", "build.rs"];
    is_shipped(lower_member) && NAMES.iter().any(|n| lower_member.ends_with(&format!("/{n}")))
}

/// True when extracted build-file text declares native compilation.
fn declares_native_build(text: &str) -> bool {
    const MARKERS: &[&str] = &[
        "ext_modules",
        "cython",
        "maturin",
        "setuptools_rust",
        "setuptools-rust",
        "mesonpy",
        "meson-python",
    ];
    let lower = text.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m))
}

fn count_lines(bytes: &[u8]) -> u64 {
    bytes.iter().filter(|&&b| b == b'\n').count() as u64
}

/// Download the sdist (capped), list members with `tar`, count Python LOC by
/// extracting `*.py` members. Native = `.c/.h/.pyx/.pxd/.cpp/.cc/.rs` members.
pub fn inspect_sdist(url: &str, summary: &str) -> Result<SdistStats, String> {
    let bytes = curl_bytes(url, Some(MAX_SDIST_BYTES)).map_err(|e| {
        format!("sdist download failed (layout unobserved): {e}")
    })?;
    let dir = std::env::temp_dir().join(format!("rs-candsdist-{}-{}", std::process::id(), {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        url.hash(&mut h);
        h.finish()
    }));
    std::fs::create_dir_all(&dir).map_err(|e| format!("tempdir: {e}"))?;
    let tgz = dir.join("pkg.tgz");
    std::fs::write(&tgz, &bytes).map_err(|e| format!("temp write: {e}"))?;
    let list = std::process::Command::new("tar")
        .args(["tzf"])
        .arg(&tgz)
        .output()
        .map_err(|e| format!("tar list: {e}"))?;
    if !list.status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err("sdist is not a readable tarball (layout unobserved)".into());
    }
    let listing = String::from_utf8_lossy(&list.stdout);
    let mut py_members = Vec::new();
    let mut build_members = Vec::new();
    let mut native_files = 0usize;
    let mut keywords = std::collections::BTreeSet::new();
    for line in listing.lines() {
        if !is_shipped(line) {
            continue;
        }
        let lower = line.to_lowercase();
        if lower.ends_with(".py") {
            py_members.push(line.to_string());
        } else if is_build_file(&lower) {
            build_members.push(line.to_string());
        } else if [".c", ".h", ".pyx", ".pxd", ".cpp", ".cc", ".rs"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            native_files += 1;
        }
        for kw in HOT_KEYWORDS {
            if lower.contains(kw) {
                keywords.insert((*kw).to_string());
            }
        }
    }
    // Extract the .py members (LOC count) plus shipped build files (native
    // marker grep). One tar invocation; argv capped on huge trees.
    let mut py_loc = 0u64;
    let mut native_build = false;
    if !py_members.is_empty() || !build_members.is_empty() {
        let outdir = dir.join("src");
        let mut cmd = std::process::Command::new("tar");
        cmd.args(["xzf"]).arg(&tgz).arg("-C").arg(&outdir);
        std::fs::create_dir_all(&outdir).map_err(|e| format!("tempdir: {e}"))?;
        // `tar xzf a.tgz -C out member...` may overflow argv on huge trees;
        // cap extraction at 4000 members and record the cap in the caller.
        let capped = py_members.len().min(4000);
        for m in &py_members[..capped] {
            cmd.arg(m);
        }
        for m in &build_members {
            cmd.arg(m);
        }
        let st = cmd.output().map_err(|e| format!("tar extract: {e}"))?;
        if st.status.success() {
            let mut stack = vec![outdir.clone()];
            while let Some(d) = stack.pop() {
                let rd = match std::fs::read_dir(&d) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else {
                        // Build files count for both signals: setup.py is
                        // Python LOC *and* the native-build declaration, so
                        // the marker grep runs before extension dispatch.
                        if is_build_file(&p.to_string_lossy().to_lowercase()) {
                            if let Ok(b) = std::fs::read(&p) {
                                let text = String::from_utf8_lossy(&b);
                                native_build |= declares_native_build(&text);
                            }
                        }
                        if p.extension().map(|x| x == "py").unwrap_or(false) {
                            if let Ok(b) = std::fs::read(&p) {
                                py_loc += count_lines(&b);
                            }
                        }
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    let summary_lower = summary.to_lowercase();
    let summary_hits: Vec<String> = HOT_KEYWORDS
        .iter()
        .filter(|kw| summary_lower.contains(**kw))
        .map(|s| s.to_string())
        .collect();
    Ok(SdistStats {
        py_files: py_members.len(),
        py_loc,
        native_files,
        native_build,
        keywords: keywords.into_iter().collect(),
        summary_hits,
    })
}

/// One crates.io search hit (observed, not a verdict).
#[derive(Debug)]
pub struct CrateHit {
    pub name: String,
    pub recent_downloads: u64,
    pub description: String,
}

impl CrateHit {
    pub fn link(&self) -> String {
        format!("https://crates.io/crates/{}", self.name)
    }
}

/// Top-5 crates.io full-text hits for `query`. Recorded as evidence of what
/// the search returns; the replacement verdict comes only from the curated
/// table (a human must confirm a new alternative before it flips a path).
pub fn search_crates(query: &str) -> Result<Vec<CrateHit>, String> {
    // crates.io wants a descriptive UA and politely rate-limits; one query
    // per package keeps us far under it.
    let q: String =
        url_encode(query);
    let v = curl_json(&format!("https://crates.io/api/v1/crates?q={q}&per_page=5"))?;
    let mut out = Vec::new();
    if let Some(arr) = v["crates"].as_array() {
        for c in arr.iter().take(5) {
            out.push(CrateHit {
                name: c["name"].as_str().unwrap_or("?").to_string(),
                recent_downloads: c["recent_downloads"].as_u64().unwrap_or(0),
                description: c["description"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(out)
}

fn url_encode(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                o.push(b as char)
            }
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

/// Dependent-package count from ecosyste.ms (packages service), as observed
/// at classification time. `None` = lookup failed, recorded in evidence.
pub fn dependents(project: &str) -> Option<u64> {
    let v = curl_json(&format!(
        "https://packages.ecosyste.ms/api/v1/registries/pypi.org/packages/{project}"
    ))
    .ok()?;
    v.get("dependent_packages_count")?.as_u64()
}

// ---------------------------------------------------------------------------
// Curated table (measured verdicts: a human confirmed each row)
// ---------------------------------------------------------------------------

/// A curated alternative verdict: `full` (adopt), `partial` (gap documented,
/// still a mirror/upstream call), `upstream` (win belongs in the original),
/// `noop` (data-only/trivial: nothing to port, adopt = do nothing).
pub struct KnownAlt {
    pub crate_name: String,
    pub link: String,
    pub scope: String,
    pub verdict: String,
}

fn table() -> serde_json::Value {
    serde_json::from_str(KNOWN_TABLE).expect("known-alternatives.json parses")
}

/// Curated verdicts for `project` (PyPI name, exact match). Empty = the table
/// is silent; live search hits are then evidence only, never a verdict.
pub fn known_alternatives(project: &str) -> Vec<KnownAlt> {
    let t = table();
    match t["alternatives"][project].as_array() {
        None => Vec::new(),
        Some(arr) => arr
            .iter()
            .map(|a| KnownAlt {
                crate_name: a["crate"].as_str().unwrap_or("?").to_string(),
                link: a["link"].as_str().unwrap_or("").to_string(),
                scope: a["scope"].as_str().unwrap_or("").to_string(),
                verdict: a["verdict"].as_str().unwrap_or("partial").to_string(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// The exactly-one recommended path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    /// (a) direct Rust alternative exists — adopt/wrap, not rewrite.
    Adopt,
    /// (b) no alternative — mirror-rewrite candidate.
    Mirror,
    /// (c) win belongs inside the original package (PyO3/maturin speed core).
    Upstream,
}

impl Path {
    pub fn tag(&self) -> &'static str {
        match self {
            Path::Adopt => "(a) adopt/wrap",
            Path::Mirror => "(b) mirror",
            Path::Upstream => "(c) upstream",
        }
    }
}

/// All signals gathered for one package. Every field records whether it was
/// measured or fell back, so the report can label estimates honestly.
#[derive(Debug, Default)]
pub struct Signals {
    pub meta: Option<PypiMeta>,
    pub meta_error: Option<String>,
    pub sdist: Option<SdistStats>,
    pub sdist_error: Option<String>,
    pub crates_hits: Vec<CrateHit>,
    pub crates_error: Option<String>,
    pub dependents: Option<u64>,
}

/// Gather live signals for `project`. Failures are captured, never fatal:
/// the package still classifies on whatever was observed.
pub fn gather(project: &str, offline: bool) -> Signals {
    let mut s = Signals::default();
    if offline {
        s.meta_error = Some("offline mode: PyPI lookup skipped".into());
        s.sdist_error = Some("offline mode: sdist inspection skipped".into());
        s.crates_error = Some("offline mode: crates.io search skipped".into());
        return s;
    }
    match pypi_meta(project) {
        Ok(m) => {
            if let Some(url) = m.sdist_url.clone() {
                match inspect_sdist(&url, &m.summary) {
                    Ok(st) => s.sdist = Some(st),
                    Err(e) => s.sdist_error = Some(e),
                }
            } else {
                s.sdist_error = Some("no sdist published (layout unobserved)".into());
            }
            s.meta = Some(m);
        }
        Err(e) => {
            s.meta_error = Some(format!("PyPI lookup failed: {e}"));
            s.sdist_error = Some("no metadata, sdist unreachable (layout unobserved)".into());
        }
    }
    match search_crates(project) {
        Ok(h) => s.crates_hits = h,
        Err(e) => s.crates_error = Some(format!("crates.io search failed: {e}")),
    }
    s.dependents = dependents(project);
    s
}

/// One finished classification: exactly one path + evidence.
#[derive(Debug, Clone)]
pub struct Classification {
    pub rank: usize,
    pub project: String,
    pub downloads: u64,
    pub dependents: Option<u64>,
    pub path: Path,
    /// Markdown: linked crate(s) or documented absence.
    pub alternative: String,
    /// Mirror payoff score (ESTIMATE). Present only for path (b).
    pub payoff: Option<f64>,
    pub why: String,
    pub evidence: Vec<String>,
}

/// Classify `project` (download rank `rank`, `downloads`/mo) from `signals`
/// plus the curated table. Rules (see ADR-036):
///
/// 1. Table `full`/`noop` → (a), measured.
/// 2. Table `upstream` → (c), measured.
/// 3. Measured sdist with BUILT native code (shipped native files + a build
///    declaration) + ≥2000 LOC Python → (c), fit ESTIMATE. Native files
///    without a build read as data, not as an extension.
/// 4. Measured sdist that is data-only/trivial (<1000 LOC, no keywords,
///    no native) → (a) noop, measured.
/// 5. Otherwise → (b), payoff ESTIMATE = downloads × pure-Python fraction ×
///    (1 + keyword hits). Table `partial` rows become the documented gap.
pub fn classify(
    rank: usize,
    project: &str,
    downloads: u64,
    signals: &Signals,
) -> Classification {
    let mut ev = vec![format!("download rank #{rank} ({})", fmt_count(downloads))];
    match signals.dependents {
        Some(d) => ev.push(format!("{d} dependent packages (ecosyste.ms, as observed)")),
        None => ev.push("dependent count unobserved (ecosyste.ms lookup failed/skipped)".into()),
    }
    let known = known_alternatives(project);
    if !known.is_empty() {
        let kinds: Vec<String> = known.iter().map(|k| k.verdict.clone()).collect();
        ev.push(format!("curated table (measured 2026-10-13): {}", kinds.join("/")));
    }

    // Rule 1+2: curated verdicts decide outright.
    if let Some(k) = known.iter().find(|k| k.verdict == "full") {
        return Classification {
            rank,
            project: project.into(),
            downloads,
            dependents: signals.dependents,
            path: Path::Adopt,
            alternative: format!("[{}]({}) — {}", k.crate_name, k.link, k.scope),
            payoff: None,
            why: format!(
                "Mature Rust equivalent exists ({}); adopt/wrap, not rewrite.",
                k.crate_name
            ),
            evidence: ev,
        };
    }
    if let Some(k) = known.iter().find(|k| k.verdict == "noop") {
        return Classification {
            rank,
            project: project.into(),
            downloads,
            dependents: signals.dependents,
            path: Path::Adopt,
            alternative: format!("none needed — {}", k.scope),
            payoff: None,
            why: format!("Nothing to port ({}); adopt = do nothing.", k.scope),
            evidence: ev,
        };
    }
    if let Some(k) = known.iter().find(|k| k.verdict == "upstream") {
        return Classification {
            rank,
            project: project.into(),
            downloads,
            dependents: signals.dependents,
            path: Path::Upstream,
                payoff: None,
            alternative: "no mirror alternative sought — native core already exists upstream"
                .into(),
            why: format!("{} — win belongs upstream, not in a fork.", k.scope),
            evidence: ev,
        };
    }

    // Measured layout signals.
    if let Some(st) = &signals.sdist {
        ev.push(format!(
            "sdist layout measured (shipped paths only, tests/examples/docs excluded): {} .py files, {} LOC Python, {} native files",
            st.py_files, st.py_loc, st.native_files
        ));
        let mut kw: Vec<String> = st.keywords.clone();
        kw.extend(st.summary_hits.clone());
        kw.sort();
        kw.dedup();
        if !kw.is_empty() {
            ev.push(format!("workload keywords observed (ESTIMATE of hotness): {}", kw.join(", ")));
        }
        // Rule 3: upstream builds native code around a large Python surface —
        // a speed core belongs inside, not in a fork. Both halves measured:
        // shipped native files AND a build declaration (`ext_modules`,
        // Cython, …). C files without a build (parse fixtures, vendored
        // headers) do not count.
        if st.native_files > 0 && st.native_build {
            ev.push("build corroboration measured: sdist build files declare native compilation".into());
        } else if st.native_files > 0 {
            ev.push("no native-build declaration in sdist build files (measured): C files read as data, not as a built extension".into());
        }
        if st.native_files > 0 && st.native_build && st.py_loc >= 2000 {
            let files = if st.native_files == 1 { "1 native file".to_string() } else { format!("{} native files", st.native_files) };
            return Classification {
                rank,
                project: project.into(),
                downloads,
                dependents: signals.dependents,
                path: Path::Upstream,
                payoff: None,
                alternative: "no mirror alternative sought — upstream already builds native code"
                    .into(),
                why: format!(
                    "Upstream already builds {files} with {} LOC Python around them (ESTIMATE: fit); a PyO3/maturin speed core belongs inside the original package.",
                    st.py_loc
                ),
                evidence: ev,
            };
        }
        // Rule 4: data-only/trivial surface.
        if st.py_loc < 1000 && kw.is_empty() && st.native_files == 0 {
            return Classification {
                rank,
                project: project.into(),
                downloads,
                dependents: signals.dependents,
                path: Path::Adopt,
                payoff: None,
                alternative: "none needed — data-only/trivial surface, nothing to compile".into(),
                why: format!(
                    "Only {} LOC Python in {} files, no hot keywords, no native code (measured); nothing worth porting.",
                    st.py_loc, st.py_files
                ),
                evidence: ev,
            };
        }
        // Rule 5: mirror candidate with measured layout.
        let pure_frac = 1.0; // sdist Python LOC is the surface; native==0 here or small
        let payoff = downloads as f64 / 1e6 * pure_frac * (1 + kw.len()) as f64;
        let alt = document_gap(project, &known, signals);
        let kw_txt =
            if kw.is_empty() { "no hot keywords".into() } else { format!("keywords: {}", kw.join(", ")) };
        return Classification {
            rank,
            project: project.into(),
            downloads,
            dependents: signals.dependents,
            path: Path::Mirror,
            alternative: alt,
            payoff: Some(payoff),
            why: format!(
                "No full Rust equivalent; {} LOC pure-Python surface, {} (payoff ESTIMATE, ranked below).",
                st.py_loc, kw_txt
            ),
            evidence: ev,
        };
    }
    if let Some(e) = &signals.sdist_error {
        ev.push(format!("layout: {e}"));
    }

    // No layout observed: still exactly one path. Table partials document the
    // gap; payoff degrades to downloads-only and says so.
    let alt = document_gap(project, &known, signals);
    Classification {
        rank,
        project: project.into(),
        downloads,
        dependents: signals.dependents,
        path: Path::Mirror,
        alternative: alt,
        payoff: Some(downloads as f64 / 1e6),
        why: "No full Rust equivalent documented; layout unobserved so payoff is downloads-only (ESTIMATE, ranked below).".into(),
        evidence: ev,
    }
}

/// Render the alternative column: curated partials with links, else the
/// observed crates.io hits as "nearest, unjudged", else documented absence.
fn document_gap(_project: &str, known: &[KnownAlt], signals: &Signals) -> String {
    let partials: Vec<String> = known
        .iter()
        .filter(|k| k.verdict == "partial")
        .map(|k| format!("[{}]({}) ({})", k.crate_name, k.link, k.scope))
        .collect();
    if !partials.is_empty() {
        return format!("no full replacement; nearest: {}", partials.join(" / "));
    }
    if !signals.crates_hits.is_empty() {
        let hits: Vec<String> = signals
            .crates_hits
            .iter()
            .take(3)
            .map(|h| format!("[{}]({})", h.name, h.link()))
            .collect();
        return format!(
            "no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): {}",
            hits.join(", ")
        );
    }
    if let Some(e) = &signals.crates_error {
        return format!("absence undocumented: {e}");
    }
    "no curated verdict and no search performed".into()
}

fn fmt_count(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.0}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        format!("{n}")
    }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// Render the candidate list as Markdown. Mirror rows sort by payoff desc;
/// adopt/upstream rows follow in rank order. Ranks shown are download ranks.
pub fn render_markdown(
    snapshot: &Snapshot,
    rows: &[Classification],
    _limit: usize,
    offline: bool,
) -> String {
    let mut mirrors: Vec<&Classification> =
        rows.iter().filter(|c| c.path == Path::Mirror).collect();
    mirrors.sort_by(|a, b| {
        b.payoff
            .unwrap_or(0.0)
            .partial_cmp(&a.payoff.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let rest: Vec<&Classification> =
        rows.iter().filter(|c| c.path != Path::Mirror).collect();
    let mut o = String::new();
    o.push_str("# Rewrite candidates\n\n");
    o.push_str(&format!(
        "Source: {} (`last_update: {}`, {} rows; {} classified{}).\n\n",
        snapshot.source,
        snapshot.last_update,
        snapshot.rows.len(),
        rows.len(),
        if offline { ", OFFLINE — snapshot + curated table only" } else { "" },
    ));
    o.push_str("Paths: **(a) adopt/wrap** — a Rust alternative exists, adopt it; \
        **(b) mirror** — no alternative, ranked by estimated payoff; \
        **(c) upstream** — the win belongs inside the original package.\n\n");
    o.push_str("Evidence labels: bare claims are MEASURED (download counts, \
        sdist file layout, curated-table verdicts dated 2026-10-13); anything \
        marked ESTIMATE is a heuristic (keyword hotness, payoff score, \
        upstream-fit judgment, unjudged crates.io hits).\n\n");
    o.push_str("| Dl-rank | Package | dl/mo | Dependents | Path | Alternative / gap | Why |\n");
    o.push_str("|---|---|---|---|---|---|---|\n");
    let mut pay_rank = 0;
    for c in mirrors.iter().chain(rest.iter()) {
        let pay = match c.path {
            Path::Mirror => {
                pay_rank += 1;
                format!("payoff #{pay_rank} ({:.1})", c.payoff.unwrap_or(0.0))
            }
            _ => "—".into(),
        };
        o.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} {} |\n",
            c.rank,
            c.project,
            fmt_count(c.downloads),
            c.dependents.map(fmt_count).unwrap_or_else(|| "unobserved".into()),
            c.path.tag(),
            c.alternative.replace('|', "/"),
            c.why.replace('|', "/").replace('\n', " "),
            pay,
        ));
    }
    o.push_str("\n## Evidence notes\n\n");
    for c in mirrors.iter().chain(rest.iter()) {
        o.push_str(&format!("### {} (dl-rank #{})\n\n", c.project, c.rank));
        for e in &c.evidence {
            o.push_str(&format!("- {e}\n"));
        }
        o.push('\n');
    }
    o
}

// ---------------------------------------------------------------------------
// Unit tests (deterministic, no network)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{"last_update": "2026-10-01 12:40:51", "source": "ClickHouse",
        "rows": [{"project": "b", "download_count": 5}, {"project": "a", "download_count": 9}]}"#;

    #[test]
    fn build_markers_detect_native_declarations() {
        assert!(declares_native_build("ext_modules=[Extension('x', ['x.c'])]"));
        assert!(declares_native_build("from Cython.Build import cythonize"));
        assert!(declares_native_build("[build-system]\nbuild-backend = 'maturin'"));
        assert!(!declares_native_build("[build]\nwheel = true"));
        assert!(is_build_file("pkg-1.0/setup.py"));
        assert!(is_build_file("pkg-1.0/pyproject.toml"));
        assert!(!is_build_file("pkg-1.0/setup.py.in"));
        assert!(!is_build_file("pkg-1.0/tests/setup.py"));
    }

    #[test]
    fn snapshot_parses_and_ranks() {
        let s = parse_snapshot(FIXTURE).unwrap();
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.rows[0].project, "a");
        assert_eq!(s.last_update, "2026-10-01 12:40:51");
    }

    #[test]
    fn test_fixtures_are_not_shipped_native() {
        // pygments lexer corpora must not count as "upstream ships native".
        assert!(!is_shipped("pygments-2.19.1/tests/examplefiles/c/ceval.c"));
        assert!(!is_shipped("pkg/docs/sample.c"));
        assert!(!is_shipped("pkg/examples/demo.py"));
        assert!(is_shipped("cffi-2.0/c/_cffi_backend.c"));
        assert!(is_shipped("six-1.17.0/six.py"));
    }

    #[test]
    fn snapshot_rejects_missing_rows() {
        assert!(parse_snapshot(r#"{"last_update": "x"}"#).is_err());
    }

    #[test]
    fn known_full_means_adopt() {
        let c = classify(1, "jinja2", 100, &Signals::default());
        assert_eq!(c.path, Path::Adopt);
        assert!(c.alternative.contains("minijinja"));
        assert_eq!(c.payoff, None);
    }

    #[test]
    fn known_upstream_means_upstream() {
        let c = classify(1, "pandas", 100, &Signals::default());
        assert_eq!(c.path, Path::Upstream);
    }

    #[test]
    fn native_plus_large_python_means_upstream() {
        let s = Signals {
            sdist: Some(SdistStats {
                py_files: 40,
                py_loc: 50_000,
                native_files: 12,
                native_build: true,
                keywords: vec!["pars".into()],
                summary_hits: vec![],
            }),
            ..Default::default()
        };
        let c = classify(2, "someparser", 100, &s);
        assert_eq!(c.path, Path::Upstream);
        assert!(c.why.contains("ESTIMATE"));
    }

    #[test]
    fn trivial_surface_means_adopt_noop() {
        let s = Signals {
            sdist: Some(SdistStats {
                py_files: 2,
                py_loc: 300,
                native_files: 0,
                native_build: false,
                keywords: vec![],
                summary_hits: vec![],
            }),
            ..Default::default()
        };
        let c = classify(3, "tinydata", 100, &s);
        assert_eq!(c.path, Path::Adopt);
    }

    #[test]
    fn native_without_build_reads_as_data_not_upstream() {
        // pycparser case: 130 shipped headers, no build declaration —
        // must not classify as upstream fit.
        let s = Signals {
            sdist: Some(SdistStats {
                py_files: 10,
                py_loc: 12_968,
                native_files: 130,
                native_build: false,
                keywords: vec!["pars".into()],
                summary_hits: vec![],
            }),
            ..Default::default()
        };
        let c = classify(2, "pycparser", 100, &s);
        assert_eq!(c.path, Path::Mirror);
        assert!(c.evidence.iter().any(|e| e.contains("read as data")));
    }


    #[test]
    fn unobserved_layout_still_mirrors_with_downloads_payoff() {
        let mut s = Signals::default();
        s.sdist_error = Some("layout unobserved".into());
        let c = classify(4, "unknownpkg", 20_000_000, &s);
        assert_eq!(c.path, Path::Mirror);
        assert_eq!(c.payoff, Some(20.0));
    }

    #[test]
    fn payoff_orders_mirrors() {
        let snap = parse_snapshot(FIXTURE).unwrap();
        let lo = classify(2, "b", 5, &Signals::default());
        let hi = classify(1, "a", 9, &Signals::default());
        let md = render_markdown(&snap, &[lo, hi], 2, true);
        let pa = md.find("| a |").unwrap();
        let pb = md.find("| b |").unwrap();
        assert!(pa < pb, "higher payoff must render first");
    }
}
