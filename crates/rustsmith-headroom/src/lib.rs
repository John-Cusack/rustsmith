//! Pre-port headroom probe: predicted Rust speedup for a PyPI candidate.
//!
//! A port lane costs weeks; download counts alone cannot tell whether a Rust
//! rewrite will actually make a package faster. The probe installs the
//! candidate from PyPI into a scratch venv (caller-owned), runs a
//! representative workload (package benchmarks if present, else a
//! doc-driven synthetic) under a sampler (py-spy preferred, cProfile
//! fallback — the row records which ran), and classifies hot-spot time into
//! five buckets:
//!
//! * `pure_python` — CPython eval-loop time, including its C micro-ops
//!   (`len`, `dict.get`, `list.append`, `re.Match.group` …). This is the
//!   headroom a Rust rewrite recovers.
//! * `native` — O(n) data-crunching already in C: `bytes.find`, `_codecs`
//!   decoders, `zlib`, plus frames from the package's own compiled modules
//!   (mypyc/Cython `.so` files keep their `.py` filenames under cProfile,
//!   so the driver records `sys.modules` `__file__` values to unmask them).
//! * `io_wait` — socket/file reads, writes, sleeps, locks: a mirror ties.
//! * `regex` — the shared C `re` engine: a port ties with itself.
//! * `overhead` — import machinery, `codecs` lookup, `abc`/`typing`.
//!
//! Score = `pure_python × loop_factor`, with three learned caps (see
//! [`score_verdict`]): shared-engine tie, I/O-bound mainline, and the
//! saturated-native thin profile (ships a compiled hot path and leaves no
//! Python hot loop behind — the charset lesson). Loop-nature follows the
//! port-boundary rule (playbook §7: keep the hot path on one side of the
//! seam): tight buffer loops score highest, dispatch-heavy plugin seams
//! lowest.
//!
//! Evidence discipline (same as the finder): every figure is measured on the
//! box named in the row; unmeasured candidates get no row, never a guess.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Thresholds (tuned on the four validation workloads, 2026-10-14)
// ---------------------------------------------------------------------------

/// Score at or above this predicts a rewrite win.
pub const REWRITE_MIN: f64 = 0.50;
/// Score at or above this (but below [`REWRITE_MIN`]) is upstream-only.
pub const UPSTREAM_MIN: f64 = 0.25;
/// `regex` fraction at or above this caps the verdict: shared-engine tie.
pub const REGEX_CAP: f64 = 0.35;
/// `io_wait` fraction at or above this caps the verdict: mirror ties on wait.
pub const IO_CAP: f64 = 0.50;
/// A workload under both of these has no hot spot worth a port lane.
pub const THIN_WALL_SECS: f64 = 2.0;
pub const THIN_TOTTIME_SECS: f64 = 0.5;
/// Reported score ceiling for the saturated-native profile.
pub const SATURATED_SCORE_CAP: f64 = 0.10;

// ---------------------------------------------------------------------------
// Buckets
// ---------------------------------------------------------------------------

/// One classified time bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    PurePython,
    Native,
    IoWait,
    Regex,
    Overhead,
}

impl Bucket {
    pub fn name(self) -> &'static str {
        match self {
            Bucket::PurePython => "pure_python",
            Bucket::Native => "native",
            Bucket::IoWait => "io_wait",
            Bucket::Regex => "regex",
            Bucket::Overhead => "overhead",
        }
    }
}

/// O(1) interpreter fabric: C micro-ops that are part of pure-Python
/// execution cost (they vanish in Rust) as opposed to O(n) C engines.
/// Only reached for `~` (builtin) frames that match neither [`IO_VERBS`]
/// nor [`NATIVE_CRUNCHERS`]; listed here for documentation, not matching.
pub const IO_VERBS: &[&str] = &[
    "read", "write", "recv", "send", "flush", "sleep", "wait", "lock", "acquire", "release",
    "socket", "connect", "accept", "poll", "select", "unlink", "mkdir", "rmtree", "fileno",
];

