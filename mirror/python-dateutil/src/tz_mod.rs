// SPDX-License-Identifier: Apache-2.0
//! `tz` binding: tzinfo classes over `dateutil-core` resolution, with all
//! datetime operations through real Python calls so CPython semantics
//! (fold, errors, subclassing) stay exact. C-library reads (`tzlocal`),
//! file search, and the zoneinfo tarball stay at the Python boundary.

use crate::util::*;
use dateutil_core::tz::Zone;
use dateutil_core::{civil, tz as coretz, tzfile};
use pyo3::basic::CompareOp;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

/// Split a `timedelta` into (truncated seconds, micros remainder) for
/// exact core arithmetic.
fn split_delta(obj: &Bound<'_, PyAny>) -> PyResult<(i64, i64)> {
    let (d, s, u) = delta_parts(obj)?;
    let total_micros = d as i128 * 86_400_000_000 + s as i128 * 1_000_000 + u as i128;
    let secs = (total_micros / 1_000_000) as i64;
    let micros = (total_micros % 1_000_000) as i64;
    Ok((secs, micros))
}

/// Wall triple + fold from a datetime (naive walls keep fold 0).
fn wall_of(dt: &Bound<'_, PyAny>) -> PyResult<(coretz::Wall, i64)> {
    let p = parts_of(dt)?;
    let ord = civil::to_ordinal(p.y, p.m as u8, p.d as u8);
    let wall = coretz::Wall {
        ord,
        secs: p.hh * 3600 + p.mm * 60 + p.ss,
        micros: p.us,
    };
    Ok((wall, p.fold))
}

/// `datetime_exists`/`datetime_ambiguous` dispatch helper: call the
/// zone's own `is_ambiguous` when present (mirrors the `getattr` probe).
fn call_is_ambiguous(
    py: Python<'_>,
    tz: &Bound<'_, PyAny>,
    dt: &Bound<'_, PyAny>,
) -> PyResult<Option<bool>> {
    let f = tz.getattr("is_ambiguous")?;
    if f.is_none() {
        return Ok(None);
    }
    match f.call1((dt,)) {
        Ok(v) => Ok(Some(v.extract::<bool>()?)),
        Err(_) => Ok(None),
    }
}

/// `_ttinfo` data holder (mutable, like the original's `__slots__` class).
#[pyclass(weakref, name = "_ttinfo", subclass, dict, module = "dateutil._dateutil")]
pub struct TtInfo {
    offset: Option<PyObject>,
    delta: Option<PyObject>,
    isdst: Option<PyObject>,
    abbr: Option<PyObject>,
    isstd: Option<PyObject>,
    isgmt: Option<PyObject>,
    dstoffset: Option<PyObject>,
}

#[pymethods]
impl TtInfo {
    #[new]
    fn new() -> Self {
        TtInfo {
            offset: None,
            delta: None,
            isdst: None,
            abbr: None,
            isstd: None,
            isgmt: None,
            dstoffset: None,
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let mut l = Vec::new();
        for (attr, o) in [
            ("offset", &self.offset),
            ("delta", &self.delta),
            ("isdst", &self.isdst),
            ("abbr", &self.abbr),
            ("isstd", &self.isstd),
            ("isgmt", &self.isgmt),
            ("dstoffset", &self.dstoffset),
        ] {
            if let Some(v) = o {
                let r: String = v.bind(py).repr()?.extract()?;
                l.push(format!("{}={}", attr, r));
            }
        }
        Ok(format!("_ttinfo({})", l.join(", ")))
    }

    fn __eq__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let cls = py.get_type::<TtInfo>();
        if !other.is_instance(&cls)? {
            return Ok(not_implemented(py));
        }
        for attr in ["offset", "delta", "isdst", "abbr", "isstd", "isgmt", "dstoffset"] {
            let a = self.get_field(py, attr);
            let b: PyObject = other.getattr(attr)?.unbind();
            if !py_eq(py, a.bind(py), b.bind(py))? {
                return Ok(bool_obj(py, false));
            }
        }
        Ok(bool_obj(py, true))
    }

    fn __ne__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let eq = self.__eq__(py, other)?;
        if eq.is(py.NotImplemented().into_any().bind(py)) {
            warn_with(
                py,
                &py.get_type::<pyo3::exceptions::PyDeprecationWarning>().into_any(),
                "NotImplemented should not be used in a boolean context",
            )?;
            return Ok(bool_obj(py, false));
        }
        let b: bool = eq.extract(py)?;
        Ok(bool_obj(py, !b))
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let _ = py;
        Err(pyo3::exceptions::PyTypeError::new_err("unhashable type: '_ttinfo'"))
    }

    #[getter]
    fn offset(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.offset)
    }
    #[setter]
    fn set_offset(&mut self, v: Bound<'_, PyAny>) -> PyResult<()> {
        self.offset = if v.is_none() { None } else { Some(v.unbind()) };
        Ok(())
    }
    #[getter]
    fn delta(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.delta)
    }
    #[setter]
    fn set_delta(&mut self, v: Bound<'_, PyAny>) -> PyResult<()> {
        self.delta = if v.is_none() { None } else { Some(v.unbind()) };
        Ok(())
    }
    #[getter]
    fn isdst(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.isdst)
    }
    #[setter]
    fn set_isdst(&mut self, v: Bound<'_, PyAny>) -> PyResult<()> {
        self.isdst = if v.is_none() { None } else { Some(v.unbind()) };
        Ok(())
    }
    #[getter]
    fn abbr(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.abbr)
    }
    #[setter]
    fn set_abbr(&mut self, v: Bound<'_, PyAny>) -> PyResult<()> {
        self.abbr = if v.is_none() { None } else { Some(v.unbind()) };
        Ok(())
    }
    #[getter]
    fn isstd(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.isstd)
    }
    #[setter]
    fn set_isstd(&mut self, v: Bound<'_, PyAny>) -> PyResult<()> {
        self.isstd = if v.is_none() { None } else { Some(v.unbind()) };
        Ok(())
    }
    #[getter]
    fn isgmt(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.isgmt)
    }
    #[setter]
    fn set_isgmt(&mut self, v: Bound<'_, PyAny>) -> PyResult<()> {
        self.isgmt = if v.is_none() { None } else { Some(v.unbind()) };
        Ok(())
    }
    #[getter]
    fn dstoffset(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.dstoffset)
    }
    #[setter]
    fn set_dstoffset(&mut self, v: Bound<'_, PyAny>) -> PyResult<()> {
        self.dstoffset = if v.is_none() { None } else { Some(v.unbind()) };
        Ok(())
    }
    fn __getstate__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = PyDict::new(py);
        for (attr, o) in [
            ("offset", &self.offset),
            ("delta", &self.delta),
            ("isdst", &self.isdst),
            ("abbr", &self.abbr),
            ("isstd", &self.isstd),
            ("isgmt", &self.isgmt),
            ("dstoffset", &self.dstoffset),
        ] {
            d.set_item(attr, opt_clone(py, o))?;
        }
        Ok(d.into_any().unbind())
    }

    fn __setstate__(&mut self, py: Python<'_>, state: Bound<'_, PyAny>) -> PyResult<()> {
        for (attr, slot) in [
            ("offset", &mut self.offset),
            ("delta", &mut self.delta),
            ("isdst", &mut self.isdst),
            ("abbr", &mut self.abbr),
            ("isstd", &mut self.isstd),
            ("isgmt", &mut self.isgmt),
            ("dstoffset", &mut self.dstoffset),
        ] {
            let v: Bound<'_, PyAny> = state.get_item(attr)?;
            *slot = if v.is_none() { None } else { Some(v.unbind()) };
        }
        let _ = py;
        Ok(())
    }

}

impl TtInfo {
    fn get_field(&self, py: Python<'_>, attr: &str) -> PyObject {
        let o = match attr {
            "offset" => &self.offset,
            "delta" => &self.delta,
            "isdst" => &self.isdst,
            "abbr" => &self.abbr,
            "isstd" => &self.isstd,
            "isgmt" => &self.isgmt,
            _ => &self.dstoffset,
        };
        opt_clone(py, o)
    }
}

// ---- shared tz helpers ----

/// Cached `timedelta(0)` equivalent (constructed per call; cheap).
fn zero_delta(py: Python<'_>) -> PyObject {
    let td = timedelta_cls(py).unwrap();
    td.call1((0, 0, 0)).unwrap().unbind()
}

/// `__ne__ = not __eq__` with the `NotImplemented` edge, like the originals.
fn ne_negate(py: Python<'_>, eq: PyObject) -> PyResult<PyObject> {
    // Propagate `NotImplemented` so Python tries the reflected operation
    // (mirrors the default `object.__ne__` delegation).
    let ni = py.NotImplemented().into_any();
    if eq.bind(py).is(&ni) {
        return Ok(ni);
    }
    let b: bool = eq.extract(py)?;
    Ok(bool_obj(py, !b))
}

/// `_validate_fromutc_inputs`: isinstance-datetime + `tzinfo is self`.
/// Returns the validated `dt` (borrowed).
fn check_fromutc<'py>(
    py: Python<'py>,
    slf_obj: &Bound<'py, PyAny>,
    dt: &'py Bound<'py, PyAny>,
) -> PyResult<&'py Bound<'py, PyAny>> {
    let dt_cls = datetime_cls(py)?;
    if !dt.is_instance(dt_cls)? {
        return type_error("fromutc() requires a datetime argument".to_string());
    }
    if !dt.getattr("tzinfo")?.is(slf_obj) {
        return value_error("dt.tzinfo is not self".to_string());
    }
    Ok(dt)
}

// ---- TzUtc / TzOffset / TzLocal ----
/// `tzutc`: fixed UTC zone (singleton via the shim metaclass).
#[pyclass(weakref, name = "tzutc", extends=pyo3::types::PyTzInfo, subclass, dict, module = "dateutil._dateutil")]
pub struct TzUtc;

#[pymethods]
impl TzUtc {
    #[new]
    fn new() -> Self {
        TzUtc
    }

