// SPDX-License-Identifier: Apache-2.0
//! `parser`/`parserinfo`/`_timelex` binding: the whole `_parse` path runs in
//! `dateutil-core`; datetimes, tz assembly and errors run here so CPython
//! semantics stay exact.
//!
//! Seam contract (documented, per the port ADR):
//! - Default `parserinfo` (exact type, pristine class tables) runs the
//!   pure-Rust fast path: lexer + `_parse` + default tables in core.
//! - Custom `parserinfo` subclasses (or globally mutated tables) run the
//!   same core loop through [`PyInfo`], which calls the Python lookup
//!   methods per token (Python-callback fallback). Construction,
//!   `convertyear`, `validate` and all instance state live in `__dict__`
//!   exactly like the original, so pickling, subclassing and attribute
//!   overrides behave identically.
//! - `ParserError`/`UnknownTimezoneWarning` stay Python classes (verbatim
//!   in the shim); Rust raises them via `from_value` so identity,
//!   `args`, `__str__`/`__repr__` formatting and warning categories match.

use parking_lot::RwLock;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use pyo3::basic::CompareOp;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple, PyType};

use dateutil_core::parser::{
    parse::parse_tokens, validate_default, DefaultInfo, Fail, Info, ParseOk, ParseOptions,
    Timelex, TzOff,
};

/// Pristine-table check: the fast path requires the exact `parserinfo`
/// type plus class tables identical (`is`) to the ones captured on the
/// type as `_rust_pristine` at module init. A globally mutated table (or
/// any subclass) takes the Python-callback slow path instead.
fn tables_pristine(py: Python<'_>, info: &Bound<'_, PyAny>) -> PyResult<bool> {
    if !info.get_type().is(&py.get_type::<ParserInfo>()) {
        return Ok(false);
    }
    let cls = info.get_type();
    let pris = cls.getattr("_rust_pristine")?;
    let names = [
        "JUMP", "WEEKDAYS", "MONTHS", "HMS", "AMPM", "UTCZONE", "PERTAIN", "TZOFFSET",
    ];
    for (i, n) in names.iter().enumerate() {
        if !cls.getattr(*n)?.is(&pris.get_item(i)?) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Custom-`parserinfo` bridge: the core loop calls these per token; each
/// forwards to the Python method. `ValueError`/`IndexError` become
/// `Fail::NoResult` (the original `_parse` catches both); any other Python
/// error is stashed and re-raised by the binding, discarding the result.
pub struct PyInfo<'py> {
    py: Python<'py>,
    obj: PyObject,
    stashed: RefCell<Option<PyErr>>,
}

impl<'py> PyInfo<'py> {
    fn new(py: Python<'py>, obj: PyObject) -> Self {
        PyInfo {
            py,
            obj,
            stashed: RefCell::new(None),
        }
    }

    fn take_stashed(&self) -> Option<PyErr> {
        self.stashed.borrow_mut().take()
    }

    fn trap<T>(&self, r: PyResult<T>, dummy: T) -> Result<T, Fail> {
        match r {
            Ok(v) => Ok(v),
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyValueError>(self.py)
                    || e.is_instance_of::<pyo3::exceptions::PyIndexError>(self.py)
                {
                    Err(Fail::NoResult)
                } else {
                    *self.stashed.borrow_mut() = Some(e);
                    Ok(dummy)
                }
            }
        }
    }

    fn call1(
        &self,
        name: &str,
        arg: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.obj.bind(self.py).call_method1(name, (arg,))
    }

    /// `int()`-protocol coercion (mirrors where the original truncates).
    fn coerce(&self, v: Bound<'py, PyAny>) -> PyResult<i64> {
        if let Ok(i) = v.extract::<i64>() {
            return Ok(i);
        }
        let ival = crate::util::int_fn(self.py)?.call1((v,))?;
        ival.extract::<i64>()
    }

    fn probe(&self, name: &str, tok: &str) -> Result<Option<i64>, Fail> {
        let r: PyResult<Option<i64>> = (|| {
            let v = self.call1(name, &PyString::new(self.py, tok).into_any())?;
            if v.is_none() {
                return Ok(None);
            }
            Ok(Some(self.coerce(v)?))
        })();
        self.trap(r, None)
    }
}

impl Info for PyInfo<'_> {
    fn jump(&self, tok: &str) -> bool {
        let r: PyResult<bool> = (|| {
            self.call1("jump", &PyString::new(self.py, tok).into_any())?
                .is_truthy()
        })();
        self.trap(r, false).unwrap_or(false)
    }

    fn weekday(&self, tok: &str) -> Option<i64> {
        self.probe("weekday", tok).unwrap_or(None)
    }

    fn month(&self, tok: &str) -> Option<i64> {
        self.probe("month", tok).unwrap_or(None)
    }

    fn hms(&self, tok: &str) -> Option<i64> {
        self.probe("hms", tok).unwrap_or(None)
    }

    fn ampm(&self, tok: &str) -> Option<i64> {
        self.probe("ampm", tok).unwrap_or(None)
    }

    fn pertain(&self, tok: &str) -> bool {
        let r: PyResult<bool> = (|| {
            self.call1("pertain", &PyString::new(self.py, tok).into_any())?
                .is_truthy()
        })();
        self.trap(r, false).unwrap_or(false)
    }

    fn utczone(&self, tok: &str) -> bool {
        let r: PyResult<bool> = (|| {
            self.call1("utczone", &PyString::new(self.py, tok).into_any())?
                .is_truthy()
        })();
        self.trap(r, false).unwrap_or(false)
    }

    fn tzoffset(&self, name: &str) -> Option<TzOff> {
        let r: PyResult<Option<TzOff>> = (|| {
            let v = self.call1("tzoffset", &PyString::new(self.py, name).into_any())?;
            if v.is_none() {
                return Ok(None);
            }
            let i = self.coerce(v)?;
            Ok(Some(TzOff::Secs(i)))
        })();
        self.trap(r, None).unwrap_or(None)
    }

    fn is_utc_abbr(&self, token: &str) -> bool {
        let r: PyResult<bool> = (|| {
            let zone = self.obj.bind(self.py).getattr("UTCZONE")?;
            zone.contains(PyString::new(self.py, token).into_any())
        })();
        self.trap(r, false).unwrap_or(false)
    }

    fn convertyear(
        &self,
        year: &str,
        century_specified: bool,
        _cur_year: i64,
        _century: i64,
    ) -> String {
        let r: PyResult<String> = (|| {
            let int_fn = crate::util::int_fn(self.py)?;
            let y = int_fn.call1((year,))?;
            let v = self.obj.bind(self.py).call_method1(
                "convertyear",
                (y, century_specified),
            )?;
            Ok(v.str()?.to_string())
        })();
        self.trap(r, year.to_string()).unwrap_or(year.to_string())
    }

    fn days_in_month(&self, year: &str, month: &str) -> Result<u8, Fail> {
        let r: PyResult<u8> = (|| {
            let cal = crate::util::calendar_mod(self.py)?;
            let int_fn = crate::util::int_fn(self.py)?;
            let y = int_fn.call1((year,))?;
            let m = int_fn.call1((month,))?;
            let mr = cal.call_method1("monthrange", (y, m))?;
            mr.get_item(1)?.extract::<u8>()
        })();
        self.trap(r, 0)
    }
}