/// O(n) data-crunching C calls: already optimal, not rewrite headroom.
pub const NATIVE_CRUNCHERS: &[&str] = &[
    "find", "index", "replace", "split", "partition", "decode", "encode", "translate",
    "charmap_decode", "utf_16", "utf_32", "utf-8", "utf_7", "decompress", "compress", "crc32",
    "adler32", "md5", "sha", "blake", "hmac", "match", "search", "fullmatch", "unicodedata",
    "binascii", "_pickle", "_json", "mmap", "escape",
];

/// Modules that are interpreter overhead, never headroom.
const OVERHEAD_MODULES: &[&str] = &[
    "abc.py",
    "typing.py",
    "inspect.py",
    "pkgutil.py",
    "codecs.py",
];

fn stem_of(file: &str) -> String {
    file.rsplit('/').next().unwrap_or(file).to_lowercase()
}

/// Classify one profiled function. `native_stems` holds lowercased module
/// stems (e.g. `md.py`) of the probed package's own compiled (`.so`/`.pyd`)
/// modules, which cProfile reports under their `.py` source names.
pub fn classify_func(file: &str, func: &str, native_stems: &HashSet<String>) -> Bucket {
    if file == "~" {
        // Builtin/method frames (co_filename "~"): the func string carries
        // the owner (`<method 'match' of 're.Pattern' objects>`).
        let fl = func.to_lowercase();
        if fl.contains("re.pattern") || fl.contains("re.match") {
            return Bucket::Regex;
        }
        if IO_VERBS.iter().any(|v| fl.contains(v)) {
            return Bucket::IoWait;
        }
        if NATIVE_CRUNCHERS.iter().any(|k| fl.contains(k)) {
            return Bucket::Native;
        }
        return Bucket::PurePython;
    }
    let base = stem_of(file);
    if native_stems.contains(&base) {
        // Compiled package modules (mypyc/Cython) do their work
        // invisibly to sampling; the frames that do surface are native.
        return Bucket::Native;
    }
    if base == "re.py" || base.contains("sre_") {
        return Bucket::Regex;
    }
    if base == "io.py"
        || ["socket", "ssl", "selectors", "asyncio", "urllib", "http", "ftplib"]
            .iter()
            .any(|k| base.contains(k))
    {
        return Bucket::IoWait;
    }
    if base.starts_with("<frozen") || base.contains("importlib") || OVERHEAD_MODULES.contains(&base.as_str()) {
        return Bucket::Overhead;
    }
    Bucket::PurePython
}

// ---------------------------------------------------------------------------
// Classification over a profile
// ---------------------------------------------------------------------------

/// One profiled function (cProfile pstats shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncRow {
    pub file: String,
    pub func: String,
    pub ncalls: u64,
    pub tottime: f64,
    pub cumtime: f64,
}

/// Classified time fractions (sum to 1.0 unless total is 0.0).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Fractions {
    pub pure_python: f64,
    pub native: f64,
    pub io_wait: f64,
    pub regex: f64,
    pub overhead: f64,
}

impl Fractions {
    pub fn of(&self, b: Bucket) -> f64 {
        match b {
            Bucket::PurePython => self.pure_python,
            Bucket::Native => self.native,
            Bucket::IoWait => self.io_wait,
            Bucket::Regex => self.regex,
            Bucket::Overhead => self.overhead,
        }
    }
}

/// Concentration signals for the loop-nature call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concentration {
    /// Top-1 function share of total profiled time.
    pub top1: f64,
    /// Top-5 functions share.
    pub top5: f64,
    /// Distinct file stems among the top-15 functions (`~` counts as one).
    pub hot_modules: usize,
}

