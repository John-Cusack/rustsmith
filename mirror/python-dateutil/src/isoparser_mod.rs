// SPDX-License-Identifier: Apache-2.0
//! `isoparser` binding: byte-level ISO parsing in `dateutil-core`, object
//! construction here so CPython range messages stay exact.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use dateutil_core::parser::isoparser::{
    parse_date_only, parse_dt, parse_time_only, parse_tz_only, IsoFail, IsoTz,
};

fn iso_err(py: Python<'_>, e: IsoFail) -> PyErr {
    match e {
        IsoFail::Int(slice) => {
            // Raise via CPython's own `int()` for the exact message.
            let b = pyo3::types::PyBytes::new(py, &slice);
            match py
                .import("builtins")
                .and_then(|m| m.getattr("int"))
                .and_then(|f| f.call1((b,)))
            {
                Ok(_) => pyo3::exceptions::PyValueError::new_err("invalid literal".to_string()),
                Err(err) => err,
            }
        }
        IsoFail::Msg(m) => pyo3::exceptions::PyValueError::new_err(m),
        IsoFail::Overflow => {
            pyo3::exceptions::PyOverflowError::new_err("date value out of range".to_string())
        }
        IsoFail::Year(y) => {
            pyo3::exceptions::PyValueError::new_err(format!("year {} is out of range", y))
        }
    }
}

/// `_takes_ascii` normalization: streams via `.read()`, `str` via ASCII
/// encode (chained `ValueError` on non-ASCII), bytes/bytearray as-is.
fn ascii_bytes(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    use pyo3::types::{PyByteArray, PyBytes, PyString};
    if let Ok(read) = obj.getattr("read") {
        let data = read.call0()?;
        return ascii_bytes(py, &data);
    }
    if obj.is_instance_of::<PyString>() {
        let s: String = obj.extract()?;
        let py_s = PyString::new(py, &s);
        match py_s.call_method1("encode", ("ascii",)) {
            Ok(b) => return b.extract::<Vec<u8>>(),
            Err(e) => {
                let ve = pyo3::exceptions::PyValueError::new_err(
                    "ISO-8601 strings should contain only ASCII characters".to_string(),
                );
                ve.set_cause(py, Some(e));
                return Err(ve);
            }
        }
    }
    if obj.is_instance_of::<PyBytes>() {
        return obj.extract::<Vec<u8>>();
    }
    if obj.is_instance_of::<PyByteArray>() {
        return obj.extract::<Vec<u8>>();
    }
    // Mirror the original's downstream `TypeError` for unsized input.
    let _ = obj.len()?;
    Err(pyo3::exceptions::PyTypeError::new_err(
        "expected string or bytes-like object".to_string(),
    ))
}

fn tz_object(py: Python<'_>, tz: IsoTz) -> PyResult<PyObject> {
    match tz {
        IsoTz::None => Ok(py.None()),
        IsoTz::Utc => Ok(py.import("dateutil.tz")?.getattr("UTC")?.unbind()),
        IsoTz::Offset(secs) => Ok(py
            .import("dateutil.tz")?
            .getattr("tzoffset")?
            .call1((py.None(), secs))?
            .unbind()),
    }
}

/// `isoparser(sep=None)`: strict ISO-8601 parser.
#[pyclass(name = "isoparser", subclass, dict, weakref, module = "dateutil._dateutil")]
pub struct Isoparser {
    sep: Option<u8>,
}

#[pymethods]
impl Isoparser {
    #[new]
    #[pyo3(signature = (sep=None))]
    fn new(py: Python<'_>, sep: Option<Bound<'_, PyAny>>) -> PyResult<Self> {
        let s = match sep {
            None => None,
            Some(o) if o.is_none() => None,
            Some(o) => {
                let n = o.len()?;
                if n != 1 {
                    return crate::util::value_error(
                        "Separator must be a single, non-numeric ASCII character".to_string(),
                    );
                }
                if !o.is_instance_of::<pyo3::types::PyString>() {
                    // Mirrors `ord(sep)` raising `TypeError` for non-str.
                    py.import("builtins")?.getattr("ord")?.call1((o.clone(),))?;
                    return crate::util::value_error(
                        "Separator must be a single, non-numeric ASCII character".to_string(),
                    );
                }
                let text: String = o.extract()?;
                let c = text.chars().next().unwrap();
                if !c.is_ascii() || c.is_ascii_digit() {
                    return crate::util::value_error(
                        "Separator must be a single, non-numeric ASCII character".to_string(),
                    );
                }
                Some(c as u8)
            }
        };
        Ok(Isoparser { sep: s })
    }