/// `parserinfo`: default tables as class attributes; per-instance scalars
/// plus lazily built lookup tables. Tables build from the runtime type on
/// first use, so subclass `MONTHS`/`WEEKDAYS`/… overrides behave exactly
/// like the original (which builds them in `__init__` from
/// `type(self)`); `__getnewargs__` keeps unpickling reconstructing through
/// the runtime class. The pure-Rust fast path never touches these (it
/// uses the static core tables), so construction stays cheap.
#[pyclass(name = "parserinfo", subclass, dict, weakref, module = "dateutil._dateutil")]
pub struct ParserInfo {
    dayfirst: PyObject,
    yearfirst: PyObject,
    year: i64,
    century: i64,
    tables: RwLock<Option<Tables>>,
    built_for: RwLock<Option<Py<PyType>>>,
}

struct Tables {
    jump: HashSet<String>,
    weekdays: HashMap<String, i64>,
    months: HashMap<String, i64>,
    hms: HashMap<String, i64>,
    ampm: HashMap<String, i64>,
    utczone: HashSet<String>,
    pertain: HashSet<String>,
}

fn local_year(py: Python<'_>) -> PyResult<i64> {
    crate::util::time_mod(py)?
        .getattr("localtime")?
        .call0()?
        .getattr("tm_year")?
        .extract()
}

fn build_tables(_py: Python<'_>, cls: &Bound<'_, PyType>) -> PyResult<Tables> {
    let get = |attr: &str| -> PyResult<(HashSet<String>, HashMap<String, i64>)> {
        convert_table(&cls.getattr(attr)?)
    };
    let (jump, _) = get("JUMP")?;
    let (_, weekdays) = get("WEEKDAYS")?;
    let (_, months) = get("MONTHS")?;
    let (_, hms) = get("HMS")?;
    let (_, ampm) = get("AMPM")?;
    let (utczone, _) = get("UTCZONE")?;
    let (pertain, _) = get("PERTAIN")?;
    Ok(Tables {
        jump,
        weekdays,
        months,
        hms,
        ampm,
        utczone,
        pertain,
    })
}

/// Read-only table access, rebuilding when the runtime type changed
/// (subclass first use after base-built construction).
fn with_tables<T>(
    slf: &Bound<'_, ParserInfo>,
    f: impl FnOnce(&Tables) -> T,
) -> PyResult<T> {
    let py = slf.py();
    let cur = slf.get_type();
    let rebuild = {
        let this = slf.borrow();
        let guard = this.built_for.read();
        match guard.as_ref() {
            Some(t) => !t.bind(py).is(&cur),
            None => true,
        }
    };
    if rebuild {
        let tables = build_tables(py, &cur)?;
        let this = slf.borrow_mut();
        *this.tables.write() = Some(tables);
        *this.built_for.write() = Some(cur.clone().unbind());
    }
    let this = slf.borrow();
    let guard = this.tables.read();
    Ok(f(guard.as_ref().expect("tables built above")))
}

fn convert_table(lst: &Bound<'_, PyAny>) -> PyResult<(HashSet<String>, HashMap<String, i64>)> {
    let mut set = HashSet::new();
    let mut map = HashMap::new();
    for (i, r) in lst.try_iter()?.enumerate() {
        let v = r?;
        if let Ok(sub) = v.downcast::<PyTuple>() {
            for item in sub.iter() {
                let low: String = item.call_method0("lower")?.extract()?;
                set.insert(low.clone());
                map.insert(low, i as i64);
            }
        } else if let Ok(sub) = v.downcast::<PyList>() {
            for item in sub.iter() {
                let low: String = item.call_method0("lower")?.extract()?;
                set.insert(low.clone());
                map.insert(low, i as i64);
            }
        } else {
            let low: String = v.call_method0("lower")?.extract()?;
            set.insert(low.clone());
            map.insert(low, i as i64);
        }
    }
    Ok((set, map))
}

fn opt_lookup(py: Python<'_>, found: Option<i64>, plus_one: bool) -> PyObject {
    match found {
        Some(v) => (if plus_one { v + 1 } else { v })
            .into_pyobject(py)
            .unwrap()
            .into_any()
            .unbind(),
        None => py.None(),
    }
}

#[pymethods]
impl ParserInfo {
    #[new]
    #[pyo3(signature = (dayfirst=None, yearfirst=None))]
    fn new(
        py: Python<'_>,
        dayfirst: Option<Bound<'_, PyAny>>,
        yearfirst: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let none = py.None();
        let year = local_year(py)?;
        let base = py.get_type::<ParserInfo>();
        Ok(ParserInfo {
            dayfirst: dayfirst.map(|o| o.unbind()).unwrap_or_else(|| none.clone_ref(py)),
            yearfirst: yearfirst.map(|o| o.unbind()).unwrap_or_else(|| none.clone_ref(py)),
            year,
            century: year / 100 * 100,
            tables: RwLock::new(Some(build_tables(py, &base)?)),
            built_for: RwLock::new(Some(base.unbind())),
        })
    }

    #[pyo3(signature = (dayfirst=None, yearfirst=None))]
    fn __init__(
        &mut self,
        py: Python<'_>,
        dayfirst: Option<Bound<'_, PyAny>>,
        yearfirst: Option<Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let none = py.None();
        self.dayfirst = dayfirst.map(|o| o.unbind()).unwrap_or_else(|| none.clone_ref(py));
        self.yearfirst = yearfirst.map(|o| o.unbind()).unwrap_or_else(|| none.clone_ref(py));
        self.year = local_year(py)?;
        self.century = self.year / 100 * 100;
        Ok(())
    }

