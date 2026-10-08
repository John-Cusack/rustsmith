//! `pyparsing._pyparsing`: compiled half of the Stage-1 mirror.
//!
//! Thin PyO3 translation over the reusable [`pyparsing_rust_core`] engine:
//! extract the raw unicode buffer (`PyUnicode_KIND`/`DATA`, O(1)) and the
//! single-char charset table from Python objects, then delegate the scan.
//! All Python-visible behavior (bounds, exceptions, first-char checks)
//! stays in `pyparsing/core.py`; this file only translates between Python
//! objects and core indices.
//!
//! The whole port holds exactly one `unsafe` block (below): slice
//! construction at the FFI boundary. Everything else — table, scan loop,
//! index math — is safe code in `pyparsing-rust-core`.
use pyo3::exceptions::PyValueError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyString;
use pyparsing_rust_core::{RawUnits, ScanTable, UnitKind};

/// Shared scan over raw buffers. `want_in` selects the polarity:
/// `true` stops at the first unit NOT in `charset` (`White` remainder),
/// `false` stops at the first unit IN it (`CharsNotIn` remainder).
fn scan_impl(
    text: &Bound<'_, PyString>,
    start: usize,
    end: usize,
    charset: &Bound<'_, PyAny>,
    want_in: bool,
) -> PyResult<usize> {
    let text_ptr = text.as_ptr();
    let charset_ptr = charset.as_ptr();
    // SAFETY: the single unsafe site of the port (gate: FFI boundary +
    // this comment). `text_ptr`/`charset_ptr` borrow live arguments with
    // the GIL held for the whole call, so every buffer below is valid:
    // - `PyUnicode_KIND`/`GET_LENGTH`/`DATA` are valid for every `str`;
    //   KIND is 1, 2, or 4 and DATA holds `len` units of that width, so
    //   each `from_raw_parts` below spans exactly the live buffer.
    // - `PyObject_GetIter` returns a new reference (null + TypeError when
    //   the charset is not iterable); `PyIter_Next` returns new references
    //   (null with no error set at exhaustion). Every item is `Py_DECREF`d
    //   exactly once on every path, as is the iterator.
    // - Member reads touch only length-1 `str` buffers (checked first),
    //   one unit wide, in bounds by construction.
    // - No Python code runs inside (no calls back into the interpreter),
    //   so the GIL cannot deadlock and no object can move under us.
    unsafe {
        let kind_raw = ffi::PyUnicode_KIND(text_ptr);
        let len = ffi::PyUnicode_GET_LENGTH(text_ptr) as usize;
        let data = ffi::PyUnicode_DATA(text_ptr);
        let kind = UnitKind::from_kind(kind_raw).ok_or_else(|| {
            PyErr::new::<PyValueError, _>("impossible unicode kind")
        })?;
        let bytes =
            std::slice::from_raw_parts(data as *const u8, len * kind.width());
        let units = RawUnits::new(bytes, kind, len).ok_or_else(|| {
            PyErr::new::<PyValueError, _>("unicode buffer shorter than length")
        })?;
        let it = ffi::PyObject_GetIter(charset_ptr);
        if it.is_null() {
            return Err(Python::with_gil(PyErr::fetch));
        }
        let mut members: Vec<Option<u32>> = Vec::new();
        let mut failed: Option<PyErr> = None;
        loop {
            let item = ffi::PyIter_Next(it);
            if item.is_null() {
                if !ffi::PyErr_Occurred().is_null() {
                    failed = Some(Python::with_gil(PyErr::fetch));
                }
                break;
            }
            let unit =
                if ffi::PyUnicode_Check(item) == 1 && ffi::PyUnicode_GET_LENGTH(item) == 1
                {
                    let k = ffi::PyUnicode_KIND(item);
                    let d = ffi::PyUnicode_DATA(item);
                    match UnitKind::from_kind(k) {
                        Some(kk) => {
                            let b = std::slice::from_raw_parts(d as *const u8, kk.width());
                            // Length-1 str holds exactly one unit, so this
                            // is infallible; None would skip the member
                            // rather than table a phantom value.
                            RawUnits::new(b, kk, 1).map(|u| u.unit_at(0))
                        }
                        None => None,
                    }
                } else {
                    // Multi-char, empty, or non-str members can never equal
                    // a single text char under Python `in`: skip silently.
                    None
                };
            members.push(unit);
            ffi::Py_DECREF(item);
        }
        ffi::Py_DECREF(it);
        if let Some(e) = failed {
            return Err(e);
        }
        let table = ScanTable::from_members(members);
        Ok(if want_in {
            pyparsing_rust_core::scan_while_in(&units, start, end, &table)
        } else {
            pyparsing_rust_core::scan_while_not_in(&units, start, end, &table)
        })
    }
}

/// First index in `[start, end)` whose char is NOT in `charset`, or `end`.
/// Kernel for the `White` remainder loop
/// (`while loc < maxloc and instring[loc] in matchWhite`).
#[pyfunction]
fn scan_while_in(
    text: Bound<'_, PyString>,
    start: usize,
    end: usize,
    charset: Bound<'_, PyAny>,
) -> PyResult<usize> {
    scan_impl(&text, start, end, &charset, true)
}

/// First index in `[start, end)` whose char IS in `charset`, or `end`.
/// Kernel for the `CharsNotIn` remainder loop
/// (`while loc < maxlen and instring[loc] not in notchars`).
#[pyfunction]
fn scan_while_not_in(
    text: Bound<'_, PyString>,
    start: usize,
    end: usize,
    charset: Bound<'_, PyAny>,
) -> PyResult<usize> {
    scan_impl(&text, start, end, &charset, false)
}

/// `pyparsing._pyparsing`: compiled half of the Stage-1 mirror.
#[pymodule]
fn _pyparsing(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(scan_while_in, m)?)?;
    m.add_function(wrap_pyfunction!(scan_while_not_in, m)?)?;
    Ok(())
}