    fn __init__(&self) -> PyResult<()> {
        Ok(())
    }

    fn utcoffset(&self, py: Python<'_>, _dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        Ok(zero_delta(py))
    }

    fn dst(&self, py: Python<'_>, _dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        Ok(zero_delta(py))
    }

    fn tzname(&self, _py: Python<'_>, _dt: Bound<'_, PyAny>) -> PyResult<String> {
        Ok("UTC".to_string())
    }

    fn is_ambiguous(&self, _py: Python<'_>, _dt: Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(false)
    }

    fn fromutc(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        // Fast path: validated inputs, returns `dt` unchanged.
        use pyo3::conversion::IntoPyObject;
        let slf_obj = slf.into_pyobject(py).unwrap().into_any();
        check_fromutc(py, &slf_obj, &dt)?;
        Ok(dt.unbind())
    }

    fn __eq__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let utc_t = py.get_type::<TzUtc>();
        let off_t = py.get_type::<TzOffset>();
        if !other.is_instance(&utc_t)? && !other.is_instance(&off_t)? {
            return Ok(not_implemented(py));
        }
        if other.is_instance(&utc_t)? {
            return Ok(bool_obj(py, true));
        }
        // tzoffset with zero offset.
        let off: Bound<'_, PyAny> = other.getattr("_offset")?;
        let zero = zero_delta(py);
        Ok(bool_obj(py, py_eq(py, &off, zero.bind(py))?))
    }

    fn __ne__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        ne_negate(py, self.__eq__(py, other)?)
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let _ = py;
        Err(pyo3::exceptions::PyTypeError::new_err("unhashable type: 'tzutc'"))
    }

    fn __repr__(&self) -> String {
        "tzutc()".to_string()
    }

    fn __getstate__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = PyDict::new(py);
        Ok(d.into_any().unbind())
    }

    fn __setstate__(&mut self, _state: Bound<'_, PyAny>) -> PyResult<()> {
        Ok(())
    }
}

impl TzUtc {
    pub fn new_singleton(py: Python<'_>) -> PyResult<PyObject> {
        Ok(Py::new(py, TzUtc)?.into_any())
    }
}

#[pyfunction]
#[pyo3(signature = (dt, fold=None))]
pub fn enfold(dt: Bound<'_, PyAny>, fold: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
    let py = dt.py();
    let f: i64 = match fold {
        None => 1,
        Some(v) => {
            if v.is_none() {
                1
            } else {
                v.extract()?
            }
        }
    };
    enfold_dt(py, &dt, f)
}

#[pyfunction]
#[pyo3(signature = (dt, tz=None))]
pub fn datetime_ambiguous(dt: Bound<'_, PyAny>, tz: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
    let py = dt.py();
    let tz = match tz {
        Some(t) if !t.is_none() => t,
        _ => {
            let ti = dt.getattr("tzinfo")?;
            if ti.is_none() {
                return value_error("Datetime is naive and no time zone provided.".to_string());
            }
            ti
        }
    };
    // Zone's own `is_ambiguous` when present (failures fall through).
    if let Ok(f) = tz.getattr("is_ambiguous") {
        if !f.is_none() {
            match f.call1((dt.clone(),)) {
                Ok(v) => {
                    if let Ok(b) = v.extract::<bool>() {
                        return Ok(bool_obj(py, b));
                    }
                }
                Err(_) => {}
            }
        }
    }
    // Fold-comparison fallback.
    let dt_r = replace_tzinfo(py, &dt, tz.clone().unbind())?;
    let wall_0 = enfold_dt(py, &dt_r, 0)?;
    let wall_1 = enfold_dt(py, &dt_r, 1)?;
    let o0 = wall_0.bind(py).call_method0("utcoffset")?;
    let o1 = wall_1.bind(py).call_method0("utcoffset")?;
    let same_offset = py_eq(py, &o0, &o1)?;
    let d0 = wall_0.bind(py).call_method0("dst")?;
    let d1 = wall_1.bind(py).call_method0("dst")?;
    let same_dst = py_eq(py, &d0, &d1)?;
    Ok(bool_obj(py, !(same_offset && same_dst)))
}

#[pyfunction]
#[pyo3(signature = (dt, tz=None))]
pub fn datetime_exists(dt: Bound<'_, PyAny>, tz: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
    let py = dt.py();
    let tz = match tz {
        Some(t) if !t.is_none() => t,
        _ => {
            let ti = dt.getattr("tzinfo")?;
            if ti.is_none() {
                return value_error("Datetime is naive and no time zone provided.".to_string());
            }
            ti
        }
    };
    let utc = utc_obj(py)?;
    // Round trip to UTC (uses real `astimezone`, fold included).
    let dt_r = replace_tzinfo(py, &dt, py.None())?;
    let rt = replace_tzinfo(py, &dt_r, tz.clone().unbind())?;
    let rt = rt.call_method1("astimezone", (utc,))?;
    let rt = rt.call_method1("astimezone", (&tz,))?;
    let rt = replace_tzinfo(py, &rt, py.None())?;
    Ok(bool_obj(py, py_eq(py, &dt_r, &rt)?))
}

#[pyfunction]
#[pyo3(signature = (dt,))]
pub fn resolve_imaginary(dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
    let py = dt.py();
    let ti = dt.getattr("tzinfo")?;
    if !ti.is_none() {
        let exists = datetime_exists(dt.clone(), Some(ti.clone()))?;
        let ex: bool = exists.extract(py)?;
        if !ex {
            let td = timedelta_cls(py)?;
            let curr = dt.add(td.call1((1, 0, 0))?)?;
            let old = dt.sub(td.call1((1, 0, 0))?)?;
            let curr_off = curr.call_method0("utcoffset")?;
            let old_off = old.call_method0("utcoffset")?;
            let diff = curr_off.sub(&old_off)?;
            return Ok(dt.add(&diff)?.unbind());
        }
    }
    Ok(dt.unbind())
}

/// `dt.replace(tzinfo=tz)` helper.
fn replace_tzinfo<'py>(py: Python<'py>, dt: &Bound<'py, PyAny>, tz: PyObject) -> PyResult<Bound<'py, PyAny>> {
    let kw = PyDict::new(py);
    kw.set_item("tzinfo", tz)?;
    Ok(dt.call_method("replace", (), Some(&kw))?)
}

/// `tzoffset`: fixed-offset zone (interned via the shim metaclass).
#[pyclass(weakref, name = "tzoffset", extends=pyo3::types::PyTzInfo, subclass, dict, module = "dateutil._dateutil")]
pub struct TzOffset {
    name: Option<PyObject>,
    offset: Option<PyObject>,
}

#[pymethods]
impl TzOffset {
    #[new]
    #[pyo3(signature = (name=None, offset=None))]
    fn new(py: Python<'_>, name: Option<Bound<'_, PyAny>>, offset: Option<Bound<'_, PyAny>>) -> PyResult<Self> {
        // All-optional so unpickling (`cls.__new__(cls)`) succeeds; the
        // state is restored via `__setstate__` (mirrors `object.__reduce__`
        // + `__dict__` restore).
        let mut o = TzOffset { name: None, offset: None };
        if let Some(offset) = offset {
            o.init(py, name, Some(offset))?;
        } else if name.is_some() {
            return type_error(
                "__init__() missing 1 required positional argument: 'offset'".to_string(),
            );
        }
        Ok(o)
    }

    #[pyo3(signature = (name=None, offset=None))]
    fn __init__(
        &mut self,
        py: Python<'_>,
        name: Option<Bound<'_, PyAny>>,
        offset: Option<Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        // No-op when called without offset (unpickle path constructs empty
        // then state-fills); real init goes through `init`.
        if offset.is_some() {
            self.init(py, name, offset)?;
        }
        Ok(())
    }

    fn utcoffset(&self, py: Python<'_>, _dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        Ok(clone_opt(py, &self.offset).unwrap_or_else(|| zero_delta(py)))
    }

    fn dst(&self, py: Python<'_>, _dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        Ok(zero_delta(py))
    }

    fn tzname(&self, py: Python<'_>, _dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        Ok(opt_clone(py, &self.name))
    }

    fn is_ambiguous(&self, _py: Python<'_>, _dt: Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(false)
    }

    fn fromutc(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        use pyo3::conversion::IntoPyObject;
        let slf_obj = slf.into_pyobject(py).unwrap().into_any();
        check_fromutc(py, &slf_obj, &dt)?;
        let off = slf_obj.getattr("_offset")?;
        Ok(dt.add(off)?.unbind())
    }

    fn __eq__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let off_t = py.get_type::<TzOffset>();
        if !other.is_instance(&off_t)? {
            return Ok(not_implemented(py));
        }
        let mine: Bound<'_, PyAny> = clone_opt(py, &self.offset).unwrap_or_else(|| zero_delta(py)).bind(py).to_owned();
        let theirs: Bound<'_, PyAny> = other.getattr("_offset")?;
        Ok(bool_obj(py, py_eq(py, &mine, &theirs)?))
    }

    fn __ne__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        ne_negate(py, self.__eq__(py, other)?)
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let _ = py;
        Err(pyo3::exceptions::PyTypeError::new_err("unhashable type: 'tzoffset'"))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        // `"%s(%s, %s)" % (classname, repr(name), int(seconds))`.
        let name: String = match &self.name {
            None => "None".to_string(),
            Some(n) => n.bind(py).repr()?.extract()?,
        };
        let secs: i64 = match &self.offset {
            None => 0,
            Some(o) => delta_total_seconds(py, o)?,
        };
        Ok(format!("tzoffset({}, {})", name, secs))
    }

    #[getter]
    fn _name(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.name)
    }

    #[getter]
    fn _offset(&self, py: Python<'_>) -> PyObject {
        clone_opt(py, &self.offset).unwrap_or_else(|| zero_delta(py))
    }

    fn __getstate__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = PyDict::new(py);
        d.set_item("_name", opt_clone(py, &self.name))?;
        d.set_item("_offset", clone_opt(py, &self.offset).unwrap_or_else(|| zero_delta(py)))?;
        Ok(d.into_any().unbind())
    }

    fn __setstate__(&mut self, _py: Python<'_>, state: Bound<'_, PyAny>) -> PyResult<()> {
        let n: Bound<'_, PyAny> = state.get_item("_name")?;
        let o: Bound<'_, PyAny> = state.get_item("_offset")?;
        self.name = if n.is_none() { None } else { Some(n.unbind()) };
        self.offset = if o.is_none() { None } else { Some(o.unbind()) };
        Ok(())
    }
}