    fn jump(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> PyResult<bool> {
        let low: String = name.call_method0("lower")?.extract()?;
        with_tables(slf, |t| t.jump.contains(&low))
    }

    fn weekday(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let low: String = name.call_method0("lower")?.extract()?;
        Ok(opt_lookup(
            py,
            with_tables(slf, |t| t.weekdays.get(&low).cloned())?,
            false,
        ))
    }

    fn month(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let low: String = name.call_method0("lower")?.extract()?;
        Ok(opt_lookup(
            py,
            with_tables(slf, |t| t.months.get(&low).cloned())?,
            true,
        ))
    }

    fn hms(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let low: String = name.call_method0("lower")?.extract()?;
        Ok(opt_lookup(
            py,
            with_tables(slf, |t| t.hms.get(&low).cloned())?,
            false,
        ))
    }

    fn ampm(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let low: String = name.call_method0("lower")?.extract()?;
        Ok(opt_lookup(
            py,
            with_tables(slf, |t| t.ampm.get(&low).cloned())?,
            false,
        ))
    }

    fn pertain(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> PyResult<bool> {
        let low: String = name.call_method0("lower")?.extract()?;
        with_tables(slf, |t| t.pertain.contains(&low))
    }

    fn utczone(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> PyResult<bool> {
        let low: String = name.call_method0("lower")?.extract()?;
        with_tables(slf, |t| t.utczone.contains(&low))
    }

    fn tzoffset(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let key: String = name.str()?.to_string();
        if with_tables(slf, |t| t.utczone.contains(&key))? {
            return Ok(0i64.into_pyobject(py)?.into_any().unbind());
        }
        Ok(slf
            .get_type()
            .getattr("TZOFFSET")?
            .call_method1("get", (name,))?
            .unbind())
    }

    #[pyo3(signature = (year, century_specified=false))]
    fn convertyear(
        &self,
        py: Python<'_>,
        year: Bound<'_, PyAny>,
        century_specified: bool,
    ) -> PyResult<PyObject> {
        match year.extract::<i64>() {
            Ok(y) => {
                if y < 0 {
                    return Err(pyo3::exceptions::PyAssertionError::new_err(""));
                }
                if y < 100 && !century_specified {
                    let mut out = y + self.century;
                    if out >= self.year + 50 {
                        out -= 100;
                    } else if out < self.year - 50 {
                        out += 100;
                    }
                    return Ok(out.into_pyobject(py)?.into_any().unbind());
                }
                Ok(year.unbind())
            }
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyOverflowError>(py) {
                    let neg: bool = year
                        .rich_compare(0i64, CompareOp::Lt)?
                        .extract()
                        .unwrap_or(false);
                    if neg {
                        return Err(pyo3::exceptions::PyAssertionError::new_err(""));
                    }
                    return Ok(year.unbind());
                }
                Err(e)
            }
        }
    }

    #[getter]
    fn dayfirst(&self, py: Python<'_>) -> PyObject {
        self.dayfirst.clone_ref(py)
    }

    #[getter]
    fn yearfirst(&self, py: Python<'_>) -> PyObject {
        self.yearfirst.clone_ref(py)
    }

    #[getter]
    fn _year(&self) -> i64 {
        self.year
    }

    #[getter]
    fn _century(&self) -> i64 {
        self.century
    }

    fn __getnewargs__(&self, py: Python<'_>) -> (PyObject, PyObject) {
        (self.dayfirst.clone_ref(py), self.yearfirst.clone_ref(py))
    }

    fn validate(slf: &Bound<'_, Self>, res: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        if !res.getattr("year")?.is_none() {
            let cy = slf.call_method1(
                "convertyear",
                (
                    res.getattr("year")?,
                    res.getattr("century_specified")?,
                ),
            )?;
            res.setattr("year", cy)?;
        }
        let tzoff = res.getattr("tzoffset")?;
        let tzoff_zero = !tzoff.is_none()
            && tzoff
                .rich_compare(0i64, CompareOp::Eq)?
                .extract::<bool>()?;
        let tzname = res.getattr("tzname")?;
        let tzname_s: String = if tzname.is_none() {
            String::new()
        } else {
            tzname.str()?.to_string()
        };
        if (tzoff_zero && tzname.is_none()) || tzname_s == "Z" || tzname_s == "z" {
            res.setattr("tzname", "UTC")?;
            res.setattr("tzoffset", 0)?;
        } else if !tzname.is_none() {
            // `res.tzoffset != 0` is true for `None` too.
            let unset: bool = tzoff.is_none()
                || !tzoff
                    .rich_compare(0i64, CompareOp::Eq)?
                    .extract::<bool>()?;
            if unset {
                let uz: bool = slf
                    .call_method1("utczone", (tzname.clone(),))?
                    .is_truthy()?;
                if uz {
                    res.setattr("tzoffset", 0)?;
                }
            }
        }
        Ok(crate::util::bool_obj(py, true))
    }
}

// ---------------------------------------------------------------------------
// Parse orchestration
// ---------------------------------------------------------------------------

fn type_error_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(format!(
        "Parser must be a string or character stream, not {}",
        obj.get_type().name()?
    ))
}