/// Classify profile rows: bucket fractions plus concentration signals.
/// Returns the fractions, the concentration, and the top-8 functions.
pub fn classify_rows(
    rows: &[FuncRow],
    native_stems: &HashSet<String>,
) -> (Fractions, Concentration, Vec<FuncRow>) {
    let total: f64 = rows.iter().map(|r| r.tottime).sum();
    let mut frac = Fractions::default();
    if total > 0.0 {
        for r in rows {
            let t = r.tottime / total;
            match classify_func(&r.file, &r.func, native_stems) {
                Bucket::PurePython => frac.pure_python += t,
                Bucket::Native => frac.native += t,
                Bucket::IoWait => frac.io_wait += t,
                Bucket::Regex => frac.regex += t,
                Bucket::Overhead => frac.overhead += t,
            }
        }
    }
    let mut sorted: Vec<FuncRow> = rows.to_vec();
    sorted.sort_by(|a, b| b.tottime.partial_cmp(&a.tottime).unwrap_or(std::cmp::Ordering::Equal));
    let top1 = sorted.first().map(|r| r.tottime / total).unwrap_or(0.0);
    let top5: f64 = sorted.iter().take(5).map(|r| r.tottime / total).sum();
    let hot_modules: usize = {
        let mut seen = HashSet::new();
        for r in sorted.iter().take(15) {
            seen.insert(if r.file == "~" { "~".to_string() } else { stem_of(&r.file) });
        }
        seen.len()
    };
    let top = sorted.into_iter().take(8).collect();
    (
        frac,
        Concentration { top1, top5, hot_modules },
        top,
    )
}

// ---------------------------------------------------------------------------
// Loop nature + score + verdict
// ---------------------------------------------------------------------------

/// Loop nature of the hot spot (playbook §7: the hot path must sit on one
/// side of the port seam; tight buffer loops port cleanly, dispatch-heavy
/// plugin seams do not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopNature {
    /// One hot loop over buffers; the whole unit ports as one seam.
    TightLoop,
    /// A small pipeline of hot stages (scanner/reader/parser); ports well.
    Pipeline,
    /// Time spread over many small dispatch frames; seam-hostile.
    Dispatch,
    /// No Python hot loop left: the compiled core already owns the path.
    ThinNative,
}

impl LoopNature {
    pub fn factor(self) -> f64 {
        match self {
            LoopNature::TightLoop => 1.0,
            LoopNature::Pipeline => 0.85,
            LoopNature::Dispatch => 0.5,
            LoopNature::ThinNative => 0.15,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LoopNature::TightLoop => "tight_loop",
            LoopNature::Pipeline => "pipeline",
            LoopNature::Dispatch => "dispatch",
            LoopNature::ThinNative => "thin_native",
        }
    }
}

pub fn loop_nature(c: &Concentration, thin: bool, saturated: bool) -> LoopNature {
    if thin && saturated {
        return LoopNature::ThinNative;
    }
    if c.top1 >= 0.25 || (c.top5 >= 0.55 && c.hot_modules <= 3) {
        LoopNature::TightLoop
    } else if c.hot_modules >= 8 || c.top5 < 0.25 {
        LoopNature::Dispatch
    } else {
        LoopNature::Pipeline
    }
}

/// Probe verdict: the predicted lane outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// High pure-Python headroom in a portable loop: launch the lane.
    Rewrite,
    /// Headroom exists but belongs upstream (shared engine, I/O, or a
    /// mature native core with a proven kernel): probe the speed core only.
    UpstreamOnly,
    /// No winnable headroom: saturated native core or nothing hot.
    Exclude,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Rewrite => "rewrite",
            Verdict::UpstreamOnly => "upstream_only",
            Verdict::Exclude => "exclude",
        }
    }

    /// Finder disposition for the learned headroom rule (ADR-043 rule 7):
    /// `rewrite` carries no disposition (evidence only).
    pub fn disposition(self) -> Option<&'static str> {
        match self {
            Verdict::Rewrite => None,
            Verdict::UpstreamOnly => Some("upstream_only"),
            Verdict::Exclude => Some("exclude"),
        }
    }
}