    /// Accept-and-ignore `__init__` (state is built in `__new__`), so
    /// Python subclasses calling `super().__init__(...)` keep working.
    #[pyo3(signature = (*_args, **_kwargs))]
    fn __init__(
        &mut self,
        _args: Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        Ok(())
    }

    fn isoparse(&self, py: Python<'_>, dt_str: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let b = ascii_bytes(py, &dt_str)?;
        let r = parse_dt(&b, self.sep).map_err(|e| iso_err(py, e))?;
        let dt = py.import("datetime")?.getattr("datetime")?;
        let tz = tz_object(py, r.tz)?;
        let kw = PyDict::new(py);
        kw.set_item("tzinfo", tz)?;
        let mut obj = dt
            .call(
                (
                    r.date.y as i64,
                    r.date.mo as i64,
                    r.date.d as i64,
                    r.time.h as i64,
                    r.time.mi as i64,
                    r.time.s as i64,
                    r.time.us as i64,
                ),
                Some(&kw),
            )?
            .unbind();
        if r.midnight24 {
            let one = py
                .import("datetime")?
                .getattr("timedelta")?
                .call1((1,))?;
            obj = py
                .import("operator")?
                .getattr("add")?
                .call1((obj.bind(py), one))?
                .unbind();
        }
        Ok(obj)
    }

    fn parse_isodate(&self, py: Python<'_>, datestr: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let b = ascii_bytes(py, &datestr)?;
        let (c, pos) = parse_date_only(&b).map_err(|e| iso_err(py, e))?;
        if pos < b.len() {
            // Exact message via Python `repr` of the ASCII-decoded string.
            let decoded = String::from_utf8_lossy(&b).to_string();
            let rep = pyo3::types::PyString::new(py, &decoded)
                .call_method0("__repr__")?;
            return crate::util::value_error(format!(
                "String contains unknown ISO components: {}",
                rep
            ));
        }
        crate::util::make_date(py, c.y as i64, c.mo as i64, c.d as i64)
    }

    fn parse_isotime(&self, py: Python<'_>, timestr: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let b = ascii_bytes(py, &timestr)?;
        let (t, tz) = parse_time_only(&b).map_err(|e| iso_err(py, e))?;
        let tzo = tz_object(py, tz)?;
        // `parse_isotime` folds 24:00 to midnight (unlike `isoparse`,
        // which rolls to the next day).
        let h = if t.h == 24 { 0 } else { t.h };
        Ok(py
            .import("datetime")?
            .getattr("time")?
            .call1((
                h as i64,
                t.mi as i64,
                t.s as i64,
                t.us as i64,
                tzo,
            ))?
            .unbind())
    }

    #[pyo3(signature = (tzstr, zero_as_utc=None))]
    fn parse_tzstr(
        &self,
        py: Python<'_>,
        tzstr: Bound<'_, PyAny>,
        zero_as_utc: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let b = ascii_bytes(py, &tzstr)?;
        // Omitted flag defaults to `True` (explicit `None` is falsy upstream,
        // but no caller passes it; documented in the seam note).
        let zau = match &zero_as_utc {
            None => true,
            Some(o) => o.is_truthy()?,
        };
        let tz = parse_tz_only(&b, zau).map_err(|e| iso_err(py, e))?;
        tz_object(py, tz)
    }
}

/// Register `isoparser`, `DEFAULT_ISOPARSER` and the bound `isoparse`.
pub fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Isoparser>()?;
    let default = py.get_type::<Isoparser>().call((), None)?;
    let bound = default.getattr("isoparse")?;
    m.add("DEFAULT_ISOPARSER", default)?;
    m.add("isoparse", bound)?;
    Ok(())
}