fn normalize_input(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<String> {
    use pyo3::types::{PyBytes, PyString};
    if obj.is_instance_of::<PyString>() {
        return obj.extract::<String>();
    }
    let bytearray_t = crate::util::bytearray_cls(py)?;
    if obj.is_instance_of::<PyBytes>() || obj.is_instance(bytearray_t)? {
        return obj.call_method0("decode")?.extract::<String>();
    }
    if obj.hasattr("read")? {
        let data = obj.call_method0("read")?;
        if data.is_instance_of::<PyString>() {
            return data.extract::<String>();
        }
        if data.is_instance_of::<PyBytes>() || data.is_instance(bytearray_t)? {
            return data.call_method0("decode")?.extract::<String>();
        }
        return crate::util::type_error(type_error_name(&data)?);
    }
    crate::util::type_error(type_error_name(obj)?)
}

fn parser_error_cls(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(crate::util::parser_error_cls(py)?.to_owned())
}

fn raise_parser_error(py: Python<'_>, fmt: &str, arg: &Bound<'_, PyAny>) -> PyErr {
    match parser_error_cls(py).and_then(|c| c.call1((fmt, arg))) {
        Ok(inst) => PyErr::from_value(inst),
        Err(_) => pyo3::exceptions::PyValueError::new_err(format!(
            "{}: {}",
            fmt,
            arg.str()
                .and_then(|s| s.extract::<String>())
                .unwrap_or_default()
        )),
    }
}

fn truthy(v: &Option<Bound<'_, PyAny>>) -> PyResult<bool> {
    match v {
        None => Ok(false),
        Some(o) => o.is_truthy(),
    }
}

fn build_naive(
    py: Python<'_>,
    ok: &ParseOk,
    default_obj: &Bound<'_, PyAny>,
    timestr: &Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    let int_fn = crate::util::int_fn(py)?;
    let pint = |s: &str| -> PyResult<PyObject> { Ok(int_fn.call1((s,))?.unbind()) };
    let kwargs = PyDict::new(py);
    if let Some(y) = &ok.year {
        kwargs.set_item("year", pint(y)?)?;
    }
    if let Some(m) = &ok.month {
        kwargs.set_item("month", pint(m)?)?;
    }
    if let Some(d) = &ok.day {
        kwargs.set_item("day", pint(d)?)?;
    }
    if let Some(h) = &ok.hour {
        kwargs.set_item("hour", pint(h)?)?;
    }
    if let Some(m) = &ok.minute {
        kwargs.set_item("minute", pint(m)?)?;
    }
    if let Some(s) = &ok.second {
        kwargs.set_item("second", pint(s)?)?;
    }
    if let Some(u) = ok.micro {
        kwargs.set_item("microsecond", pint(&u.to_string())?)?;
    }
    let naive = (|| -> PyResult<PyObject> {
        if !kwargs.contains("day")? {
            let cyear = match &ok.year {
                Some(y) => pint(y)?,
                None => default_obj.getattr("year")?.unbind(),
            };
            let cmonth = match &ok.month {
                Some(m) => pint(m)?,
                None => default_obj.getattr("month")?.unbind(),
            };
            let cday = match &ok.day {
                Some(d) => pint(d)?,
                None => default_obj.getattr("day")?.unbind(),
            };
            let cal = crate::util::calendar_mod(py)?;
            let mr = cal.call_method1(
                "monthrange",
                (cyear.bind(py), cmonth.bind(py)),
            )?;
            let dim = mr.get_item(1)?;
            let over: bool = cday
                .bind(py)
                .rich_compare(&dim, CompareOp::Gt)?
                .extract()?;
            if over {
                kwargs.set_item("day", dim)?;
            }
        }
        Ok(default_obj
            .call_method("replace", (), Some(&kwargs))?
            .unbind())
    })();
    let mut naive = match naive {
        Ok(v) => v,
        Err(e) => {
            if e.is_instance_of::<pyo3::exceptions::PyValueError>(py) {
                let msg = format!("{}: %s", e.value(py).str()?);
                let pe = raise_parser_error(py, &msg, timestr);
                pe.set_cause(py, Some(e));
                return Err(pe);
            }
            return Err(e);
        }
    };
    if ok.weekday.is_some() && ok.day.is_none() {
        let wd = ok.weekday.unwrap();
        let rd_t = py.get_type::<crate::relativedelta_mod::Relativedelta>();
        let kw = PyDict::new(py);
        kw.set_item("weekday", wd)?;
        let rd = rd_t.call((), Some(&kw))?;
        let shifted = crate::util::operator_add_fn(py)?.call1((naive.bind(py), rd))?;
        naive = shifted.unbind();
    }
    Ok(naive)
}

fn tz_kwargs<'py>(
    py: Python<'py>,
    tzinfo: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let kw = PyDict::new(py);
    kw.set_item("tzinfo", tzinfo)?;
    Ok(kw)
}

fn info_utczone(
    _py: Python<'_>,
    info: &Bound<'_, PyAny>,
    fast: bool,
    name: &str,
) -> PyResult<bool> {
    if fast {
        return Ok(DefaultInfo.utczone(name));
    }
    info.call_method1("utczone", (name,))?
        .is_truthy()
}

fn assign_tzname(
    py: Python<'_>,
    dt: &Bound<'_, PyAny>,
    tzname: &Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    let cur = dt.call_method0("tzname")?;
    if !crate::util::py_eq(py, &cur, tzname)? {
        let one = 1i64.into_pyobject(py)?.into_any();
        let folded = crate::tz_mod::enfold(dt.clone(), Some(one))?;
        let cur2 = folded.bind(py).call_method0("tzname")?;
        if crate::util::py_eq(py, &cur2, tzname)? {
            return Ok(folded);
        }
    }
    Ok(dt.clone().unbind())
}