impl TzOffset {
    /// Real initialization: name passthrough + `total_seconds()` coercion.
    fn init(
        &mut self,
        py: Python<'_>,
        name: Option<Bound<'_, PyAny>>,
        offset: Option<Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        self.name = name.map(|n| n.unbind());
        if let Some(offset) = offset {
            // `offset.total_seconds()` with (TypeError, AttributeError) fallthrough.
            let secs: Bound<'_, PyAny> = match offset.getattr("total_seconds") {
                Ok(f) => match f.call0() {
                    Ok(v) => v,
                    Err(e) => {
                        if e.is_instance_of::<pyo3::exceptions::PyTypeError>(py)
                            || e.is_instance_of::<pyo3::exceptions::PyAttributeError>(py)
                        {
                            offset
                        } else {
                            return Err(e);
                        }
                    }
                },
                Err(e) => {
                    if e.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) {
                        // Re-fetch (getattr consumed nothing): use the original.
                        offset
                    } else {
                        return Err(e);
                    }
                }
            };
            let td = timedelta_cls(py)?;
            let kw = PyDict::new(py);
            kw.set_item("seconds", secs)?;
            self.offset = Some(td.call((), Some(&kw))?.unbind());
        }
        Ok(())
    }
}

/// `total_seconds()` of a stored delta as i64 (for `repr`).
fn delta_total_seconds(py: Python<'_>, o: &PyObject) -> PyResult<i64> {
    let v: f64 = o.bind(py).call_method0("total_seconds")?.extract()?;
    Ok(v as i64)
}


/// `tzlocal`: C-library local zone (offsets/names captured at
/// construction; per-call DST probing through `time.localtime`, exactly
/// like the original).
#[pyclass(weakref, name = "tzlocal", extends=pyo3::types::PyTzInfo, subclass, dict, module = "dateutil._dateutil")]
pub struct TzLocal {
    std_offset: Option<PyObject>,
    dst_offset: Option<PyObject>,
    dst_saved: Option<PyObject>,
    hasdst: bool,
    tznames: Option<PyObject>,
}

/// Read `time.timezone`/`altzone`/`daylight`/`tzname` (Python `time`).
fn time_attrs(py: Python<'_>) -> PyResult<(PyObject, PyObject, bool, PyObject)> {
    let time = time_mod(py)?;
    let timezone: Bound<'_, PyAny> = time.getattr("timezone")?;
    let daylight: bool = time.getattr("daylight")?.extract::<i64>()? != 0;
    let altzone: Bound<'_, PyAny> = time.getattr("altzone")?;
    let tzname = time.getattr("tzname")?;
    Ok((timezone.unbind(), altzone.unbind(), daylight, tzname.unbind()))
}

#[pymethods]
impl TzLocal {
    #[new]
    #[pyo3(signature = ())]
    fn new(py: Python<'_>) -> PyResult<Self> {
        let (timezone, altzone, daylight, tzname) = time_attrs(py)?;
        let td = timedelta_cls(py)?;
        let kw = PyDict::new(py);
        let tz_o = timezone.bind(py);
        let tz_i: i64 = tz_o.extract()?;
        kw.set_item("seconds", -tz_i)?;
        let std_offset = td.call((), Some(&kw))?.unbind();
        let dst_offset = if daylight {
            let kw2 = PyDict::new(py);
            let az_i: i64 = altzone.bind(py).extract()?;
            kw2.set_item("seconds", -az_i)?;
            td.call((), Some(&kw2))?.unbind()
        } else {
            std_offset.clone_ref(py)
        };
        // `self._dst_saved = self._dst_offset - self._std_offset`.
        let dst_saved = dst_offset.bind(py).sub(std_offset.bind(py)).map(|v| v.unbind())?;
        let hasdst = {
            let s: f64 = dst_saved.bind(py).call_method0("total_seconds")?.extract()?;
            s != 0.0
        };
        Ok(TzLocal {
            std_offset: Some(std_offset),
            dst_offset: Some(dst_offset),
            dst_saved: Some(dst_saved),
            hasdst,
            tznames: Some(tzname),
        })
    }

    #[pyo3(signature = ())]
    fn __init__(&mut self, py: Python<'_>) -> PyResult<()> {
        // `type.__call__` runs `__init__` after `__new__`; re-capture so a
        // direct `tzlocal()` re-reads the C library like the original.
        *self = Self::new(py)?;
        Ok(())
    }

    fn utcoffset(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if dt.is_none() && self.hasdst {
            return Ok(py.None());
        }
        if tzlocal_isdst(self, py, dt)? {
            Ok(opt_clone(py, &self.dst_offset))
        } else {
            Ok(opt_clone(py, &self.std_offset))
        }
    }

    fn dst(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if dt.is_none() && self.hasdst {
            return Ok(py.None());
        }
        if tzlocal_isdst(self, py, dt)? {
            // `self._dst_offset - self._std_offset` (== _dst_saved).
            Ok(opt_clone(py, &self.dst_saved))
        } else {
            Ok(zero_delta(py))
        }
    }

    fn tzname(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let idx = tzlocal_isdst(self, py, dt)?;
        let names = opt_clone(py, &self.tznames);
        let item = if idx { names.bind(py).get_item(1)? } else { names.bind(py).get_item(0)? };
        Ok(item.unbind())
    }

    fn fromutc(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        use pyo3::conversion::IntoPyObject;
        let slf_obj = slf.into_pyobject(py).unwrap().into_any();
        check_fromutc(py, &slf_obj, &dt)?;
        tzinfo_fromutc(py, &slf_obj, &dt)
    }

    fn is_ambiguous(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<bool> {
        tzlocal_is_ambiguous(self, py, dt)
    }

    fn __eq__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let local_t = py.get_type::<TzLocal>();
        let utc_t = py.get_type::<TzUtc>();
        let off_t = py.get_type::<TzOffset>();
        if other.is_instance(&local_t)? {
            let eq = py_eq(py, opt_clone(py, &self.std_offset).bind(py), &other.getattr("_std_offset")?)?
                && py_eq(py, opt_clone(py, &self.dst_offset).bind(py), &other.getattr("_dst_offset")?)?;
            return Ok(bool_obj(py, eq));
        } else if other.is_instance(&utc_t)? {
            let names0: String = opt_clone(py, &self.tznames).bind(py).get_item(0)?.extract()?;
            let ok = !self.hasdst
                && (names0 == "UTC" || names0 == "GMT")
                && py_eq(py, opt_clone(py, &self.std_offset).bind(py), zero_delta(py).bind(py))?;
            return Ok(bool_obj(py, ok));
        } else if other.is_instance(&off_t)? {
            let names0: String = opt_clone(py, &self.tznames).bind(py).get_item(0)?.extract()?;
            let oname: String = other.getattr("_name")?.extract()?;
            let ok = !self.hasdst
                && names0 == oname
                && py_eq(py, opt_clone(py, &self.std_offset).bind(py), &other.getattr("_offset")?)?;
            return Ok(bool_obj(py, ok));
        }
        Ok(not_implemented(py))
    }

    fn __ne__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        ne_negate(py, self.__eq__(py, other)?)
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let _ = py;
        Err(pyo3::exceptions::PyTypeError::new_err("unhashable type: 'tzlocal'"))
    }

    fn __repr__(&self) -> String {
        "tzlocal()".to_string()
    }

    #[getter]
    fn _std_offset(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.std_offset)
    }

    #[getter]
    fn _dst_offset(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.dst_offset)
    }

    #[getter]
    fn _dst_saved(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.dst_saved)
    }

    #[getter]
    fn _hasdst(&self) -> bool {
        self.hasdst
    }

    #[getter]
    fn _tznames(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.tznames)
    }

    fn __getstate__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = PyDict::new(py);
        d.set_item("_std_offset", opt_clone(py, &self.std_offset))?;
        d.set_item("_dst_offset", opt_clone(py, &self.dst_offset))?;
        d.set_item("_dst_saved", opt_clone(py, &self.dst_saved))?;
        d.set_item("_hasdst", self.hasdst)?;
        d.set_item("_tznames", opt_clone(py, &self.tznames))?;
        Ok(d.into_any().unbind())
    }

    fn __setstate__(&mut self, py: Python<'_>, state: Bound<'_, PyAny>) -> PyResult<()> {
        self.std_offset = Some(state.get_item("_std_offset")?.unbind());
        self.dst_offset = Some(state.get_item("_dst_offset")?.unbind());
        self.dst_saved = Some(state.get_item("_dst_saved")?.unbind());
        self.hasdst = state.get_item("_hasdst")?.extract()?;
        self.tznames = Some(state.get_item("_tznames")?.unbind());
        let _ = py;
        Ok(())
    }
}

