//! Host conversions: Python values to/from core data (`attrs`, `meta`,
//! `env`, linkify matches, `mdurl` URL normalization).

use std::collections::HashMap;

use markdown_it_rust_core::options_env::{EnvData, EnvValue, ExtraVal};
use markdown_it_rust_core::punycode;
use markdown_it_rust_core::token::{AttrVal, MetaVal};
use markdown_it_rust_core::{DuplicateRef, EnvSnapshot, EnvSnapshotValue, Reference};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyModule};


pub fn attr_val_from_py(py: Python, v: &Bound<PyAny>) -> PyResult<AttrVal> {
    let _ = py;
    if let Ok(s) = v.extract::<String>() {
        return Ok(AttrVal::Str(s));
    }
    if v.is_instance_of::<PyBool>() {
        return Ok(AttrVal::Bool(v.extract::<bool>()?));
    }
    if let Ok(i) = v.extract::<i64>() {
        return Ok(AttrVal::Int(i));
    }
    if let Ok(f) = v.extract::<f64>() {
        return Ok(AttrVal::Float(f));
    }
    Err(PyTypeError::new_err("attr value must be str, int, float, or bool"))
}

pub fn attr_val_to_py(py: Python, v: &AttrVal) -> PyResult<Py<PyAny>> {
    Ok(match v {
        AttrVal::Str(s) => s.into_pyobject(py)?.into_any().unbind(),
        AttrVal::Int(i) => i.into_pyobject(py)?.into_any().unbind(),
        AttrVal::Float(f) => f.into_pyobject(py)?.into_any().unbind(),
        AttrVal::Bool(b) => PyBool::new(py, *b).to_owned().into_any().unbind(),
    })
}

pub fn meta_val_from_py(py: Python, v: &Bound<PyAny>) -> PyResult<MetaVal> {
    let _ = py;
    if v.is_none() {
        return Ok(MetaVal::Null);
    }
    if v.is_instance_of::<PyBool>() {
        return Ok(MetaVal::Bool(v.extract::<bool>()?));
    }
    if let Ok(i) = v.extract::<i64>() {
        return Ok(MetaVal::Int(i));
    }
    if let Ok(f) = v.extract::<f64>() {
        return Ok(MetaVal::Float(f));
    }
    if let Ok(s) = v.extract::<String>() {
        return Ok(MetaVal::Str(s));
    }
    Err(PyTypeError::new_err(
        "meta value must be None, bool, int, float, or str",
    ))
}

pub fn meta_val_to_py(py: Python, v: &MetaVal) -> PyResult<Py<PyAny>> {
    Ok(match v {
        MetaVal::Null => py.None(),
        MetaVal::Bool(b) => PyBool::new(py, *b).to_owned().into_any().unbind(),
        MetaVal::Int(i) => i.into_pyobject(py)?.into_any().unbind(),
        MetaVal::Float(f) => f.into_pyobject(py)?.into_any().unbind(),
        MetaVal::Str(s) => s.into_pyobject(py)?.into_any().unbind(),
    })
}

pub fn extra_val_from_py(py: Python, v: &Bound<PyAny>) -> PyResult<ExtraVal> {
    let _ = py;
    if v.is_none() {
        return Ok(ExtraVal::Null);
    }
    if v.is_instance_of::<PyBool>() {
        return Ok(ExtraVal::Bool(v.extract::<bool>()?));
    }
    if let Ok(i) = v.extract::<i64>() {
        return Ok(ExtraVal::Int(i));
    }
    if let Ok(f) = v.extract::<f64>() {
        return Ok(ExtraVal::Float(f));
    }
    if let Ok(s) = v.extract::<String>() {
        return Ok(ExtraVal::Str(s));
    }
    Err(PyTypeError::new_err("not a scalar option value"))
}

pub fn env_value_from_py(_py: Python, v: &Bound<PyAny>) -> PyResult<EnvValue> {
    if v.is_none() {
        return Ok(EnvValue::Null);
    }
    if v.is_instance_of::<PyBool>() {
        return Ok(EnvValue::Bool(v.extract::<bool>()?));
    }
    if let Ok(i) = v.extract::<i64>() {
        return Ok(EnvValue::Int(i));
    }
    if let Ok(f) = v.extract::<f64>() {
        return Ok(EnvValue::Float(f));
    }
    if let Ok(s) = v.extract::<String>() {
        return Ok(EnvValue::Str(s));
    }
    if let Ok(list) = v.downcast::<PyList>() {
        let mut out = Vec::new();
        for item in list.iter() {
            out.push(env_value_from_py(_py, &item)?);
        }
        return Ok(EnvValue::List(out));
    }
    if let Ok(dict) = v.downcast::<PyDict>() {
        let mut out = Vec::new();
        for (k, val) in dict.iter() {
            out.push((k.extract::<String>()?, env_value_from_py(_py, &val)?));
        }
        return Ok(EnvValue::Map(out));
    }
    Err(PyTypeError::new_err("not a JSON-shaped env value"))
}

