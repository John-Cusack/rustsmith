//!
//! Thin PyO3 binding over the independent `multipart-rust-core` crate: same
//! module boundary (`python_multipart._python_multipart`), same public names,
//! same error messages and offsets. All byte-loop logic lives in
//! `multipart_core`; this file only translates between Python objects and
//! core values, and dispatches core events through the live Python
//! `callback()`/`logger` objects.
//!
//! License: Apache-2.0 (preserved from the original).

use multipart_core::{Emitter, ParseFail, WriteErr};
use pyo3::exceptions::PyAssertionError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};

/// Forward core events to the live Python parser (`callback`) and logger.
/// `input` is the original input object (bytes/bytearray: same identity the
/// original passes around); `boundary` is the framed boundary bytes object.
struct PyEmit<'py> {
    py: Python<'py>,
    parser: Bound<'py, PyAny>,
    logger: Bound<'py, PyAny>,
    input: Bound<'py, PyAny>,
    boundary: Bound<'py, PyAny>,
}

impl Emitter for PyEmit<'_> {
    type Err = PyErr;

    fn event(&mut self, name: &str) -> Result<(), PyErr> {
        self.parser.call_method1("callback", (name,))?;
        Ok(())
    }

    fn input(&mut self, name: &str, start: usize, end: usize) -> Result<(), PyErr> {
        self.parser
            .call_method1("callback", (name, &self.input, start, end))?;
        Ok(())
    }

    fn boundary(&mut self, name: &str, end: usize) -> Result<(), PyErr> {
        self.parser
            .call_method1("callback", (name, &self.boundary, 0, end))?;
        Ok(())
    }

    fn owned(&mut self, name: &str, buf: &[u8]) -> Result<(), PyErr> {
        let b = PyBytes::new(self.py, buf);
        let n = buf.len();
        self.parser.call_method1("callback", (name, b, 0, n))?;
        Ok(())
    }

    fn warn(&mut self, msg: &str) -> Result<(), PyErr> {
        self.logger.call_method1("warning", (msg,))?;
        Ok(())
    }
}

/// Raise the matching Python exception class with the core offset.
fn parse_pyerr(
    py: Python<'_>,
    fail: &ParseFail,
) -> PyErr {
    let cls_name = if fail.multipart {
        "MultipartParseError"
    } else {
        "QuerystringParseError"
    };
    match py
        .import("python_multipart.exceptions")
        .and_then(|m| m.getattr(cls_name))
    {
        Ok(cls) => {
            let kwargs = PyDict::new(py);
            // `offset` kwarg mirrors `ParseError(message, *, offset=...)`.
            if kwargs.set_item("offset", fail.offset).is_err() {
                return PyAssertionError::new_err("offset kwarg failed");
            }
            match cls.call((fail.message.clone(),), Some(&kwargs)) {
                Ok(obj) => PyErr::from_value(obj),
                Err(e) => e,
            }
        }
        Err(e) => e,
    }
}

/// Core half of `MultipartParser`: evolving state only. Config (framed
/// boundary, header limits) arrives per `internal_write` call from the live
/// Python attributes, so post-init mutation behaves exactly like the
/// original.
#[pyclass(name = "_MultipartCore", module = "python_multipart._python_multipart")]
struct MultipartCoreRs {
    inner: multipart_core::multipart::MultipartCore,
}

#[pymethods]
impl MultipartCoreRs {
    #[new]
    fn new() -> Self {
        MultipartCoreRs {
            inner: multipart_core::multipart::MultipartCore::new(),
        }
    }

    /// Current state as the `MultipartState` member (enum call returns the
    /// singleton, so `is` identity holds).
    #[getter]
    fn state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("python_multipart.multipart")?
            .getattr("MultipartState")?
            .call1((self.inner.state,))
    }

    #[setter]
    fn set_state(&mut self, value: Bound<PyAny>) -> PyResult<()> {
        let v: i64 = value.extract()?;
        self.inner.state = v;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn internal_write(
        &mut self,
        py: Python<'_>,
        input: Bound<PyAny>,
        length: usize,
        boundary_obj: Bound<PyAny>,
        max_header_count: u64,
        max_header_size: u64,
        parser: Bound<PyAny>,
        logger: Bound<PyAny>,
    ) -> PyResult<usize> {
        // `bytes` borrows in place; `bytearray` snapshots once per call
        // (the original object still rides to the callbacks, as upstream).
        let cow: std::borrow::Cow<'_, [u8]> = input.extract()?;
        let data: &[u8] = &cow;
        let boundary_cow: std::borrow::Cow<'_, [u8]> = boundary_obj.extract()?;
        let boundary: &[u8] = &boundary_cow;
        let mut emit = PyEmit {
            py,
            parser,
            logger,
            input: input.clone(),
            boundary: boundary_obj.clone(),
        };
        match self.inner.internal_write(
            data,
            length,
            boundary,
            max_header_count,
            max_header_size,
            &mut emit,
        ) {
            Ok(n) => Ok(n),
            Err(WriteErr::Parse(f)) => Err(parse_pyerr(py, &f)),
            Err(WriteErr::Callback(e)) => Err(e),
        }
    }
}