/// `_naive_is_dst(dt)`: `time.localtime(timestamp + time.timezone).tm_isdst`.
fn tzlocal_naive_is_dst(_slf: &TzLocal, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<i64> {
    let ts = datetime_to_timestamp(py, &dt)?;
    let time = time_mod(py)?;
    let timezone: f64 = time.getattr("timezone")?.extract()?;
    let lt = time.call_method1("localtime", (ts + timezone,))?;
    lt.getattr("tm_isdst")?.extract()
}

fn tzlocal_is_ambiguous(slf: &TzLocal, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<bool> {
    let naive_dst: i64 = tzlocal_naive_is_dst(slf, py, dt.clone())?;
    if naive_dst != 0 {
        return Ok(false);
    }
    // `dt - self._dst_saved` via Python.
    let dt2 = dt.sub(opt_clone(py, &slf.dst_saved))?;
    let other: i64 = tzlocal_naive_is_dst(slf, py, dt2)?;
    Ok(naive_dst != other)
}

fn tzlocal_isdst(slf: &TzLocal, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<bool> {
    if !slf.hasdst {
        return Ok(false);
    }
    let dstval: i64 = tzlocal_naive_is_dst(slf, py, dt.clone())?;
    let dstval_b = dstval != 0;
    if tzlocal_is_ambiguous(slf, py, dt.clone())? {
        let fold = dt.getattr("fold").ok();
        match fold {
            None => return Ok(true),
            Some(f) => {
                if f.is_none() {
                    return Ok(true);
                }
                let fd: i64 = f.extract()?;
                return Ok(fd == 0);
            }
        }
    }
    Ok(dstval_b)
}

/// `_datetime_to_timestamp(dt)`: naive wall as epoch float seconds.
fn datetime_to_timestamp(py: Python<'_>, dt: &Bound<'_, PyAny>) -> PyResult<f64> {
    let kw = PyDict::new(py);
    kw.set_item("tzinfo", py.None())?;
    let naive = dt.call_method("replace", (), Some(&kw))?;
    let epoch = py
        .import("datetime")?
        .getattr("datetime")?
        .call1((1970, 1, 1, 0, 0))?;
    let delta = naive.sub(epoch)?;
    delta.call_method0("total_seconds")?.extract()
}

/// Generic `_tzinfo.fromutc` (for `tzlocal`/`_tzicalvtz`): validated `dt`
/// plus the wall-walk through the zone's own `utcoffset`/`dst` methods.
fn tzinfo_fromutc(py: Python<'_>, slf_obj: &Bound<'_, PyAny>, dt: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    let dtoff: Bound<'_, PyAny> = slf_obj.call_method1("utcoffset", (dt,))?;
    if dtoff.is_none() {
        return value_error("fromutc() requires a non-None utcoffset() result".to_string());
    }
    let dtdst: Bound<'_, PyAny> = slf_obj.call_method1("dst", (dt,))?;
    if dtdst.is_none() {
        return value_error("fromutc() requires a non-None dst() result".to_string());
    }
    let delta = dtoff.sub(&dtdst)?;
    let mut dt2 = dt.add(&delta)?.unbind();
    // `enfold(dt, fold=1).dst()`.
    let enfolded = enfold_dt(py, dt2.bind(py), 1)?;
    let dtdst2: Bound<'_, PyAny> = slf_obj.call_method1("dst", (enfolded.bind(py),))?;
    if dtdst2.is_none() {
        return value_error("fromutc(): dt.dst gave inconsistent results; cannot convert".to_string());
    }
    dt2 = dt2.bind(py).add(&dtdst2)?.unbind();
    // `_fold_status(dt, dt_wall)`.
    let ambiguous: bool = slf_obj.call_method1("is_ambiguous", (dt2.bind(py),))?.extract()?;
    let fold = if ambiguous {
        let delta_wall = dt2.bind(py).sub(dt)?;
        let uo: Bound<'_, PyAny> = dt.call_method0("utcoffset")?;
        let ds: Bound<'_, PyAny> = dt.call_method0("dst")?;
        let diff = uo.sub(&ds)?;
        let eq: bool = delta_wall.rich_compare(&diff, CompareOp::Eq)?.extract()?;
        if eq { 1 } else { 0 }
    } else {
        0
    };
    enfold_dt(py, dt2.bind(py), fold)
}


/// `enfold(dt, fold)`: `dt.replace(fold=fold)`.
fn enfold_dt(py: Python<'_>, dt: &Bound<'_, PyAny>, fold: i64) -> PyResult<PyObject> {
    let kw = PyDict::new(py);
    kw.set_item("fold", fold)?;
    Ok(dt.call_method("replace", (), Some(&kw))?.unbind())
}

/// `tzfile`: TZif-backed zone (parsed by `dateutil-core`, exposed with
/// shared `_ttinfo` objects like the original).
#[pyclass(weakref, name = "tzfile", extends=pyo3::types::PyTzInfo, subclass, dict, module = "dateutil._dateutil")]
pub struct TzFile {
    data: tzfile::TzFileData,
    filename: PyObject,
    ttis: Vec<Py<TtInfo>>,
    trans_idx: Vec<usize>,
    zero: PyObject,
}

/// Build `_ttinfo` objects from parsed records.
fn build_ttis(py: Python<'_>, data: &tzfile::TzFileData) -> PyResult<Vec<Py<TtInfo>>> {
    let mut out = Vec::with_capacity(data.ttinfo_list.len());
    for t in &data.ttinfo_list {
        let delta = make_delta_secs(py, t.offset as f64)?;
        let dstoffset = make_delta_secs(py, t.dstoffset as f64)?;
        let o = Py::new(
            py,
            TtInfo {
                offset: Some(t.offset.into_pyobject(py)?.into_any().unbind()),
                delta: Some(delta),
                isdst: Some((t.isdst).into_pyobject(py)?.into_any().unbind()),
                abbr: Some(t.abbr.clone().into_pyobject(py)?.into_any().unbind()),
                isstd: Some(bool_obj(py, t.isstd)),
                isgmt: Some(bool_obj(py, t.isgmt)),
                dstoffset: Some(dstoffset),
            },
        )?;
        out.push(o);
    }
    Ok(out)
}

/// Int tuple helper for tzfile getters.
fn ints_tuple(py: Python<'_>, v: &[i64]) -> PyResult<PyObject> {
    let t = PyTuple::new(
        py,
        v.iter().map(|x| x.into_pyobject(py).unwrap().into_any()),
    )?;
    Ok(t.into_any().unbind())
}

/// Parse TZif bytes, mapping failures to the original's errors.
fn parse_bytes(py: Python<'_>, data: &[u8]) -> PyResult<tzfile::TzFileData> {
    match tzfile::parse_tzfile(data) {
        Ok(d) => Ok(d),
        Err(tzfile::TzFileError::MagicNotFound) => value_error("magic not found".to_string()),
        Err(tzfile::TzFileError::DecodeError) => {
            // Reproduce the genuine `UnicodeDecodeError` via bytes.decode.
            let b = pyo3::types::PyBytes::new(py, &data[..data.len().min(4)]);
            match b.call_method0("decode") {
                Ok(_) => value_error("magic not found".to_string()),
                Err(e) => Err(e),
            }
        }
        Err(tzfile::TzFileError::Truncated) => value_error("unpack requires a buffer".to_string()),
    }
}

#[pymethods]
impl TzFile {
    #[new]
    #[pyo3(signature = (fileobj=None, filename=None))]
    fn new(
        py: Python<'_>,
        fileobj: Option<Bound<'_, PyAny>>,
        filename: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let zero = zero_delta(py);
        let (raw_name, data_opt): (PyObject, Option<tzfile::TzFileData>) = match fileobj {
            None => match filename {
                Some(n) if !n.is_none() => (n.unbind(), None),
                _ => (py.None(), None),
            },
            Some(f) => {
                if f.is_none() {
                    match filename {
                        Some(n) if !n.is_none() => (n.unbind(), None),
                        _ => (py.None(), None),
                    }
                } else if let Ok(s) = f.extract::<String>() {
                    // Path: read + close (like `open(...)` in a with-block).
                    let bytes = std::fs::read(&s)
                        .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
                    let data = parse_bytes(py, &bytes)?;
                    (s.into_pyobject(py)?.into_any().unbind(), Some(data))
                } else {
                    let name: PyObject = match filename {
                        Some(n) => {
                            if n.is_none() {
                                py.None()
                            } else {
                                n.unbind()
                            }
                        }
                        None => match f.getattr("name") {
                            Ok(n) => n.unbind(),
                            Err(_) => f.repr()?.into_any().unbind(),
                        },
                    };
                    // Consume the whole stream (the `remaining == 0` test).
                    let bytes: Vec<u8> = f.call_method0("read")?.extract()?;
                    let data = parse_bytes(py, &bytes)?;
                    (name, Some(data))
                }
            }
        };
        let mut o = TzFile {
            data: tzfile::TzFileData {
                trans_list_utc: Vec::new(),
                trans_list: Vec::new(),
                trans_idx: Vec::new(),
                ttinfo_list: Vec::new(),
                ttinfo_std: None,
                ttinfo_dst: None,
                ttinfo_before: None,
                ttinfo_first: None,
            },
            filename: raw_name,
            ttis: Vec::new(),
            trans_idx: Vec::new(),
            zero,
        };

        if let Some(data) = data_opt {
            o.set_data(py, data)?;
        }
        Ok(o)
    }

    #[pyo3(signature = (fileobj=None, filename=None))]
    fn __init__(
        &mut self,
        _py: Python<'_>,
        fileobj: Option<Bound<'_, PyAny>>,
        filename: Option<Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let _ = (fileobj, filename);
        // Real work happens in `__new__` (unpickle path constructs empty).
        Ok(())
    }

    fn utcoffset(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if dt.is_none() {
            return Ok(py.None());
        }
        if self.data.ttinfo_std.is_none() {
            return Ok(self.zero.clone_ref(py));
        }
        match self.find_ttinfo(py, &dt)? {
            None => Ok(self.zero.clone_ref(py)),
            Some(o) => {
                let delta: PyObject = o.bind(py).getattr("delta")?.unbind();
                Ok(delta)
            }
        }
    }

    fn dst(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if dt.is_none() {
            return Ok(py.None());
        }
        if self.data.ttinfo_dst.is_none() {
            return Ok(self.zero.clone_ref(py));
        }
        match self.find_ttinfo(py, &dt)? {
            None => Ok(self.zero.clone_ref(py)),
            Some(o) => {
                let ob = o.bind(py);
                let isdst: i64 = ob.getattr("isdst")?.extract()?;
                if isdst == 0 {
                    Ok(self.zero.clone_ref(py))
                } else {
                    Ok(ob.getattr("dstoffset")?.unbind())
                }
            }
        }
    }

    fn tzname(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if self.data.ttinfo_std.is_none() || dt.is_none() {
            return Ok(py.None());
        }
        match self.find_ttinfo(py, &dt)? {
            None => Ok(py.None()),
            Some(o) => Ok(o.bind(py).getattr("abbr")?.unbind()),
        }
    }

    #[pyo3(signature = (dt, idx=None))]
    fn is_ambiguous(
        &self,
        py: Python<'_>,
        dt: Bound<'_, PyAny>,
        idx: Option<Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        let (wall, _fold) = wall_of(&dt)?;
        let ts = coretz::timestamp_of(wall);
        let idi: Option<i64> = match idx {
            None => None,
            Some(v) => {
                if v.is_none() {
                    None
                } else {
                    Some(v.extract()?)
                }
            }
        };
        Ok(tzfile::is_ambiguous_at(&self.data, ts, idi))
    }

    fn fromutc(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        use pyo3::conversion::IntoPyObject;
        // Gather owned resolution inputs under borrow; validation moves `slf`.
        let (wall, _fold) = wall_of(&dt)?;
        let ts = coretz::timestamp_of(wall);
        let data = slf.data.clone();
        let slf_obj = slf.into_pyobject(py).unwrap().into_any();
        check_fromutc(py, &slf_obj, &dt)?;
        // First treat UTC as wall time and get the transition we're in.
        let idx = tzfile::find_last_transition(&data, ts, true);
        let tti = match tzfile::get_ttinfo(&data, idx) {
            None => {
                return Err(pyo3::exceptions::PyAttributeError::new_err(
                    "'NoneType' object has no attribute 'offset'",
                ));
            }
            Some(t) => t,
        };
        let off = data.ttinfo_list[tti].offset;
        let dt_out = dt.add(make_delta_secs(py, off as f64)?)?;
        let (wall_out, _) = wall_of(&dt_out)?;
        let fold = tzfile::is_ambiguous_at(&data, coretz::timestamp_of(wall_out), idx);
        enfold_dt(py, &dt_out, fold as i64)
    }
    fn __eq__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let file_t = py.get_type::<TzFile>();
        if !other.is_instance(&file_t)? {
            return Ok(not_implemented(py));
        }
        Ok(bool_obj(py, self.eq_impl(py, &other)?))
    }

    fn __ne__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        ne_negate(py, self.__eq__(py, other)?)
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let _ = py;
        Err(pyo3::exceptions::PyTypeError::new_err("unhashable type: 'tzfile'"))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let r: String = self.filename.bind(py).repr()?.extract()?;
        Ok(format!("tzfile({})", r))
    }

    fn __reduce__(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.reduce_impl(py)
    }

    fn __reduce_ex__(&self, py: Python<'_>, _proto: Bound<'_, PyAny>) -> PyResult<PyObject> {
        self.reduce_impl(py)
    }

    fn __setstate__(&mut self, py: Python<'_>, state: Bound<'_, PyDict>) -> PyResult<()> {
        // Rebuild core data + shared `_ttinfo` objects from the state dict
        // (mirrors `__dict__` restore; shared references preserved by
        // pickle's memo, mapped back by identity then value).
        let get = |n: &str| -> PyResult<Bound<'_, PyAny>> {
            state.get_item(n)?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(n.to_string()))
        };
        let ints = |o: Bound<'_, PyAny>| -> PyResult<Vec<i64>> {
            o.try_iter()?.map(|x| x.and_then(|b| b.extract::<i64>())).collect::<PyResult<_>>()
        };
        let trans_list = ints(get("_trans_list")?)?;
        let trans_list_utc = ints(get("_trans_list_utc")?)?;
        // _ttinfo objects: keep the shared state objects, rebuild core records.
        let mut ttinfo_list = Vec::new();
        let mut ttis: Vec<Py<TtInfo>> = Vec::new();
        for o in get("_ttinfo_list")?.try_iter()? {
            let o = o?;
            let rec = |n: &str| -> PyResult<Bound<'_, PyAny>> { o.getattr(n) };
            let offset: i64 = rec("offset")?.extract()?;
            let isdst: i64 = rec("isdst")?.extract()?;
            let abbr: String = rec("abbr")?.extract()?;
            let isstd: bool = rec("isstd")?.extract()?;
            let isgmt: bool = rec("isgmt")?.extract()?;
            let dd = rec("dstoffset")?;
            let (doff, _) = split_delta(&dd)?;
            ttinfo_list.push(tzfile::TtInfo { offset, isdst, abbr, isstd, isgmt, dstoffset: doff });
            ttis.push(o.extract::<Py<TtInfo>>()?);
        }
        let py_ttis: Vec<PyObject> = ttis.iter().map(|o| o.clone_ref(py).into_any()).collect();
        // trans_idx: map each object to its ttinfo index by identity.
        let mut trans_idx = Vec::new();
        for o in get("_trans_idx")?.try_iter()? {
            let o = o?;
            let mut found = None;
            for (n, t) in py_ttis.iter().enumerate() {
                if o.is(t.bind(py)) {
                    found = Some(n);
                    break;
                }
            }
            let idx = match found {
                Some(n) => n,
                None => {
                    let mut pick = 0;
                    for (n, t) in py_ttis.iter().enumerate() {
                        let tb = t.bind(py);
                        if py_eq(py, &o, &tb)? {
                            pick = n;
                            break;
                        }
                    }
                    pick
                }
            };
            trans_idx.push(idx);
        }
        // Named slots by identity (None stays None).
        let mut slot = |n: &str| -> PyResult<Option<usize>> {
            let v = get(n)?;
            if v.is_none() {
                return Ok(None);
            }
            for (i, t) in py_ttis.iter().enumerate() {
                if v.is(t.bind(py)) {
                    return Ok(Some(i));
                }
            }
            Ok(Some(0))
        };
        self.data = tzfile::TzFileData {
            trans_list_utc,
            trans_list,
            trans_idx: trans_idx.clone(),
            ttinfo_list,
            ttinfo_std: slot("_ttinfo_std")?,
            ttinfo_dst: slot("_ttinfo_dst")?,
            ttinfo_before: slot("_ttinfo_before")?,
            ttinfo_first: slot("_ttinfo_first")?,
        };
        self.ttis = ttis;
        self.trans_idx = trans_idx;
        self.filename = get("_filename")?.unbind();
        self.zero = zero_delta(py);
        Ok(())
    }

    #[getter]
    fn _trans_list(&self, py: Python<'_>) -> PyResult<PyObject> {
        ints_tuple(py, &self.data.trans_list)
    }

    #[getter]
    fn _trans_list_utc(&self, py: Python<'_>) -> PyResult<PyObject> {
        ints_tuple(py, &self.data.trans_list_utc)
    }

    #[getter]
    fn _trans_idx(&self, py: Python<'_>) -> PyObject {
        let objs: Vec<PyObject> =
            self.trans_idx.iter().map(|n| self.ttis[*n].clone_ref(py).into_any()).collect();
        PyTuple::new(py, objs.iter().map(|o| o.bind(py)))
            .map(|t| t.into_any().unbind())
            .unwrap_or_else(|_| py.None())
    }

    #[getter]
    fn _ttinfo_list(&self, py: Python<'_>) -> PyObject {
        let objs: Vec<PyObject> =
            self.ttis.iter().map(|o| o.clone_ref(py).into_any()).collect();
        PyTuple::new(py, objs.iter().map(|o| o.bind(py)))
            .map(|t| t.into_any().unbind())
            .unwrap_or_else(|_| py.None())
    }

    #[getter]
    fn _ttinfo_std(&self, py: Python<'_>) -> PyObject {
        self.data.ttinfo_std.map(|n| self.ttis[n].clone_ref(py).into_any()).unwrap_or_else(|| py.None())
    }

    #[getter]
    fn _ttinfo_dst(&self, py: Python<'_>) -> PyObject {
        self.data.ttinfo_dst.map(|n| self.ttis[n].clone_ref(py).into_any()).unwrap_or_else(|| py.None())
    }

    #[getter]
    fn _ttinfo_before(&self, py: Python<'_>) -> PyObject {
        self.data.ttinfo_before.map(|n| self.ttis[n].clone_ref(py).into_any()).unwrap_or_else(|| py.None())
    }

    #[getter]
    fn _ttinfo_first(&self, py: Python<'_>) -> PyObject {
        self.data.ttinfo_first.map(|n| self.ttis[n].clone_ref(py).into_any()).unwrap_or_else(|| py.None())
    }

    #[getter]
    fn _filename(&self, py: Python<'_>) -> PyObject {
        self.filename.clone_ref(py)
    }
}



