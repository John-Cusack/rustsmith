//! Legacy chardet-compatible shim, version constants, and CLI detector.
//!
//! Rust mirror of, in order:
//! - `charset_normalizer/legacy.py` — [`detect`] / [`ResultDict`]
//! - `charset_normalizer/version.py` — [`VERSION_STR`] / [`VERSION_PARTS`]
//! - `charset_normalizer/cli/__main__.py` — [`query_yes_no`] / [`cli_detect`]
//! - `charset_normalizer/cli/__init__.py` — re-exports only (orig lines 1-8);
//!   the equivalent here is `pub(crate)` visibility: downstream `use`s replace it.
//! - `charset_normalizer/__main__.py` (orig lines 1-6) — calls `cli_detect()` with
//!   no args; there is no `fn main` here on purpose: binary wiring (read
//!   `std::env::args`, strip argv\[0\], return exit code) belongs to the
//!   integrator-owned `lib.rs`/binary target, exactly like the `pyo3` wiring.
//!
//! Fidelity rule: the Python control flow is mirrored branch-for-branch, and every
//! user-visible string is copied verbatim. Each item cites `orig <file>:<line>`.
//! `stderr` vs `stdout` routing is preserved (`print(..., file=sys.stderr)` →
//! `eprint!/eprintln!`; plain `print` → `print!/println!`).
//!
//! # Name risks (assumed sibling interfaces; integrator reconciles mismatches)
//! - `super::api::DetectOptions { threshold: f64, explain: bool,
//!   preemptive_behaviour: bool }` + `Default` (threshold 0.2, explain false,
//!   preemptive true). Fallback: rename fields or construct the sibling's options
//!   type at the single `DetectOptions { ... }` site in [`cli_detect`].
//! - `super::api::from_bytes(&[u8]) -> CharsetMatches` (defaults) and
//!   `super::api::from_bytes_with_options(&[u8], &DetectOptions) -> CharsetMatches`.
//!   Fallback: if the api porter exposes one entry point with an options struct,
//!   route both call sites through it (`detect` passes `&DetectOptions::default()`).
//! - `super::models::CharsetMatches`: `best() -> Option<&CharsetMatch>`,
//!   `len() -> usize`, `iter()` over `&CharsetMatch`. If `best()` returns an owned
//!   `Option<CharsetMatch>` instead, replace `found.as_ref()` with `found.as_ref()`
//!   on the owned value — call sites already go through one `found_ref` binding.
//! - `CharsetMatch` accessors used: `encoding()`, `language()`, `chaos()`,
//!   `bom()`, `percent_chaos()`, `percent_coherence()`, `encoding_aliases()`,
//!   `alphabets()`, `could_be_from_charset()`, plus `PartialEq` (`encoding` +
//!   `fingerprint`, orig `models.py:40-49`). `.to_string()` is used so
//!   `encoding()`/`language()` may return `&str` or `String`; `.to_vec()` so the
//!   list accessors may return `Vec<String>`, `&Vec<String>`, or `&[String]`.
//! - `CharsetMatch::output(&mut self, "utf_8") -> Result<Vec<u8>, ModelError>`
//!   (orig `models.py:224` raises on strict-decode/unknown-encoding): consumed as
//!   `best.clone().output("utf_8").unwrap_or_default()` (`Clone` assumed;
//!   fallback is a by-index re-fetch once the api surface freezes).
//! - `CliDetectionResult::new` takes the 11 `models.py:342-355` parameters in
//!   Python order, exposes pub `path`/`encoding`/`unicode_path` fields, and
//!   `to_json() -> String` renders the `models.py:369-382` dict exactly
//!   (`json.dumps(..., ensure_ascii=True, indent=4)`).
//! - `super::tables_constant::{TOO_SMALL_SEQUENCE, chardet_name}` already exist.
//!
//! # Intentional non-mirrors (no Python equivalent expressible in safe Rust)
//! - `legacy.py:32-35` (`**kwargs` warning) and `legacy.py:37-43` (`TypeError` on
//!   non-bytes input, `bytearray` → `bytes` coercion): `detect(&[u8], ...)` is
//!   statically typed, so there is nothing to warn or coerce.
//! - `cli/__main__.py:62-78` (`FileType.__call__`): files are opened with
//!   `std::fs::read` at detection time instead of during argument parsing, and
//!   closed by RAII instead of the explicit `my_file.close()` calls
//!   (`:187-188, :194-195, :201-202, :296-298, :307-308, :319-321, :330-331,
//!   :334-335`). Exit codes are unchanged. `"-"` (stdin, `:64-71`) is read from
//!   standard input under the display name `"-"`; the original would crash
//!   (`abspath` of a non-path stdin name), so this is a safe superset.
//! - `query_yes_no` on EOF/IO-error returns the default; the original raises
//!   `EOFError` out of `input()`. Returning (not hanging, not panicking) is the
//!   closest non-crashing behaviour.
//! - argparse itself (exit code 2 + `usage:`/`error:` preamble on malformed
//!   invocations) is re-implemented minimally; only the documented exit codes
//!   (0/1/2) are contractual, not argparse's exact wrapping.
//! - `OSError` text (`:329`) and `fs::canonicalize` paths cannot be byte-identical
//!   across runtimes; the routing (stderr, exit 2) is.
//! - `--help` layout is ours; the description/help strings inside it are verbatim.
//! - `--version` keeps the exact format string (orig `:174-179`); the
//!   Python/Unicode values are the reference-interpreter constants below
//!   (CPython 3.12 ships Unicode 15.0.0, matching the UCD 15.0 tables), and
//!   SpeedUp is `ON` (pure-Rust core has no `.py` fallback module, so the orig
//!   `md_module.__file__.lower().endswith(".py")` test is always false here).