fn build_tzinfo(
    py: Python<'_>,
    tzdata: &Bound<'_, PyAny>,
    tzname: &Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    use pyo3::types::PyString;
    let dt_tzinfo = crate::util::tzinfo_cls(py)?;
    if tzdata.is_instance(dt_tzinfo)? || tzdata.is_none() {
        return Ok(tzdata.clone().unbind());
    }
    if tzdata.is_instance_of::<PyString>() {
        return Ok(crate::util::tzstr_cls(py)?.call1((tzdata,))?.unbind());
    }
    let long_t = crate::util::int_fn(py)?;
    if tzdata.is_instance(long_t)? {
        return Ok(crate::util::tzoffset_cls(py)?
            .call1((tzname, tzdata))?
            .unbind());
    }
    crate::util::type_error(
        "Offset must be tzinfo subclass, tz string, or int offset.".to_string(),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_tzaware(
    py: Python<'_>,
    naive: &Bound<'_, PyAny>,
    ok: &ParseOk,
    tzinfos: &Bound<'_, PyAny>,
    info: &Bound<'_, PyAny>,
    fast: bool,
) -> PyResult<PyObject> {
    use pyo3::types::PyString;
    let int_fn = crate::util::int_fn(py)?;
    let tzname_obj: PyObject = match &ok.tzname {
        Some(n) => PyString::new(py, n).into_any().unbind(),
        None => py.None(),
    };
    let tzoff_obj: PyObject = match &ok.tzoffset {
        Some(o) => int_fn.call1((o.as_str(),))?.unbind(),
        None => py.None(),
    };
    let tzn = tzname_obj.bind(py);
    if tzinfos.is_callable()
        || (tzinfos.is_truthy()? && tzinfos.contains(tzn)?)
    {
        let tzdata = if tzinfos.is_callable() {
            tzinfos.call1((tzn, tzoff_obj.bind(py)))?
        } else {
            tzinfos.call_method1("get", (tzn,))?
        };
        let tzinfo = build_tzinfo(py, &tzdata, tzn)?;
        let aware = naive.call_method("replace", (), Some(&tz_kwargs(py, tzinfo.bind(py))?))?;
        return assign_tzname(py, &aware, tzn);
    }
    if !tzn.is_truthy()? {
        // No tzname: offset alone decides below.
    } else if crate::util::time_mod(py)?.getattr("tzname")?.contains(tzn)? {
        let local = crate::util::tzlocal_cls(py)?.call0()?;
        let aware =
            naive.call_method("replace", (), Some(&tz_kwargs(py, &local)?))?;
        let aware = assign_tzname(py, &aware, tzn)?;
        let aware_b = aware.bind(py);
        let cur = aware_b.call_method0("tzname")?;
        if !crate::util::py_eq(py, &cur, tzn)?
            && info_utczone(py, info, fast, ok.tzname.as_deref().unwrap_or(""))?
        {
            let utc = crate::util::utc_obj(py)?;
            return Ok(aware_b
                .call_method("replace", (), Some(&tz_kwargs(py, utc)?))?
                .unbind());
        }
        return Ok(aware);
    }
    let off_zero = matches!(&ok.tzoffset, Some(o) if o.is_zero());
    let off_some = ok.tzoffset.is_some();
    if off_zero {
        let utc = crate::util::utc_obj(py)?;
        return Ok(naive
            .call_method("replace", (), Some(&tz_kwargs(py, utc)?))?
            .unbind());
    }
    if off_some {
        let off = crate::util::tzoffset_cls(py)?.call1((tzn, tzoff_obj.bind(py)))?;
        return Ok(naive
            .call_method("replace", (), Some(&tz_kwargs(py, &off)?))?
            .unbind());
    }
    if tzn.is_none() {
        return Ok(naive.clone().unbind());
    }
    let tzname_s: String = tzn.str()?.to_string();
    let warn_cls = crate::util::dateutil_parser_inner(py)?.getattr("UnknownTimezoneWarning")?;
    crate::util::warn_with(
        py,
        &warn_cls,
        &format!(
            "tzname {} identified but not understood.  Pass `tzinfos` argument in order to correctly return a timezone-aware datetime.  In a future version, this will raise an exception.",
            tzname_s
        ),
    )?;
    Ok(naive.clone().unbind())
}

// ---------------------------------------------------------------------------
// `parser`
// ---------------------------------------------------------------------------

/// `parser`: holds the `parserinfo` object; `parse`/`_parse` run the core
/// loop (fast path for the exact default type, callback bridge otherwise).
#[pyclass(name = "parser", subclass, dict, weakref, module = "dateutil._dateutil")]
pub struct Parser {
    info: PyObject,
}

#[allow(clippy::too_many_arguments)]
fn parse_impl(
    py: Python<'_>,
    info_obj: &Bound<'_, PyAny>,
    timestr: &Bound<'_, PyAny>,
    default: Option<Bound<'_, PyAny>>,
    ignoretz: &Option<Bound<'_, PyAny>>,
    tzinfos: Option<Bound<'_, PyAny>>,
    dayfirst: &Option<Bound<'_, PyAny>>,
    yearfirst: &Option<Bound<'_, PyAny>>,
    fuzzy: &Option<Bound<'_, PyAny>>,
    fuzzy_with_tokens: &Option<Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let s = normalize_input(py, timestr)?;
    let fwt = truthy(fuzzy_with_tokens)?;
    let fz = truthy(fuzzy)? || fwt;
    let fast = tables_pristine(py, info_obj)?;
    let (ok, info_for_tz) = if fast {
        let df = match dayfirst {
            Some(o) => o.is_truthy()?,
            None => info_obj.getattr("dayfirst")?.is_truthy()?,
        };
        let yf = match yearfirst {
            Some(o) => o.is_truthy()?,
            None => info_obj.getattr("yearfirst")?.is_truthy()?,
        };
        let year: i64 = info_obj.getattr("_year")?.extract()?;
        let century: i64 = info_obj.getattr("_century")?.extract()?;
        let info = DefaultInfo;
        let mut toks = Timelex::split(&s);
        let mut ok = parse_tokens(
            &mut toks,
            &info,
            &ParseOptions {
                dayfirst: df,
                yearfirst: yf,
                fuzzy: fz,
                cur_year: year,
                century,
            },
        )
        .map_err(|e| match e {
            Fail::NoResult | Fail::Aborted => {
                raise_parser_error(py, "Unknown string format: %s", timestr)
            }
            Fail::Empty => unreachable!("core never returns Empty"),
        })?;
        if ok.is_empty() {
            return Err(raise_parser_error(
                py,
                "String does not contain a date: %s",
                timestr,
            ));
        }
        validate_default(&mut ok, year, century);
        (ok, true)
    } else {
        let df = match dayfirst {
            Some(o) => o.is_truthy()?,
            None => info_obj.getattr("dayfirst")?.is_truthy()?,
        };
        let yf = match yearfirst {
            Some(o) => o.is_truthy()?,
            None => info_obj.getattr("yearfirst")?.is_truthy()?,
        };
        let bridge = PyInfo::new(py, info_obj.clone().unbind());
        let mut toks = Timelex::split(&s);
        let r = parse_tokens(
            &mut toks,
            &bridge,
            &ParseOptions {
                dayfirst: df,
                yearfirst: yf,
                fuzzy: fz,
                cur_year: 0,
                century: 0,
            },
        );
        if let Some(e) = bridge.take_stashed() {
            return Err(e);
        }
        let mut ok = r.map_err(|e| match e {
            Fail::NoResult | Fail::Aborted => {
                raise_parser_error(py, "Unknown string format: %s", timestr)
            }
            Fail::Empty => unreachable!("core never returns Empty"),
        })?;
        if ok.is_empty() {
            return Err(raise_parser_error(
                py,
                "String does not contain a date: %s",
                timestr,
            ));
        }
        let cls = py
            .import("dateutil.parser._parser")?
            .getattr("_result")?;
        let ns = cls.call0()?;
        fill_result_full(py, &ns, &ok)?;
        let valid = info_obj.call_method1("validate", (ns.clone(),))?;
        if !valid.is_truthy()? {
            return Err(raise_parser_error(
                py,
                "Unknown string format: %s",
                timestr,
            ));
        }
        read_result(py, &ns, &mut ok)?;
        (ok, false)
    };
    let default_obj = match default {
        Some(d) => d,
        None => {
            let dt = crate::util::datetime_cls(py)?;
            let now = dt.call_method0("now")?;
            let kw = PyDict::new(py);
            kw.set_item("hour", 0)?;
            kw.set_item("minute", 0)?;
            kw.set_item("second", 0)?;
            kw.set_item("microsecond", 0)?;
            now.call_method("replace", (), Some(&kw))?
        }
    };
    let naive = build_naive(py, &ok, &default_obj, timestr)?;
    let naive_b = naive.bind(py);
    let ret = if truthy(ignoretz)? {
        naive_b.clone().unbind()
    } else {
        let tz = match tzinfos {
            Some(t) => t,
            None => py.None().into_bound(py),
        };
        build_tzaware(py, naive_b, &ok, &tz, info_obj, info_for_tz)?
    };
    if fwt {
        let tup = pyo3::types::PyTuple::new(
            py,
            ok.skipped.iter().map(|t| t.as_str()),
        )?;
        Ok(pyo3::types::PyTuple::new(py, [ret.bind(py), tup.as_any()])?.into_any().unbind())
    } else {
        Ok(ret)
    }
}


fn fill_result_full(py: Python<'_>, ns: &Bound<'_, PyAny>, ok: &ParseOk) -> PyResult<()> {
    let int_fn = crate::util::int_fn(py)?;
    let pint = |s: &str| -> PyResult<PyObject> { Ok(int_fn.call1((s,))?.unbind()) };
    let opt_int = |v: &Option<String>| -> PyResult<PyObject> {
        match v {
            Some(s) => pint(s),
            None => Ok(py.None()),
        }
    };
    ns.setattr("year", opt_int(&ok.year)?)?;
    ns.setattr("month", opt_int(&ok.month)?)?;
    ns.setattr("day", opt_int(&ok.day)?)?;
    ns.setattr(
        "weekday",
        ok.weekday
            .map(|w| w.into_pyobject(py).map(|v| v.into_any().unbind()))
            .transpose()?
            .unwrap_or_else(|| py.None()),
    )?;
    ns.setattr("hour", opt_int(&ok.hour)?)?;
    ns.setattr("minute", opt_int(&ok.minute)?)?;
    ns.setattr("second", opt_int(&ok.second)?)?;
    ns.setattr(
        "microsecond",
        ok.micro
            .map(|u| pint(&u.to_string()))
            .transpose()?
            .unwrap_or_else(|| py.None()),
    )?;
    ns.setattr(
        "tzname",
        ok.tzname
            .clone()
            .map(|n| n.into_pyobject(py).map(|v| v.into_any().unbind()))
            .transpose()?
            .unwrap_or_else(|| py.None()),
    )?;
    ns.setattr(
        "tzoffset",
        ok.tzoffset
            .as_ref()
            .map(|o| pint(&o.as_str()))
            .transpose()?
            .unwrap_or_else(|| py.None()),
    )?;
    ns.setattr(
        "ampm",
        ok.ampm
            .map(|a| a.into_pyobject(py).map(|v| v.into_any().unbind()))
            .transpose()?
            .unwrap_or_else(|| py.None()),
    )?;
    ns.setattr("century_specified", ok.century_specified)?;
    Ok(())
}

fn read_result(py: Python<'_>, ns: &Bound<'_, PyAny>, ok: &mut ParseOk) -> PyResult<()> {
    let opt_str = |v: Bound<'_, PyAny>| -> PyResult<Option<String>> {
        if v.is_none() {
            return Ok(None);
        }
        Ok(Some(v.str()?.to_string()))
    };
    // Custom `validate` honors the int protocol; read back canonically.
    let canon = |v: Bound<'_, PyAny>| -> PyResult<Option<String>> {
        if v.is_none() {
            return Ok(None);
        }
        let int_fn = crate::util::int_fn(py)?;
        Ok(Some(int_fn.call1((v,))?.str()?.to_string()))
    };
    ok.year = canon(ns.getattr("year")?)?;
    ok.month = canon(ns.getattr("month")?)?;
    ok.day = canon(ns.getattr("day")?)?;
    ok.hour = canon(ns.getattr("hour")?)?;
    ok.minute = canon(ns.getattr("minute")?)?;
    ok.second = canon(ns.getattr("second")?)?;
    ok.micro = opt_str(ns.getattr("microsecond")?)?
        .map(|s| s.parse::<u32>())
        .transpose()
        .map_err(|_| {
            pyo3::exceptions::PyValueError::new_err("invalid microsecond".to_string())
        })?;
    ok.tzname = opt_str(ns.getattr("tzname")?)?;
    ok.tzoffset = canon(ns.getattr("tzoffset")?)?.map(|s| {
        if s.len() <= 18 {
            s.parse::<i64>().map(TzOff::Secs).unwrap_or(TzOff::Big(s))
        } else {
            TzOff::Big(s)
        }
    });
    ok.weekday = opt_str(ns.getattr("weekday")?)?
        .map(|s| s.parse::<i64>())
        .transpose()
        .map_err(|_| {
            pyo3::exceptions::PyValueError::new_err("invalid weekday".to_string())
        })?;
    ok.ampm = opt_str(ns.getattr("ampm")?)?
        .map(|s| s.parse::<i64>())
        .transpose()
        .map_err(|_| {
            pyo3::exceptions::PyValueError::new_err("invalid ampm".to_string())
        })?;
    ok.century_specified = ns.getattr("century_specified")?.is_truthy()?;
    Ok(())
}