pub fn env_value_to_py(py: Python, v: &EnvValue) -> PyResult<Py<PyAny>> {
    Ok(match v {
        EnvValue::Null => py.None(),
        EnvValue::Bool(b) => PyBool::new(py, *b).to_owned().into_any().unbind(),
        EnvValue::Int(i) => i.into_pyobject(py)?.into_any().unbind(),
        EnvValue::Float(f) => f.into_pyobject(py)?.into_any().unbind(),
        EnvValue::Str(s) => s.into_pyobject(py)?.into_any().unbind(),
        EnvValue::List(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(env_value_to_py(py, item)?)?;
            }
            list.into_any().unbind()
        }
        EnvValue::Map(pairs) => {
            let dict = PyDict::new(py);
            for (k, val) in pairs {
                dict.set_item(k, env_value_to_py(py, val)?)?;
            }
            dict.into_any().unbind()
        }
    })
}

fn snapshot_value_from_env(v: &EnvValue) -> EnvSnapshotValue {
    match v {
        EnvValue::Null => EnvSnapshotValue::Null,
        EnvValue::Bool(b) => EnvSnapshotValue::Bool(*b),
        EnvValue::Int(i) => EnvSnapshotValue::Int(*i),
        EnvValue::Float(f) => EnvSnapshotValue::Float(*f),
        EnvValue::Str(s) => EnvSnapshotValue::Str(s.clone()),
        EnvValue::List(items) => {
            EnvSnapshotValue::List(items.iter().map(snapshot_value_from_env).collect())
        }
        EnvValue::Map(pairs) => EnvSnapshotValue::Map(
            pairs.iter().map(|(k, v)| (k.clone(), snapshot_value_from_env(v))).collect(),
        ),
    }
}

fn snapshot_value_to_env(v: &EnvSnapshotValue) -> EnvValue {
    match v {
        EnvSnapshotValue::Null => EnvValue::Null,
        EnvSnapshotValue::Bool(b) => EnvValue::Bool(*b),
        EnvSnapshotValue::Int(i) => EnvValue::Int(*i),
        EnvSnapshotValue::Float(f) => EnvValue::Float(*f),
        EnvSnapshotValue::Str(s) => EnvValue::Str(s.clone()),
        EnvSnapshotValue::List(items) => {
            EnvValue::List(items.iter().map(snapshot_value_to_env).collect())
        }
        EnvSnapshotValue::Map(pairs) => EnvValue::Map(
            pairs.iter().map(|(k, v)| (k.clone(), snapshot_value_to_env(v))).collect(),
        ),
        EnvSnapshotValue::Opaque(_) => EnvValue::Null,
    }
}

fn snapshot_value_to_py(py: Python, v: &EnvSnapshotValue) -> PyResult<Py<PyAny>> {
    match v {
        EnvSnapshotValue::Opaque(_) => Ok(py.None()),
        _ => env_value_to_py(py, &snapshot_value_to_env(v)),
    }
}

fn snapshot_value_from_py(py: Python, v: &Bound<PyAny>) -> PyResult<EnvSnapshotValue> {
    match env_value_from_py(py, v) {
        Ok(json) => Ok(match json {
            EnvValue::Null => EnvSnapshotValue::Null,
            EnvValue::Bool(b) => EnvSnapshotValue::Bool(b),
            EnvValue::Int(i) => EnvSnapshotValue::Int(i),
            EnvValue::Float(f) => EnvSnapshotValue::Float(f),
            EnvValue::Str(s) => EnvSnapshotValue::Str(s),
            EnvValue::List(items) => EnvSnapshotValue::List(
                items.into_iter().map(|x| snapshot_value_from_env(&x)).collect(),
            ),
            EnvValue::Map(pairs) => EnvSnapshotValue::Map(
                pairs.into_iter().map(|(k, x)| (k, snapshot_value_from_env(&x))).collect(),
            ),
        }),
        Err(_) => Err(PyTypeError::new_err(
            "env values must be None, bool, int, float, str, list, or dict",
        )),
    }
}