use std::fs;
use std::io::{self, BufRead, Write};

use super::api::{DetectOptions, from_bytes_1arg as from_bytes, from_bytes_with_options};
use super::models::{CharsetMatches, CliDetectionResult};
use super::tables_constant::{TOO_SMALL_SEQUENCE, chardet_name};

// ---------------------------------------------------------------------------
// version.py
// ---------------------------------------------------------------------------

/// orig `version.py:7` — `__version__ = "3.5.1"`.
pub(crate) const VERSION_STR: &str = "3.5.1";

/// orig `version.py:8` — `VERSION = __version__.split(".")`.
pub(crate) const VERSION_PARTS: [&str; 3] = ["3", "5", "1"];

/// Reference interpreter for `--version` output (tables generated from CPython 3.12).
pub(crate) const REFERENCE_PYTHON_VERSION: &str = "3.12";

/// Reference Unicode version for `--version` output (CPython 3.12's `unicodedata`).
pub(crate) const UNIDATA_VERSION: &str = "15.0.0";

/// orig `cli/__main__.py:174-179` — version action string, format kept verbatim:
/// `"Charset-Normalizer {} - Python {} - Unicode {} - SpeedUp {}"`.
pub(crate) fn version_text(python_version: &str, unidata_version: &str, speedup_on: bool) -> String {
    format!(
        "Charset-Normalizer {} - Python {} - Unicode {} - SpeedUp {}",
        VERSION_STR,
        python_version,
        unidata_version,
        if speedup_on { "ON" } else { "OFF" },
    )
}

/// `--version` payload for this (pure-Rust, no `.py` fallback) build: SpeedUp ON.
pub(crate) fn default_version_text() -> String {
    version_text(REFERENCE_PYTHON_VERSION, UNIDATA_VERSION, true)
}

// ---------------------------------------------------------------------------
// legacy.py
// ---------------------------------------------------------------------------

/// orig `legacy.py:12-15` — `ResultDict` TypedDict
/// (`encoding: str | None`, `language: str`, `confidence: float | None`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResultDict {
    pub encoding: Option<String>,
    pub language: String,
    pub confidence: Option<f64>,
}