/// Score the classified workload. Returns `(score, verdict, why)`.
///
/// * `native_cover` — the probed package's own `.so`/`.pyd`-backed modules
///   imported during the workload (measured from `sys.modules`).
/// * `thin` — the workload left no hot spot (`wall < 2s` and `tottime < 0.5s`).
pub fn score_verdict(
    frac: &Fractions,
    nature: LoopNature,
    native_cover: &[String],
    thin: bool,
) -> (f64, Verdict, String) {
    let raw = frac.pure_python * nature.factor();
    if thin && !native_cover.is_empty() {
        // Saturated native core (the charset lesson): the package ships a
        // compiled hot path and the profile shows no Python loop behind it —
        // cProfile only sees the thin dispatch skin. ~0% is not a win.
        return (
            raw.min(SATURATED_SCORE_CAP),
            Verdict::Exclude,
            format!(
                "saturated native core: ships compiled {} and leaves no Python hot loop \
                 (thin profile, pure-python residual {:.0}% is dispatch skin); do not nominate",
                native_cover.join("/"),
                100.0 * frac.pure_python,
            ),
        );
    }
    if frac.regex >= REGEX_CAP {
        return (
            raw,
            Verdict::UpstreamOnly,
            format!(
                "shared-engine tie: {:.0}% of hot time already runs in the C re engine; \
                 only the Python dispatch around it ({:.0}%) is portable",
                100.0 * frac.regex,
                100.0 * frac.pure_python,
            ),
        );
    }
    if frac.io_wait >= IO_CAP {
        return (
            raw,
            Verdict::UpstreamOnly,
            format!(
                "I/O-bound mainline: {:.0}% of hot time is wait; a mirror would tie on it",
                100.0 * frac.io_wait,
            ),
        );
    }
    if raw >= REWRITE_MIN {
        (
            raw,
            Verdict::Rewrite,
            format!(
                "{:.0}% pure-Python in a {} loop: portable headroom predicts a rewrite win",
                100.0 * frac.pure_python,
                nature.name(),
            ),
        )
    } else if raw >= UPSTREAM_MIN {
        (
            raw,
            Verdict::UpstreamOnly,
            format!(
                "middling headroom ({:.0}% pure-Python, {} loop): probe a speed core upstream, not a mirror",
                100.0 * frac.pure_python,
                nature.name(),
            ),
        )
    } else {
        (
            raw,
            Verdict::Exclude,
            format!(
                "low headroom ({:.0}% pure-Python, {} loop): no winnable hot spot",
                100.0 * frac.pure_python,
                nature.name(),
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// Probe rows (the machine-written board source)
// ---------------------------------------------------------------------------

/// The representative workload that produced the row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadDesc {
    /// Short workload name, e.g. `safe_load-4000svc`.
    pub name: String,
    /// `package-bench` (shipped benchmarks) or `doc-synthetic` (doc-driven).
    pub source: String,
    /// Trials inside the profiled window.
    pub iters: u32,
    /// One-line input description, e.g. `4000-service YAML doc, 3 loads`.
    pub detail: String,
}

/// One machine-written board row. Boards render from these rows, never from
/// hand-written markdown. Unmeasured packages get no row — never a guess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRow {
    pub package: String,
    pub version: String,
    pub workload: WorkloadDesc,
    /// Sampler that produced the fractions: `cprofile` or `py-spy`.
    pub tool: String,
    /// Measured-on box, e.g. `Ryzen 9 5950X`.
    pub machine: String,
    pub python: String,
    pub wall_secs: f64,
    pub tottime_secs: f64,
    pub fractions: Fractions,
    pub concentration: Concentration,
    /// Top-8 functions by self time (`file:func`, ms, calls).
    pub top: Vec<TopFunc>,
    pub loop_nature: LoopNature,
    pub loop_factor: f64,
    /// Package-owned compiled modules imported during the workload.
    pub native_cover: Vec<String>,
    pub score: f64,
    pub verdict: Verdict,
    pub why: String,
}

/// Top-function entry for the row (render-sized, not full pstats).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopFunc {
    pub file: String,
    pub func: String,
    pub tottime_ms: f64,
    pub ncalls: u64,
}

impl ProbeRow {
    /// Finder disposition for the learned headroom rule, if any.
    pub fn disposition(&self) -> Option<&'static str> {
        self.verdict.disposition()
    }

    /// One-line measured evidence for the finder board.
    pub fn evidence_line(&self) -> String {
        format!(
            "headroom probe (measured {} on {}, {} {}): workload `{}` ({} iters, {}), \
             pure-Python {:.0}% / native {:.0}% / io-wait {:.0}% / regex {:.0}% / overhead {:.0}%, \
             {} loop (top1 {:.0}%, top5 {:.0}%), score {:.2} → {} — {}",
            self.tool,
            self.machine,
            self.python,
            self.workload.source,
            self.workload.name,
            self.workload.iters,
            self.workload.detail,
            100.0 * self.fractions.pure_python,
            100.0 * self.fractions.native,
            100.0 * self.fractions.io_wait,
            100.0 * self.fractions.regex,
            100.0 * self.fractions.overhead,
            self.loop_nature.name(),
            100.0 * self.concentration.top1,
            100.0 * self.concentration.top5,
            self.score,
            self.verdict.name(),
            self.why,
        )
    }
}