pub fn env_to_snapshot(env: &EnvData, opaques: &[String]) -> EnvSnapshot {
    let mut entries = Vec::new();
    if !env.references.is_empty() {
        let mut refs = Vec::new();
        for (label, r) in &env.references {
            refs.push((
                label.clone(),
                EnvSnapshotValue::Map(vec![
                    ("title".to_string(), EnvSnapshotValue::Str(r.title.clone())),
                    ("href".to_string(), EnvSnapshotValue::Str(r.href.clone())),
                    (
                        "map".to_string(),
                        match r.map {
                            Some((a, b)) => EnvSnapshotValue::List(vec![
                                EnvSnapshotValue::Int(a as i64),
                                EnvSnapshotValue::Int(b as i64),
                            ]),
                            None => EnvSnapshotValue::Null,
                        },
                    ),
                ]),
            ));
        }
        entries.push(("references".to_string(), EnvSnapshotValue::Map(refs)));
    }
    if !env.duplicate_refs.is_empty() {
        let mut dups = Vec::new();
        for d in &env.duplicate_refs {
            dups.push(EnvSnapshotValue::Map(vec![
                ("title".to_string(), EnvSnapshotValue::Str(d.title.clone())),
                ("href".to_string(), EnvSnapshotValue::Str(d.href.clone())),
                ("label".to_string(), EnvSnapshotValue::Str(d.label.clone())),
                (
                    "map".to_string(),
                    match d.map {
                        Some((a, b)) => EnvSnapshotValue::List(vec![
                            EnvSnapshotValue::Int(a as i64),
                            EnvSnapshotValue::Int(b as i64),
                        ]),
                        None => EnvSnapshotValue::Null,
                    },
                ),
            ]));
        }
        entries.push(("duplicate_refs".to_string(), EnvSnapshotValue::List(dups)));
    }
    for (k, v) in &env.extra {
        entries.push((k.clone(), snapshot_value_from_env(v)));
    }
    for name in opaques {
        entries.push((name.clone(), EnvSnapshotValue::Opaque(name.clone())));
    }
    EnvSnapshot { entries }
}

/// Materialize the live `env` dict for a Python rule.
pub fn env_to_py(py: Python, env: &EnvData) -> Py<PyDict> {
        let snapshot = env_to_snapshot(env, &[]);
        let dict = PyDict::new(py);
        for (k, v) in &snapshot.entries {
            if let Ok(val) = snapshot_value_to_py(py, v) {
                dict.set_item(k, val).ok();
            }
        }
        dict.unbind()
    }

/// Merge a Python rule's `env` writes back (deleted `references` clears).
pub fn env_from_py(py: Python, env: &mut EnvData, dict: &Bound<PyAny>) {
    let Ok(dict) = dict.downcast::<PyDict>() else {
        return;
    };
    // `references`
    match dict.get_item("references").ok().flatten() {
        Some(v) if !v.is_none() => {
            if let Ok(rd) = v.downcast::<PyDict>() {
                let mut refs = HashMap::new();
                for (label, val) in rd.iter() {
                    if let (Ok(l), Ok(vd)) =
                        (label.extract::<String>(), val.downcast::<PyDict>())
                    {
                        let title = vd
                            .get_item("title")
                            .ok()
                            .flatten()
                            .and_then(|x| x.extract::<String>().ok())
                            .unwrap_or_default();
                        let href = vd
                            .get_item("href")
                            .ok()
                            .flatten()
                            .and_then(|x| x.extract::<String>().ok())
                            .unwrap_or_default();
                        let map = vd
                            .get_item("map")
                            .ok()
                            .flatten()
                            .and_then(|x| x.extract::<Vec<usize>>().ok())
                            .and_then(|m| if m.len() == 2 { Some((m[0], m[1])) } else { None });
                        refs.insert(l, Reference { title, href, map });
                    }
                }
                env.references = refs;
            }
        }
        _ => {
            env.references.clear();
        }
    }
    // `duplicate_refs`
    if let Ok(Some(v)) = dict.get_item("duplicate_refs") {
        if let Ok(list) = v.downcast::<PyList>() {
            let mut dups = Vec::new();
            for item in list.iter() {
                if let Ok(d) = item.downcast::<PyDict>() {
                    let get = |k: &str| -> String {
                        d.get_item(k).ok().flatten().and_then(|x| x.extract().ok()).unwrap_or_default()
                    };
                    let map = d
                        .get_item("map")
                        .ok()
                        .flatten()
                        .and_then(|x| x.extract::<Vec<usize>>().ok())
                        .and_then(|m| if m.len() == 2 { Some((m[0], m[1])) } else { None });
                    dups.push(DuplicateRef {
                        title: get("title"),
                        href: get("href"),
                        label: get("label"),
                        map,
                    });
                }
            }
            env.duplicate_refs = dups;
        }
    }
    // Everything else: JSON into core extra (other values are dropped).
    for (k, v) in dict.iter() {
        let Ok(key) = k.extract::<String>() else {
            continue;
        };
        if key == "references" || key == "duplicate_refs" {
            continue;
        }
        match snapshot_value_from_py(py, &v) {
            Ok(EnvSnapshotValue::Opaque(_)) => {}
            Ok(sv) => {
                env.extra.insert(key, snapshot_value_to_env(&sv));
            }
            Err(_) => {}
        }
    }
}