/// orig `legacy.py:18-79` — chardet-compatible `detect()`.
///
/// `should_rename_legacy = False` (the default) KEEPS the confusing original
/// semantics: the encoding IS renamed through `CHARDET_CORRESPONDENCE`
/// (orig `:72-73`).
pub(crate) fn detect(data: &[u8], should_rename_legacy: bool) -> ResultDict {
    let matches: CharsetMatches = from_bytes(data); // orig `legacy.py:45`
    let found = matches.best(); // orig `legacy.py:45` (`.best()`)
    let found_ref = found.as_ref();

    let mut encoding: Option<String> = found_ref.map(|m| m.encoding().to_string()); // orig `:47`
    // orig `:48`: `r.language if r is not None and r.language != "Unknown" else ""`
    let language: String = found_ref
        .map(|m| m.language().to_string())
        .filter(|l| l != "Unknown")
        .unwrap_or_default();
    // orig `:49`: `confidence = 1.0 - r.chaos if r is not None else None`
    let mut confidence: Option<f64> = found_ref.map(|m| 1.0 - m.chaos());

    // orig `:51-65` — small-sample penalty
    // (https://github.com/jawah/charset_normalizer/issues/391).
    if let Some(conf) = confidence {
        if conf >= 0.9
            && encoding.as_deref() != Some("utf_8") // orig `:57-61`
            && encoding.as_deref() != Some("ascii")
            && !found_ref.is_some_and(|m| m.bom()) // orig `:62` (`not r.bom`)
            && data.len() < TOO_SMALL_SEQUENCE // orig `:63` (`TOO_SMALL_SEQUENCE == 32`)
        {
            confidence = Some(conf - 0.2); // orig `:65`
        }
    }

    // orig `:67-70` — chardet returns `UTF-8-SIG`; the SIG is stripped during
    // detection, so re-attach the suffix marker here.
    if encoding.as_deref() == Some("utf_8") && found_ref.is_some_and(|m| m.bom()) {
        encoding = Some("utf_8_sig".to_string()); // orig `:70` (`encoding += "_sig"`)
    }

    // orig `:72-73` — note the inverted flag: rename UNLESS should_rename_legacy.
    if !should_rename_legacy {
        if let Some(enc) = encoding.clone() {
            if let Some(mapped) = chardet_name(&enc) {
                encoding = Some(mapped.to_string());
            }
        }
    }

    // orig `:75-79`
    ResultDict {
        encoding,
        language,
        confidence,
    }
}

// ---------------------------------------------------------------------------
// cli/__main__.py — query_yes_no
// ---------------------------------------------------------------------------

/// Pure answer classifier behind [`query_yes_no`]; `None` means "reprompt".
/// orig `cli/__main__.py:21-27`.
pub(crate) fn parse_yes_no_answer(choice: &str, default_yes: bool) -> Option<bool> {
    if choice.is_empty() {
        return Some(default_yes); // orig `:22-23` (`if not choice: return default == "yes"`)
    }
    if choice == "y" || choice == "yes" {
        return Some(true); // orig `:24-25`
    }
    if choice == "n" || choice == "no" {
        return Some(false); // orig `:26-27`
    }
    None
}

/// orig `cli/__main__.py:16-28` — ask a yes/no question, return the answer.
pub(crate) fn query_yes_no(question: &str, default: &str) -> bool {
    let default_yes = default == "yes";
    // orig `:18`
    let prompt = if default_yes { " [Y/n] " } else { " [y/N] " };
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        // orig `:21` — `input(question + prompt)` writes the prompt with no newline
        print!("{question}{prompt}");
        let _ = io::stdout().flush();
        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) => return default_yes, // EOF: orig raises EOFError; return default (see header)
            Ok(_) => {}
            Err(_) => return default_yes, // IO error: same non-hanging choice
        }
        // orig `:21` — `.strip().lower()`
        let choice = line.trim().to_lowercase();
        if let Some(answer) = parse_yes_no_answer(&choice, default_yes) {
            return answer;
        }
        println!("Please respond with 'y' or 'n'."); // orig `:28`
    }
}

// ---------------------------------------------------------------------------
// cli/__main__.py — argument parsing (argparse replaced with a std-only parser)
// ---------------------------------------------------------------------------

/// Parsed CLI state. Defaults mirror the `ArgumentParser.add_argument` calls:
/// `threshold` default `0.2` (orig `:166`), every flag default `False`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CliArgs {
    /// orig `:102-104` — `files`, `FileType("rb")`, `nargs="+"`.
    pub files: Vec<String>,
    /// orig `:105-113` — `-v/--verbose`.
    pub verbose: bool,
    /// orig `:114-121` — `-a/--with-alternative`.
    pub alternatives: bool,
    /// orig `:122-129` — `-n/--normalize`.
    pub normalize: bool,
    /// orig `:131-137` — `-m/--minimal`.
    pub minimal: bool,
    /// orig `:138-145` — `-r/--replace`.
    pub replace: bool,
    /// orig `:146-153` — `-f/--force`.
    pub force: bool,
    /// orig `:154-161` — `-i/--no-preemptive`.
    pub no_preemptive: bool,
    /// orig `:162-170` — `-t/--threshold` (`type=float`).
    pub threshold: f64,
    /// `-h/--help` (argparse builtin).
    pub show_help: bool,
    /// orig `:171-181` — `--version` action.
    pub show_version: bool,
}