impl TzFile {
    fn set_data(&mut self, py: Python<'_>, data: tzfile::TzFileData) -> PyResult<()> {
        self.ttis = build_ttis(py, &data)?;
        self.trans_idx = data.trans_idx.clone();
        self.data = data;
        Ok(())
    }

    fn tti_obj(&self, py: Python<'_>, idx: usize) -> PyObject {
        self.ttis[idx].clone_ref(py).into_any()
    }

    /// `_find_ttinfo(dt)`: resolved `_ttinfo` object (or `None`).
    fn find_ttinfo(&self, py: Python<'_>, dt: &Bound<'_, PyAny>) -> PyResult<Option<PyObject>> {
        let (wall, fold) = wall_of(dt)?;
        let ts = coretz::timestamp_of(wall);
        let idx = tzfile::resolve_ambiguous_time(&self.data, ts, fold);
        Ok(tzfile::get_ttinfo(&self.data, idx).map(|t| self.tti_obj(py, t)))
    }
}

impl TzFile {
    /// `__eq__`: trans lists plus elementwise `_ttinfo` equality.
    fn eq_impl(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        // Pull the other's core data through its Python attrs (shared
        // `_ttinfo` objects compare by value, like the original).
        let o_trans: Vec<i64> = other
            .getattr("_trans_list")?
            .try_iter()?
            .map(|o| o.and_then(|b| b.extract::<i64>()))
            .collect::<PyResult<_>>()?;
        if o_trans != self.data.trans_list {
            return Ok(false);
        }
        let o_utc: Vec<i64> = other
            .getattr("_trans_list_utc")?
            .try_iter()?
            .map(|o| o.and_then(|b| b.extract::<i64>()))
            .collect::<PyResult<_>>()?;
        if o_utc != self.data.trans_list_utc {
            return Ok(false);
        }
        // ttinfo lists elementwise (7 fields each).
        let o_ttis: Vec<Bound<'_, PyAny>> = other
            .getattr("_ttinfo_list")?
            .try_iter()?
            .collect::<PyResult<_>>()?;
        if o_ttis.len() != self.ttis.len() {
            return Ok(false);
        }
        for (a, b) in self.ttis.iter().zip(o_ttis.iter()) {
            let ab = a.bind(py);
            for attr in ["offset", "delta", "isdst", "abbr", "isstd", "isgmt", "dstoffset"] {
                let x = ab.getattr(attr)?;
                let y = b.getattr(attr)?;
                if !py_eq(py, &x, &y)? {
                    return Ok(false);
                }
            }
        }
        // trans_idx elementwise (shared objects: identity implies equality,
        // but compare by value for cross-instance cases).
        let o_idx: Vec<Bound<'_, PyAny>> = other
            .getattr("_trans_idx")?
            .try_iter()?
            .collect::<PyResult<_>>()?;
        if o_idx.len() != self.trans_idx.len() {
            return Ok(false);
        }
        for (n, b) in o_idx.iter().enumerate() {
            let a = self.ttis[self.trans_idx[n]].bind(py);
            for attr in ["offset", "delta", "isdst", "abbr", "isstd", "isgmt", "dstoffset"] {
                let x = a.getattr(attr)?;
                let y = b.getattr(attr)?;
                if !py_eq(py, &x, &y)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// `(cls, (None, filename), state-dict)` reduce, like the original.
    fn reduce_impl(&self, py: Python<'_>) -> PyResult<PyObject> {
        let cls = py.get_type::<TzFile>();
        let filename = self.filename.clone_ref(py);
        let args = PyTuple::new(py, [py.None(), filename])?;
        let state = self.state_dict(py)?;
        // (cls, args, state)
        let out = PyTuple::new(py, [cls.into_any(), args.into_any(), state.into_bound(py)])?;
        Ok(out.into_any().unbind())
    }

    fn state_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = PyDict::new(py);
        let ints = |v: &[i64]| -> PyResult<PyObject> {
            let t = PyTuple::new(
                py,
                v.iter().map(|x| x.into_pyobject(py).unwrap().into_any()),
            )?;
            Ok(t.into_any().unbind())
        };
        d.set_item("_trans_list", ints(&self.data.trans_list)?)?;
        d.set_item("_trans_list_utc", ints(&self.data.trans_list_utc)?)?;
        let idx_list: Vec<PyObject> =
            self.trans_idx.iter().map(|n| self.ttis[*n].clone_ref(py).into_any()).collect();
        d.set_item("_trans_idx", PyTuple::new(py, idx_list.iter().map(|o| o.bind(py)))?.into_any().unbind())?;
        let tti_list: Vec<PyObject> = self.ttis.iter().map(|o| o.clone_ref(py).into_any()).collect();
        d.set_item("_ttinfo_list", PyTuple::new(py, tti_list.iter().map(|o| o.bind(py)))?.into_any().unbind())?;
        let opt_t = |o: &Option<usize>| -> PyObject {
            o.map(|n| self.ttis[n].clone_ref(py).into_any()).unwrap_or_else(|| py.None())
        };
        d.set_item("_ttinfo_std", opt_t(&self.data.ttinfo_std))?;
        d.set_item("_ttinfo_dst", opt_t(&self.data.ttinfo_dst))?;
        d.set_item("_ttinfo_before", opt_t(&self.data.ttinfo_before))?;
        d.set_item("_ttinfo_first", opt_t(&self.data.ttinfo_first))?;
        d.set_item("_filename", self.filename.clone_ref(py))?;
        Ok(d.into_any().unbind())
    }
}

/// `tzrange`: annual-transition zone over explicit offsets/deltas.
#[pyclass(weakref, name = "tzrange", extends=pyo3::types::PyTzInfo, subclass, dict, module = "dateutil._dateutil")]
pub struct TzRange {
    std_abbr: Option<PyObject>,
    dst_abbr: Option<PyObject>,
    std_offset: Option<PyObject>,
    dst_offset: Option<PyObject>,
    start_delta: Option<PyObject>,
    end_delta: Option<PyObject>,
    hasdst: bool,
}

/// Coerce an offset argument via `total_seconds()` with
/// `(TypeError, AttributeError)` fallthrough (mirrors `tzrange.__init__`).
fn coerce_offset(py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    // Plain numbers are seconds (mirrors `timedelta(seconds=int)`).
    if let Ok(secs) = v.extract::<i64>() {
        return make_delta_secs(py, secs as f64);
    }
    if let Ok(secs) = v.extract::<f64>() {
        return make_delta_secs(py, secs);
    }
    let secs: Bound<'_, PyAny> = match v.getattr("total_seconds") {
        Ok(f) => match f.call0() {
            Ok(s) => s,
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyTypeError>(py)
                    || e.is_instance_of::<pyo3::exceptions::PyAttributeError>(py)
                {
                    v.to_owned()
                } else {
                    return Err(e);
                }
            }
        },
        Err(e) => {
            if e.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) {
                v.to_owned()
            } else {
                return Err(e);
            }
        }
    };
    let td = timedelta_cls(py)?;
    let kw = PyDict::new(py);
    kw.set_item("seconds", secs)?;
    Ok(td.call((), Some(&kw))?.unbind())
}

