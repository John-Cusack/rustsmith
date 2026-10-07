// SPDX-License-Identifier: Apache-2.0
//! Shared Python-boundary helpers: datetime <-> wall-triple conversion,
//! comparisons, constructors, and error shapes.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Wall datetime fields read from a Python `datetime`.
pub struct Parts {
    pub y: i64,
    pub m: i64,
    pub d: i64,
    pub hh: i64,
    pub mm: i64,
    pub ss: i64,
    pub us: i64,
    pub fold: i64,
    pub tz: Option<PyObject>,
}

fn get_i64(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<i64> {
    obj.getattr(name)?.extract::<i64>()
}

/// Read wall fields from a `datetime` (or `date` for y/m/d only via
/// [`date_parts`]).
pub fn parts_of(obj: &Bound<'_, PyAny>) -> PyResult<Parts> {
    let fold = match obj.getattr("fold") {
        Ok(f) => f.extract::<i64>()?,
        Err(_) => 0,
    };
    let tz = match obj.getattr("tzinfo") {
        Ok(t) => {
            if t.is_none() {
                None
            } else {
                Some(t.unbind())
            }
        }
        Err(_) => None,
    };
    Ok(Parts {
        y: get_i64(obj, "year")?,
        m: get_i64(obj, "month")?,
        d: get_i64(obj, "day")?,
        hh: get_i64(obj, "hour")?,
        mm: get_i64(obj, "minute")?,
        ss: get_i64(obj, "second")?,
        us: get_i64(obj, "microsecond")?,
        fold,
        tz,
    })
}

/// Read (year, month, day) from a `date` or `datetime`.
pub fn date_parts(obj: &Bound<'_, PyAny>) -> PyResult<(i64, i64, i64)> {
    Ok((get_i64(obj, "year")?, get_i64(obj, "month")?, get_i64(obj, "day")?))
}

/// True when `obj` is a `datetime.datetime` (subclasses included).
pub fn is_datetime(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let dt = py.import("datetime")?.getattr("datetime")?;
    obj.is_instance(&dt)
}

/// True when `obj` is a `datetime.date` (datetimes included).
pub fn is_date(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let dt = py.import("datetime")?.getattr("date")?;
    obj.is_instance(&dt)
}

/// Construct `datetime(y, m, d, hh, mm, ss, us, tzinfo=tz, fold=fold)`.
/// Mirrors `datetime.combine` output for engine yields (fold from the
/// fresh time is always 0 there; callers pass the fold explicitly).
pub fn make_datetime(
    py: Python<'_>,
    y: i64,
    m: i64,
    d: i64,
    hh: i64,
    mm: i64,
    ss: i64,
    us: i64,
    tz: Option<&PyObject>,
    fold: i64,
) -> PyResult<PyObject> {
    let cls = py.import("datetime")?.getattr("datetime")?;
    let kwargs = PyDict::new(py);
    if let Some(t) = tz {
        kwargs.set_item("tzinfo", t)?;
    }
    kwargs.set_item("fold", fold)?;
    Ok(cls.call((y, m, d, hh, mm, ss, us), Some(&kwargs))?.unbind())
}

/// Construct `datetime.date(y, m, d)`.
pub fn make_date(py: Python<'_>, y: i64, m: i64, d: i64) -> PyResult<PyObject> {
    let cls = py.import("datetime")?.getattr("date")?;
    Ok(cls.call1((y, m, d))?.unbind())
}

/// Construct `datetime.timedelta(days=d, seconds=s, microseconds=u)`.
pub fn make_delta(py: Python<'_>, days: i64, seconds: i64, micros: i64) -> PyResult<PyObject> {
    let cls = py.import("datetime")?.getattr("timedelta")?;
    Ok(cls.call1((days, seconds, micros))?.unbind())
}

/// `timedelta(seconds=float_secs)` (fractional offsets preserved).
pub fn make_delta_secs(py: Python<'_>, secs: f64) -> PyResult<PyObject> {
    let cls = py.import("datetime")?.getattr("timedelta")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("seconds", secs)?;
    Ok(cls.call((), Some(&kwargs))?.unbind())
}

/// Read a `timedelta` as (days, seconds, microseconds).
pub fn delta_parts(obj: &Bound<'_, PyAny>) -> PyResult<(i64, i64, i64)> {
    Ok((
        get_i64(obj, "days")?,
        get_i64(obj, "seconds")?,
        get_i64(obj, "microseconds")?,
    ))
}

/// Python `==` returning a Rust bool (`NotImplemented` constituents fall
/// back through the interpreter: use [`py_eq`] which returns `None`
/// when the comparison itself is `NotImplemented`).
pub fn py_eq(py: Python<'_>, a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(a.eq(b)?)
}

/// Python `<` returning a Rust bool.
pub fn py_lt(py: Python<'_>, a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
    let _ = py;
    Ok(a.lt(b)?)
}

/// Sort a `Vec<PyObject>` of datetimes in place with Python ordering
/// (mirrors `list.sort()` exactly, including mixed-type errors).
pub fn py_sort(py: Python<'_>, items: &mut Vec<PyObject>) -> PyResult<()> {
    let list = PyList::new(py, items.iter().map(|o| o.bind(py)))?;
    list.sort()?;
    items.clear();
    for o in list.iter() {
        items.push(o.unbind());
    }
    Ok(())
}

/// Is `obj` truthy (`bool(obj)`)?
pub fn py_bool(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_truthy()
}

/// `value_error(msg)`, `type_error(msg)`, etc. with exact messages.
pub fn value_error<T>(msg: String) -> PyResult<T> {
    Err(pyo3::exceptions::PyValueError::new_err(msg))
}

pub fn type_error<T>(msg: String) -> PyResult<T> {
    Err(pyo3::exceptions::PyTypeError::new_err(msg))
}

pub fn overflow_error<T>(msg: String) -> PyResult<T> {
    Err(pyo3::exceptions::PyOverflowError::new_err(msg))
}

pub fn index_error<T>(msg: String) -> PyResult<T> {
    Err(pyo3::exceptions::PyIndexError::new_err(msg))
}

pub fn zero_division_error<T>(msg: String) -> PyResult<T> {
    Err(pyo3::exceptions::PyZeroDivisionError::new_err(msg))
}

/// Emit a warning with an explicit category object (e.g. a
/// `DeprecationWarning` subclass from the kept package init).
pub fn warn_with(py: Python<'_>, category: &Bound<'_, PyAny>, msg: &str) -> PyResult<()> {
    let warnings = py.import("warnings")?;
    warnings.call_method1("warn", (msg, category))?;
    Ok(())
}

/// The interpreter's `NotImplemented` singleton as an object.
pub fn not_implemented(py: Python<'_>) -> PyObject {
    py.NotImplemented().into_any()
}

/// Python `bool` as an owned object.
pub fn bool_obj(py: Python<'_>, v: bool) -> PyObject {
    pyo3::types::PyBool::new(py, v).to_owned().into_any().unbind()
}

/// Dynamic `type(self)` for `PyRef` receivers (subclass-friendly operators).
pub fn slf_type<'py, T: pyo3::PyClass>(
    py: Python<'py>,
    slf: &'py pyo3::PyRef<'py, T>,
) -> Bound<'py, pyo3::types::PyType> {
    use pyo3::conversion::IntoPyObject;
    slf.into_pyobject(py).unwrap().to_owned().into_any().get_type()
}

/// Clone a slice of owned objects.
pub fn clone_vec(py: Python<'_>, v: &[PyObject]) -> Vec<PyObject> {
    v.iter().map(|o| o.clone_ref(py)).collect()
}

/// Clone an optional owned object.
pub fn clone_opt(py: Python<'_>, v: &Option<PyObject>) -> Option<PyObject> {
    v.as_ref().map(|o| o.clone_ref(py))
}

/// Clone an optional vec of owned objects.
pub fn clone_opt_vec(py: Python<'_>, v: &Option<Vec<PyObject>>) -> Option<Vec<PyObject>> {
    v.as_ref().map(|x| clone_vec(py, x))
}

/// Clone an optional object, or `None`.
pub fn opt_clone(py: Python<'_>, v: &Option<PyObject>) -> PyObject {
    clone_opt(py, v).unwrap_or_else(|| py.None())
}