impl Default for CliArgs {
    fn default() -> Self {
        CliArgs {
            files: Vec::new(),
            verbose: false,
            alternatives: false,
            normalize: false,
            minimal: false,
            replace: false,
            force: false,
            no_preemptive: false,
            threshold: 0.2, // orig `:166` (`default=0.2`)
            show_help: false,
            show_version: false,
        }
    }
}

/// Parse `argv` (without the program name; mirrors `parse_args(argv)`, orig `:183`).
/// `Err(message)` maps to argparse's exit code 2 with a `usage:`/`error:` preamble.
pub(crate) fn parse_cli_args(argv: &[String]) -> Result<CliArgs, String> {
    let mut args = CliArgs::default();
    let mut i = 0;
    let mut positional_only = false;

    while i < argv.len() {
        let token = argv[i].as_str();
        if !positional_only && token == "--" {
            positional_only = true;
            i += 1;
            continue;
        }
        if !positional_only && token.starts_with("--") && token.len() > 2 {
            let (name, inline_value) = match token.find('=') {
                Some(pos) => (&token[..pos], Some(token[pos + 1..].to_string())),
                None => (token, None),
            };
            // orig `:105-181` long options
            match name {
                "--verbose" => {
                    reject_option_value(name, inline_value)?;
                    args.verbose = true;
                }
                "--with-alternative" => {
                    reject_option_value(name, inline_value)?;
                    args.alternatives = true;
                }
                "--normalize" => {
                    reject_option_value(name, inline_value)?;
                    args.normalize = true;
                }
                "--minimal" => {
                    reject_option_value(name, inline_value)?;
                    args.minimal = true;
                }
                "--replace" => {
                    reject_option_value(name, inline_value)?;
                    args.replace = true;
                }
                "--force" => {
                    reject_option_value(name, inline_value)?;
                    args.force = true;
                }
                "--no-preemptive" => {
                    reject_option_value(name, inline_value)?;
                    args.no_preemptive = true;
                }
                "--threshold" => {
                    let raw = match inline_value {
                        Some(v) => v,
                        None => {
                            i += 1;
                            argv.get(i)
                                .map(String::as_str)
                                .ok_or_else(|| {
                                    "argument -t/--threshold: expected one argument".to_string()
                                })?
                                .to_string()
                        }
                    };
                    args.threshold = parse_threshold(&raw)?; // orig `:167` (`type=float`)
                }
                "--version" => {
                    reject_option_value(name, inline_value)?;
                    args.show_version = true;
                }
                "--help" => {
                    reject_option_value(name, inline_value)?;
                    args.show_help = true;
                }
                _ => return Err(format!("unrecognized arguments: {token}")),
            }
            i += 1;
        } else if !positional_only && token.starts_with('-') && token.len() > 1 {
            // Short-option cluster (`-vn`, `-t0.5`); argparse-compatible subset.
            // orig short flags: `:106 -v`, `:115 -a`, `:123 -n`, `:132 -m`,
            // `:139 -r`, `:147 -f`, `:155 -i`, `:163 -t`.
            let bytes = token.as_bytes();
            let mut j = 1;
            while j < bytes.len() {
                match bytes[j] as char {
                    'v' => args.verbose = true,
                    'a' => args.alternatives = true,
                    'n' => args.normalize = true,
                    'm' => args.minimal = true,
                    'r' => args.replace = true,
                    'f' => args.force = true,
                    'i' => args.no_preemptive = true,
                    'h' => args.show_help = true,
                    't' => {
                        let rest = token[j + 1..].to_string();
                        let raw = if !rest.is_empty() {
                            rest
                        } else {
                            i += 1;
                            argv.get(i)
                                .map(String::as_str)
                                .ok_or_else(|| {
                                    "argument -t/--threshold: expected one argument".to_string()
                                })?
                                .to_string()
                        };
                        args.threshold = parse_threshold(&raw)?;
                        break;
                    }
                    other => return Err(format!("unrecognized arguments: -{other}")),
                }
                j += 1;
            }
            i += 1;
        } else {
            args.files.push(token.to_string());
            i += 1;
        }
    }

    // `nargs="+"` (orig `:103`): at least one file unless help/version short-circuits.
    if args.files.is_empty() && !args.show_help && !args.show_version {
        return Err("the following arguments are required: files".to_string());
    }
    Ok(args)
}