// ---------------------------------------------------------------------------
// Running the probe
// ---------------------------------------------------------------------------

/// Raw driver output: profile rows plus the module-file map.
#[derive(Debug)]
pub struct RawProfile {
    pub funcs: Vec<FuncRow>,
    pub modules: HashMap<String, String>,
    pub wall_secs: f64,
    pub version: String,
}

const DRIVER_PY: &str = r#"
import cProfile, importlib.metadata, json, pstats, sys, time
sys.path.insert(0, __RUNDIR__)
import wl
if hasattr(wl, "setup"):
    wl.setup()
wl.run()
pr = cProfile.Profile()
pr.enable()
t0 = time.perf_counter()
wl.run()
pr.disable()
wall = time.perf_counter() - t0
st = pstats.Stats(pr)
funcs = [
    {"file": f, "line": ln, "func": fn, "ncalls": nc, "tottime": tt, "cumtime": ct}
    for (f, ln, fn), (cc, nc, tt, ct, _callers) in st.stats.items()
]
mods = {}
for name, m in sys.modules.items():
    f = getattr(m, "__file__", None)
    if f:
        mods[name] = f
try:
    version = importlib.metadata.version(__PACKAGE__)
except Exception:
    version = "unknown"
json.dump(
    {"funcs": funcs, "modules": mods, "wall_secs": wall, "version": version},
    open(__OUT__, "w"),
)
"#;

#[allow(clippy::too_many_arguments)]
/// Run the probe: execute `workload_file` (must define `run()`, may define
/// `setup()`) under `python` with cProfile, classify, and score.
///
/// * `package` — PyPI name (for the row + version lookup).
/// * `import_root` — import name of the package under test (e.g. `yaml`
///   for PyPI `pyyaml`); used to detect compiled package modules.
/// * `meta` — workload/tool/machine/python labels for the row.
pub fn run_probe(
    python: &std::path::Path,
    workload_file: &std::path::Path,
    package: &str,
    import_root: &str,
    meta: &WorkloadDesc,
    tool: &str,
    machine: &str,
    python_version: &str,
) -> Result<ProbeRow, String> {
    let raw = run_cprofile(python, workload_file, package)?;
    Ok(build_row(package, import_root, meta, tool, machine, python_version, &raw))
}
#[allow(clippy::too_many_arguments)]
/// Execute the workload file under cProfile and return the raw profile.
pub fn run_cprofile(
    python: &std::path::Path,
    workload_file: &std::path::Path,
    package: &str,
) -> Result<RawProfile, String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let rundir = dir.path().join("rundir");
    std::fs::create_dir(&rundir).map_err(|e| format!("mkdir rundir: {e}"))?;
    let wl = std::fs::read(workload_file)
        .map_err(|e| format!("{}: {e}", workload_file.display()))?;
    std::fs::write(rundir.join("wl.py"), wl).map_err(|e| format!("write wl.py: {e}"))?;
    let out = dir.path().join("profile.json");
    let driver = DRIVER_PY
        .replace("__RUNDIR__", &format!("{:?}", rundir.to_string_lossy()))
        .replace("__OUT__", &format!("{:?}", out.to_string_lossy()))
        .replace("__PACKAGE__", &format!("{package:?}"));
    std::fs::write(dir.path().join("driver.py"), driver)
        .map_err(|e| format!("write driver.py: {e}"))?;
    let status = std::process::Command::new(python)
        .arg(dir.path().join("driver.py"))
        .output()
        .map_err(|e| format!("{}: cannot run python: {e}", python.display()))?;
    if !status.status.success() {
        return Err(format!(
            "workload failed under {}: {}",
            python.display(),
            String::from_utf8_lossy(&status.stderr)
        ));
    }
    let text =
        std::fs::read_to_string(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("profile json: {e}"))?;
    let funcs: Vec<FuncRow> =
        serde_json::from_value(v["funcs"].clone()).map_err(|e| format!("funcs: {e}"))?;
    let modules: HashMap<String, String> =
        serde_json::from_value(v["modules"].clone()).unwrap_or_default();
    Ok(RawProfile {
        funcs,
        modules,
        wall_secs: v["wall_secs"].as_f64().unwrap_or(0.0),
        version: v["version"].as_str().unwrap_or("unknown").into(),
    })
}