#[pymethods]
impl Parser {
    #[new]
    #[pyo3(signature = (info=None))]
    fn new(py: Python<'_>, info: Option<Bound<'_, PyAny>>) -> PyResult<Self> {
        let info_obj = match info {
            Some(o) => o.unbind(),
            None => py.get_type::<ParserInfo>().call((), None)?.unbind(),
        };
        Ok(Parser { info: info_obj })
    }

    #[getter]
    fn info(&self, py: Python<'_>) -> PyObject {
        self.info.clone_ref(py)
    }

    #[setter]
    fn set_info(&mut self, v: PyObject) {
        self.info = v;
    }

    /// Accept-and-ignore `__init__` (state is built in `__new__`), so
    /// Python subclasses calling `super().__init__(...)` keep working.
    #[pyo3(signature = (*_args, **_kwargs))]
    fn __init__(
        &mut self,
        _args: Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (timestr, default=None, ignoretz=None, tzinfos=None, *, dayfirst=None, yearfirst=None, fuzzy=None, fuzzy_with_tokens=None))]
    fn parse(
        &self,
        py: Python<'_>,
        timestr: Bound<'_, PyAny>,
        default: Option<Bound<'_, PyAny>>,
        ignoretz: Option<Bound<'_, PyAny>>,
        tzinfos: Option<Bound<'_, PyAny>>,
        dayfirst: Option<Bound<'_, PyAny>>,
        yearfirst: Option<Bound<'_, PyAny>>,
        fuzzy: Option<Bound<'_, PyAny>>,
        fuzzy_with_tokens: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        parse_impl(
            py,
            self.info.bind(py),
            &timestr,
            default,
            &ignoretz,
            tzinfos,
            &dayfirst,
            &yearfirst,
            &fuzzy,
            &fuzzy_with_tokens,
        )
    }

    #[pyo3(signature = (timestr, dayfirst=None, yearfirst=None, fuzzy=None, fuzzy_with_tokens=None))]
    fn _parse(
        &self,
        py: Python<'_>,
        timestr: Bound<'_, PyAny>,
        dayfirst: Option<Bound<'_, PyAny>>,
        yearfirst: Option<Bound<'_, PyAny>>,
        fuzzy: Option<Bound<'_, PyAny>>,
        fuzzy_with_tokens: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let s = normalize_input(py, &timestr)?;
        let fwt = truthy(&fuzzy_with_tokens)?;
        let fz = truthy(&fuzzy)? || fwt;
        let info_obj = self.info.bind(py);
        let fast = tables_pristine(py, info_obj)?;
        let df = match &dayfirst {
            Some(o) => o.is_truthy()?,
            None => info_obj.getattr("dayfirst")?.is_truthy()?,
        };
        let yf = match &yearfirst {
            Some(o) => o.is_truthy()?,
            None => info_obj.getattr("yearfirst")?.is_truthy()?,
        };
        // `_parse` returns `(res, skipped)` with `(None, None)` on failure
        // and `skipped=None` unless `fuzzy_with_tokens` (unlike `parse`,
        // it never raises `ParserError` itself).
        let pair = |py: Python<'_>, res: PyObject, skipped: PyObject| -> PyResult<PyObject> {
            Ok(pyo3::types::PyTuple::new(py, [res, skipped])?
                .into_any()
                .unbind())
        };
        let none_pair = || -> PyResult<PyObject> {
            let n = py.None();
            pair(py, n.clone_ref(py), n)
        };
        let fill_pair = |py: Python<'_>, ok: &ParseOk| -> PyResult<PyObject> {
            let cls = py
                .import("dateutil.parser._parser")?
                .getattr("_result")?;
            let ns = cls.call0()?;
            fill_result_full(py, &ns, ok)?;
            let skips: PyObject = if fwt {
                pyo3::types::PyTuple::new(py, ok.skipped.iter().map(|t| t.as_str()))?
                    .into_any()
                    .unbind()
            } else {
                py.None()
            };
            pair(py, ns.unbind(), skips)
        };
        if fast {
            let year: i64 = info_obj.getattr("_year")?.extract()?;
            let century: i64 = info_obj.getattr("_century")?.extract()?;
            let info = DefaultInfo;
            let mut toks = Timelex::split(&s);
            let r = parse_tokens(
                &mut toks,
                &info,
                &ParseOptions {
                    dayfirst: df,
                    yearfirst: yf,
                    fuzzy: fz,
                    cur_year: year,
                    century,
                },
            );
            let mut ok = match r {
                Ok(v) => v,
                Err(_) => return none_pair(),
            };
            validate_default(&mut ok, year, century);
            return fill_pair(py, &ok);
        }
        let bridge = PyInfo::new(py, self.info.clone_ref(py));
        let mut toks = Timelex::split(&s);
        let r = parse_tokens(
            &mut toks,
            &bridge,
            &ParseOptions {
                dayfirst: df,
                yearfirst: yf,
                fuzzy: fz,
                cur_year: 0,
                century: 0,
            },
        );
        if let Some(e) = bridge.take_stashed() {
            return Err(e);
        }
        let mut ok = match r {
            Ok(v) => v,
            Err(_) => return none_pair(),
        };
        let cls = py
            .import("dateutil.parser._parser")?
            .getattr("_result")?;
        let ns = cls.call0()?;
        fill_result_full(py, &ns, &ok)?;
        let valid = info_obj.call_method1("validate", (ns.clone(),))?;
        if !valid.is_truthy()? {
            return none_pair();
        }
        read_result(py, &ns, &mut ok)?;
        fill_pair(py, &ok)
    }
}