fn reject_option_value(name: &str, value: Option<String>) -> Result<(), String> {
    if let Some(value) = value {
        return Err(format!("argument {name}: ignored explicit value '{value}'"));
    }
    Ok(())
}

fn parse_threshold(raw: &str) -> Result<f64, String> {
    raw.parse::<f64>()
        .map_err(|_| format!("argument -t/--threshold: invalid float value: '{raw}'"))
}

/// `--help` text. Layout is ours (see header); description and per-option help
/// strings are verbatim copies of orig `:97-100` (description) and the `help=`
/// arguments on `:103, :111-112, :120, :128, :136, :144, :152, :160, :169, :180`.
pub(crate) fn help_text() -> String {
    "\
usage: charset-normalizer [-h] [-v] [-a] [-n] [-m] [-r] [-f] [-i] [-t THRESHOLD]\n                         [--version]\n                         files [files ...]\n\
\n\
The Real First Universal Charset Detector. Discover originating encoding used on text file. Normalize text to unicode.\n\
\n\
positional arguments:\n\
  files                 File(s) to be analysed\n\
\n\
options:\n\
  -h, --help            show this help message and exit\n\
  -v, --verbose         Display complementary information about file if any. Stdout will contain logs about the detection process.\n\
  -a, --with-alternative\n\
                        Output complementary possibilities if any. Top-level JSON WILL be a list.\n\
  -n, --normalize       Permit to normalize input file. If not set, program does not write anything.\n\
  -m, --minimal         Only output the charset detected to STDOUT. Disabling JSON output.\n\
  -r, --replace         Replace file when trying to normalize it instead of creating a new one.\n\
  -f, --force           Replace file without asking if you are sure, use this flag with caution.\n\
  -i, --no-preemptive   Disable looking at a charset declaration to hint the detector.\n\
  -t THRESHOLD, --threshold THRESHOLD\n\
                        Define a custom maximum amount of noise allowed in decoded content. 0. <= noise <= 1.\n\
  --version             Show version information and exit.\
"
    .to_string()
}

// ---------------------------------------------------------------------------
// cli/__main__.py — cli_detect
// ---------------------------------------------------------------------------

/// Python `os.path.abspath` without touching the filesystem (no symlink
/// resolution; `os.path.abspath` only normalizes).
pub(crate) fn abspath(name: &str) -> String {
    let joined = if std::path::Path::new(name).is_absolute() {
        name.to_string()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(name).to_string_lossy().into_owned(),
            Err(_) => return name.to_string(),
        }
    };
    lexical_normalize(&joined)
}

/// Python `os.path.realpath`: resolve symlinks, falling back to [`abspath`]
/// when the path cannot be canonicalized (orig `:300-301`).
pub(crate) fn realpath(name: &str) -> String {
    match fs::canonicalize(name) {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(_) => abspath(name),
    }
}

fn lexical_normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        } else if part == ".." {
            if parts.pop().is_none() && !absolute {
                parts.push("..");
            }
        } else {
            parts.push(part);
        }
    }
    let mut out = parts.join("/");
    if absolute {
        out.insert(0, '/');
    }
    if out.is_empty() {
        out.push('.');
    }
    out
}

/// Python `os.path.dirname` on a `/`-separated path (orig `:300`).
pub(crate) fn dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => String::new(),
    }
}

/// Python `os.path.basename` on a `/`-separated path (orig `:301`).
pub(crate) fn basename(path: &str) -> String {
    match path.rfind('/') {
        Some(pos) => path[pos + 1..].to_string(),
        None => path.to_string(),
    }
}