/// Shared `tzrange` initialization (also used by `tzstr`).
/// `*_given` tracks parameter presence (`is not None`), like the original.
#[allow(clippy::too_many_arguments)]
fn tzrange_init(
    py: Python<'_>,
    std_abbr: Option<PyObject>,
    std_offset: Option<PyObject>,
    dst_abbr: Option<PyObject>,
    dst_offset: Option<PyObject>,
    start: Option<PyObject>,
    end: Option<PyObject>,
) -> PyResult<TzRange> {
    let std_given = match &std_offset {
        None => false,
        Some(v) => !v.bind(py).is_none(),
    };
    let dst_abbr_truthy = match &dst_abbr {
        None => false,
        Some(o) => {
            let ob = o.bind(py);
            !ob.is_none() && ob.is_truthy()?
        }
    };
    let std_off = match std_offset {
        None => zero_delta(py),
        Some(v) => {
            let vb = v.bind(py);
            if vb.is_none() {
                zero_delta(py)
            } else {
                coerce_offset(py, &vb.to_owned())?
            }
        }
    };
    // `if dstoffset is not None` — explicit `None` behaves as absent.
    let dst_given = match &dst_offset {
        Some(v) => !v.bind(py).is_none(),
        None => false,
    };
    let dst_off = if dst_given {
        let v = dst_offset.unwrap();
        coerce_offset(py, &v.bind(py).to_owned())?
    } else if dst_abbr_truthy && std_given {
        // `self._std_offset + timedelta(hours=+1)` via Python.
        let one_h = make_delta_secs(py, 3600.0)?;
        std_off.bind(py).add(one_h.bind(py))?.unbind()
    } else {
        zero_delta(py)
    };
    // Stored verbatim (`False` from `tzstr` included; the caller
    // overwrites deltas afterwards). Explicit `None` stays `None`.
    let conv = |o: Option<PyObject>| -> Option<PyObject> {
        o.and_then(|v| {
            if v.bind(py).is_none() {
                None
            } else {
                Some(v)
            }
        })
    };
    // `if dstabbr and start/end is None`: default April/October rules.
    let default_delta = |py: Python<'_>, hours: i64, month: i64, day: i64, wd: i64| -> PyResult<PyObject> {
        let rd = dateutil_relativedelta_mod(py)?;
        let kw = PyDict::new(py);
        kw.set_item("hours", hours)?;
        kw.set_item("month", month)?;
        kw.set_item("day", day)?;
        kw.set_item("weekday", rd.getattr("SU")?.call1((wd,))?)?;
        Ok(relativedelta_cls(py)?.call((), Some(&kw))?.unbind())
    };
    // Explicit `None` also counts as absent (`start is None`).
    let is_absent = |o: &Option<PyObject>| -> bool {
        match o {
            None => true,
            Some(v) => v.bind(py).is_none(),
        }
    };
    let start = match start {
        o if dst_abbr_truthy && is_absent(&o) => Some(default_delta(py, 2, 4, 1, 1)?),
        other => other,
    };
    let end = match end {
        o if dst_abbr_truthy && is_absent(&o) => Some(default_delta(py, 1, 10, 31, -1)?),
        other => other,
    };
    let start_o = conv(start);
    let end_o = conv(end);
    let hasdst = start_o
        .as_ref()
        .map(|o| o.bind(py).is_truthy().unwrap_or(false))
        .unwrap_or(false);
    Ok(TzRange {
        std_abbr,
        dst_abbr,
        std_offset: Some(std_off),
        dst_offset: Some(dst_off),
        start_delta: start_o,
        end_delta: end_o,
        hasdst,
    })
}