// ---------------------------------------------------------------------------
// `_timelex`
// ---------------------------------------------------------------------------

/// Stateful lexer object (mirrors `_timelex`; the hot path uses
/// [`Timelex`] directly, this preserves the deprecated private interface
/// including `instream`/`charstack`/`tokenstack`/`eof` attributes).
#[pyclass(name = "_timelex", subclass, dict, weakref, module = "dateutil._dateutil")]
pub struct TimelexObj {
    lx: Timelex,
    instream: PyObject,
}

#[pymethods]
impl TimelexObj {
    #[new]
    fn new(py: Python<'_>, instream: Bound<'_, PyAny>) -> PyResult<Self> {
        use pyo3::types::{PyBytes, PyString};
        let bytearray_t = crate::util::bytearray_cls(py)?;
        if instream.is_instance_of::<PyString>() {
            let text: String = instream.extract()?;
            let sio = crate::util::stringio_cls(py)?.call1((text.clone(),))?;
            return Ok(TimelexObj {
                lx: Timelex::new(&text),
                instream: sio.unbind(),
            });
        }
        if instream.is_instance_of::<PyBytes>() || instream.is_instance(bytearray_t)? {
            let text: String = instream.call_method0("decode")?.extract()?;
            let sio = crate::util::stringio_cls(py)?.call1((text.clone(),))?;
            return Ok(TimelexObj {
                lx: Timelex::new(&text),
                instream: sio.unbind(),
            });
        }
        if instream.hasattr("read")? {
            let text = normalize_input(py, &instream)?;
            return Ok(TimelexObj {
                lx: Timelex::new(&text),
                instream: instream.unbind(),
            });
        }
        crate::util::type_error(type_error_name(&instream)?)
    }

    /// Accept-and-ignore `__init__` so Python subclasses calling
    /// `super().__init__(*args, **kwargs)` (like the deprecation wrapper
    /// in `dateutil.parser`) keep working; state is built in `__new__`.
    #[pyo3(signature = (*_args, **_kwargs))]
    fn __init__(
        &mut self,
        _args: Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Ok(())
    }

    fn get_token(&mut self) -> Option<String> {
        self.lx.get_token()
    }