/// Pretty-print several `to_json()` payloads exactly like
/// `json.dumps([...], ensure_ascii=True, indent=4)` (orig `:340-345`, list branch):
/// each element block is indented one extra level inside the array brackets.
pub(crate) fn json_array_pretty(items: &[CliDetectionResult]) -> String {
    let blocks: Vec<String> = items
        .iter()
        .map(|item| {
            item.to_json()
                .lines()
                .map(|line| format!("    {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    format!("[\n{}\n]", blocks.join(",\n"))
}

/// orig `cli/__main__.py:90-359` — CLI entry point.
/// Returns `CliOutcome`: plain `Return(code)` for computed exits (`0`
/// success, `1` usage guards, `2` write failure), `Raise(code)` where
/// argparse raises `SystemExit` (parse errors, unopenable files),
/// `Version` for `--version` (binding prints text + raises `SystemExit(0)`).
pub(crate) enum CliOutcome {
    Return(i32),
    Raise(i32),
    Version,
}

pub(crate) fn cli_detect(argv: &[String]) -> CliOutcome {
    let args = match parse_cli_args(argv) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("usage: charset-normalizer [-h] [-v] [-a] [-n] [-m] [-r] [-f] [-i] [-t THRESHOLD]");
            eprintln!("charset-normalizer: error: {message}");
            return CliOutcome::Raise(2);
        }
    };

    if args.show_help {
        print!("{}", help_text());
        return CliOutcome::Raise(0);
    }
    if args.show_version {
        // orig `:171-181` (`action="version"` raises `SystemExit(0)`)
        return CliOutcome::Version;
    }

    // orig `:185-190` (file closing there is RAII here)
    if args.replace && !args.normalize {
        eprintln!("Use --replace in addition of --normalize only."); // orig `:189`
        return CliOutcome::Return(1); // orig `:190`
    }

    // orig `:192-197`
    if args.force && !args.replace {
        eprintln!("Use --force in addition of --replace only."); // orig `:196`
        return CliOutcome::Return(1); // orig `:197`
    }

    // orig `:199-204`
    if !(0.0..=1.0).contains(&args.threshold) {
        eprintln!("--threshold VALUE should be between 0. AND 1."); // orig `:203`
        return CliOutcome::Return(1); // orig `:204`
    }

    let mut collected: Vec<CliDetectionResult> = Vec::new(); // orig `:206` (`x_ = []`)
    let mut input_names: Vec<String> = Vec::new();

    for name in &args.files {
        // orig `:103` (`FileType("rb")` open) + `api.py:827` (`fp.read()`).
        // `"-"` reads stdin (safe superset; see header).
        let data: Vec<u8> = if name == "-" {
            let mut buffer = Vec::new();
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            use std::io::Read;
            if let Err(error) = handle.read_to_end(&mut buffer) {
                eprintln!("argument files: can't open '{name}': {error}");
                return CliOutcome::Raise(2);
            }
            buffer
        } else {
            match fs::read(name) {
                // orig `:77` (`"can't open '{string}': {e}"`) via argparse exit 2
                Ok(bytes) => bytes,
                Err(error) => {
                    eprintln!("argument files: can't open '{name}': {error}");
                    return CliOutcome::Raise(2);
                }
            }
        };
        input_names.push(name.clone());

        // orig `:209-214`
        let options = DetectOptions {
            steps: 5,
            chunk_size: 512,
            threshold: args.threshold,
            cp_isolation: None,
            cp_exclusion: None,
            preemptive: !args.no_preemptive,
            language_threshold: 0.1,
            enable_fallback: true,
        };
        let matches: CharsetMatches = from_bytes_with_options(&data, &options);

        let path = abspath(name); // orig `:232` (`abspath(my_file.name)`)
        match matches.best() {
            // orig `:216` (`best_guess = matches.best()`)
            None => {
                // orig `:218-229`
                if args.threshold < 1.0 {
                    eprintln!(
                        "Unable to identify originating encoding for \"{name}\". Maybe try increasing maximum amount of chaos."
                    );
                } else {
                    eprintln!("Unable to identify originating encoding for \"{name}\". ");
                }
                collected.push(
                    // orig `:230-244`
                    CliDetectionResult::new(
                        path,
                        None,
                        Vec::new(),
                        Vec::new(),
                        "Unknown".to_string(),
                        Vec::new(),
                        false,
                        1.0,
                        0.0,
                        None,
                        true,
                    ),
                );
            }
            Some(best) => {
                let encoding = best.encoding().to_string();
                // orig `:250-254` (`could_be_from_charset` minus the best encoding)
                let alternative_encodings: Vec<String> = best
                    .could_be_from_charset()
                    .iter()
                    .map(|cp| cp.to_string())
                    .filter(|cp| *cp != encoding)
                    .collect();
                collected.push(
                    // orig `:246-262`
                    CliDetectionResult::new(
                        path.clone(),
                        Some(encoding.clone()),
                        best.encoding_aliases().to_vec(),
                        alternative_encodings,
                        best.language().to_string(),
                        best.clone().alphabets().to_vec(),
                        best.bom(),
                        best.percent_chaos(),
                        best.percent_coherence(),
                        None,
                        true,
                    ),
                );
                let best_index = collected.len() - 1;

                // orig `:265-286`
                if matches.len() > 1 && args.alternatives {
                    for other in matches.iter() {
                        // orig `:267` (`if el != best_guess`, value eq per `models.py:40-49`)
                        if other != best {
                            let other_encoding = other.encoding().to_string();
                            collected.push(
                                // orig `:268-286` (same shape, `is_preferred=False`)
                                CliDetectionResult::new(
                                    path.clone(),
                                    Some(other_encoding.clone()),
                                    other.encoding_aliases().to_vec(),
                                    other
                                        .could_be_from_charset()
                                        .iter()
                                        .map(|cp| cp.to_string())
                                        .filter(|cp| *cp != other_encoding)
                                        .collect(),
                                    other.language().to_string(),
                                    other.clone().alphabets().to_vec(),
                                    other.bom(),
                                    other.percent_chaos(),
                                    other.percent_coherence(),
                                    None,
                                    false,
                                ),
                            );
                        }
                    }
                }

                // orig `:288-332`
                if args.normalize {
                    // orig `:289` (`.startswith("utf")`)
                    if encoding.starts_with("utf") {
                        eprintln!(
                            // orig `:291`
                            "\"{name}\" file does not need to be normalized, as it already came from unicode."
                        );
                        continue; // orig `:296-298`
                    }

                    // orig `:300-303`
                    let canonical = realpath(name);
                    let dir_path = dirname(&canonical);
                    let file_name = basename(&canonical);
                    let mut parts: Vec<String> =
                        file_name.split('.').map(str::to_string).collect();

                    if !args.replace {
                        // orig `:305-308` (`o_.insert(-1, best_guess.encoding)`)
                        let at = parts.len() - 1;
                        parts.insert(at, encoding.clone()); // orig `:306`
                    } else if !args.force
                        && !query_yes_no(
                            // orig `:310-316`
                            &format!(
                                "Are you sure to normalize \"{name}\" by replacing it ?"
                            ), // orig `:312`
                            "no",
                        )
                    {
                        continue; // orig `:318-321`
                    }

                    // orig `:323-327` (`os.path.join` + `open(..., "wb")` + `write(output())`)
                    let joined = parts.join(".");
                    let unicode_path = if dir_path.is_empty() {
                        joined
                    } else {
                        format!("{dir_path}/{joined}")
                    };
                    // `output(&mut self, "utf_8") -> Result<Vec<u8>, ModelError>`;
                    // orig `models.py:224` raises on strict-decode/unknown-encoding
                    // and the CLI installs no handler, so default (Defensive).
                    let bytes: Vec<u8> = best.clone().output("utf_8").unwrap_or_default(); // orig `:327`
                    match fs::write(&unicode_path, &bytes) {
                        Ok(()) => {
                            collected[best_index].unicode_path = Some(unicode_path); // orig `:324`
                        }
                        Err(error) => {
                            // orig `:328-332` (`except OSError as e: ... return 2`, Defensive)
                            eprintln!("{error}"); // orig `:329` (`print(str(e), file=sys.stderr)`)
                            return CliOutcome::Return(2); // orig `:332`
                        }
                    }
                }
            }
        }
        // orig `:334-335` (`my_file.close()`) is RAII here.
    }

    if !args.minimal {
        // orig `:337-346`
        if collected.len() > 1 {
            println!("{}", json_array_pretty(&collected)); // orig `:341-345` (list branch)
        } else if let Some(first) = collected.first() {
            println!("{}", first.to_json()); // orig `:341-345` (single-object branch)
        } else {
            // Unreachable (`files` is non-empty and every file pushes ≥1 result);
            // the original would raise `IndexError` on `x_[0]`.
            println!("[]");
        }
    } else {
        // orig `:347-357`
        for name in &input_names {
            let wanted = abspath(name); // orig `:354` (`el.path == abspath(my_file.name)`)
            let encodings: Vec<String> = collected
                .iter()
                .filter(|result| result.path == wanted)
                .map(|result| {
                    result
                        .encoding
                        .clone()
                        .unwrap_or_else(|| "undefined".to_string()) // orig `:352`
                })
                .collect();
            println!("{}", encodings.join(", ")); // orig `:349-357`
        }
    }

    CliOutcome::Return(0) // orig `:359`
}
