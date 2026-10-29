//! `elmer-regrade` — re-run the Fortran substitution gate over a list of
//! Elmer campaign units and print one verdict row per unit.
//!
//! The gate decision is the campaign's own: `fortran_substitution_verdict`
//! (the reason-carrying form of `CmakeBridge::check_substitutable`).
//! This driver only resolves a ledger unit id to the `UnitDecl` the lane
//! graded and records the verdict plus the refusal reasons.
//!
//! Unit model per ledger (ADR-047, ADR-049):
//!
//! * `lane-fem-a` rows are file-level campaign units: the frozen recon
//!   inventory (`dag.json`) owns the export list, so the gate sees the
//!   frozen linkages.
//! * `lane-fem-b` rows are per-scope: the current Fortran frontend's
//!   scope split owns the export list (the scope ids the fem-b ledger
//!   records).
//!
//! Usage:
//!   elmer-regrade --dag <dag.json> --tree <elmer-fork> \
//!                  --units <units.json> --out <out.json> \
//!                  [--mode substitution|entry] \
//!                  [--library-manifest <libs.json> [--reference-dir <dir>]]
//!
//! `<units.json>` is a JSON array of `{"unit": "<id>", "ledger": "<id>"}`.
//! `--mode` selects the gate (default `substitution`, the historical
//! behavior): `substitution` calls `fortran_substitution_verdict`, `entry`
//! calls `fortran_plugin_entry_verdict`. This flag replaces the temporary
//! `ELMER_REGRADE_MODE` driver patch the campaign used for entry censuses
//! (rs-elmer-next §1/§6.5); neither gate function is changed here, so
//! default runs stay byte-identical and `--mode entry` reproduces the
//! archived entry totals verdict-for-verdict.
//!
//! `--library-manifest` appends per-library verdicts for whole-library
//! ports (Slice 1: bundled `.so` libs). Each library entry names its
//! member file-units plus the expected dynamic-symbol set; the library
//! verdict is the per-file PluginEntry AND (entry semantics always —
//! that is the library-verdict definition), vetoed by a dynamic-symbol
//! mismatch when `--reference-dir` points at built reference `.so` files
//! (`nm -D --defined-only` comparison). Without `--reference-dir` the
//! symbol set is recorded, not compared (`dynsym_match: null`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rustsmith_adapters::{
    fortran_plugin_entry_verdict, fortran_scope_units, fortran_substitution_verdict, Abi, Symbol,
    UnitDecl, UnitId,
};

fn main() {
    if let Err(e) = run() {
        eprintln!("elmer-regrade: {e}");
        std::process::exit(1);
    }
}

fn arg(name: &str) -> Result<String, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            return args
                .get(i + 1)
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"));
        }
        i += 1;
    }
    Err(format!("missing {name}"))
}

fn arg_opt(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            return args.get(i + 1).cloned();
        }
        i += 1;
    }
    None
}

/// The frozen export list of one campaign unit, as gate symbols.
fn frozen_symbols(unit_id: &str, exports: &[(String, bool)]) -> Vec<Symbol> {
    let prefix = unit_id.split_once(':').map(|(head, _)| head).unwrap_or("");
    exports
        .iter()
        .map(|(linkage, bind_c)| Symbol {
            linkage: linkage.clone(),
            abi: match prefix {
                "fortran" => Abi::Fortran { bind_c: *bind_c },
                "c" => Abi::C,
                "cxx" => Abi::Cxx,
                "python" => Abi::Python,
                _ => Abi::C,
            },
        })
        .collect()
}