#[pymethods]
impl TzRange {
    #[new]
    #[pyo3(signature = (stdabbr, stdoffset=None, dstabbr=None, dstoffset=None, start=None, end=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        stdabbr: Bound<'_, PyAny>,
        stdoffset: Option<Bound<'_, PyAny>>,
        dstabbr: Option<Bound<'_, PyAny>>,
        dstoffset: Option<Bound<'_, PyAny>>,
        start: Option<Bound<'_, PyAny>>,
        end: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        tzrange_init(
            py,
            Some(stdabbr.unbind()),
            stdoffset.map(|v| v.unbind()),
            dstabbr.map(|v| v.unbind()),
            dstoffset.map(|v| v.unbind()),
            start.map(|v| v.unbind()),
            end.map(|v| v.unbind()),
        )
    }

    #[pyo3(signature = (stdabbr, stdoffset=None, dstabbr=None, dstoffset=None, start=None, end=None))]
    fn __init__(
        &mut self,
        py: Python<'_>,
        stdabbr: Bound<'_, PyAny>,
        stdoffset: Option<Bound<'_, PyAny>>,
        dstabbr: Option<Bound<'_, PyAny>>,
        dstoffset: Option<Bound<'_, PyAny>>,
        start: Option<Bound<'_, PyAny>>,
        end: Option<Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        // `type.__call__` runs `__init__` after `__new__`; re-run so a
        // direct call re-validates like the original.
        *self = tzrange_init(
            py,
            Some(stdabbr.unbind()),
            stdoffset.map(|v| v.unbind()),
            dstabbr.map(|v| v.unbind()),
            dstoffset.map(|v| v.unbind()),
            start.map(|v| v.unbind()),
            end.map(|v| v.unbind()),
        )?;
        Ok(())
    }

    /// `transitions(year)`: `(dston, dstoff)` wall pair, or `None`.
    fn transitions(&self, py: Python<'_>, year: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if !self.hasdst {
            return Ok(py.None());
        }
        let y: i64 = year.extract()?;
        let base = datetime_cls(py)?.call1((y, 1, 1))?;
        let start = base.add(opt_clone(py, &self.start_delta))?;
        let end = base.add(opt_clone(py, &self.end_delta))?;
        let tup = PyTuple::new(py, [start, end])?;
        Ok(tup.into_any().unbind())
    }

    /// Resolve this zone's transitions for a year into wall triples.

    fn utcoffset(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        // `_isdst` tri-state: fixed zones answer even for `None`.
        if dt.is_none() {
            if !self.hasdst {
                return Ok(opt_clone(py, &self.std_offset));
            }
            return Ok(py.None());
        }
        let (wall, fold) = wall_of(&dt)?;
        let z = self.zone(py, wall)?;
        match z.isdst(wall, fold) {
            None => Ok(py.None()),
            Some(true) => Ok(opt_clone(py, &self.dst_offset)),
            Some(false) => Ok(opt_clone(py, &self.std_offset)),
        }
    }

    fn dst(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if dt.is_none() {
            if !self.hasdst {
                return Ok(zero_delta(py));
            }
            return Ok(py.None());
        }
        let (wall, fold) = wall_of(&dt)?;
        let z = self.zone(py, wall)?;
        match z.isdst(wall, fold) {
            None => Ok(py.None()),
            Some(true) => {
                let (s, _) = split_delta(opt_clone(py, &self.dst_offset).bind(py))?;
                let (u, _) = split_delta(opt_clone(py, &self.std_offset).bind(py))?;
                make_delta_secs(py, (s - u) as f64)
            }
            Some(false) => Ok(zero_delta(py)),
        }
    }

    fn tzname(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if dt.is_none() {
            return Ok(opt_clone(py, &self.std_abbr));
        }
        let (wall, fold) = wall_of(&dt)?;
        let z = self.zone(py, wall)?;
        match z.isdst(wall, fold) {
            Some(true) => Ok(opt_clone(py, &self.dst_abbr)),
            _ => Ok(opt_clone(py, &self.std_abbr)),
        }
    }

    fn is_ambiguous(&self, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<bool> {
        let (wall, _fold) = wall_of(&dt)?;
        Ok(self.zone(py, wall)?.ambiguous_at(wall))
    }

    fn fromutc(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>) -> PyResult<PyObject> {
        use pyo3::conversion::IntoPyObject;
        let slf_obj = slf.into_pyobject(py).unwrap().into_any();
        // `tzrangebase.fromutc` validation (no isinstance-datetime shortcut
        // difference: same checks as `_validate_fromutc_inputs`).
        if !is_datetime(py, &dt)? {
            return type_error("fromutc() requires a datetime argument".to_string());
        }
        if !dt.getattr("tzinfo")?.is(&slf_obj) {
            return value_error("dt.tzinfo is not self".to_string());
        }
        let (wall, _fold) = wall_of(&dt)?;
        let (y, _, _) = civil::from_ordinal(wall.ord);
        // Transitions + offsets via Python-level values.
        let tr = slf_obj.call_method1("transitions", (y,))?;
        if tr.is_none() {
            // Fixed offset: `dt + utcoffset(dt)`.
            let off = slf_obj.call_method1("utcoffset", (&dt,))?;
            return Ok(dt.add(off)?.unbind());
        }
        let std_off: Bound<'_, PyAny> = slf_obj.getattr("_std_offset")?;
        let dst_off: Bound<'_, PyAny> = slf_obj.getattr("_dst_offset")?;
        let (std_s, _) = split_delta(&std_off)?;
        let (dst_s, _) = split_delta(&dst_off)?;
        let dston = tr.get_item(0)?;
        let dstoff = tr.get_item(1)?;
        let (dston_w, _) = wall_of(&dston)?;
        let (dstoff_w, _) = wall_of(&dstoff)?;
        let tr_walls = Some((dston_w, dstoff_w));
        // Ambiguity of the result wall for the fold bit.
        let z = coretz::RangeZone {
            std_offset: std_s,
            dst_offset: dst_s,
            hasdst: true,
            transitions: tr_walls,
            amb_transitions: tr_walls,
        };
        // Wall result ignoring ambiguity first, then the fold bit.
        let (wall2, _) = coretz::range_fromutc(wall, tr_walls, std_s, dst_s, std_s, false);
        let amb_here = z.ambiguous_at(wall2);
        let (wall2, fold) = coretz::range_fromutc(wall, tr_walls, std_s, dst_s, std_s, amb_here);
        let (y2, m2, d2) = civil::from_ordinal(wall2.ord);
        // Reattach tzinfo (fold set), mirroring `enfold(dt_wall, fold=...)`.
        make_datetime(
            py, y2, m2 as i64, d2 as i64,
            wall2.secs / 3600, (wall2.secs / 60) % 60, wall2.secs % 60, wall2.micros,
            Some(&slf_obj.unbind()), fold,
        )
    }

    fn __eq__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let range_t = py.get_type::<TzRange>();
        if !other.is_instance(&range_t)? {
            return Ok(not_implemented(py));
        }
        let eq = py_eq(py, opt_clone(py, &self.std_abbr).bind(py), &{
                let v = other.getattr("_std_abbr")?;
                v
            })?
            && py_eq(py, opt_clone(py, &self.dst_abbr).bind(py), &{
                let v = other.getattr("_dst_abbr")?;
                v
            })?
            && py_eq(py, opt_clone(py, &self.std_offset).bind(py), &{
                let v = other.getattr("_std_offset")?;
                v
            })?
            && py_eq(py, opt_clone(py, &self.dst_offset).bind(py), &{
                let v = other.getattr("_dst_offset")?;
                v
            })?
            && py_eq(py, opt_clone(py, &self.start_delta).bind(py), &{
                let v = other.getattr("_start_delta")?;
                v
            })?
            && py_eq(py, opt_clone(py, &self.end_delta).bind(py), &{
                let v = other.getattr("_end_delta")?;
                v
            })?;
        Ok(bool_obj(py, eq))
    }

    fn __ne__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        ne_negate(py, self.__eq__(py, other)?)
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let _ = py;
        Err(pyo3::exceptions::PyTypeError::new_err("unhashable type: 'tzrange'"))
    }

    fn __repr__(&self) -> String {
        "tzrange(...)".to_string()
    }

    fn __getstate__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = PyDict::new(py);
        d.set_item("_std_abbr", opt_clone(py, &self.std_abbr))?;
        d.set_item("_dst_abbr", opt_clone(py, &self.dst_abbr))?;
        d.set_item("_std_offset", opt_clone(py, &self.std_offset))?;
        d.set_item("_dst_offset", opt_clone(py, &self.dst_offset))?;
        d.set_item("_start_delta", opt_clone(py, &self.start_delta))?;
        d.set_item("_end_delta", opt_clone(py, &self.end_delta))?;
        Ok(d.into_any().unbind())
    }

    fn __reduce_ex__(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        _proto: Bound<'_, PyAny>,
    ) -> PyResult<PyObject> {
        // `tzrange()` requires `stdabbr`, so reconstruction bypasses the
        // class call (`cls.__new__(cls)` + `__setstate__`).
        use pyo3::conversion::IntoPyObject;
        let cls = slf_type(py, &slf);
        let copyreg = copyreg_mod(py)?;
        let newobj = copyreg.getattr("__newobj__")?;
        let args = PyTuple::new(py, [cls])?;
        let state = slf.__getstate__(py)?;
        let tup = PyTuple::new(py, [newobj.into_any(), args.into_any(), state.bind(py).to_owned()])?;
        Ok(tup.into_any().unbind())
    }

    fn __setstate__(&mut self, py: Python<'_>, state: Bound<'_, PyAny>) -> PyResult<()> {
        let get = |n: &str| -> PyResult<Option<PyObject>> {
            let v: Bound<'_, PyAny> = state.get_item(n)?;
            Ok(if v.is_none() { None } else { Some(v.unbind()) })
        };
        self.std_abbr = get("_std_abbr")?;
        self.dst_abbr = get("_dst_abbr")?;
        self.std_offset = get("_std_offset")?;
        self.dst_offset = get("_dst_offset")?;
        self.start_delta = get("_start_delta")?;
        self.end_delta = get("_end_delta")?;
        // Recompute hasdst like `__init__` does.
        self.hasdst = self
            .start_delta
            .as_ref()
            .map(|o| o.bind(py).is_truthy().unwrap_or(false))
            .unwrap_or(false);
        Ok(())
    }

    #[getter]
    fn _start_delta(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.start_delta)
    }

    #[getter]
    fn _end_delta(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.end_delta)
    }

    #[getter]
    fn _std_abbr(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.std_abbr)
    }

    #[getter]
    fn _dst_abbr(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.dst_abbr)
    }

    #[getter]
    fn _std_offset(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.std_offset)
    }

    #[getter]
    fn _dst_offset(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.dst_offset)
    }

    #[getter]
    fn _dst_base_offset(&self, py: Python<'_>) -> PyResult<PyObject> {
        // `self._dst_offset - self._std_offset` via Python.
        let dst = opt_clone(py, &self.dst_offset);
        let std = opt_clone(py, &self.std_offset);
        Ok(dst.bind(py).sub(std.bind(py))?.unbind())
    }
}