/// Package-owned compiled modules imported during the workload: names at or
/// under `import_root` whose file is a `.so`/`.pyd`.
pub fn native_cover(modules: &HashMap<String, String>, import_root: &str) -> Vec<String> {
    let mut out: Vec<String> = modules
        .iter()
        .filter(|(name, file)| {
            (*name == import_root || name.starts_with(&format!("{import_root}.")))
                && (file.ends_with(".so") || file.ends_with(".pyd"))
        })
        .map(|(name, _)| name.clone())
        .collect();
    out.sort();
    out
}

/// Stems (`md.py`) of the compiled cover, for [`classify_func`].
pub fn native_stems(modules: &HashMap<String, String>, import_root: &str) -> HashSet<String> {
    native_cover(modules, import_root)
        .iter()
        .filter_map(|n| {
            modules.get(n).map(|f| {
                f.rsplit('/').next().unwrap_or(f).to_lowercase().replace(".so", ".py")
            })
        })
        .collect()
}

/// Score a raw profile into a finished row.
#[allow(clippy::too_many_arguments)]
pub fn build_row(
    package: &str,
    import_root: &str,
    meta: &WorkloadDesc,
    tool: &str,
    machine: &str,
    python_version: &str,
    raw: &RawProfile,
) -> ProbeRow {
    let cover = native_cover(&raw.modules, import_root);
    let stems = native_stems(&raw.modules, import_root);
    let (fractions, concentration, top_rows) = classify_rows(&raw.funcs, &stems);
    let tottime: f64 = raw.funcs.iter().map(|r| r.tottime).sum();
    let thin = raw.wall_secs < THIN_WALL_SECS && tottime < THIN_TOTTIME_SECS;
    let nature = loop_nature(&concentration, thin, !cover.is_empty());
    let (score, verdict, why) = score_verdict(&fractions, nature, &cover, thin);
    ProbeRow {
        package: package.into(),
        version: raw.version.clone(),
        workload: WorkloadDesc {
            name: meta.name.clone(),
            source: meta.source.clone(),
            iters: meta.iters,
            detail: meta.detail.clone(),
        },
        tool: tool.into(),
        machine: machine.into(),
        python: python_version.into(),
        wall_secs: raw.wall_secs,
        tottime_secs: tottime,
        fractions,
        concentration,
        top: top_rows
            .into_iter()
            .map(|r| TopFunc {
                file: short_file(&r.file),
                func: r.func,
                tottime_ms: 1000.0 * r.tottime,
                ncalls: r.ncalls,
            })
            .collect(),
        loop_nature: nature,
        loop_factor: nature.factor(),
        native_cover: cover,
        score,
        verdict,
        why,
    }
}

fn short_file(f: &str) -> String {
    if f == "~" {
        return "~".into();
    }
    match f.rfind("site-packages/") {
        Some(i) => f[i + "site-packages/".len()..].into(),
        None => f.rsplit('/').next().unwrap_or(f).into(),
    }
}