/// Core half of `QuerystringParser`: evolving state only.
#[pyclass(name = "_QuerystringCore", module = "python_multipart._python_multipart")]
struct QuerystringCoreRs {
    inner: multipart_core::querystring::QuerystringCore,
}

#[pymethods]
impl QuerystringCoreRs {
    #[new]
    fn new() -> Self {
        QuerystringCoreRs {
            inner: multipart_core::querystring::QuerystringCore::new(),
        }
    }

    /// Current state as the `QuerystringState` member (singleton identity).
    #[getter]
    fn state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("python_multipart.multipart")?
            .getattr("QuerystringState")?
            .call1((self.inner.state,))
    }

    #[setter]
    fn set_state(&mut self, value: Bound<PyAny>) -> PyResult<()> {
        let v: i64 = value.extract()?;
        self.inner.state = v;
        Ok(())
    }

    fn internal_write(
        &mut self,
        py: Python<'_>,
        input: Bound<PyAny>,
        length: usize,
        strict: bool,
        parser: Bound<PyAny>,
        logger: Bound<PyAny>,
    ) -> PyResult<usize> {
        let cow: std::borrow::Cow<'_, [u8]> = input.extract()?;
        let data: &[u8] = &cow;
        // Querystring events reference the input chunk only; the boundary
        // slot reuses the input object (never addressed by this parser).
        let mut emit = PyEmit {
            py,
            parser,
            logger,
            input: input.clone(),
            boundary: input.clone(),
        };
        match self
            .inner
            .internal_write(data, length, strict, &mut emit)
        {
            Ok(n) => Ok(n),
            Err(WriteErr::Parse(f)) => Err(parse_pyerr(py, &f)),
            Err(WriteErr::Callback(e)) => Err(e),
        }
    }
}

/// `True` for the `bytes` type (subclasses included); `bytearray` is not
/// `bytes`, matching the original's `isinstance(value, bytes)` gate.
fn is_bytes_instance(obj: &Bound<PyAny>) -> bool {
    obj.is_instance_of::<PyBytes>()
}

/// `parse_options_header(value)`: type dispatch and latin-1 codec behavior
/// run through CPython itself, so `UnicodeEncodeError`/`AssertionError`
/// identity is exact; the splitting rules live in the core.
#[pyfunction]
fn parse_options_header<'py>(py: Python<'py>, value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if !value.is_truthy()? {
        let empty = PyBytes::new(py, b"");
        let opts = PyDict::new(py);
        let tup = (empty, opts).into_pyobject(py)?;
        return Ok(tup.into_any());
    }
    let text: String = if is_bytes_instance(&value) {
        let b: &[u8] = value.extract()?;
        b.iter().map(|&c| c as char).collect()
    } else if value.is_instance_of::<PyString>() {
        value.extract()?
    } else {
        return Err(PyAssertionError::new_err(
            "Value should be a string by now",
        ));
    };
    let (ctype, pairs) = multipart_core::headers::parse_options_header_str(&text);
    // Encode each piece through CPython in original order (key, value per
    // segment, ctype last) so the first failing piece raises exactly like
    // the original.
    let opts = PyDict::new(py);
    for (k, v) in &pairs {
        let kb = PyString::new(py, k).call_method1("encode", ("latin-1",))?;
        let vb = PyString::new(py, v).call_method1("encode", ("latin-1",))?;
        opts.set_item(kb, vb)?;
    }
    let cb = PyString::new(py, &ctype).call_method1("encode", ("latin-1",))?;
    let tup = (cb, opts).into_pyobject(py)?;
    Ok(tup.into_any())
}

/// `_parseparam(s)`: the `;`-split helper, core-owned.
#[pyfunction]
fn parseparam(s: &str) -> Vec<String> {
    multipart_core::headers::parseparam(s)
}

/// `python_multipart._python_multipart`: compiled half of the Stage-1 mirror.
#[pymodule]
fn _python_multipart(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MultipartCoreRs>()?;
    m.add_class::<QuerystringCoreRs>()?;
    m.add_function(wrap_pyfunction!(parse_options_header, m)?)?;
    m.add_function(wrap_pyfunction!(parseparam, m)?)?;
    Ok(())
}