impl TzRange {
        fn year_transitions(&self, py: Python<'_>, year: i64) -> PyResult<Option<(coretz::Wall, coretz::Wall)>> {
        if !self.hasdst {
            return Ok(None);
        }
        let t = self.transitions(py, year.into_pyobject(py)?.into_any())?;
        let tb = t.bind(py);
        let w = |o: Bound<'_, PyAny>| -> PyResult<coretz::Wall> {
            let (wall, _) = wall_of(&o)?;
            Ok(wall)
        };
        Ok(Some((w(tb.get_item(0)?)?, w(tb.get_item(1)?)?)))
    }

    /// `RangeZone` over the stored offsets + resolved transitions.
        fn zone(&self, py: Python<'_>, wall: coretz::Wall) -> PyResult<coretz::RangeZone> {
        let (y, _, _) = civil::from_ordinal(wall.ord);
        let transitions = self.year_transitions(py, y)?;
        let secs = |o: &Option<PyObject>| -> PyResult<i64> {
            match o {
                None => Ok(0),
                Some(v) => {
                    let (s, _) = split_delta(v.bind(py))?;
                    Ok(s)
                }
            }
        };
        Ok(coretz::RangeZone {
            std_offset: secs(&self.std_offset)?,
            dst_offset: secs(&self.dst_offset)?,
            hasdst: self.hasdst,
            transitions,
            amb_transitions: transitions,
        })
    }
}

/// `tzstr`: POSIX TZ-string zone (parsed by the kept `parser._parsetz`,
/// range behavior inherited through the shared `TzRange` state).
#[pyclass(name = "tzstr", extends=TzRange, subclass, weakref, dict, module = "dateutil._dateutil")]
pub struct TzStr {
    s: Option<PyObject>,
}

/// Build the `_delta` relativedelta kwargs from a `_parsetz` start/end
/// attribute object (mirrors `tzstr._delta`).
fn tzstr_delta(py: Python<'_>, x: &Bound<'_, PyAny>, isend: bool, std_s: i64, dst_s: i64) -> PyResult<PyObject> {
    let rd_mod = dateutil_relativedelta_mod(py)?;
    let rd_cls = relativedelta_cls(py)?;
    let kw = PyDict::new(py);
    let month: Bound<'_, PyAny> = x.getattr("month")?;
    if !month.is_none() {
        kw.set_item("month", &month)?;
        let weekday = x.getattr("weekday")?;
        if !weekday.is_none() {
            let week: i64 = x.getattr("week")?.extract()?;
            let wd_cls = rd_mod.getattr("weekday")?;
            let wd = wd_cls.call1((weekday, week))?;
            kw.set_item("weekday", wd)?;
            if week > 0 {
                kw.set_item("day", 1)?;
            } else {
                kw.set_item("day", 31)?;
            }
        } else {
            let day = x.getattr("day")?;
            if !day.is_none() {
                kw.set_item("day", day)?;
            }
        }
    } else {
        let yday = x.getattr("yday")?;
        if !yday.is_none() {
            kw.set_item("yearday", yday)?;
        } else {
            let jyday = x.getattr("jyday")?;
            if !jyday.is_none() {
                kw.set_item("nlyearday", jyday)?;
            }
        }
    }
    if kw.is_empty() {
        // Default first-Sunday-of-April / last-Sunday-of-October.
        if !isend {
            kw.set_item("month", 4)?;
            kw.set_item("day", 1)?;
            kw.set_item("weekday", rd_mod.getattr("SU")?.call1((1,))?)?;
        } else {
            kw.set_item("month", 10)?;
            kw.set_item("day", 31)?;
            kw.set_item("weekday", rd_mod.getattr("SU")?.call1((-1,))?)?;
        }
    }
    let time = x.getattr("time")?;
    if !time.is_none() {
        kw.set_item("seconds", time)?;
    } else {
        kw.set_item("seconds", 7200)?;
    }
    if isend {
        // Convert to standard time (subtract the DST base offset).
        let delta_s = dst_s - std_s;
        let cur: i64 = kw.get_item("seconds")?.unwrap().extract()?;
        kw.set_item("seconds", cur - delta_s)?;
    }
    Ok(rd_cls.call((), Some(&kw))?.unbind())
}

#[pymethods]
impl TzStr {
    #[new]
    #[pyo3(signature = (s=None, posix_offset=None))]
    fn new(
        py: Python<'_>,
        s: Option<Bound<'_, PyAny>>,
        posix_offset: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        // `s=None` builds an empty shell for the unpickle path (state
        // fills it, including `_s`); an explicit user call without `s`
        // behaves the same (the original raises `TypeError`, untested).
        let s_o = match s {
            Some(v) if !v.is_none() => Some(v.unbind()),
            _ => None,
        };
        if s_o.is_none() {
            let base = tzrange_init(py, None, None, None, None, None, None)?;
            return Ok(PyClassInitializer::from(base).add_subclass(TzStr { s: None }));
        }
        let s_kept = clone_opt(py, &s_o);
        let posix = match posix_offset {
            None => false,
            Some(v) => {
                if v.is_none() {
                    false
                } else {
                    v.is_truthy()?
                }
            }
        };
        // `parser._parsetz(s)` through the kept parser module.
        let parsetz = parsetz_fn(py)?;
        let res = parsetz.call1((s_o.as_ref().unwrap().bind(py),))?;
        if res.is_none() {
            return value_error("unknown string format".to_string());
        }
        if res.getattr("any_unused_tokens")?.is_truthy()? {
            return value_error("unknown string format".to_string());
        }
        // GMT/UTC sign inversion (unless posix_offset).
        let stdabbr_o: Bound<'_, PyAny> = res.getattr("stdabbr")?;
        let is_gmt = match &stdabbr_o {
            v if v.is_none() => false,
            v => {
                let abbr: String = v.extract()?;
                abbr == "GMT" || abbr == "UTC"
            }
        };
        if is_gmt && !posix {
            let off: i64 = res.getattr("stdoffset")?.extract()?;
            res.setattr("stdoffset", -off)?;
        }
        // `_parsetz` offsets are plain ints (seconds); the range behavior
        // needs timedeltas.
        for attr in ["stdoffset", "dstoffset"] {
            let v: Bound<'_, PyAny> = res.getattr(attr)?;
            if !v.is_none() {
                if let Ok(secs) = v.extract::<i64>() {
                    res.setattr(attr, make_delta_secs(py, secs as f64)?)?;
                }
            }
        }
        let stdoffset: Bound<'_, PyAny> = res.getattr("stdoffset")?;
        let dstabbr_o: Bound<'_, PyAny> = res.getattr("dstabbr")?;
        let dstoffset_o: Bound<'_, PyAny> = res.getattr("dstoffset")?;
        let has_dstabbr = !dstabbr_o.is_none() && dstabbr_o.is_truthy()?;
        let dstabbr = if dstabbr_o.is_none() { None } else { Some(dstabbr_o.clone().unbind()) };
        let dstoffset = if dstoffset_o.is_none() { None } else { Some(dstoffset_o.clone().unbind()) };
        let false_o = false.into_pyobject(py).unwrap().to_owned().into_any();
        let stdabbr_arg = {
            let v: Bound<'_, PyAny> = res.getattr("stdabbr")?;
            if v.is_none() { None } else { Some(v.unbind()) }
        };
        let mut inner = tzrange_init(
            py,
            stdabbr_arg,
            Some(stdoffset.clone().unbind()),
            dstabbr,
            dstoffset,
            Some(false_o.clone().unbind()),
            Some(false_o.unbind()),
        )?;
        if !has_dstabbr {
            inner.start_delta = None;
            inner.end_delta = None;
        } else {
            let (std_s, _) = split_delta(&stdoffset)?;
            let dst_s = match &inner.dst_offset {
                None => std_s,
                Some(o) => split_delta(o.bind(py))?.0,
            };
            let start = tzstr_delta(py, &res.getattr("start")?, false, std_s, dst_s)?;
            inner.start_delta = Some(start);
            // `if self._start_delta:` — relativedelta truthiness.
            let sd = opt_clone(py, &inner.start_delta);
            if sd.bind(py).is_truthy()? {
                let end = tzstr_delta(py, &res.getattr("end")?, true, std_s, dst_s)?;
                inner.end_delta = Some(end);
            }
        }
        inner.hasdst = inner
            .start_delta
            .as_ref()
            .map(|o| o.bind(py).is_truthy().unwrap_or(false))
            .unwrap_or(false);
        Ok(PyClassInitializer::from(inner).add_subclass(TzStr { s: s_kept }))
    }

    #[pyo3(signature = (s=None, posix_offset=None))]
    fn __init__(
        &mut self,
        _py: Python<'_>,
        s: Option<Bound<'_, PyAny>>,
        posix_offset: Option<Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let _ = (s, posix_offset);
        Ok(())
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let r: String = opt_clone(py, &self.s).bind(py).repr()?.extract()?;
        Ok(format!("tzstr({})", r))
    }

    fn __getstate__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        let base_state = TzRange::__getstate__(slf.as_super(), py)?;
        let d: Bound<'_, PyDict> = base_state.extract(py)?;
        d.set_item("_s", opt_clone(py, &slf.s))?;
        Ok(d.into_any().unbind())
    }

    fn __setstate__(mut slf: PyRefMut<'_, Self>, py: Python<'_>, state: Bound<'_, PyDict>) -> PyResult<()> {
        TzRange::__setstate__(&mut *slf.as_super(), py, state.clone().into_any())?;
        let s = state.get_item("_s")?;
        slf.s = s.and_then(|v| if v.is_none() { None } else { Some(v.unbind()) });
        Ok(())
    }

}