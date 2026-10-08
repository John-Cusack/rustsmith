// SPDX-License-Identifier: Apache-2.0
//! Shared Python-boundary helpers: datetime <-> wall-triple conversion,
//! comparisons, constructors, and error shapes.

use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyList, PyModule};

// --- Cached module/class objects (no `py.import` on hot paths) ---
//
// Every helper below imports its module once per process (first use wins,
// via `GILOnceCell`) and hands out a `Bound` ref tied to the caller's
// `Python` token. Init is lazy, not `#[pymodule]`-init time, so import order
// is identical to the pre-cache code: a module is first imported exactly
// when the old code would have imported it, and circular-import behavior
// cannot change. The cached object is the same `sys.modules` entry
// `py.import` would return.
macro_rules! cached_module {
    ($cell:ident, $func:ident, $name:literal) => {
        static $cell: GILOnceCell<Py<PyModule>> = GILOnceCell::new();
        pub fn $func(py: Python<'_>) -> PyResult<&Bound<'_, PyModule>> {
            Ok($cell
                .get_or_try_init(py, || py.import($name).map(|m| m.unbind()))?
                .bind(py))
        }
    };
}
macro_rules! cached_attr {
    ($cell:ident, $func:ident, $modfunc:ident, $attr:literal) => {
        static $cell: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
        pub fn $func(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
            Ok($cell
                .get_or_try_init(py, || $modfunc(py)?.getattr($attr).map(|o| o.unbind()))?
                .bind(py))
        }
    };
}
cached_module!(DATETIME_MOD_CELL, datetime_mod, "datetime");
cached_module!(BUILTINS_MOD_CELL, builtins_mod, "builtins");
cached_module!(CALENDAR_MOD_CELL, calendar_mod, "calendar");
cached_module!(TIME_MOD_CELL, time_mod, "time");
cached_module!(MATH_MOD_CELL, math_mod, "math");
cached_module!(ITERTOOLS_MOD_CELL, itertools_mod, "itertools");
cached_module!(THREAD_MOD_CELL, thread_mod, "_thread");
cached_module!(COPYREG_MOD_CELL, copyreg_mod, "copyreg");
cached_module!(IO_MOD_CELL, io_mod, "io");
cached_module!(WARNINGS_MOD_CELL, warnings_mod, "warnings");
cached_module!(DU_TZ_MOD_CELL, dateutil_tz_mod, "dateutil.tz");
cached_module!(DU_PARSER_MOD_CELL, dateutil_parser_mod, "dateutil.parser");
cached_module!(
    DU_PARSER_INNER_CELL,
    dateutil_parser_inner,
    "dateutil.parser._parser"
);
cached_module!(DU_RRULE_MOD_CELL, dateutil_rrule_mod, "dateutil.rrule");
cached_module!(
    DU_RD_MOD_CELL,
    dateutil_relativedelta_mod,
    "dateutil.relativedelta"
);
cached_module!(OPERATOR_MOD_CELL, operator_mod, "operator");
cached_module!(DU_EXT_MOD_CELL, dateutil_ext_mod, "dateutil._dateutil");
cached_attr!(DATETIME_CLS_CELL, datetime_cls, datetime_mod, "datetime");
cached_attr!(DATE_CLS_CELL, date_cls, datetime_mod, "date");
cached_attr!(TIME_CLS_CELL, time_cls, datetime_mod, "time");
cached_attr!(TIMEDELTA_CLS_CELL, timedelta_cls, datetime_mod, "timedelta");
cached_attr!(TZINFO_CLS_CELL, tzinfo_cls, datetime_mod, "tzinfo");
cached_attr!(INT_FN_CELL, int_fn, builtins_mod, "int");
cached_attr!(FLOAT_FN_CELL, float_fn, builtins_mod, "float");
cached_attr!(ORD_FN_CELL, ord_fn, builtins_mod, "ord");
cached_attr!(ABS_FN_CELL, abs_fn, builtins_mod, "abs");
cached_attr!(SET_FN_CELL, set_fn, builtins_mod, "set");
cached_attr!(SORTED_FN_CELL, sorted_fn, builtins_mod, "sorted");
cached_attr!(TUPLE_FN_CELL, tuple_fn, builtins_mod, "tuple");
cached_attr!(ITER_FN_CELL, iter_fn, builtins_mod, "iter");
cached_attr!(DIVMOD_FN_CELL, divmod_fn, builtins_mod, "divmod");
cached_attr!(BYTEARRAY_CLS_CELL, bytearray_cls, builtins_mod, "bytearray");
cached_attr!(STRINGIO_CLS_CELL, stringio_cls, io_mod, "StringIO");
cached_attr!(GCD_FN_CELL, gcd_fn, math_mod, "gcd");
cached_attr!(ISLICE_FN_CELL, islice_fn, itertools_mod, "islice");
cached_attr!(UTC_OBJ_CELL, utc_obj, dateutil_tz_mod, "UTC");
cached_attr!(TZOFFSET_CLS_CELL, tzoffset_cls, dateutil_tz_mod, "tzoffset");
cached_attr!(TZSTR_CLS_CELL, tzstr_cls, dateutil_tz_mod, "tzstr");
cached_attr!(TZLOCAL_CLS_CELL, tzlocal_cls, dateutil_tz_mod, "tzlocal");
cached_attr!(GETTZ_FN_CELL, gettz_fn, dateutil_tz_mod, "gettz");
cached_attr!(
    PARSER_PARSE_CELL,
    parser_parse_fn,
    dateutil_parser_mod,
    "parse"
);
cached_attr!(
    PARSER_ERROR_CELL,
    parser_error_cls,
    dateutil_parser_inner,
    "ParserError"
);
cached_attr!(
    PARSETZ_FN_CELL,
    parsetz_fn,
    dateutil_parser_inner,
    "_parsetz"
);
cached_attr!(WEEKDAY_CLS_CELL, weekday_cls, dateutil_rrule_mod, "weekday");
cached_attr!(RRULE_CLS_CELL, rrule_cls, dateutil_rrule_mod, "rrule");
cached_attr!(
    RD_CLS_CELL,
    relativedelta_cls,
    dateutil_relativedelta_mod,
    "relativedelta"
);
cached_attr!(
    WEEKDAYS_OBJ_CELL,
    weekdays_obj,
    dateutil_relativedelta_mod,
    "weekdays"
);
cached_attr!(OPERATOR_ADD_CELL, operator_add_fn, operator_mod, "add");
cached_attr!(
    DEFAULTPARSER_CELL,
    defaultparser_obj,
    dateutil_ext_mod,
    "DEFAULTPARSER"
);

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
    let dt = datetime_cls(py)?;
    obj.is_instance(dt)
}

