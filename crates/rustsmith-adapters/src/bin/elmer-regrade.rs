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
//!                  --units <units.json> --out <out.json>
//!
//! `<units.json>` is a JSON array of `{"unit": "<id>", "ledger": "<id>"}`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rustsmith_adapters::{
    fortran_scope_units, fortran_substitution_verdict, Abi, Symbol, UnitDecl, UnitId,
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

fn run() -> Result<(), String> {
    let dag_path = PathBuf::from(arg("--dag")?);
    let tree = PathBuf::from(arg("--tree")?);
    let units_path = PathBuf::from(arg("--units")?);
    let out_path = PathBuf::from(arg("--out")?);

    let dag: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&dag_path).map_err(|e| format!("read {}: {e}", dag_path.display()))?,
    )
    .map_err(|e| format!("parse {}: {e}", dag_path.display()))?;
    let dag_units = dag_exports(&dag);

    let units: Vec<serde_json::Value> = serde_json::from_str(
        &std::fs::read_to_string(&units_path)
            .map_err(|e| format!("read {}: {e}", units_path.display()))?,
    )
    .map_err(|e| format!("parse {}: {e}", units_path.display()))?;

    let mut rows: Vec<serde_json::Value> = Vec::new();
    for entry in &units {
        let unit_id = entry
            .get("unit")
            .and_then(|u| u.as_str())
            .ok_or_else(|| "unit entry without a 'unit' id".to_string())?;
        let ledger = entry.get("ledger").and_then(|l| l.as_str()).unwrap_or("lane-fem-a");
        let mut row = serde_json::json!({
            "unit": unit_id,
            "ledger": ledger,
        });
        match resolve_unit(unit_id, ledger, &tree, &dag_units) {
            Ok(unit) => {
                let n_exports = unit.exports.len();
                let n_fortran = unit
                    .exports
                    .iter()
                    .filter(|e| matches!(e.abi, Abi::Fortran { bind_c: false }))
                    .count();
                let verdict = fortran_substitution_verdict(&unit, &tree);
                row["n_exports"] = serde_json::json!(n_exports);
                row["n_non_bind_c_fortran"] = serde_json::json!(n_fortran);
                match verdict {
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
        rows.push(row);
    }

    let out = serde_json::json!({
        "generated_by": "rustsmith-adapters elmer-regrade",
        "dag": dag_path.display().to_string(),
        "tree": tree.display().to_string(),
        "units": rows,
    });
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&out_path, format!("{}\n", serde_json::to_string_pretty(&out).unwrap()))
        .map_err(|e| format!("write {}: {e}", out_path.display()))?;
    let admits = rows
        .iter()
        .filter(|r| r["verdict"] == serde_json::json!("admit"))
        .count();
    let unresolved = rows
        .iter()
        .filter(|r| r["verdict"] == serde_json::json!("unresolved"))
        .count();
    eprintln!("elmer-regrade: {} units, {admits} admit, {} refuse, {unresolved} unresolved",
        rows.len(), rows.len() - admits - unresolved);
    Ok(())
}