fn dag_exports(dag: &serde_json::Value) -> BTreeMap<String, Vec<(String, bool)>> {
    let mut out: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    let Some(units) = dag.get("units").and_then(|u| u.as_array()) else {
        return out;
    };
    for u in units {
        let Some(id) = u.get("id").and_then(|i| i.as_str()) else {
            continue;
        };
        let exports = u
            .get("exports")
            .and_then(|e| e.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|e| {
                        let linkage = e.get("linkage")?.as_str()?.to_string();
                        let bind_c = e.get("bind_c").and_then(|b| b.as_bool()).unwrap_or(false);
                        Some((linkage, bind_c))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.insert(id.to_string(), exports);
    }
    out
}

/// The `UnitDecl` a ledger row grades.
fn resolve_unit(
    unit_id: &str,
    ledger: &str,
    tree: &Path,
    dag: &BTreeMap<String, Vec<(String, bool)>>,
) -> Result<UnitDecl, String> {
    let (rel, scope) = match unit_id.split_once('#') {
        Some((file, scope)) => (file, Some(scope)),
        None => (unit_id, None),
    };
    let rel = rel.split_once(':').map(|(_, path)| path).unwrap_or(rel);
    if ledger == "lane-fem-b" {
        // Scope model: the current frontend's split owns the export list.
        let units = fortran_scope_units(tree, rel)?;
        let want = scope.map(|s| format!("#{s}"));
        let unit = want
            .as_ref()
            .and_then(|w| units.iter().find(|u| u.id.0.ends_with(w)))
            .or_else(|| units.iter().find(|u| u.id.0 == unit_id))
            .ok_or_else(|| format!("no scope {:?} in {rel}", scope))?;
        return Ok(unit.clone());
    }
    // File model: the frozen campaign inventory owns the export list.
    let exports = dag.get(unit_id).cloned().unwrap_or_default();
    Ok(UnitDecl {
        id: UnitId(unit_id.to_string()),
        files: vec![rel.to_string()],
        generated_from: None,
        exports: frozen_symbols(unit_id, &exports),
        imports: Vec::new(),
    })
}

/// Gate selected by `--mode`. `Substitution` is the historical default;
/// `Entry` reproduces the archived entry censuses without the temporary
/// driver patch (same `fortran_plugin_entry_verdict` call).
#[derive(Clone, Copy, PartialEq, Eq)]
enum RegradeMode {
    Substitution,
    Entry,
}

fn parse_mode() -> Result<(RegradeMode, bool), String> {
    match arg_opt("--mode").as_deref() {
        None => Ok((RegradeMode::Substitution, false)),
        Some("substitution") => Ok((RegradeMode::Substitution, true)),
        Some("entry") => Ok((RegradeMode::Entry, true)),
        Some(other) => Err(format!("--mode must be substitution|entry, got {other}")),
    }
}

fn unit_verdict(mode: RegradeMode, unit: &UnitDecl, tree: &Path) -> Result<(), Vec<String>> {
    match mode {
        RegradeMode::Substitution => fortran_substitution_verdict(unit, tree),
        RegradeMode::Entry => fortran_plugin_entry_verdict(unit, tree),
    }
}

/// One library from `--library-manifest`: member file-units plus the
/// expected dynamic-symbol set pinned from the reference build.
struct LibSpec {
    name: String,
    so: String,
    units: Vec<(String, String)>,
    expected_dynsyms: Vec<String>,
}

fn parse_manifest(path: &Path) -> Result<Vec<LibSpec>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let manifest: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let libs = manifest
        .get("libraries")
        .and_then(|l| l.as_array())
        .ok_or_else(|| format!("{}: missing 'libraries' array", path.display()))?;
    let mut out = Vec::new();
    for lib in libs {
        let name = lib
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| "library entry without a 'name'".to_string())?;
        let so = lib
            .get("so")
            .and_then(|s| s.as_str())
            .unwrap_or(&format!("{name}.so"))
            .to_string();
        let members = lib
            .get("units")
            .and_then(|u| u.as_array())
            .ok_or_else(|| format!("library {name}: missing 'units' array"))?;
        let mut units = Vec::new();
        for m in members {
            let unit = m
                .get("unit")
                .and_then(|u| u.as_str())
                .ok_or_else(|| format!("library {name}: member without a 'unit' id"))?;
            let ledger = m
                .get("ledger")
                .and_then(|l| l.as_str())
                .unwrap_or("lane-fem-a");
            units.push((unit.to_string(), ledger.to_string()));
        }
        let mut expected: Vec<String> = lib
            .get("expected_dynsyms")
            .and_then(|e| e.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|s| s.as_str().map(|t| t.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        expected.sort();
        expected.dedup();
        out.push(LibSpec {
            name: name.to_string(),
            so,
            units,
            expected_dynsyms: expected,
        });
    }
    Ok(out)
}

/// Defined dynamic symbols of a built `.so` (`nm -D --defined-only`),
/// sorted and deduplicated for comparison with the manifest pin.
fn nm_defined_symbols(so: &Path) -> Result<Vec<String>, String> {
    let out = std::process::Command::new("nm")
        .args(["-D", "--defined-only"])
        .arg(so)
        .output()
        .map_err(|e| format!("run nm on {}: {e}", so.display()))?;
    if !out.status.success() {
        return Err(format!("nm failed on {}", so.display()));
    }
    let text = String::from_utf8(out.stdout)
        .map_err(|e| format!("nm output on {} is not UTF-8: {e}", so.display()))?;
    let mut syms: Vec<String> = text
        .lines()
        .filter_map(|line| line.split_whitespace().last().map(|s| s.to_string()))
        .collect();
    syms.sort();
    syms.dedup();
    Ok(syms)
}

/// One flat verdict row for a single campaign unit under `mode`.
fn grade_row(
    unit_id: &str,
    ledger: &str,
    mode: RegradeMode,
    tree: &Path,
    dag_units: &BTreeMap<String, Vec<(String, bool)>>,
) -> serde_json::Value {
    let mut row = serde_json::json!({
        "unit": unit_id,
        "ledger": ledger,
    });
    match resolve_unit(unit_id, ledger, tree, dag_units) {
        Ok(unit) => {
            let n_exports = unit.exports.len();
            let n_fortran = unit
                .exports
                .iter()
                .filter(|e| matches!(e.abi, Abi::Fortran { bind_c: false }))
                .count();
            row["n_exports"] = serde_json::json!(n_exports);
            row["n_non_bind_c_fortran"] = serde_json::json!(n_fortran);
            match unit_verdict(mode, &unit, tree) {
                Ok(()) => {
                    row["verdict"] = serde_json::json!("admit");
                    row["reason"] = serde_json::json!([]);
                }
                Err(reasons) => {
                    row["verdict"] = serde_json::json!("refuse");
                    row["reason"] = serde_json::json!(reasons);
                }
            }
        }
        Err(e) => {
            row["verdict"] = serde_json::json!("unresolved");
            row["reason"] = serde_json::json!([e]);
        }
    }
    row
}

fn run() -> Result<(), String> {
    let dag_path = PathBuf::from(arg("--dag")?);
    let tree = PathBuf::from(arg("--tree")?);
    let out_path = PathBuf::from(arg("--out")?);
    let (mode, mode_explicit) = parse_mode()?;
    let manifest_path = arg_opt("--library-manifest").map(PathBuf::from);
    let reference_dir = arg_opt("--reference-dir").map(PathBuf::from);
    if reference_dir.is_some() && manifest_path.is_none() {
        return Err("--reference-dir needs --library-manifest".to_string());
    }
    let units_path = match arg_opt("--units") {
        Some(p) => Some(PathBuf::from(p)),
        None if manifest_path.is_some() => None,
        None => return Err("missing --units".to_string()),
    };

    let dag: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&dag_path)
            .map_err(|e| format!("read {}: {e}", dag_path.display()))?,
    )
    .map_err(|e| format!("parse {}: {e}", dag_path.display()))?;
    let dag_units = dag_exports(&dag);

    // Ordered unit requests: flat `--units` first, then manifest members
    // (deduplicated so a unit in both is graded once).
    let mut requests: Vec<(String, String, Option<String>)> = Vec::new();
    if let Some(path) = &units_path {
        let units: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?,
        )
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
        for entry in &units {
            let unit_id = entry
                .get("unit")
                .and_then(|u| u.as_str())
                .ok_or_else(|| "unit entry without a 'unit' id".to_string())?;
            let ledger = entry
                .get("ledger")
                .and_then(|l| l.as_str())
                .unwrap_or("lane-fem-a");
            requests.push((unit_id.to_string(), ledger.to_string(), None));
        }
    }
    let libs: Vec<LibSpec> = match &manifest_path {
        Some(path) => parse_manifest(path)?,
        None => Vec::new(),
    };
    for lib in &libs {
        for (unit_id, ledger) in &lib.units {
            if !requests.iter().any(|(u, l, _)| u == unit_id && l == ledger) {
                requests.push((unit_id.clone(), ledger.clone(), Some(lib.name.clone())));
            }
        }
    }

    let mut rows: Vec<serde_json::Value> = Vec::new();
    for (unit_id, ledger, library) in &requests {
        let mut row = grade_row(unit_id, ledger, mode, &tree, &dag_units);
        if let Some(name) = library {
            row["library"] = serde_json::json!(name);
        }
        rows.push(row);
    }

    // Per-library verdicts: per-file PluginEntry AND (the library-verdict
    // definition — always entry semantics), vetoed by a dynamic-symbol
    // mismatch against the reference build when `--reference-dir` is set.
    let mut lib_rows: Vec<serde_json::Value> = Vec::new();
    for lib in &libs {
        let mut reasons: Vec<String> = Vec::new();
        let mut n_admit = 0usize;
        for (unit_id, ledger) in &lib.units {
            let row = grade_row(unit_id, ledger, RegradeMode::Entry, &tree, &dag_units);
            if row["verdict"] != serde_json::json!("admit") {
                reasons.push(format!("{unit_id}: {}", row["reason"]));
            } else {
                n_admit += 1;
            }
        }
        let (actual, dynsym_match) = match &reference_dir {
            Some(dir) => match nm_defined_symbols(&dir.join(&lib.so)) {
                Ok(syms) => {
                    let same = syms == lib.expected_dynsyms;
                    if !same {
                        let missing: Vec<&String> = lib
                            .expected_dynsyms
                            .iter()
                            .filter(|s| !syms.contains(s))
                            .collect();
                        let extra: Vec<&String> = syms
                            .iter()
                            .filter(|s| !lib.expected_dynsyms.contains(s))
                            .collect();
                        reasons.push(format!(
                            "dynsym-mismatch: missing {missing:?}, extra {extra:?}"
                        ));
                    }
                    (Some(syms), Some(same))
                }
                Err(e) => {
                    reasons.push(format!("reference-unreadable: {e}"));
                    (None, Some(false))
                }
            },
            None => (None, None),
        };
        lib_rows.push(serde_json::json!({
            "name": lib.name,
            "so": lib.so,
            "mode": "entry",
            "units": lib.units.iter().map(|(u, _)| u).collect::<Vec<_>>(),
            "n_units": lib.units.len(),
            "n_admit": n_admit,
            "verdict": if reasons.is_empty() { "admit" } else { "refuse" },
            "reason": reasons,
            "expected_dynsyms": lib.expected_dynsyms,
            "actual_dynsyms": actual,
            "dynsym_match": dynsym_match,
        }));
    }

    let mut out = serde_json::json!({
        "generated_by": "rustsmith-adapters elmer-regrade",
        "dag": dag_path.display().to_string(),
        "tree": tree.display().to_string(),
        "units": rows,
    });
    // Default invocation stays byte-identical to the pre-flag driver:
    // `mode`/`libraries` appear only when explicitly requested.
    if mode_explicit {
        out["mode"] = serde_json::json!(match mode {
            RegradeMode::Substitution => "substitution",
            RegradeMode::Entry => "entry",
        });
    }
    if !lib_rows.is_empty() {
        out["libraries"] = serde_json::json!(lib_rows);
    }
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(
        &out_path,
        format!("{}\n", serde_json::to_string_pretty(&out).unwrap()),
    )
    .map_err(|e| format!("write {}: {e}", out_path.display()))?;
    let admits = rows
        .iter()
        .filter(|r| r["verdict"] == serde_json::json!("admit"))
        .count();
    let unresolved = rows
        .iter()
        .filter(|r| r["verdict"] == serde_json::json!("unresolved"))
        .count();
    let mode_name = match mode {
        RegradeMode::Substitution => "substitution",
        RegradeMode::Entry => "entry",
    };
    let lib_admits = lib_rows
        .iter()
        .filter(|l| l["verdict"] == serde_json::json!("admit"))
        .count();
    eprintln!(
        "elmer-regrade [{mode_name}]: {} units, {admits} admit, {} refuse, {unresolved} unresolved{}",
        rows.len(),
        rows.len() - admits - unresolved,
        if lib_rows.is_empty() {
            String::new()
        } else {
            format!(", {} libraries, {lib_admits} admit", lib_rows.len())
        },
    );
    Ok(())
}