/// True when `obj` is a `datetime.date` (datetimes included).
pub fn is_date(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let dt = date_cls(py)?;
    obj.is_instance(dt)
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
    let cls = datetime_cls(py)?;
    let kwargs = PyDict::new(py);
    if let Some(t) = tz {
        kwargs.set_item("tzinfo", t)?;
    }
    kwargs.set_item("fold", fold)?;
    Ok(cls.call((y, m, d, hh, mm, ss, us), Some(&kwargs))?.unbind())
}

/// Construct `datetime.date(y, m, d)`.
pub fn make_date(py: Python<'_>, y: i64, m: i64, d: i64) -> PyResult<PyObject> {
    let cls = date_cls(py)?;
    Ok(cls.call1((y, m, d))?.unbind())
}

/// Construct `datetime.timedelta(days=d, seconds=s, microseconds=u)`.
pub fn make_delta(py: Python<'_>, days: i64, seconds: i64, micros: i64) -> PyResult<PyObject> {
    let cls = timedelta_cls(py)?;
    Ok(cls.call1((days, seconds, micros))?.unbind())
}

/// `timedelta(seconds=float_secs)` (fractional offsets preserved).
pub fn make_delta_secs(py: Python<'_>, secs: f64) -> PyResult<PyObject> {
    let cls = timedelta_cls(py)?;
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
    let warnings = warnings_mod(py)?;
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