/// Write-back into the user's mapping (verbatim mutation of the passed
/// dict: current values for input keys plus produced keys).
pub fn env_to_user(py: Python, env: &EnvData, dict: &Bound<PyDict>, input_keys: &[String]) {
    let live = env_to_py(py, env);
    let live = live.bind(py);
    for key in input_keys {
        if let Ok(Some(v)) = live.get_item(key) {
            dict.set_item(key, v).ok();
        } else {
            dict.del_item(key).ok();
        }
    }
    for key in ["references", "duplicate_refs"] {
        if !input_keys.iter().any(|k| k == key) {
            if let Ok(Some(v)) = live.get_item(key) {
                dict.set_item(key, v).ok();
            }
        }
    }
}

/// `normalizeLink` / `normalizeLinkText` through the Python `mdurl` module
/// with a core punycode step (verbatim `markdown_it/common/normalize_url.py`).
pub fn normalize_link_py(py: Python, mdurl: &Py<PyModule>, url: &str, for_text: bool) -> String {
    let m = mdurl.bind(py);
    let kwargs = PyDict::new(py);
    kwargs.set_item("slashes_denote_host", true).ok();
    let parsed: Bound<PyAny> = m.call_method("parse", (url,), Some(&kwargs)).unwrap();
    // NOTE: `hostname`/`protocol` are fields, not methods.
    let hostname: String = parsed
        .getattr("hostname")
        .ok()
        .and_then(|v| v.extract().ok())
        .unwrap_or_default();
    let protocol: String = parsed
        .getattr("protocol")
        .ok()
        .and_then(|v| v.extract().ok())
        .unwrap_or_default();
    let mut parsed = parsed;
    if !hostname.is_empty() && (protocol.is_empty() || ["http:", "https:", "mailto:"].contains(&protocol.as_str())) {
        // `with suppress(Exception)` verbatim: punycode/decode failures keep `parsed`.
        // Verbatim: `to_unicode` errors propagate (only `to_ascii` is
        // `suppress`ed, and it is infallible).
        let converted = if for_text {
            match punycode::to_unicode(&hostname) {
                Ok(h) => Some(h),
                Err(e) => markdown_it_rust_core::panic_unicode_error(e.0),
            }
        } else {
            Some(punycode::to_ascii(&hostname))
        };
        if let Some(host) = converted {
            let kw = PyDict::new(py);
            kw.set_item("hostname", host).ok();
            if let Ok(replaced) = parsed.call_method("_replace", (), Some(&kw)) {
                parsed = replaced;
            }
        }
    }
    let formatted: String = m.call_method1("format", (parsed,)).unwrap().extract().unwrap();
    if for_text {
        let default_chars: String = m.getattr("DECODE_DEFAULT_CHARS").unwrap().extract().unwrap();
        m.call_method1("decode", (formatted, default_chars + "%"))
            .unwrap()
            .extract()
            .unwrap()
    } else {
        m.call_method1("encode", (formatted,)).unwrap().extract().unwrap()
    }
}

/// Convert a user `env` mapping into fresh core env data (parse entry).
pub fn env_snapshot_to_core(py: Python, dict: &Bound<PyAny>) -> PyResult<EnvData> {
    let mut env = EnvData::default();
    env_from_py(py, &mut env, dict);
    Ok(env)
}