/// Load probe rows from a file (one row or an array) or a directory of
/// `.json` row files. Missing path = no rows (unmeasured stays unmeasured).
pub fn load_rows(path: &std::path::Path) -> Result<Vec<ProbeRow>, String> {
    let mut rows = Vec::new();
    let files: Vec<std::path::PathBuf> = if path.is_dir() {
        let mut v: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        v.sort();
        v
    } else {
        vec![path.to_path_buf()]
    };
    for f in files {
        let text =
            std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", f.display()))?;
        if v.is_array() {
            for item in v.as_array().unwrap_or(&vec![]) {
                rows.push(
                    serde_json::from_value(item.clone())
                        .map_err(|e| format!("{}: {e}", f.display()))?,
                );
            }
        } else {
            rows.push(
                serde_json::from_value(v).map_err(|e| format!("{}: {e}", f.display()))?,
            );
        }
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Unit tests (deterministic, no network; synthetic pstats fixtures)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn row(file: &str, func: &str, tottime: f64) -> FuncRow {
        FuncRow { file: file.into(), func: func.into(), ncalls: 1, tottime, cumtime: tottime }
    }

    fn fracs(rows: &[FuncRow], stems: &HashSet<String>) -> Fractions {
        classify_rows(rows, stems).0
    }

    #[test]
    fn builtin_frames_split_by_engine_vs_fabric() {
        let empty = HashSet::new();
        // C re engine (match-object accessors ride with the engine).
        assert_eq!(
            classify_func("~", "<method 'match' of 're.Pattern' objects>", &empty),
            Bucket::Regex
        );
        assert_eq!(
            classify_func("~", "<method 'group' of 're.Match' objects>", &empty),
            Bucket::Regex
        );
        // O(n) crunchers vs O(1) fabric.
        assert_eq!(
            classify_func("~", "<method 'find' of 'bytes' objects>", &empty),
            Bucket::Native
        );
        assert_eq!(
            classify_func("~", "<built-in method _codecs.charmap_decode>", &empty),
            Bucket::Native
        );
        assert_eq!(
            classify_func("~", "<method 'append' of 'list' objects>", &empty),
            Bucket::PurePython
        );
        // I/O verbs.
        assert_eq!(
            classify_func("~", "<method 'write' of '_io.BufferedRandom' objects>", &empty),
            Bucket::IoWait
        );
        assert_eq!(
            classify_func("~", "<built-in method posix.unlink>", &empty),
            Bucket::IoWait
        );
    }

    #[test]
    fn module_frames_split_by_owner() {
        let empty = HashSet::new();
        assert_eq!(classify_func("re.py", "match", &empty), Bucket::Regex);
        assert_eq!(classify_func("sre_compile.py", "_compile", &empty), Bucket::Regex);
        assert_eq!(
            classify_func("/usr/lib/python3.12/socket.py", "recv", &empty),
            Bucket::IoWait
        );
        assert_eq!(
            classify_func("<frozen importlib._bootstrap>", "_find_and_load", &empty),
            Bucket::Overhead
        );
        assert_eq!(
            classify_func("yaml/scanner.py", "scan_plain", &empty),
            Bucket::PurePython
        );
    }

    #[test]
    fn compiled_package_frames_count_native() {
        let stems: HashSet<String> = ["md.py".into()].into_iter().collect();
        assert_eq!(
            classify_func("charset_normalizer/md.py", "probe", &stems),
            Bucket::Native
        );
        assert_eq!(
            classify_func("charset_normalizer/api.py", "from_bytes", &stems),
            Bucket::PurePython
        );
    }

    #[test]
    fn native_cover_matches_owned_extensions_only() {
        let modules: HashMap<String, String> = [
            ("charset_normalizer".into(), "/p/charset_normalizer/__init__.py".into()),
            ("charset_normalizer.md".into(), "/p/charset_normalizer/md.cpython-312-x86_64-linux-gnu.so".into()),
            ("charset_normalizer.cd".into(), "/p/charset_normalizer/cd.cpython-312-x86_64-linux-gnu.so".into()),
            ("_socket".into(), "/usr/lib/python3.12/lib-dynload/_socket.so".into()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            native_cover(&modules, "charset_normalizer"),
            vec!["charset_normalizer.cd".to_string(), "charset_normalizer.md".to_string()]
        );
        assert!(native_cover(&modules, "other").is_empty());
    }

    fn verdict_of(rows: &[FuncRow], stems: &HashSet<String>, cover: &[String], wall: f64) -> (f64, Verdict) {
        let (f, c, _) = classify_rows(rows, stems);
        let total: f64 = rows.iter().map(|r| r.tottime).sum();
        let thin = wall < THIN_WALL_SECS && total < THIN_TOTTIME_SECS;
        let nature = loop_nature(&c, thin, !cover.is_empty());
        let (s, v, _) = score_verdict(&f, nature, cover, thin);
        (s, v)
    }

    #[test]
    fn pyyaml_shape_scores_rewrite() {
        // Pure-Python scanner pipeline (measured 99.3% pure, pipeline loop).
        let rows: Vec<FuncRow> = (0..10)
            .map(|i| row("yaml/scanner.py", &format!("scan_{i}"), 1.0))
            .chain((0..4).map(|i| row("yaml/reader.py", &format!("peek_{i}"), 0.5)))
            .collect();
        let (s, v) = verdict_of(&rows, &HashSet::new(), &[], 11.4);
        assert_eq!(v, Verdict::Rewrite);
        assert!(s >= REWRITE_MIN, "score {s}");
    }

    #[test]
    fn pygments_shape_capped_shared_engine() {
        // C re engine dominates with Python dispatch around it.
        let rows = vec![
            row("~", "<method 'match' of 're.Pattern' objects>", 0.56),
            row("pygments/lexer.py", "get_tokens_unprocessed", 0.35),
            row("pygments/formatters/html.py", "_format_lines", 0.09),
        ];
        let (s, v) = verdict_of(&rows, &HashSet::new(), &[], 1.2);
        assert_eq!(v, Verdict::UpstreamOnly, "score {s}");
        let f = fracs(&rows, &HashSet::new());
        assert!(f.regex >= REGEX_CAP);
    }

    #[test]
    fn charset_shape_excluded_saturated() {
        // Thin dispatch skin over a shipped compiled core.
        let rows = vec![
            row("charset_normalizer/api.py", "from_bytes", 0.005),
            row("~", "<built-in method _codecs.charmap_decode>", 0.002),
            row("charset_normalizer/utils.py", "cut_sequence_chunks", 0.001),
        ];
        let cover = ["charset_normalizer.md".to_string(), "charset_normalizer.cd".to_string()];
        let (s, v) = verdict_of(&rows, &HashSet::new(), &cover, 0.01);
        assert_eq!(v, Verdict::Exclude, "score {s}");
        assert!(s <= SATURATED_SCORE_CAP, "score {s}");
    }

    #[test]
    fn io_bound_shape_capped_upstream() {
        let rows = vec![
            row("~", "<method 'recv' of '_socket.socket' objects>", 0.6),
            row("urllib3/response.py", "read", 0.2),
            row("urllib3/connection.py", "request", 0.2),
        ];
        let (_, v) = verdict_of(&rows, &HashSet::new(), &[], 5.0);
        assert_eq!(v, Verdict::UpstreamOnly);
    }

    #[test]
    fn loop_nature_prefers_tight_over_dispatch() {
        let tight = Concentration { top1: 0.49, top5: 0.92, hot_modules: 4 };
        assert_eq!(loop_nature(&tight, false, false), LoopNature::TightLoop);
        let pipe = Concentration { top1: 0.08, top5: 0.32, hot_modules: 6 };
        assert_eq!(loop_nature(&pipe, false, false), LoopNature::Pipeline);
        let disp = Concentration { top1: 0.05, top5: 0.20, hot_modules: 12 };
        assert_eq!(loop_nature(&disp, false, false), LoopNature::Dispatch);
        let thin = Concentration { top1: 0.53, top5: 0.88, hot_modules: 5 };
        assert_eq!(loop_nature(&thin, true, true), LoopNature::ThinNative);
    }

    #[test]
    fn row_round_trips_json() {
        let raw = RawProfile {
            funcs: vec![row("yaml/scanner.py", "scan_plain", 1.0)],
            modules: HashMap::new(),
            wall_secs: 3.0,
            version: "6.0.3".into(),
        };
        let meta = WorkloadDesc {
            name: "w".into(),
            source: "doc-synthetic".into(),
            iters: 1,
            detail: "d".into(),
        };
        let r = build_row("pyyaml", "yaml", &meta, "cprofile", "box", "3.12.3", &raw);
        let text = serde_json::to_string(&r).unwrap();
        let back: ProbeRow = serde_json::from_str(&text).unwrap();
        assert_eq!(back.package, "pyyaml");
        assert_eq!(back.verdict, Verdict::Rewrite);
    }
}