    fn __next__(&mut self) -> Option<String> {
        self.lx.get_token()
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn next(&mut self) -> Option<String> {
        self.lx.get_token()
    }

    #[classmethod]
    fn split(_cls: &Bound<'_, PyType>, py: Python<'_>, s: Bound<'_, PyAny>) -> PyResult<Vec<String>> {
        Ok(Timelex::split(&normalize_input(py, &s)?))
    }

    #[classmethod]
    fn isword(_cls: &Bound<'_, PyType>, c: &str) -> bool {
        use dateutil_core::parser::is_word;
        !c.is_empty() && c.chars().all(is_word)
    }

    #[classmethod]
    fn isnum(_cls: &Bound<'_, PyType>, c: &str) -> bool {
        use dateutil_core::parser::is_num;
        !c.is_empty() && c.chars().all(is_num)
    }

    #[classmethod]
    fn isspace(_cls: &Bound<'_, PyType>, c: &str) -> bool {
        use dateutil_core::parser::is_space;
        !c.is_empty() && c.chars().all(is_space)
    }

    #[getter]
    fn instream(&self, py: Python<'_>) -> PyObject {
        self.instream.clone_ref(py)
    }

    #[getter]
    fn charstack(&self) -> Vec<String> {
        self.lx.charstack()
    }

    #[getter]
    fn tokenstack(&self) -> Vec<String> {
        self.lx.tokenstack()
    }

    #[getter]
    fn eof(&self) -> bool {
        self.lx.is_eof()
    }
}

// ---------------------------------------------------------------------------
// Module function + registration
// ---------------------------------------------------------------------------

/// Module-level `parse` (uses `DEFAULTPARSER` unless `parserinfo` is given).
#[pyfunction(name = "parse")]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (timestr, parserinfo=None, *, default=None, ignoretz=None, tzinfos=None, dayfirst=None, yearfirst=None, fuzzy=None, fuzzy_with_tokens=None))]
fn parse_fn(
    py: Python<'_>,
    timestr: Bound<'_, PyAny>,
    parserinfo: Option<Bound<'_, PyAny>>,
    default: Option<Bound<'_, PyAny>>,
    ignoretz: Option<Bound<'_, PyAny>>,
    tzinfos: Option<Bound<'_, PyAny>>,
    dayfirst: Option<Bound<'_, PyAny>>,
    yearfirst: Option<Bound<'_, PyAny>>,
    fuzzy: Option<Bound<'_, PyAny>>,
    fuzzy_with_tokens: Option<Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    match parserinfo {
        None => {
            let dp = crate::util::defaultparser_obj(py)?;
            let meth = dp.getattr("parse")?;
            let kw = PyDict::new(py);
            if let Some(v) = default {
                kw.set_item("default", v)?;
            }
            if let Some(v) = ignoretz {
                kw.set_item("ignoretz", v)?;
            }
            if let Some(v) = tzinfos {
                kw.set_item("tzinfos", v)?;
            }
            if let Some(v) = dayfirst {
                kw.set_item("dayfirst", v)?;
            }
            if let Some(v) = yearfirst {
                kw.set_item("yearfirst", v)?;
            }
            if let Some(v) = fuzzy {
                kw.set_item("fuzzy", v)?;
            }
            if let Some(v) = fuzzy_with_tokens {
                kw.set_item("fuzzy_with_tokens", v)?;
            }
            Ok(meth.call((timestr,), Some(&kw))?.unbind())
        }
        Some(pi) => {
            let p = Parser { info: pi.unbind() };
            p.parse(
                py,
                timestr,
                default,
                ignoretz,
                tzinfos,
                dayfirst,
                yearfirst,
                fuzzy,
                fuzzy_with_tokens,
            )
        }
    }
}

/// Register classes, functions, default tables and `DEFAULTPARSER`.
pub fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ParserInfo>()?;
    m.add_class::<Parser>()?;
    m.add_class::<TimelexObj>()?;
    m.add_function(wrap_pyfunction!(parse_fn, m)?)?;
    let t = py.get_type::<ParserInfo>();
    t.setattr(
        "JUMP",
        vec![
            " ", ".", ",", ";", "-", "/", "'", "at", "on", "and", "ad", "m", "t", "of", "st",
            "nd", "rd", "th",
        ],
    )?;
    t.setattr(
        "WEEKDAYS",
        vec![
            ("Mon", "Monday"),
            ("Tue", "Tuesday"),
            ("Wed", "Wednesday"),
            ("Thu", "Thursday"),
            ("Fri", "Friday"),
            ("Sat", "Saturday"),
            ("Sun", "Sunday"),
        ],
    )?;
    // Heterogeneous arities do not mix in one `vec!`; build the Python
    // list explicitly like the original nested tuples.
    let months = pyo3::types::PyList::empty(py);
    for tup in [
        vec!["Jan", "January"],
        vec!["Feb", "February"],
        vec!["Mar", "March"],
        vec!["Apr", "April"],
        vec!["May", "May"],
        vec!["Jun", "June"],
        vec!["Jul", "July"],
        vec!["Aug", "August"],
        vec!["Sep", "Sept", "September"],
        vec!["Oct", "October"],
        vec!["Nov", "November"],
        vec!["Dec", "December"],
    ] {
        months.append(pyo3::types::PyTuple::new(py, tup)?)?;
    }
    t.setattr("MONTHS", months)?;
    t.setattr(
        "HMS",
        vec![
            ("h", "hour", "hours"),
            ("m", "minute", "minutes"),
            ("s", "second", "seconds"),
        ],
    )?;
    t.setattr("AMPM", vec![("am", "a"), ("pm", "p")])?;
    t.setattr("UTCZONE", vec!["UTC", "GMT", "Z", "z"])?;
    t.setattr("PERTAIN", vec!["of"])?;
    t.setattr("TZOFFSET", PyDict::new(py))?;
    let mut pris_vec: Vec<PyObject> = Vec::with_capacity(8);
    for n in [
        "JUMP", "WEEKDAYS", "MONTHS", "HMS", "AMPM", "UTCZONE", "PERTAIN", "TZOFFSET",
    ] {
        pris_vec.push(t.getattr(n)?.unbind());
    }
    t.setattr("_rust_pristine", PyTuple::new(py, pris_vec)?)?;
    let info = py.get_type::<ParserInfo>().call((), None)?;
    let parser = py.get_type::<Parser>().call1((info,))?;
    m.add("DEFAULTPARSER", parser)?;
    Ok(())
}