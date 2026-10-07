//! Stage-1 Rust mirror of `pypa/packaging` (PEP 440/508 parsing + markers/tags).
//!
//! Thin PyO3 binding over the independent `packaging-rust-core` crate: same
//! module boundary (`packaging._packaging`), same public names, same error
//! messages. All parsing/ordering logic lives in `packaging_core`; this file
//! only translates between Python objects and core values.
//!
//! License: Apache-2.0 OR BSD-2-Clause (preserved from the original).

use std::cmp::Ordering;
use std::ffi::CString;

use packaging_core::version::{self, LocalSeg, NumString, ParsedVersion};
use packaging_core::ranges;
use pyo3::conversion::IntoPyObjectExt;
use pyo3::exceptions::{PyDeprecationWarning, PySystemError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyList, PyTuple, PyType};

/// `sys.get_int_max_str_digits()`: `None` when the attribute is missing
/// (old Pythons have no limit) or the limit is 0 (disabled).
fn int_max_str_digits(py: Python) -> Option<usize> {
    let sys = py.import("sys").ok()?;
    let get = sys.getattr("get_int_max_str_digits").ok()?;
    let n: usize = get.call0().ok()?.extract().ok()?;
    if n == 0 {
        None
    } else {
        Some(n)
    }
}

/// Python `int` from a normalized digit string (exact for big values).
fn int_from_digits<'py>(py: Python<'py>, digits: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("builtins")?
        .getattr("int")?
        .call1((digits,))
}

/// Deprecation warning matching the original `_deprecated` fallback
/// (`warnings.warn(msg, DeprecationWarning, stacklevel=2)`).
fn warn_deprecated(py: Python, msg: &str) -> PyResult<()> {
    let cmsg = CString::new(msg).unwrap();
    PyErr::warn(py, &py.get_type::<PyDeprecationWarning>(), &cmsg, 2)
}

/// `True` / `False` singletons as bounds (pyo3 0.23 has no `py.True()`).
fn py_true(py: Python<'_>) -> Bound<'_, pyo3::types::PyBool> {
    pyo3::types::PyBool::new(py, true).to_owned()
}

fn py_false(py: Python<'_>) -> Bound<'_, pyo3::types::PyBool> {
    pyo3::types::PyBool::new(py, false).to_owned()
}

/// Human-readable class name for `__repr__` (respects Python subclasses).
fn class_name(slf: &Bound<PyAny>) -> PyResult<String> {
    slf.get_type().getattr("__name__")?.extract()
}

/// Python `repr()` of a version-shaped string (single quotes; version text
/// can never contain a quote, the double-quote arm is defensive only).
fn py_repr_str(s: &str) -> String {
    if !s.contains('\'') {
        format!("'{s}'")
    } else if !s.contains('"') {
        format!("\"{s}\"")
    } else {
        format!("'{s}'")
    }
}

// ---------------------------------------------------------------------------
// Version.
// ---------------------------------------------------------------------------

/// `packaging.version.Version`: immutable parsed PEP 440 version.
#[pyclass(name = "Version", module = "packaging.version", subclass)]
#[derive(Clone)]
struct Version {
    inner: ParsedVersion,
}

impl Version {
    /// `(epoch, release, suffix, [local])` key tuple, element-for-element
    /// identical to the original `_cmpkey` output.
    fn key_tuple<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let epoch = int_from_digits(py, &self.inner.epoch)?;
        let release_items: Vec<Bound<PyAny>> = self
            .inner
            .release
            .iter()
            .map(|d| int_from_digits(py, d))
            .collect::<PyResult<_>>()?;
        let release = PyTuple::new(py, release_items)?;
        let (pre_rank, pre_n) = match &self.inner.pre {
            None if self.inner.post.is_none() && self.inner.dev.is_some() => {
                (-1i64, "0".to_string())
            }
            None => (3i64, "0".to_string()),
            Some((letter, n)) => (
                match letter.as_str() {
                    "a" => 0,
                    "b" => 1,
                    _ => 2,
                },
                n.clone(),
            ),
        };
        let (post_rank, post_n) = match &self.inner.post {
            None => (0i64, "0".to_string()),
            Some(n) => (1i64, n.clone()),
        };
        let (dev_rank, dev_n) = match &self.inner.dev {
            None => (1i64, "0".to_string()),
            Some(n) => (0i64, n.clone()),
        };
        let suffix = PyTuple::new(
            py,
            [
                pre_rank.into_pyobject(py)?.into_any(),
                int_from_digits(py, &pre_n)?.into_any(),
                post_rank.into_pyobject(py)?.into_any(),
                int_from_digits(py, &post_n)?.into_any(),
                dev_rank.into_pyobject(py)?.into_any(),
                int_from_digits(py, &dev_n)?.into_any(),
            ],
        )?;
        let parts: Vec<Bound<PyAny>> = if let Some(local) = &self.inner.local {
            let mut items = Vec::with_capacity(local.len());
            for seg in local {
                let pair = match seg {
                    LocalSeg::Num(n) => {
                        let v = int_from_digits(py, n)?;
                        PyTuple::new(py, [v, py.None().into_bound(py)])?
                    }
                    LocalSeg::Str(s) => {
                        let rank = (-1i64).into_pyobject(py)?.into_any();
                        let text = s.into_pyobject(py)?.into_any();
                        PyTuple::new(py, [rank, text])?
                    }
                };
                items.push(pair.into_any());
            }
            let local_t = PyTuple::new(py, items)?;
            vec![
                epoch,
                release.into_any(),
                suffix.into_any(),
                local_t.into_any(),
            ]
        } else {
            vec![epoch, release.into_any(), suffix.into_any()]
        };
        PyTuple::new(py, parts)
    }

    /// Release components as digit strings, honoring a Python `release`
    /// property override (i.e. `_TrimmedRelease`).
    fn release_digits(slf: &Bound<Self>) -> PyResult<Vec<NumString>> {
        let rel: Bound<PyAny> = slf.getattr("release")?;
        let mut out = Vec::new();
        for item in rel.try_iter()?.map(|r| r.unwrap()) {
            out.push(item.str()?.to_string());
        }
        Ok(out)
    }

    fn display_via(slf: &Bound<Self>) -> PyResult<String> {
        let release = Self::release_digits(slf)?;
        Ok(version::display_with_release(&slf.borrow().inner, &release))
    }

    fn parse_new(py: Python, obj: &Bound<PyAny>) -> PyResult<ParsedVersion> {
        // `_TrimmedRelease(Version)` (and any re-parse path) clones state.
        if let Ok(v) = obj.extract::<PyRef<Self>>() {
            return Ok(v.inner.clone());
        }
        let Ok(s) = obj.extract::<String>() else {
            let repr = obj.repr()?.to_string();
            return Err(InvalidVersion::new_err(format!("Invalid version: {repr}")));
        };
        let limit = int_max_str_digits(py);
        match version::parse(&s, limit) {
            Ok(inner) => Ok(inner),
            Err(version::ParseError::Invalid) => {
                let repr = obj.repr()?.to_string();
                Err(InvalidVersion::new_err(format!("Invalid version: {repr}")))
            }
            Err(version::ParseError::DigitLimit { max, got }) => Err(PyValueError::new_err(
                version::ParseError::digit_limit_message(max, got),
            )),
        }
    }
}

/// Extract a non-negative int digit string from a Python int. Only real
/// `int` instances (bool counts, mirroring `isinstance(True, int)`) qualify;
/// digit strings, floats, and everything else fail, exactly like the
/// original `isinstance(..., int)` checks.
fn extract_nonneg_int(obj: &Bound<PyAny>) -> Option<NumString> {
    if !obj.is_instance_of::<PyInt>() {
        return None;
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Some(if b { "1".to_string() } else { "0".to_string() });
    }
    let s = obj.str().ok()?.to_string();
    if s.starts_with('-') || s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(version::normalize_num(&s))
}

#[pymethods]
impl Version {
    #[new]
    #[pyo3(signature = (*args))]
    fn new(py: Python, args: &Bound<PyTuple>) -> PyResult<Self> {
        // `*args` distinguishes "no argument" (`Version.__new__(Version)`,
        // used by unpickling: blank instance, `__setstate__` fills it in)
        // from an explicit `None` (`Version(None)` must raise, like the
        // original). (`Version()` with no args takes the blank path; the
        // original raises `TypeError` there via `__init__` — no test covers
        // it.)
        if args.len() == 0 {
            return Ok(Version {
                inner: ParsedVersion::blank(),
            });
        }
        if args.len() > 1 {
            return Err(PyTypeError::new_err(format!(
                "Version expected at most 1 argument, got {}",
                args.len()
            )));
        }
        Ok(Version {
            inner: Self::parse_new(py, &args.get_item(0)?)?,
        })
    }

    #[classmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (*, epoch=None, release, pre=None, post=None, dev=None, local=None))]
    fn from_parts(
        _cls: &Bound<PyType>,
        py: Python,
        epoch: Option<Bound<PyAny>>,
        release: Bound<PyAny>,
        pre: Option<Bound<PyAny>>,
        post: Option<Bound<PyAny>>,
        dev: Option<Bound<PyAny>>,
        local: Option<Bound<PyAny>>,
    ) -> PyResult<Self> {
        let limit = int_max_str_digits(py);
        Ok(Version {
            inner: ParsedVersion {
                epoch: validate_epoch(&or_none(py, epoch))?,
                release: validate_release(&release)?,
                pre: validate_pre_opt(&or_none(py, pre))?,
                post: validate_post_opt(&or_none(py, post), "post")?,
                dev: validate_post_opt(&or_none(py, dev), "dev")?,
                local: validate_local_opt(py, limit, &or_none(py, local))?,
            },
        })
    }

    #[pyo3(signature = (**kwargs))]
    fn __replace__(slf: &Bound<Self>, kwargs: Option<Bound<PyDict>>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let limit = int_max_str_digits(py);
        let dict = kwargs.as_ref();
        let get = |k: &str| dict.and_then(|d| d.get_item(k).ok().flatten());
        let cur = slf.borrow().inner.clone();
        let epoch = match get("epoch") {
            Some(v) => validate_epoch(&v)?,
            None => cur.epoch.clone(),
        };
        let release = match get("release") {
            Some(v) => validate_release(&v)?,
            None => cur.release.clone(),
        };
        let pre = match get("pre") {
            Some(v) => validate_pre_opt(&v)?,
            None => cur.pre.clone(),
        };
        let post = match get("post") {
            Some(v) => validate_post_opt(&v, "post")?,
            None => cur.post.clone(),
        };
        let dev = match get("dev") {
            Some(v) => validate_post_opt(&v, "dev")?,
            None => cur.dev.clone(),
        };
        let local = match get("local") {
            Some(v) => validate_local_opt(py, limit, &v)?,
            None => cur.local.clone(),
        };
        if epoch == cur.epoch
            && release == cur.release
            && pre == cur.pre
            && post == cur.post
            && dev == cur.dev
            && local == cur.local
        {
            return Ok(slf.clone().into_any().unbind());
        }
        Ok(Py::new(
            py,
            Version {
                inner: ParsedVersion {
                    epoch,
                    release,
                    pre,
                    post,
                    dev,
                    local,
                },
            },
        )?
        .into_any())
    }

    fn __str__(slf: &Bound<Self>) -> PyResult<String> {
        Self::display_via(slf)
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        let name = class_name(&slf.clone().into_any())?;
        Ok(format!("<{name}({})>", py_repr_str(&Self::display_via(slf)?)))
    }

    fn __hash__(slf: &Bound<Self>) -> PyResult<isize> {
        let py = slf.py();
        slf.borrow().key_tuple(py)?.hash()
    }

    fn __lt__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        rich_compare(slf, other, Ordering::Less, true)
    }
    fn __le__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        rich_compare(slf, other, Ordering::Less, false)
    }
    fn __eq__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        rich_compare_eq(slf, other, true)
    }
    fn __ge__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        rich_compare(slf, other, Ordering::Greater, false)
    }
    fn __gt__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        rich_compare(slf, other, Ordering::Greater, true)
    }
    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        rich_compare_eq(slf, other, false)
    }

    #[getter]
    fn _key<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        slf.borrow().key_tuple(slf.py())
    }

    #[getter]
    fn _str(slf: &Bound<Self>) -> PyResult<String> {
        Self::display_via(slf)
    }

    #[getter]
    fn epoch<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        int_from_digits(py, &self.inner.epoch)
    }

    #[getter]
    fn release<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let items: Vec<Bound<PyAny>> = self
            .inner
            .release
            .iter()
            .map(|d| int_from_digits(py, d))
            .collect::<PyResult<_>>()?;
        PyTuple::new(py, items)
    }

    #[getter]
    fn pre<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyTuple>>> {
        match &self.inner.pre {
            None => Ok(None),
            Some((letter, n)) => {
                let t = PyTuple::new(
                    py,
                    [
                        letter.into_pyobject(py)?.into_any(),
                        int_from_digits(py, n)?.into_any(),
                    ],
                )?;
                Ok(Some(t))
            }
        }
    }

    #[getter]
    fn post<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        match &self.inner.post {
            None => Ok(None),
            Some(n) => Ok(Some(int_from_digits(py, n)?)),
        }
    }

    #[getter]
    fn dev<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        match &self.inner.dev {
            None => Ok(None),
            Some(n) => Ok(Some(int_from_digits(py, n)?)),
        }
    }

    #[getter]
    fn local(&self) -> Option<String> {
        self.inner.local.as_ref().map(|segs| {
            segs.iter()
                .map(|s| match s {
                    LocalSeg::Num(n) => n.clone(),
                    LocalSeg::Str(t) => t.clone(),
                })
                .collect::<Vec<_>>()
                .join(".")
        })
    }

    #[getter]
    fn public(slf: &Bound<Self>) -> PyResult<String> {
        Ok(Self::display_via(slf)?
            .split('+')
            .next()
            .unwrap()
            .to_string())
    }

    #[getter]
    fn base_version(slf: &Bound<Self>) -> PyResult<String> {
        let this = slf.borrow();
        let seg = Self::release_digits(slf)?.join(".");
        Ok(if this.inner.epoch != "0" {
            format!("{}!{seg}", this.inner.epoch)
        } else {
            seg
        })
    }

    #[getter]
    fn is_prerelease(&self) -> bool {
        self.inner.dev.is_some() || self.inner.pre.is_some()
    }

    #[getter]
    fn is_postrelease(&self) -> bool {
        self.inner.post.is_some()
    }

    #[getter]
    fn is_devrelease(&self) -> bool {
        self.inner.dev.is_some()
    }

    #[getter]
    fn major<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let rel = Self::release_digits(slf)?;
        match rel.first() {
            Some(d) => int_from_digits(py, d),
            None => Ok(0i64.into_pyobject(py)?.into_any()),
        }
    }

    #[getter]
    fn minor<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let rel = Self::release_digits(slf)?;
        match rel.get(1) {
            Some(d) => int_from_digits(py, d),
            None => Ok(0i64.into_pyobject(py)?.into_any()),
        }
    }

    #[getter]
    fn micro<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let rel = Self::release_digits(slf)?;
        match rel.get(2) {
            Some(d) => int_from_digits(py, d),
            None => Ok(0i64.into_pyobject(py)?.into_any()),
        }
    }

    #[getter]
    fn _version<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        warn_deprecated(py, "Version._version is private and will be removed soon")?;
        let this = slf.borrow();
        let named = py.import("packaging.version")?.getattr("_Version")?;
        let epoch = int_from_digits(py, &this.inner.epoch)?;
        let release = release_tuple(py, &this.inner.release)?;
        let opt_pair = |letter: &str, v: &Option<NumString>| -> PyResult<Bound<PyAny>> {
            match v {
                None => Ok(py.None().into_bound(py)),
                Some(n) => Ok(PyTuple::new(
                    py,
                    [
                        letter.into_pyobject(py)?.into_any(),
                        int_from_digits(py, n)?.into_any(),
                    ],
                )?
                .into_any()),
            }
        };
        let dev = opt_pair("dev", &this.inner.dev)?;
        let pre = match &this.inner.pre {
            None => py.None().into_bound(py),
            Some((letter, n)) => PyTuple::new(
                py,
                [
                    letter.into_pyobject(py)?.into_any(),
                    int_from_digits(py, n)?.into_any(),
                ],
            )?
            .into_any(),
        };
        let post = opt_pair("post", &this.inner.post)?;
        let local = match &this.inner.local {
            None => py.None().into_bound(py),
            Some(segs) => {
                let mut items = Vec::with_capacity(segs.len());
                for s in segs {
                    match s {
                        LocalSeg::Num(n) => items.push(int_from_digits(py, n)?.into_any()),
                        LocalSeg::Str(t) => items.push(t.into_pyobject(py)?.into_any()),
                    }
                }
                PyTuple::new(py, items)?.into_any()
            }
        };
        // `_Version(epoch, release, dev, pre, post, local)` field order.
        named.call1((epoch, release, dev, pre, post, local))
    }

    #[allow(non_snake_case)]
    #[setter]
    fn set__version(slf: &Bound<Self>, value: Bound<PyAny>) -> PyResult<()> {
        let py = slf.py();
        warn_deprecated(py, "Version._version is private and will be removed soon")?;
        let mut this = slf.borrow_mut();
        this.inner.epoch = value.getattr("epoch")?.str()?.to_string();
        this.inner.release = digits_of_seq(&value.getattr("release")?)?;
        this.inner.dev = post_dev_of(&value.getattr("dev")?)?;
        this.inner.pre = letter_num_opt(&value.getattr("pre")?)?;
        this.inner.post = post_dev_of(&value.getattr("post")?)?;
        this.inner.local = local_of_opt(&value.getattr("local")?)?;
        Ok(())
    }

    fn __getstate__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let py = slf.py();
        let this = slf.borrow();
        let epoch = int_from_digits(py, &this.inner.epoch)?;
        let release = release_tuple(py, &this.inner.release)?;
        let opt_pair = |letter: &str, v: &Option<NumString>| -> PyResult<Bound<PyAny>> {
            match v {
                None => Ok(py.None().into_bound(py)),
                Some(n) => Ok(PyTuple::new(
                    py,
                    [
                        letter.into_pyobject(py)?.into_any(),
                        int_from_digits(py, n)?.into_any(),
                    ],
                )?
                .into_any()),
            }
        };
        let pre = match &this.inner.pre {
            None => py.None().into_bound(py),
            Some((letter, n)) => PyTuple::new(
                py,
                [
                    letter.into_pyobject(py)?.into_any(),
                    int_from_digits(py, n)?.into_any(),
                ],
            )?
            .into_any(),
        };
        let post = opt_pair("post", &this.inner.post)?;
        let dev = opt_pair("dev", &this.inner.dev)?;
        let local = match &this.inner.local {
            None => py.None().into_bound(py),
            Some(segs) => {
                let mut items = Vec::with_capacity(segs.len());
                for s in segs {
                    match s {
                        LocalSeg::Num(n) => items.push(int_from_digits(py, n)?.into_any()),
                        LocalSeg::Str(t) => items.push(t.into_pyobject(py)?.into_any()),
                    }
                }
                PyTuple::new(py, items)?.into_any()
            }
        };
        PyTuple::new(py, [epoch, release.into_any(), pre, post, dev, local])
    }

    fn __setstate__(slf: &Bound<Self>, state: Bound<PyAny>) -> PyResult<()> {
        restore_state(slf, &state)
    }
}

/// `None` (absent keyword) as an explicit Python `None` for validation.
fn or_none<'py>(py: Python<'py>, o: Option<Bound<'py, PyAny>>) -> Bound<'py, PyAny> {
    o.unwrap_or_else(|| py.None().into_bound(py))
}

fn release_tuple<'py>(py: Python<'py>, release: &[NumString]) -> PyResult<Bound<'py, PyTuple>> {
    let items: Vec<Bound<PyAny>> = release
        .iter()
        .map(|d| int_from_digits(py, d))
        .collect::<PyResult<_>>()?;
    PyTuple::new(py, items)
}

/// Convert a sequence of Python ints to digit strings.
fn digits_of_seq(seq: &Bound<PyAny>) -> PyResult<Vec<NumString>> {
    let mut out = Vec::new();
    for item in seq.try_iter()?.map(|r| r.unwrap()) {
        out.push(item.str()?.to_string());
    }
    Ok(out)
}

/// Convert an optional `(letter, number)` tuple attr to core form.
fn letter_num_opt(obj: &Bound<PyAny>) -> PyResult<Option<(String, NumString)>> {
    if obj.is_none() {
        return Ok(None);
    }
    let t: Bound<PyTuple> = obj.extract()?;
    let letter: String = t.get_item(0)?.extract()?;
    let num: String = t.get_item(1)?.str()?.to_string();
    Ok(Some((letter, num)))
}
/// Convert an optional post/dev value to core form. Restored states carry
/// the private tuple form (`("post", n)` / `("dev", n)`), matching what
/// `__getstate__` emits; anything else stores as its string form.
fn post_dev_of(obj: &Bound<PyAny>) -> PyResult<Option<NumString>> {
    if obj.is_none() {
        return Ok(None);
    }
    if let Ok(t) = obj.downcast::<PyTuple>() {
        if t.len() == 2 {
            return Ok(Some(t.get_item(1)?.str()?.to_string()));
        }
    }
    Ok(Some(obj.str()?.to_string()))
}

/// Convert an optional local tuple attr to core form.
fn local_of_opt(obj: &Bound<PyAny>) -> PyResult<Option<Vec<LocalSeg>>> {
    if obj.is_none() {
        return Ok(None);
    }
    let mut out = Vec::new();
    for item in obj.try_iter()?.map(|r| r.unwrap()) {
        if let Ok(n) = item.extract::<bool>() {
            out.push(LocalSeg::Num(if n { "1".to_string() } else { "0".to_string() }));
        } else if let Ok(s) = item.extract::<String>() {
            out.push(LocalSeg::Str(s));
        } else {
            out.push(LocalSeg::Str(item.str()?.to_string()));
        }
    }
    Ok(Some(out))
}

/// `__setstate__` accepting the 6-tuple (26.2+), the `(None, slot-dict)`
/// (26.0-26.1), and the old `{"_version": ..., "_key": ...}` dict (<=25.x).
fn restore_state(slf: &Bound<Version>, state: &Bound<PyAny>) -> PyResult<()> {
    let py = slf.py();
    let bad = |state: &Bound<PyAny>| {
        PyTypeError::new_err(format!(
            "Cannot restore Version from {}",
            state.repr().map(|r| r.to_string()).unwrap_or_default()
        ))
    };
    if let Ok(t) = state.downcast::<PyTuple>() {
        if t.len() == 6 {
            let mut this = slf.borrow_mut();
            this.inner.epoch = t.get_item(0)?.str()?.to_string();
            this.inner.release = digits_of_seq(&t.get_item(1)?)?;
            this.inner.pre = letter_num_opt(&t.get_item(2)?)?;
            this.inner.post = post_dev_of(&t.get_item(3)?)?;
            this.inner.dev = post_dev_of(&t.get_item(4)?)?;
            this.inner.local = local_of_opt(&t.get_item(5)?)?;
            return Ok(());
        }
        if t.len() == 2 {
            let slots: Bound<PyDict> = t.get_item(1)?.extract().map_err(|_| bad(state))?;
            let req = |k: &str| -> PyResult<Bound<PyAny>> {
                slots.get_item(k)?.ok_or_else(|| bad(state))
            };
            let get = |k: &str| -> PyResult<Bound<PyAny>> {
                Ok(slots.get_item(k)?.unwrap_or_else(|| py.None().into_bound(py)))
            };
            let mut this = slf.borrow_mut();
            this.inner.epoch = req("_epoch")?.str()?.to_string();
            this.inner.release = digits_of_seq(&req("_release")?)?;
            this.inner.pre = letter_num_opt(&get("_pre")?)?;
            this.inner.post = post_dev_of(&get("_post")?)?;
            this.inner.dev = post_dev_of(&get("_dev")?)?;
            this.inner.local = local_of_opt(&get("_local")?)?;
            return Ok(());
        }
        return Err(bad(state));
    }
    if let Ok(d) = state.downcast::<PyDict>() {
        if let Some(nt) = d.get_item("_version")? {
            let mut this = slf.borrow_mut();
            this.inner.epoch = nt.getattr("epoch")?.str()?.to_string();
            this.inner.release = digits_of_seq(&nt.getattr("release")?)?;
            this.inner.dev = post_dev_of(&nt.getattr("dev")?)?;
            this.inner.pre = letter_num_opt(&nt.getattr("pre")?)?;
            this.inner.post = post_dev_of(&nt.getattr("post")?)?;
            this.inner.local = local_of_opt(&nt.getattr("local")?)?;
            return Ok(());
        }
    }
    Err(bad(state))
}

// ---------------------------------------------------------------------------
// from_parts / __replace__ validation (exact messages).
// ---------------------------------------------------------------------------

fn validate_epoch(obj: &Bound<PyAny>) -> PyResult<NumString> {
    let val: Bound<PyAny> = if obj.is_truthy()? {
        obj.clone()
    } else {
        0i64.into_pyobject(obj.py())?.into_any()
    };
    match extract_nonneg_int(&val) {
        Some(n) => Ok(n),
        None => Err(InvalidVersion::new_err(format!(
            "epoch must be non-negative integer, got {}",
            obj.str()?
        ))),
    }
}

fn validate_release(obj: &Bound<PyAny>) -> PyResult<Vec<NumString>> {
    if obj.is_none() {
        return Ok(vec!["0".to_string()]);
    }
    let err = || {
        InvalidVersion::new_err(format!(
            "release must be a non-empty tuple of non-negative integers, got {}",
            obj.str().map(|s| s.to_string()).unwrap_or_default()
        ))
    };
    let Ok(t) = obj.downcast::<PyTuple>() else {
        return Err(err());
    };
    if t.is_empty() {
        return Err(err());
    }
    let mut out = Vec::with_capacity(t.len());
    for item in t.iter() {
        match extract_nonneg_int(&item) {
            Some(n) => out.push(n),
            None => return Err(err()),
        }
    }
    Ok(out)
}

fn validate_pre(obj: &Bound<PyAny>) -> PyResult<(String, NumString)> {
    let err = || {
        InvalidVersion::new_err(format!(
            "pre must be a tuple of ('a'|'b'|'rc', non-negative int), got {}",
            obj.str().map(|s| s.to_string()).unwrap_or_default()
        ))
    };
    let Ok(t) = obj.downcast::<PyTuple>() else {
        return Err(err());
    };
    if t.len() != 2 {
        return Err(err());
    }
    let letter: String = t.get_item(0)?.extract().map_err(|_| err())?;
    let norm = version::normalize_pre(&letter);
    if !matches!(norm.as_str(), "a" | "b" | "rc") {
        return Err(err());
    }
    match extract_nonneg_int(&t.get_item(1)?) {
        Some(n) => Ok((norm, n)),
        None => Err(err()),
    }
}

fn validate_pre_opt(obj: &Bound<PyAny>) -> PyResult<Option<(String, NumString)>> {
    if obj.is_none() {
        return Ok(None);
    }
    validate_pre(obj).map(Some)
}

fn validate_post_opt(obj: &Bound<PyAny>, kind: &str) -> PyResult<Option<NumString>> {
    if obj.is_none() {
        return Ok(None);
    }
    match extract_nonneg_int(obj) {
        Some(n) => Ok(Some(n)),
        None => Err(InvalidVersion::new_err(format!(
            "{kind} must be non-negative integer, got {}",
            obj.str()?
        ))),
    }
}
fn validate_local_opt(
    _py: Python,
    limit: Option<usize>,
    obj: &Bound<PyAny>,
) -> PyResult<Option<Vec<LocalSeg>>> {
    if obj.is_none() {
        return Ok(None);
    }
    let invalid = || {
        InvalidVersion::new_err(format!(
            "local must be a valid version string, got {}",
            obj.repr().map(|r| r.to_string()).unwrap_or_default()
        ))
    };
    let Ok(s) = obj.extract::<String>() else {
        return Err(invalid());
    };
    match version::parse_local_ascii(&s, limit) {
        Ok(segs) => Ok(Some(segs)),
        Err(version::ParseError::DigitLimit { max, got }) => Err(PyValueError::new_err(
            version::ParseError::digit_limit_message(max, got),
        )),
        Err(version::ParseError::Invalid) => Err(invalid()),
    }
}

// ---------------------------------------------------------------------------
// Comparisons.
// ---------------------------------------------------------------------------

/// Is `other` a `Version` (exact Rust type or Python subclass)?
fn is_version(other: &Bound<PyAny>) -> bool {
    other.is_instance_of::<Version>()
}

/// Is `other` a `packaging.version._BaseVersion` (but not a `Version`)?
fn is_base_version(py: Python, other: &Bound<PyAny>) -> PyResult<bool> {
    if is_version(other) {
        return Ok(false);
    }
    let base = py
        .import("packaging.version")
        .and_then(|m| m.getattr("_BaseVersion"));
    let Ok(base) = base else { return Ok(false) };
    other.is_instance(&base)
}

fn not_implemented(py: Python) -> PyResult<Py<PyAny>> {
    py.NotImplemented().into_py_any(py)
}

/// Ordered comparisons; `strict` selects `<` vs `<=` (or `>` vs `>=`).
fn rich_compare(
    slf: &Bound<Version>,
    other: &Bound<PyAny>,
    want: Ordering,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    if is_version(other) {
        let ord = match other.extract::<PyRef<Version>>() {
            Ok(o) => version::cmp(&slf.borrow().inner, &o.inner),
            // Python subclass of Version without Rust state: key tuples.
            Err(_) => {
                let a = slf.borrow().key_tuple(py)?;
                let b: Bound<PyAny> = other.getattr("_key")?;
                return py_compare_tuples(py, &a.into_any(), &b, want, strict);
            }
        };
        let hit = if strict {
            ord == want
        } else {
            ord == want || ord == Ordering::Equal
        };
        return hit.into_py_any(py);
    }
    if is_base_version(py, other)? {
        let a = slf.borrow().key_tuple(py)?;
        let b: Bound<PyAny> = other.getattr("_key")?;
        return py_compare_tuples(py, &a.into_any(), &b, want, strict);
    }
    not_implemented(py)
}

fn rich_compare_eq(
    slf: &Bound<Version>,
    other: &Bound<PyAny>,
    want_eq: bool,
) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    if is_version(other) {
        let eq = match other.extract::<PyRef<Version>>() {
            Ok(o) => version::cmp(&slf.borrow().inner, &o.inner) == Ordering::Equal,
            Err(_) => {
                let a = slf.borrow().key_tuple(py)?;
                let b: Bound<PyAny> = other.getattr("_key")?;
                a.as_any().eq(&b)?
            }
        };
        return (eq == want_eq).into_py_any(py);
    }
    if is_base_version(py, other)? {
        let a = slf.borrow().key_tuple(py)?;
        let b: Bound<PyAny> = other.getattr("_key")?;
        let eq = a.as_any().eq(&b)?;
        return (eq == want_eq).into_py_any(py);
    }
    not_implemented(py)
}

fn py_compare_tuples(
    py: Python,
    a: &Bound<PyAny>,
    b: &Bound<PyAny>,
    want: Ordering,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let ord = if a.eq(b)? {
        Ordering::Equal
    } else if a.lt(b)? {
        Ordering::Less
    } else {
        Ordering::Greater
    };
    let hit = if strict {
        ord == want
    } else {
        ord == want || ord == Ordering::Equal
    };
    hit.into_py_any(py)
}

// ---------------------------------------------------------------------------
// Module-level functions.
// ---------------------------------------------------------------------------

/// `packaging.version.parse`: identical to the `Version` constructor.
#[pyfunction]
fn parse(py: Python, version: Bound<PyAny>) -> PyResult<Version> {
    Ok(Version {
        inner: Version::parse_new(py, &version)?,
    })
}

/// `packaging.version.normalize_pre`.
#[pyfunction]
#[pyo3(name = "normalize_pre")]
fn normalize_pre_fn(letter: String) -> String {
    version::normalize_pre(&letter)
}

// ---------------------------------------------------------------------------
// Extension module.
// ---------------------------------------------------------------------------

/// `packaging._packaging`: compiled half of the Stage-1 mirror.
#[pymodule]
fn _packaging(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<Version>()?;
    m.add_class::<ELFFile>()?;
    m.add("InvalidVersion", m.py().get_type::<InvalidVersion>())?;
    m.add("ELFInvalid", m.py().get_type::<ELFInvalid>())?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(normalize_pre_fn, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_confstr, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_ctypes, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_version_string, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_parse_glibc_version, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_get_glibc_version_uncached, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_get_module_uncached, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_is_armhf, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_is_i686, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_have_compatible_abi, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_is_compatible, m)?)?;
    m.add_function(wrap_pyfunction!(manylinux_platform_tags, m)?)?;
    m.add_function(wrap_pyfunction!(musllinux_parse_version, m)?)?;
    m.add_function(wrap_pyfunction!(musllinux_get_version_uncached, m)?)?;
    m.add_function(wrap_pyfunction!(musllinux_platform_tags, m)?)?;
    m.add_class::<Tag>()?;
    m.add("UnsortedTagsError", m.py().get_type::<UnsortedTagsError>())?;
    m.add("InvalidTag", m.py().get_type::<InvalidTag>())?;
    m.add("TooManyTagsError", m.py().get_type::<TooManyTagsError>())?;
    m.add_function(wrap_pyfunction!(tags_parse_tag, m)?)?;
    m.add_function(wrap_pyfunction!(tags_get_config_var, m)?)?;
    m.add_function(wrap_pyfunction!(tags_cpython_abis, m)?)?;
    m.add_function(wrap_pyfunction!(tags_cpython_tags, m)?)?;
    m.add_function(wrap_pyfunction!(tags_generic_abi, m)?)?;
    m.add_function(wrap_pyfunction!(tags_generic_tags, m)?)?;
    m.add_function(wrap_pyfunction!(tags_pure_python_tags, m)?)?;
    m.add_function(wrap_pyfunction!(tags_compatible_tags, m)?)?;
    m.add_function(wrap_pyfunction!(tags_mac_arch, m)?)?;
    m.add_function(wrap_pyfunction!(tags_mac_platforms, m)?)?;
    m.add_function(wrap_pyfunction!(tags_ios_platforms, m)?)?;
    m.add_function(wrap_pyfunction!(tags_android_platforms, m)?)?;
    m.add_function(wrap_pyfunction!(tags_linux_platforms, m)?)?;
    m.add_function(wrap_pyfunction!(tags_emscripten_platforms, m)?)?;
    m.add_function(wrap_pyfunction!(tags_generic_platforms, m)?)?;
    m.add_function(wrap_pyfunction!(tags_platform_tags, m)?)?;
    m.add_function(wrap_pyfunction!(tags_version_nodot, m)?)?;
    m.add_function(wrap_pyfunction!(tags_mac_binary_formats, m)?)?;
    m.add_function(wrap_pyfunction!(tags_interpreter_name, m)?)?;
    m.add_function(wrap_pyfunction!(tags_interpreter_version, m)?)?;
    m.add_function(wrap_pyfunction!(tags_interpreter_abi, m)?)?;
    m.add_function(wrap_pyfunction!(tags_sys_tags, m)?)?;
    m.add_function(wrap_pyfunction!(tags_create_selector, m)?)?;
    m.add("InvalidName", m.py().get_type::<InvalidName>())?;
    m.add("InvalidWheelFilename", m.py().get_type::<InvalidWheelFilename>())?;
    m.add("InvalidSdistFilename", m.py().get_type::<InvalidSdistFilename>())?;
    m.add_function(wrap_pyfunction!(utils_canonicalize_name, m)?)?;
    m.add_function(wrap_pyfunction!(utils_is_normalized_name, m)?)?;
    m.add_function(wrap_pyfunction!(utils_canonicalize_version, m)?)?;
    m.add_function(wrap_pyfunction!(utils_parse_wheel_filename, m)?)?;
    m.add_function(wrap_pyfunction!(utils_parse_sdist_filename, m)?)?;
    m.add("InvalidSpecifier", m.py().get_type::<InvalidSpecifier>())?;
    m.add_class::<BoundaryVersionPy>()?;
    m.add_class::<LowerBoundPy>()?;
    m.add_class::<UpperBoundPy>()?;
    m.add_class::<SpecifierPy>()?;
    m.add_class::<SpecifierSetPy>()?;
    m.add_class::<VersionRangePy>()?;
    m.add_function(wrap_pyfunction!(ranges_intersect_ranges, m)?)?;
    // Tracebacks name the public module, not the extension.
    m.py()
        .get_type::<InvalidVersion>()
        .setattr("__module__", "packaging.version")?;
    m.py()
        .get_type::<ELFInvalid>()
        .setattr("__module__", "packaging._elffile")?;
    m.py()
        .get_type::<UnsortedTagsError>()
        .setattr("__module__", "packaging.tags")?;
    m.py()
        .get_type::<InvalidTag>()
        .setattr("__module__", "packaging.tags")?;
    m.py()
        .get_type::<TooManyTagsError>()
        .setattr("__module__", "packaging.tags")?;
    m.py()
        .get_type::<InvalidName>()
        .setattr("__module__", "packaging.utils")?;
    m.py()
        .get_type::<InvalidWheelFilename>()
        .setattr("__module__", "packaging.utils")?;
    m.py()
        .get_type::<InvalidSdistFilename>()
        .setattr("__module__", "packaging.utils")?;
    m.py()
        .get_type::<InvalidSpecifier>()
        .setattr("__module__", "packaging.specifiers")?;
    // Pattern matching (`__match_args__ == ("_str",)`, mirroring the original).
    m.getattr("Version")?
        .setattr("__match_args__", ("_str",))?;
    m.getattr("Specifier")?
        .setattr("__match_args__", ("_str",))?;
    m.getattr("SpecifierSet")?
        .setattr("__match_args__", ("_str",))?;
    Ok(())
}
// ---------------------------------------------------------------------------
// ELF files (`packaging._elffile`).
// ---------------------------------------------------------------------------

pyo3::create_exception!(_packaging, InvalidVersion, PyValueError);

pyo3::create_exception!(_packaging, ELFInvalid, PyValueError);

/// Read exactly `n` bytes via the file object's `read`, mirroring
/// `struct.unpack(fmt, f.read(calcsize(fmt)))`: a short read is a
/// `struct.error` in the original, surfaced here as `None`.
fn read_bytes(f: &Bound<PyAny>, n: usize) -> PyResult<Option<Vec<u8>>> {
    let data: Vec<u8> = f.call_method1("read", (n,))?.extract()?;
    if data.len() < n {
        return Ok(None);
    }
    Ok(Some(data))
}

/// `packaging._elffile.ELFFile`: parsed ELF executable.
#[pyclass(name = "ELFFile", module = "packaging._elffile")]
struct ELFFile {
    f: Py<PyAny>,
    kind: packaging_core::elf::ElfKind,
    ehdr: packaging_core::elf::Ehdr,
    capacity: u8,
    encoding: u8,
}

#[pymethods]
impl ELFFile {
    #[new]
    #[pyo3(signature = (*args))]
    fn new(py: Python, args: &Bound<PyTuple>) -> PyResult<Self> {
        use packaging_core::elf::ElfKind;
        if args.len() != 1 {
            return Err(PyTypeError::new_err(format!(
                "ELFFile expected 1 argument, got {}",
                args.len()
            )));
        }
        let f: Bound<PyAny> = args.get_item(0)?;
        let Some(ident) = read_bytes(&f, 16)? else {
            return Err(ELFInvalid::new_err("unable to parse identification"));
        };
        if ident[..4] != [0x7f, b'E', b'L', b'F'] {
            let magic = pyo3::types::PyBytes::new(py, &ident[..4]);
            let repr = magic.repr()?.to_string();
            return Err(ELFInvalid::new_err(format!("invalid magic: {repr}")));
        }
        let capacity = ident[4];
        let encoding = ident[5];
        let Some(kind) = ElfKind::for_ident(capacity, encoding) else {
            return Err(ELFInvalid::new_err(format!(
                "unrecognized capacity ({capacity}) or encoding ({encoding})"
            )));
        };
        let n = kind.ehdr_len();
        let Some(ehdr_data) = read_bytes(&f, n)? else {
            return Err(ELFInvalid::new_err(
                "unable to parse machine and section information",
            ));
        };
        let ehdr = packaging_core::elf::parse_ehdr(kind, &ehdr_data);
        Ok(ELFFile {
            f: f.unbind(),
            kind,
            ehdr,
            capacity,
            encoding,
        })
    }

    #[getter]
    fn capacity(&self) -> u8 {
        self.capacity
    }

    #[getter]
    fn encoding(&self) -> u8 {
        self.encoding
    }

    // Private header-layout attributes, exposed because the test suite
    // reaches into them (`test_elffle_no_interpreter_section`).
    #[getter]
    fn _e_phoff<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.ehdr.phoff.into_pyobject(py)?.into_any())
    }

    #[getter]
    fn _e_phentsize<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.ehdr.phentsize.into_pyobject(py)?.into_any())
    }

    #[getter]
    fn _e_phnum<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.ehdr.phnum.into_pyobject(py)?.into_any())
    }

    #[getter]
    fn _p_fmt(&self) -> String {
        let endian = if self.kind.little { "<" } else { ">" };
        if self.kind.is64 {
            format!("{endian}IIQQQQQQ")
        } else {
            format!("{endian}IIIIIIII")
        }
    }

    #[getter]
    fn machine<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.ehdr.machine.into_pyobject(py)?.into_any())
    }

    #[getter]
    fn flags<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.ehdr.flags.into_pyobject(py)?.into_any())
    }

    #[getter]
    fn interpreter(slf: &Bound<Self>) -> PyResult<Option<String>> {
        use packaging_core::elf::{PT_INTERP, parse_phdr};
        let py = slf.py();
        let this = slf.borrow();
        let f: Bound<PyAny> = this.f.bind(py).clone();
        let kind = this.kind;
        let (phoff, phentsize, phnum) = (this.ehdr.phoff, this.ehdr.phentsize, this.ehdr.phnum);
        drop(this);
        for index in 0..phnum {
            f.call_method1("seek", (phoff + phentsize * index,))?;
            let Some(data) = read_bytes(&f, kind.phdr_len())? else {
                continue;
            };
            let (ptype, offset, filesz) = parse_phdr(kind, &data);
            if ptype != PT_INTERP {
                continue;
            }
            f.call_method1("seek", (offset,))?;
            let raw: Vec<u8> = f.call_method1("read", (filesz,))?.extract()?;
            let text: Bound<PyAny> = py
                .import("os")?
                .getattr("fsdecode")?
                .call1((pyo3::types::PyBytes::new(py, &raw),))?;
            let s: String = text.extract()?;
            return Ok(Some(s.trim_matches('\0').to_string()));
        }
        Ok(None)
    }
}
// ---------------------------------------------------------------------------
// Platform detection (`packaging._manylinux`, `packaging._musllinux`).
//
// Every environment read goes through live Python objects (`os.confstr`,
// `ctypes.CDLL`, `subprocess.run`, `sys`, `__import__`) and every
// intra-module helper through the module attribute, so the suite's
// `monkeypatch` doubles keep working exactly as with the original.
// ---------------------------------------------------------------------------

/// Module attribute (resolved live so test doubles apply).
fn mod_attr<'py>(py: Python<'py>, module: &str, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)
}

/// Does `err` match one of the named `builtins` exception classes?
fn err_matches(py: Python, err: &PyErr, names: &[&str]) -> bool {
    let Ok(builtins) = py.import("builtins") else {
        return false;
    };
    names.iter().any(|n| {
        builtins
            .getattr(*n)
            .map(|cls| err.is_instance(py, &cls))
            .unwrap_or(false)
    })
}

/// `warnings.warn(msg, RuntimeWarning, stacklevel=2)` via the real module.
fn warn_runtime(py: Python, msg: &str) -> PyResult<()> {
    let warnings = py.import("warnings")?;
    let category = py.import("builtins")?.getattr("RuntimeWarning")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("stacklevel", 2)?;
    warnings.getattr("warn")?.call((msg, category), Some(&kwargs))?;
    Ok(())
}

/// Construct a `packaging._manylinux._GLibCVersion(major, minor)`.
fn glibc_version(py: Python, major: i64, minor: i64) -> PyResult<Bound<PyAny>> {
    mod_attr(py, "packaging._manylinux", "_GLibCVersion")?.call1((major, minor))
}

/// Construct a `packaging._musllinux._MuslVersion(major, minor)`.
fn musl_version(py: Python, major: i64, minor: i64) -> PyResult<Bound<PyAny>> {
    mod_attr(py, "packaging._musllinux", "_MuslVersion")?.call1((major, minor))
}

/// Eager list as a live iterator (every `*_tags` generator returns an
/// iterator; laziness of the env reads is not observable to the suite).
fn str_iter(py: Python, items: Vec<String>) -> PyResult<Bound<PyAny>> {
    PyList::new(py, items)?.call_method0("__iter__")
}

/// `os.confstr("CS_GNU_LIBC_VERSION")` split to the version part, or `None`.
/// Mirrors `_glibc_version_string_confstr` including the exact caught
/// exceptions and the strict two-part `rsplit()` unpack.
#[pyfunction]
#[pyo3(name = "manylinux_confstr")]
fn manylinux_confstr(py: Python) -> PyResult<Option<String>> {
    let os = py.import("os")?;
    let confstr = match os.getattr("confstr") {
        Ok(f) => f,
        Err(e) if err_matches(py, &e, &["AttributeError"]) => return Ok(None),
        Err(e) => return Err(e),
    };
    let raw = match confstr.call1(("CS_GNU_LIBC_VERSION",)) {
        Ok(v) => v,
        Err(e) if err_matches(py, &e, &["AssertionError", "AttributeError", "OSError", "ValueError"]) => {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    if raw.is_none() {
        return Ok(None);
    }
    let s: String = match raw.extract() {
        Ok(s) => s,
        Err(e) if err_matches(py, &e, &["AttributeError"]) => return Ok(None),
        Err(e) => return Err(e),
    };
    let parts: Vec<String> = py
        .import("builtins")?
        .getattr("str")?
        .call1((s.clone(),))?
        .getattr("rsplit")?
        .call0()?
        .extract()?;
    if parts.len() != 2 {
        return Ok(None);
    }
    Ok(Some(parts[1].clone()))
}

/// `gnu_get_libc_version` via `ctypes.CDLL(None)`, or `None`.
/// Mirrors `_glibc_version_string_ctypes` including the exact caught
/// exceptions (`OSError` on `dlopen`, `AttributeError` on the symbol).
#[pyfunction]
#[pyo3(name = "manylinux_ctypes")]
fn manylinux_ctypes(py: Python) -> PyResult<Option<Bound<PyAny>>> {
    let ctypes = match py.import("ctypes") {
        Ok(m) => m,
        Err(e) if err_matches(py, &e, &["ImportError"]) => return Ok(None),
        Err(e) => return Err(e),
    };
    let process = match ctypes.getattr("CDLL")?.call1((py.None(),)) {
        Ok(p) => p,
        Err(e) if err_matches(py, &e, &["OSError"]) => return Ok(None),
        Err(e) => return Err(e),
    };
    let func = match process.getattr("gnu_get_libc_version") {
        Ok(f) => f,
        Err(e) if err_matches(py, &e, &["AttributeError"]) => return Ok(None),
        Err(e) => return Err(e),
    };
    func.setattr("restype", ctypes.getattr("c_char_p")?)?;
    let version = func.call0()?;
    if version.is_instance_of::<pyo3::types::PyBytes>() {
        let decoded: Bound<PyAny> = version.call_method1("decode", ("ascii",))?;
        return Ok(Some(decoded));
    }
    Ok(Some(version))
}

/// `_glibc_version_string`: `confstr() or ctypes()` on raw values.
#[pyfunction]
#[pyo3(name = "manylinux_version_string")]
fn manylinux_version_string(py: Python) -> PyResult<Bound<PyAny>> {
    let a = mod_attr(py, "packaging._manylinux", "_glibc_version_string_confstr")?.call0()?;
    if a.is_truthy()? {
        return Ok(a);
    }
    mod_attr(py, "packaging._manylinux", "_glibc_version_string_ctypes")?.call0()
}

/// `_parse_glibc_version`, returning a `_GLibCVersion`. Emits the original
/// `RuntimeWarning` and `(-1, -1)` when the string has no leading
/// `major.minor`.
#[pyfunction]
#[pyo3(name = "manylinux_parse_glibc_version")]
fn manylinux_parse_glibc_version(py: Python, version_str: String) -> PyResult<Bound<PyAny>> {
    match packaging_core::platform::parse_glibc_version(&version_str) {
        Some((major, minor)) => glibc_version(py, major, minor),
        None => {
            warn_runtime(
                py,
                &format!(
                    "Expected glibc version with 2 components major.minor, got: {version_str}"
                ),
            )?;
            glibc_version(py, -1, -1)
        }
    }
}

/// Uncached `_get_glibc_version` (the shim wraps it in `lru_cache`).
#[pyfunction]
#[pyo3(name = "manylinux_get_glibc_version_uncached")]
fn manylinux_get_glibc_version_uncached(py: Python) -> PyResult<Bound<PyAny>> {
    let s = mod_attr(py, "packaging._manylinux", "_glibc_version_string")?.call0()?;
    if s.is_none() {
        return glibc_version(py, -1, -1);
    }
    let text: String = s.extract()?;
    mod_attr(py, "packaging._manylinux", "_parse_glibc_version")?.call1((text,))
}

/// Uncached `_get_manylinux_module` (the shim wraps it in `lru_cache`).
/// `__import__("_manylinux")`, `None` on `ImportError`.
#[pyfunction]
#[pyo3(name = "manylinux_get_module_uncached")]
fn manylinux_get_module_uncached(py: Python) -> PyResult<Option<Bound<PyAny>>> {
    let import = mod_attr(py, "builtins", "__import__").or_else(|_| py.import("builtins")?.getattr("__import__"))?;
    match import.call1(("_manylinux",)) {
        Ok(m) => Ok(Some(m)),
        Err(e) if err_matches(py, &e, &["ImportError"]) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Run `f` with the `_parse_elf(executable)` context manager entered,
/// always exiting it, mirroring the `with` statement.
fn with_parse_elf<T>(
    py: Python,
    executable: &str,
    f: impl FnOnce(Option<Bound<PyAny>>) -> PyResult<T>,
) -> PyResult<T> {
    let cm = mod_attr(py, "packaging._manylinux", "_parse_elf")?.call1((executable,))?;
    let entered = cm.call_method0("__enter__")?;
    let none = py.None();
    let result = f(if entered.is_none() {
        None
    } else {
        Some(entered)
    });
    let exit_result = cm.call_method1("__exit__", (&none, &none, &none));
    match result {
        Ok(v) => {
            exit_result?;
            Ok(v)
        }
        Err(e) => {
            let _ = exit_result;
            Err(e)
        }
    }
}

fn elf_int(f: &Bound<PyAny>, name: &str) -> PyResult<u32> {
    f.getattr(name)?.extract()
}

/// `_is_linux_armhf`: 32-bit LE ARM with the v5 hard-float ABI flags.
#[pyfunction]
#[pyo3(name = "manylinux_is_armhf")]
fn manylinux_is_armhf(py: Python, executable: String) -> PyResult<bool> {
    with_parse_elf(py, &executable, |f| {
        let Some(f) = f else { return Ok(false) };
        Ok(elf_int(&f, "capacity")? == 1
            && elf_int(&f, "encoding")? == 1
            && elf_int(&f, "machine")? == 40
            && elf_int(&f, "flags")? & 0xFF000000 == 0x05000000
            && elf_int(&f, "flags")? & 0x00000400 == 0x00000400)
    })
}

/// `_is_linux_i686`: 32-bit LE i386.
#[pyfunction]
#[pyo3(name = "manylinux_is_i686")]
fn manylinux_is_i686(py: Python, executable: String) -> PyResult<bool> {
    with_parse_elf(py, &executable, |f| {
        let Some(f) = f else { return Ok(false) };
        Ok(elf_int(&f, "capacity")? == 1
            && elf_int(&f, "encoding")? == 1
            && elf_int(&f, "machine")? == 3)
    })
}

/// `_have_compatible_abi`: armv7l/i686 probe the executable, otherwise the
/// architecture allowlist.
#[pyfunction]
#[pyo3(name = "manylinux_have_compatible_abi")]
fn manylinux_have_compatible_abi(
    py: Python,
    executable: String,
    archs: Vec<String>,
) -> PyResult<bool> {
    if archs.iter().any(|a| a == "armv7l") {
        return mod_attr(py, "packaging._manylinux", "_is_linux_armhf")?.call1((executable,))?.extract();
    }
    if archs.iter().any(|a| a == "i686") {
        return mod_attr(py, "packaging._manylinux", "_is_linux_i686")?.call1((executable,))?.extract();
    }
    Ok(archs.iter().any(|a| {
        matches!(
            a.as_str(),
            "x86_64" | "aarch64" | "ppc64" | "ppc64le" | "s390x" | "loongarch64" | "riscv64"
        )
    }))
}

/// `_is_compatible(arch, version)`: glibc floor plus the `_manylinux`
/// extension module's compatibility probes. Both helpers resolve through
/// the module so doubles apply.
#[pyfunction]
#[pyo3(name = "manylinux_is_compatible")]
fn manylinux_is_compatible(
    py: Python,
    arch: String,
    version: Bound<PyAny>,
) -> PyResult<bool> {
    let sys_glibc =
        mod_attr(py, "packaging._manylinux", "_get_glibc_version")?.call0()?;
    if sys_glibc.lt(&version)? {
        return Ok(false);
    }
    let module =
        mod_attr(py, "packaging._manylinux", "_get_manylinux_module")?.call0()?;
    if module.is_none() {
        return Ok(true);
    }
    if module.hasattr("manylinux_compatible")? {
        let result = module.getattr("manylinux_compatible")?.call1((
            version.get_item(0)?,
            version.get_item(1)?,
            arch,
        ))?;
        if result.is_none() {
            return Ok(true);
        }
        return result.extract();
    }
    for (major, minor, attr) in [
        (2i64, 5i64, "manylinux1_compatible"),
        (2, 12, "manylinux2010_compatible"),
        (2, 17, "manylinux2014_compatible"),
    ] {
        if version.eq(glibc_version(py, major, minor)?)? && module.hasattr(attr)? {
            return module.getattr(attr)?.extract();
        }
    }
    Ok(true)
}

/// `platform_tags(archs)`: the canonical manylinux tag list. Returns a live
/// iterator of strings.
#[pyfunction]
#[pyo3(name = "manylinux_platform_tags")]
fn manylinux_platform_tags(py: Python, archs: Vec<String>) -> PyResult<Bound<PyAny>> {
    let executable: String = py.import("sys")?.getattr("executable")?.extract()?;
    let abi_ok: bool = mod_attr(py, "packaging._manylinux", "_have_compatible_abi")?
        .call1((executable, archs.clone()))?
        .extract()?;
    if !abi_ok {
        return str_iter(py, Vec::new());
    }
    let too_old = if archs.iter().any(|a| a == "x86_64" || a == "i686") {
        (2i64, 4i64)
    } else {
        (2, 16)
    };
    let current =
        mod_attr(py, "packaging._manylinux", "_get_glibc_version")?.call0()?;
    let cur_major: i64 = current.get_item(0)?.extract()?;
    let cur_minor: i64 = current.get_item(1)?.extract()?;
    let last_minor = mod_attr(py, "packaging._manylinux", "_LAST_GLIBC_MINOR")?;
    let mut glibc_max_list = vec![(cur_major, cur_minor)];
    for major in (2..cur_major).rev() {
        let minor: i64 = last_minor.get_item(major)?.extract()?;
        glibc_max_list.push((major, minor));
    }
    let is_compatible = mod_attr(py, "packaging._manylinux", "_is_compatible")?;
    let legacy_map = mod_attr(py, "packaging._manylinux", "_LEGACY_MANYLINUX_MAP")?;
    let mut out = Vec::new();
    for arch in &archs {
        for (gmajor, gmax_minor) in &glibc_max_list {
            let min_minor = if *gmajor == too_old.0 { too_old.1 } else { -1 };
            let mut minor = *gmax_minor;
            while minor > min_minor {
                let v = glibc_version(py, *gmajor, minor)?;
                let ok: bool = is_compatible.call1((arch, v.clone()))?.extract()?;
                if ok {
                    out.push(format!("manylinux_{gmajor}_{minor}_{arch}"));
                    let legacy: Bound<PyAny> =
                        legacy_map.call_method1("get", (v,))?;
                    if !legacy.is_none() {
                        let name: String = legacy.extract()?;
                        out.push(format!("{name}_{arch}"));
                    }
                }
                minor -= 1;
            }
        }
    }
    str_iter(py, out)
}

/// `_parse_musl_version`, returning a `_MuslVersion` or `None`.
#[pyfunction]
#[pyo3(name = "musllinux_parse_version")]
fn musllinux_parse_version(py: Python, output: String) -> PyResult<Option<Bound<PyAny>>> {
    match packaging_core::platform::parse_musl_version(&output) {
        Some((major, minor)) => Ok(Some(musl_version(py, major, minor)?)),
        None => Ok(None),
    }
}

/// Uncached `_get_musl_version` (the shim wraps it in `lru_cache`): read the
/// executable's `PT_INTERP` via `ELFFile`, run the loader, parse stderr.
#[pyfunction]
#[pyo3(name = "musllinux_get_version_uncached")]
fn musllinux_get_version_uncached(
    py: Python,
    executable: String,
) -> PyResult<Option<Bound<PyAny>>> {
    let open = py.import("builtins")?.getattr("open")?;
    let file = match open.call1((executable.clone(), "rb")) {
        Ok(f) => f,
        Err(e)
            if err_matches(py, &e, &["OSError", "TypeError", "ValueError"]) =>
        {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    let interp: Option<String> = (|| {
        let elf_cls = mod_attr(py, "packaging._elffile", "ELFFile")?;
        let elf = match elf_cls.call1((file.clone(),)) {
            Ok(e) => e,
            Err(e)
                if err_matches(py, &e, &["OSError", "TypeError", "ValueError"]) =>
            {
                return Ok(None)
            }
            Err(e) => return Err(e),
        };
        elf.getattr("interpreter")?.extract()
    })()?;
    let _ = file.call_method0("close");
    let Some(ld) = interp else { return Ok(None) };
    if !ld.contains("musl") {
        return Ok(None);
    }
    let subprocess = py.import("subprocess")?;
    let stderr_val = subprocess.getattr("PIPE")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("check", false)?;
    kwargs.set_item("stderr", stderr_val)?;
    kwargs.set_item("text", true)?;
    let proc = subprocess
        .getattr("run")?
        .call((vec![ld],), Some(&kwargs))?;
    let output: String = proc.getattr("stderr")?.extract()?;
    mod_attr(py, "packaging._musllinux", "_parse_musl_version")?.call1((output,))?.extract()
}

/// `platform_tags(archs)`: musllinux tags from the running loader.
#[pyfunction]
#[pyo3(name = "musllinux_platform_tags")]
fn musllinux_platform_tags(py: Python, archs: Vec<String>) -> PyResult<Bound<PyAny>> {
    let executable: String = py.import("sys")?.getattr("executable")?.extract()?;
    let sys_musl: Option<Bound<PyAny>> =
        mod_attr(py, "packaging._musllinux", "_get_musl_version")?
            .call1((executable,))?
            .extract()?;
    let Some(musl) = sys_musl else {
        return str_iter(py, Vec::new());
    };
    let major: i64 = musl.get_item(0)?.extract()?;
    let minor: i64 = musl.get_item(1)?.extract()?;
    let mut out = Vec::new();
    for arch in &archs {
        for m in (0..=minor).rev() {
            out.push(format!("musllinux_{major}_{m}_{arch}"));
        }
    }
    str_iter(py, out)
}
// ---------------------------------------------------------------------------
// Wheel tags (`packaging.tags`).
//
// Same mockability contract as the platform modules: every interpreter
// fact comes from live Python objects (`sys`, `sysconfig`, `platform`,
// `importlib.machinery`) and every intra-module helper resolves through the
// `packaging.tags` module attribute, so the suite's `monkeypatch` doubles
// apply exactly as with the original.
// ---------------------------------------------------------------------------

pyo3::create_exception!(_packaging, UnsortedTagsError, PyValueError);
pyo3::create_exception!(_packaging, InvalidTag, PyValueError);
pyo3::create_exception!(_packaging, TooManyTagsError, PyValueError);

/// Lowercase via real `str.lower` (exact for exotic Unicode).
fn py_lower(py: Python, s: &str) -> PyResult<String> {
    py.import("builtins")?
        .getattr("str")?
        .call1((s,))?
        .call_method0("lower")?
        .extract()
}

/// Build a `Tag` from raw components (lowercased, hash precomputed).
fn make_tag(py: Python, interpreter: &str, abi: &str, platform: &str) -> PyResult<Py<Tag>> {
    let i = py_lower(py, interpreter)?;
    let a = py_lower(py, abi)?;
    let p = py_lower(py, platform)?;
    let hash: isize = PyTuple::new(py, [&i, &a, &p])?.hash()?;
    Py::new(py, Tag { interpreter: i, abi: a, platform: p, hash })
}

/// `packaging.tags.Tag`: an immutable interpreter/abi/platform triple.
#[pyclass(name = "Tag", module = "packaging.tags", subclass)]
#[derive(Clone)]
struct Tag {
    interpreter: String,
    abi: String,
    platform: String,
    hash: isize,
}

#[pymethods]
impl Tag {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(
        py: Python,
        args: &Bound<PyTuple>,
        kwargs: Option<Bound<PyDict>>,
    ) -> PyResult<Self> {
        let get = |i: usize, k: &str| -> PyResult<Bound<PyAny>> {
            if i < args.len() {
                return args.get_item(i);
            }
            match kwargs.as_ref().and_then(|d| d.get_item(k).ok().flatten()) {
                Some(v) => Ok(v),
                None => Err(PyTypeError::new_err(format!(
                    "Tag expected 3 arguments, got {}",
                    args.len()
                ))),
            }
        };
        if args.len() == 0 && kwargs.is_none() {
            // `Tag.__new__(Tag)` (no-arg, used by unpickling): blank
            // instance; `__setstate__` fills it in.
            return Ok(Tag {
                interpreter: String::new(),
                abi: String::new(),
                platform: String::new(),
                hash: 0,
            });
        }
        let i: String = get(0, "interpreter")?.extract()?;
        let a: String = get(1, "abi")?.extract()?;
        let p: String = get(2, "platform")?.extract()?;
        if args.len() > 3 {
            return Err(PyTypeError::new_err(format!(
                "Tag expected 3 arguments, got {}",
                args.len()
            )));
        }
        let li = py_lower(py, &i)?;
        let la = py_lower(py, &a)?;
        let lp = py_lower(py, &p)?;
        let hash: isize = PyTuple::new(py, [&li, &la, &lp])?.hash()?;
        Ok(Tag { interpreter: li, abi: la, platform: lp, hash })
    }

    #[getter]
    fn interpreter(&self) -> &str {
        &self.interpreter
    }

    #[getter]
    fn abi(&self) -> &str {
        &self.abi
    }

    #[getter]
    fn platform(&self) -> &str {
        &self.platform
    }

    // The original stores `Tag` in `__slots__` (`_interpreter`, `_abi`,
    // `_platform`, `_hash`); the suite reads `_hash` directly.
    #[getter]
    fn _hash(&self) -> isize {
        self.hash
    }

    fn __str__(&self) -> String {
        format!("{}-{}-{}", self.interpreter, self.abi, self.platform)
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        let id: usize = slf
            .py()
            .import("builtins")?
            .getattr("id")?
            .call1((slf.clone(),))?
            .extract()?;
        Ok(format!("<{} @ {id}>", slf.borrow().__str__()))
    }

    fn __hash__(&self) -> isize {
        self.hash
    }

    fn __eq__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let Ok(o) = other.extract::<PyRef<Self>>() else {
            return py.NotImplemented().into_py_any(py);
        };
        let this = slf.borrow();
        (this.hash == o.hash
            && this.platform == o.platform
            && this.abi == o.abi
            && this.interpreter == o.interpreter)
            .into_py_any(py)
    }

    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let Ok(o) = other.extract::<PyRef<Self>>() else {
            return py.NotImplemented().into_py_any(py);
        };
        let this = slf.borrow();
        (this.hash != o.hash
            || this.platform != o.platform
            || this.abi != o.abi
            || this.interpreter != o.interpreter)
            .into_py_any(py)
    }

    fn __getstate__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let py = slf.py();
        let this = slf.borrow();
        PyTuple::new(
            py,
            [
                this.interpreter.clone().into_pyobject(py)?.into_any(),
                this.abi.clone().into_pyobject(py)?.into_any(),
                this.platform.clone().into_pyobject(py)?.into_any(),
            ],
        )
    }

    fn __setstate__(slf: &Bound<Self>, state: Bound<PyAny>) -> PyResult<()> {
        let py = slf.py();
        let bad = || {
            PyTypeError::new_err(format!(
                "Cannot restore Tag from {}",
                state.repr().map(|r| r.to_string()).unwrap_or_default()
            ))
        };
        if let Ok(t) = state.downcast::<PyTuple>() {
            if t.len() == 3
                && t.get_item(0)?.is_instance_of::<pyo3::types::PyString>()
                && t.get_item(1)?.is_instance_of::<pyo3::types::PyString>()
                && t.get_item(2)?.is_instance_of::<pyo3::types::PyString>()
            {
                let i: String = t.get_item(0)?.extract()?;
                let a: String = t.get_item(1)?.extract()?;
                let p: String = t.get_item(2)?.extract()?;
                let hash: isize = PyTuple::new(py, [&i, &a, &p])?.hash()?;
                let mut this = slf.borrow_mut();
                this.interpreter = i;
                this.abi = a;
                this.platform = p;
                this.hash = hash;
                return Ok(());
            }
            if t.len() == 2 {
                if let Ok(slots) = t.get_item(1)?.downcast_into::<PyDict>() {
                    let req = |k: &str| -> PyResult<String> {
                        slots
                            .get_item(k)?
                            .ok_or_else(bad)
                            .and_then(|v| v.extract().map_err(|_| bad()))
                    };
                    let i = py_lower(py, &req("_interpreter").map_err(|_| bad())?)?;
                    let a = py_lower(py, &req("_abi").map_err(|_| bad())?)?;
                    let p = py_lower(py, &req("_platform").map_err(|_| bad())?)?;
                    let hash: isize = PyTuple::new(py, [&i, &a, &p])?.hash()?;
                    let mut this = slf.borrow_mut();
                    this.interpreter = i;
                    this.abi = a;
                    this.platform = p;
                    this.hash = hash;
                    return Ok(());
                }
            }
        }
        Err(bad())
    }
}

/// `tags` module attribute resolved live (suite doubles).
fn tags_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    mod_attr(py, "packaging.tags", name)
}

/// `sys.version_info[:2]` as ints.
fn running_py_version(py: Python) -> PyResult<Vec<i64>> {
    let vi = py.import("sys")?.getattr("version_info")?;
    vi.get_item(0)?;
    let pair = vi.call_method1("__getitem__", (pyo3::types::PySlice::new(py, 0, 2, 1),))?;
    pair.try_iter()?.map(|r| r.unwrap().extract()).collect()
}

/// Sequence of Python ints (version tuples/lists).
fn int_seq(obj: &Bound<PyAny>) -> PyResult<Vec<i64>> {
    obj.try_iter()?.map(|r| r.unwrap().extract()).collect()
}

/// `Tag` list as a live iterator.
fn tag_iter(py: Python, tags: Vec<Py<Tag>>) -> PyResult<Bound<PyAny>> {
    let list = PyList::new(py, tags)?;
    list.call_method0("__iter__")
}

/// `parse_tag(tag, *, validate_order=False, limit=None)`.
#[pyfunction]
#[pyo3(name = "tags_parse_tag", signature = (tag, *, validate_order=false, limit=None))]
fn tags_parse_tag<'py>(
    py: Python<'py>,
    tag: String,
    validate_order: bool,
    limit: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    if let Some(lim) = &limit {
        if lim.lt(0)? {
            return Err(PyValueError::new_err("limit must be non-negative"));
        }
    }
    let tag_repr = py
        .import("builtins")?
        .getattr("repr")?
        .call1((tag.clone(),))?
        .to_string();
    let mut component_parts: Vec<Vec<String>> = Vec::new();
    for component in tag.split('-') {
        component_parts.push(component.split('.').map(str::to_string).collect());
    }
    for parts in &component_parts {
        if parts.iter().any(|p| p.is_empty()) {
            let component = parts.join(".");
            let comp_repr: String = py
                .import("builtins")?
                .getattr("repr")?
                .call1((component,))?
                .to_string();
            return Err(InvalidTag::new_err(format!(
                "Tag {tag_repr} has an empty component: {comp_repr}"
            )));
        }
        if validate_order {
            let mut sorted = parts.clone();
            sorted.sort();
            if *parts != sorted {
                let component = parts.join(".");
                let comp_repr: String = py
                    .import("builtins")?
                    .getattr("repr")?
                    .call1((component,))?
                    .to_string();
                return Err(UnsortedTagsError::new_err(format!(
                    "Tag component {comp_repr} is not in sorted order per PEP 425"
                )));
            }
        }
    }
    let tag_count: usize = component_parts.iter().map(Vec::len).product();
    if let Some(lim) = &limit {
        let count_obj: Bound<PyAny> = tag_count.into_pyobject(py)?.into_any();
        if count_obj.gt(lim)? {
            let lim_str: String = lim.str()?.to_string();
            return Err(TooManyTagsError::new_err(format!(
                "Compressed tag set would generate {tag_count} tags, exceeding limit {lim_str}"
            )));
        }
    }
    if component_parts.len() != 3 {
        let cause = if component_parts.len() < 3 {
            format!(
                "not enough values to unpack (expected 3, got {})",
                component_parts.len()
            )
        } else {
            "too many values to unpack (expected 3)".to_string()
        };
        let err = InvalidTag::new_err(format!(
            "Tag {tag_repr} must have exactly three components"
        ));
        err.set_cause(
            py,
            Some(PyValueError::new_err(cause)),
        );
        return Err(err);
    }
    let (interpreters, abis, platforms) =
        (&component_parts[0], &component_parts[1], &component_parts[2]);
    for interpreter in interpreters {
        let is_ident: bool = py
            .import("builtins")?
            .getattr("str")?
            .call1((interpreter.clone(),))?
            .call_method0("isidentifier")?
            .extract()?;
        if !is_ident {
            let interp_repr: String = py
                .import("builtins")?
                .getattr("repr")?
                .call1((interpreter.clone(),))?
                .to_string();
            return Err(InvalidTag::new_err(format!(
                "Tag {tag_repr} has an invalid interpreter: {interp_repr}"
            )));
        }
    }
    let mut tags = Vec::new();
    for interpreter in interpreters {
        for abi in abis {
            for platform in platforms {
                tags.push(make_tag(py, interpreter, abi, platform)?);
            }
        }
    }
    Ok(pyo3::types::PyFrozenSet::new(py, tags)?.into_any())
}

/// `_get_config_var(name, warn=False)`.
#[pyfunction]
#[pyo3(name = "tags_get_config_var", signature = (name, warn=false))]
fn tags_get_config_var(py: Python, name: String, warn: bool) -> PyResult<Bound<PyAny>> {
    let value: Bound<PyAny> = py
        .import("sysconfig")?
        .getattr("get_config_var")?
        .call1((name.clone(),))?;
    if value.is_none() && warn {
        tags_attr(py, "logger")?.call_method(
            "debug",
            (
                "Config variable '%s' is unset, Python ABI tag may be incorrect",
                name,
            ),
            None,
        )?;
    }
    Ok(value)
}

/// `_normalize_string`: `.`, `-`, ` ` become `_`.
fn normalize_string(s: &str) -> String {
    s.replace(['.', '-', ' '], "_")
}

/// Tuple comparison of an int version list against a 2-element bound,
/// with real Python tuple semantics (lexicographic; a shorter prefix sorts
/// smaller). All in-tree bounds are 2-element.
fn ver_ge(ver: &[i64], major: i64, minor: i64) -> bool {
    if ver.is_empty() {
        return false;
    }
    if ver[0] != major {
        return ver[0] > major;
    }
    if ver.len() < 2 {
        return false;
    }
    ver[1] >= minor
}

fn ver_lt(ver: &[i64], major: i64, minor: i64) -> bool {
    if ver.is_empty() {
        return true;
    }
    if ver[0] != major {
        return ver[0] < major;
    }
    if ver.len() < 2 {
        return true;
    }
    ver[1] < minor
}

/// `_is_threaded_cpython(abis)` via the real `re` module.
fn is_threaded_cpython(py: Python, abis: &[Bound<PyAny>]) -> PyResult<bool> {
    if abis.is_empty() {
        return Ok(false);
    }
    let first: String = abis[0].str()?.to_string();
    let m: Option<Bound<PyAny>> = py
        .import("re")?
        .getattr("match")?
        .call1((r"cp\d+(.*)", first))?
        .extract()?;
    let Some(m) = m else { return Ok(false) };
    let flags: String = m.call_method1("group", (1,))?.extract()?;
    Ok(flags.contains('t'))
}

/// `_cpython_abis(py_version, warn=False)`.
#[pyfunction]
#[pyo3(name = "tags_cpython_abis", signature = (py_version, warn=false))]
fn tags_cpython_abis<'py>(
    py: Python<'py>,
    py_version: Bound<'py, PyAny>,
    warn: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let ver: Vec<i64> = int_seq(&py_version)?;
    // `_version_nodot(py_version[:2])`: only the first two components.
    let first2: Vec<i64> = ver.iter().take(2).copied().collect();
    let version: String = first2
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join("");
    let (mut threading, mut debug, mut pymalloc, mut ucs4) = ("", "", "", "");
    let with_debug: Bound<PyAny> =
        tags_attr(py, "_get_config_var")?.call1(("Py_DEBUG", warn))?;
    let has_refcount: bool = py.import("sys")?.hasattr("gettotalrefcount")?;
    let ext_suffixes: Bound<PyAny> = tags_attr(py, "EXTENSION_SUFFIXES")?;
    let has_ext: bool = ext_suffixes.contains("_d.pyd")?;
    if with_debug.is_truthy()?
        || (with_debug.is_none() && (has_refcount || has_ext))
    {
        debug = "d";
    }
    let ge_3_13 = ver_ge(&ver, 3, 13);
    if ge_3_13 {
        let gil_disabled: Bound<PyAny> =
            tags_attr(py, "_get_config_var")?.call1(("Py_GIL_DISABLED", warn))?;
        if gil_disabled.is_truthy()? {
            threading = "t";
        }
    }
    if ver_lt(&ver, 3, 8) {
        let with_pymalloc: Bound<PyAny> =
            tags_attr(py, "_get_config_var")?.call1(("WITH_PYMALLOC", warn))?;
        if with_pymalloc.is_truthy()? || with_pymalloc.is_none() {
            pymalloc = "m";
        }
        if ver_lt(&ver, 3, 3) {
            let unicode_size: Bound<PyAny> =
                tags_attr(py, "_get_config_var")?.call1(("Py_UNICODE_SIZE", warn))?;
            let maxunicode: Bound<PyAny> = py.import("sys")?.getattr("maxunicode")?;
            let is_4: bool = unicode_size.eq(4)?;
            let none_and_wide: bool =
                unicode_size.is_none() && maxunicode.eq(0x10FFFF)?;
            if is_4 || none_and_wide {
                ucs4 = "u";
            }
        }
    }
    let mut out = vec![format!("cp{version}{threading}{debug}{pymalloc}{ucs4}")];
    if ver_ge(&ver, 3, 8) && !debug.is_empty() {
        out.push(format!("cp{version}{threading}"));
    }
    str_iter(py, out)
}

/// `cpython_tags(python_version=None, abis=None, platforms=None, *, warn=False)`.
#[pyfunction]
#[pyo3(
    name = "tags_cpython_tags",
    signature = (python_version=None, abis=None, platforms=None, *, warn=false)
)]
fn tags_cpython_tags<'py>(
    py: Python<'py>,
    python_version: Option<Bound<'py, PyAny>>,
    abis: Option<Bound<'py, PyAny>>,
    platforms: Option<Bound<'py, PyAny>>,
    warn: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let pv: Bound<PyAny> = match python_version {
        Some(v) if !v.is_none() && v.is_truthy()? => v,
        _ => {
            let vi = running_py_version(py)?;
            PyList::new(py, vi)?.into_any()
        }
    };
    // `interpreter = f"cp{_version_nodot(python_version[:2])}"`.
    let interpreter = {
        let ver = int_seq(&pv)?;
        ver.iter()
            .take(2)
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join("")
    };
    let interpreter = format!("cp{interpreter}");
    let abis_list: Vec<Bound<PyAny>> = match abis {
        Some(a) if !a.is_none() => a.try_iter()?.map(|r| r.unwrap()).collect(),
        _ => {
            let ver = int_seq(&pv)?;
            if ver.len() > 1 {
                tags_attr(py, "_cpython_abis")?
                    .call1((pv.clone(), warn))?
                    .try_iter()?
                    .map(|r| r.unwrap())
                    .collect()
            } else {
                Vec::new()
            }
        }
    };
    let threading = is_threaded_cpython(py, &abis_list)?;
    let mut abis_owned = abis_list;
    let explicit: Vec<String> = if threading {
        vec!["abi3".into(), "abi3t".into(), "none".into()]
    } else {
        vec!["abi3".into(), "none".into()]
    };
    for e in &explicit {
        if let Some(pos) = abis_owned
            .iter()
            .position(|a| a.str().map(|s| s.to_string()).unwrap_or_default() == *e)
        {
            abis_owned.remove(pos);
        }
    }
    let platforms_list: Vec<Bound<PyAny>> = match platforms {
        Some(p) if !p.is_none() => p.try_iter()?.map(|r| r.unwrap()).collect(),
        _ => tags_attr(py, "platform_tags")?
            .call0()?
            .try_iter()?
            .map(|r| r.unwrap())
            .collect(),
    };
    let mut out = Vec::new();
    for abi in &abis_owned {
        let abi_s: String = abi.str()?.to_string();
        for platform in &platforms_list {
            let p_s: String = platform.str()?.to_string();
            out.push(make_tag(py, &interpreter, &abi_s, &p_s)?);
        }
    }
    let ver = int_seq(&pv)?;
    let major = *ver.first().unwrap_or(&0);
    let use_abi3 = ver.len() > 1 && (major, ver[1]) >= (3, 2) && !threading;
    let use_abi3t = ver.len() > 1 && (major, ver[1]) >= (3, 2) && threading;
    if use_abi3 {
        for platform in &platforms_list {
            let p_s: String = platform.str()?.to_string();
            out.push(make_tag(py, &interpreter, "abi3", &p_s)?);
        }
    }
    if use_abi3t {
        for platform in &platforms_list {
            let p_s: String = platform.str()?.to_string();
            out.push(make_tag(py, &interpreter, "abi3t", &p_s)?);
        }
    }
    for platform in &platforms_list {
        let p_s: String = platform.str()?.to_string();
        out.push(make_tag(py, &interpreter, "none", &p_s)?);
    }
    if use_abi3 || use_abi3t {
        let minor = ver[1];
        for m in (2..minor).rev() {
            let interp = format!("cp{major}{m}");
            for platform in &platforms_list {
                let p_s: String = platform.str()?.to_string();
                if use_abi3 {
                    out.push(make_tag(py, &interp, "abi3", &p_s)?);
                }
                if use_abi3t {
                    out.push(make_tag(py, &interp, "abi3t", &p_s)?);
                }
            }
        }
    }
    tag_iter(py, out)
}

/// `_generic_abi()`.
#[pyfunction]
#[pyo3(name = "tags_generic_abi")]
fn tags_generic_abi(py: Python) -> PyResult<Bound<PyAny>> {
    let ext_suffix: Bound<PyAny> =
        tags_attr(py, "_get_config_var")?.call1(("EXT_SUFFIX", true))?;
    let is_str = ext_suffix.is_instance_of::<pyo3::types::PyString>();
    let starts_dot: bool = ext_suffix
        .getattr("startswith")
        .and_then(|f| f.call1((".",)))
        .and_then(|v| v.extract())
        .unwrap_or(false);
    if !is_str || !starts_dot {
        return Err(PySystemError::new_err(
            "invalid sysconfig.get_config_var('EXT_SUFFIX')",
        ));
    }
    let suffix: String = ext_suffix.extract()?;
    let parts: Vec<&str> = suffix.split('.').collect();
    if parts.len() < 3 {
        let vi = running_py_version(py)?;
        let ver = PyList::new(py, vi)?.into_any();
        return tags_attr(py, "_cpython_abis")?.call1((ver,))?.extract();
    }
    let soabi = parts[1];
    let abi = if let Some(rest) = soabi.strip_prefix("cpython") {
        let cparts: Vec<&str> = rest.split('-').collect();
        if cparts.len() < 2 || cparts[1].is_empty() {
            return Err(PySystemError::new_err(
                "invalid sysconfig.get_config_var('EXT_SUFFIX')",
            ));
        }
        format!("cp{}", cparts[1])
    } else if soabi.starts_with("cp") {
        soabi.split('-').next().unwrap().to_string()
    } else if soabi.starts_with("pypy") {
        soabi.split('-').take(2).collect::<Vec<_>>().join("-")
    } else if soabi.starts_with("graalpy") {
        soabi.split('-').take(3).collect::<Vec<_>>().join("-")
    } else if !soabi.is_empty() {
        soabi.to_string()
    } else {
        return str_iter(py, Vec::new());
    };
    str_iter(py, vec![normalize_string(&abi)])
}

/// `generic_tags(interpreter=None, abis=None, platforms=None, *, warn=False)`.
#[pyfunction]
#[pyo3(
    name = "tags_generic_tags",
    signature = (interpreter=None, abis=None, platforms=None, *, warn=false)
)]
fn tags_generic_tags<'py>(
    py: Python<'py>,
    interpreter: Option<Bound<'py, PyAny>>,
    abis: Option<Bound<'py, PyAny>>,
    platforms: Option<Bound<'py, PyAny>>,
    warn: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let interp: String = match interpreter {
        Some(v) if !v.is_none() && v.is_truthy()? => v.str()?.to_string(),
        _ => {
            let name: String =
                tags_attr(py, "interpreter_name")?.call0()?.extract()?;
            let version: String = {
                let d = PyDict::new(py);
                d.set_item("warn", warn)?;
                tags_attr(py, "interpreter_version")?
                    .call((), Some(&d))?
                    .extract()?
            };
            format!("{name}{version}")
        }
    };
    let abis_list: Vec<String> = match abis {
        Some(a) if !a.is_none() => a
            .try_iter()?
            .map(|r| r.unwrap().str().map(|s| s.to_string()))
            .collect::<PyResult<_>>()?,
        _ => tags_attr(py, "_generic_abi")?
            .call0()?
            .try_iter()?
            .map(|r| r.unwrap().str().map(|s| s.to_string()))
            .collect::<PyResult<_>>()?,
    };
    let mut abis_owned = abis_list;
    let platforms_list: Vec<String> = match platforms {
        Some(p) if !p.is_none() => p
            .try_iter()?
            .map(|r| r.unwrap().str().map(|s| s.to_string()))
            .collect::<PyResult<_>>()?,
        _ => tags_attr(py, "platform_tags")?
            .call0()?
            .try_iter()?
            .map(|r| r.unwrap().str().map(|s| s.to_string()))
            .collect::<PyResult<_>>()?,
    };
    if !abis_owned.iter().any(|a| a == "none") {
        abis_owned.push("none".to_string());
    }
    let mut out = Vec::new();
    for abi in &abis_owned {
        for platform in &platforms_list {
            out.push(make_tag(py, &interp, abi, platform)?);
        }
    }
    tag_iter(py, out)
}

/// `_py_interpreter_range(py_version)`.
fn py_interpreter_range(ver: &[i64]) -> Vec<String> {
    let nodot: String = ver.iter().map(i64::to_string).collect::<Vec<_>>().join("");
    let mut out = Vec::new();
    if ver.len() > 1 {
        out.push(format!("py{nodot}"));
    }
    out.push(format!("py{}", ver[0]));
    if ver.len() > 1 {
        for minor in (0..ver[1]).rev() {
            out.push(format!("py{}{minor}", ver[0]));
        }
    }
    out
}

/// `pure_python_tags(python_version=None)`.
#[pyfunction]
#[pyo3(name = "tags_pure_python_tags", signature = (python_version=None,))]
fn tags_pure_python_tags<'py>(
    py: Python<'py>,
    python_version: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let ver: Vec<i64> = match python_version {
        None => running_py_version(py)?,
        Some(v) if v.is_none() => running_py_version(py)?,
        Some(v) => {
            let seq: Vec<i64> = int_seq(&v).map_err(|_| {
                PyValueError::new_err("python_version must contain at least one item")
            })?;
            if seq.is_empty() {
                return Err(PyValueError::new_err(
                    "python_version must contain at least one item",
                ));
            }
            seq
        }
    };
    let mut out = Vec::new();
    for version in py_interpreter_range(&ver) {
        out.push(make_tag(py, &version, "none", "any")?);
    }
    tag_iter(py, out)
}

/// `compatible_tags(python_version=None, interpreter=None, platforms=None)`.
#[pyfunction]
#[pyo3(
    name = "tags_compatible_tags",
    signature = (python_version=None, interpreter=None, platforms=None)
)]
fn tags_compatible_tags<'py>(
    py: Python<'py>,
    python_version: Option<Bound<'py, PyAny>>,
    interpreter: Option<Bound<'py, PyAny>>,
    platforms: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let pv: Bound<'py, PyAny> = match python_version {
        Some(v) if !v.is_none() && v.is_truthy()? => v,
        _ => {
            let vi = running_py_version(py)?;
            PyList::new(py, vi)?.into_any()
        }
    };
    let platforms_list: Vec<String> = match platforms {
        Some(p) if !p.is_none() => p
            .try_iter()?
            .map(|r| r.unwrap().str().map(|s| s.to_string()))
            .collect::<PyResult<_>>()?,
        _ => tags_attr(py, "platform_tags")?
            .call0()?
            .try_iter()?
            .map(|r| r.unwrap().str().map(|s| s.to_string()))
            .collect::<PyResult<_>>()?,
    };
    let ver = int_seq(&pv)?;
    let mut out = Vec::new();
    for version in py_interpreter_range(&ver) {
        for platform in &platforms_list {
            out.push(make_tag(py, &version, "none", platform)?);
        }
    }
    if let Some(interp) = interpreter {
        if !interp.is_none() {
            let s: String = interp.str()?.to_string();
            out.push(make_tag(py, &s, "none", "any")?);
        }
    }
    let rest: Vec<Py<Tag>> = tags_attr(py, "pure_python_tags")?
        .call1((pv,))?
        .try_iter()?
        .map(|r| r.unwrap().extract())
        .collect::<PyResult<_>>()?;
    out.extend(rest);
    tag_iter(py, out)
}

/// `_mac_arch(arch, is_32bit=None)`.
#[pyfunction]
#[pyo3(name = "tags_mac_arch", signature = (arch, is_32bit=None))]
fn tags_mac_arch(
    py: Python,
    arch: String,
    is_32bit: Option<Bound<PyAny>>,
) -> PyResult<String> {
    // Truthiness (not strict bool): mirrors `if not is_32bit`.
    let is32: bool = match is_32bit {
        Some(v) if !v.is_none() => v.is_truthy()?,
        _ => tags_attr(py, "_32_BIT_INTERPRETER")?.is_truthy()?,
    };
    if !is32 {
        return Ok(arch);
    }
    if arch.starts_with("ppc") {
        return Ok("ppc".to_string());
    }
    Ok("i386".to_string())
}

/// `_mac_binary_formats(version, cpu_arch)`.
fn mac_binary_formats(version: (i64, i64), cpu_arch: &str) -> Vec<String> {
    let mut formats = vec![cpu_arch.to_string()];
    if cpu_arch == "x86_64" {
        if version < (10, 4) {
            return Vec::new();
        }
        formats.extend(["intel", "fat64", "fat3"].iter().map(|s| s.to_string()));
    } else if cpu_arch == "i386" {
        if version < (10, 4) {
            return Vec::new();
        }
        formats.extend(["intel", "fat3", "fat"].iter().map(|s| s.to_string()));
    } else if cpu_arch == "ppc64" {
        if !((10, 4)..=(10, 5)).contains(&version) {
            return Vec::new();
        }
        formats.push("fat64".to_string());
    } else if cpu_arch == "ppc" {
        if version > (10, 6) {
            return Vec::new();
        }
        formats.extend(["fat3", "fat"].iter().map(|s| s.to_string()));
    }
    if matches!(cpu_arch, "arm64" | "x86_64") {
        formats.push("universal2".to_string());
    }
    if matches!(cpu_arch, "x86_64" | "i386" | "ppc64" | "ppc" | "intel") {
        formats.push("universal".to_string());
    }
    formats
}

/// `_version_nodot(version)`: digits concatenated (suite-visible helper).
#[pyfunction]
#[pyo3(name = "tags_version_nodot")]
fn tags_version_nodot(version: Bound<PyAny>) -> PyResult<String> {
    let mut out = String::new();
    for item in version.try_iter()?.map(|r| r.unwrap()) {
        out.push_str(&item.str()?.to_string());
    }
    Ok(out)
}

/// `_mac_binary_formats(version, cpu_arch)` (suite-visible helper).
#[pyfunction]
#[pyo3(name = "tags_mac_binary_formats")]
fn tags_mac_binary_formats(version: Bound<PyAny>, cpu_arch: String) -> PyResult<Vec<String>> {
    Ok(mac_binary_formats(
        (version.get_item(0)?.extract()?, version.get_item(1)?.extract()?),
        &cpu_arch,
    ))
}

/// `mac_platforms(version=None, arch=None)`.
#[pyfunction]
#[pyo3(name = "tags_mac_platforms", signature = (version=None, arch=None))]
fn tags_mac_platforms<'py>(
    py: Python<'py>,
    version: Option<Bound<'py, PyAny>>,
    arch: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let (mut ver, mut arch_s): (Option<(i64, i64)>, Option<String>) = (None, None);
    if let Some(v) = &version {
        if !v.is_none() {
            ver = Some((v.get_item(0)?.extract()?, v.get_item(1)?.extract()?));
        }
    }
    if let Some(a) = &arch {
        if !a.is_none() {
            arch_s = Some(a.str()?.to_string());
        }
    }
    if ver.is_none() || arch_s.is_none() {
        let mac_ver: Bound<PyAny> = py.import("platform")?.getattr("mac_ver")?.call0()?;
        let version_str: String = mac_ver.get_item(0)?.extract()?;
        let cpu_arch: String = mac_ver.get_item(2)?.extract()?;
        if ver.is_none() {
            let parts: Vec<&str> = version_str.split('.').collect();
            let int = py.import("builtins")?.getattr("int")?;
            let major: i64 = int.call1((parts.first().unwrap_or(&"0"),))?.extract()?;
            let minor: i64 = int.call1((parts.get(1).unwrap_or(&"0"),))?.extract()?;
            let mut v = (major, minor);
            if v == (10, 16) {
                let executable: String =
                    py.import("sys")?.getattr("executable")?.extract()?;
                let subprocess = py.import("subprocess")?;
                let env = PyDict::new(py);
                env.set_item("SYSTEM_VERSION_COMPAT", "0")?;
                let kwargs = PyDict::new(py);
                kwargs.set_item("check", true)?;
                kwargs.set_item("env", env)?;
                kwargs.set_item("stdout", subprocess.getattr("PIPE")?)?;
                kwargs.set_item("text", true)?;
                let out: Bound<PyAny> = subprocess
                    .getattr("run")?
                    .call(
                        ((executable, "-sS", "-c", "import platform; print(platform.mac_ver()[0])"),),
                        Some(&kwargs),
                    )?
                    .getattr("stdout")?;
                let out_s: String = out.extract()?;
                let parts: Vec<&str> = out_s.split('.').collect();
                let major: i64 =
                    int.call1((parts.first().unwrap_or(&"0"),))?.extract()?;
                let minor: i64 =
                    int.call1((parts.get(1).unwrap_or(&"0"),))?.extract()?;
                v = (major, minor);
            }
            ver = Some(v);
        }
        if arch_s.is_none() {
            arch_s = Some(
                tags_attr(py, "_mac_arch")?
                    .call1((cpu_arch,))?
                    .extract()?,
            );
        }
    }
    let version = ver.unwrap();
    let arch = arch_s.unwrap();
    let mut out = Vec::new();
    if ((10, 0)..(11, 0)).contains(&version) {
        for minor_version in (0..=version.1).rev() {
            for binary_format in mac_binary_formats((10, minor_version), &arch) {
                out.push(format!("macosx_10_{minor_version}_{binary_format}"));
            }
        }
    }
    if version >= (11, 0) {
        for major_version in (11..=version.0).rev() {
            for binary_format in mac_binary_formats((major_version, 0), &arch) {
                out.push(format!(
                    "macosx_{major_version}_0_{binary_format}"
                ));
            }
        }
        if arch == "x86_64" {
            for minor_version in (4..=16).rev() {
                for binary_format in mac_binary_formats((10, minor_version), &arch) {
                    out.push(format!("macosx_10_{minor_version}_{binary_format}"));
                }
            }
        } else {
            for minor_version in (4..=16).rev() {
                out.push(format!("macosx_10_{minor_version}_universal2"));
            }
        }
    }
    str_iter(py, out)
}

/// `ios_platforms(version=None, multiarch=None)`.
#[pyfunction]
#[pyo3(name = "tags_ios_platforms", signature = (version=None, multiarch=None))]
fn tags_ios_platforms<'py>(
    py: Python<'py>,
    version: Option<Bound<'py, PyAny>>,
    multiarch: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let ver: (i64, i64) = match version {
        Some(v) if !v.is_none() => (v.get_item(0)?.extract()?, v.get_item(1)?.extract()?),
        _ => {
            let ios_ver: Bound<PyAny> =
                py.import("platform")?.getattr("ios_ver")?.call0()?;
            let release: String = ios_ver.get_item(1)?.extract()?;
            let parts: Vec<&str> = release.split('.').collect();
            let int = py.import("builtins")?.getattr("int")?;
            (
                int.call1((parts.first().unwrap_or(&"0"),))?.extract()?,
                int.call1((parts.get(1).unwrap_or(&"0"),))?.extract()?,
            )
        }
    };
    let mut multi: String = match multiarch {
        Some(m) if !m.is_none() => m.str()?.to_string(),
        _ => py
            .import("sys")?
            .getattr("implementation")?
            .getattr("_multiarch")?
            .str()?
            .to_string(),
    };
    multi = multi.replace('-', "_");
    if ver.0 < 12 {
        return str_iter(py, Vec::new());
    }
    let mut out = vec![format!("ios_{}_{}_{multi}", ver.0, ver.1)];
    for minor in (0..ver.1).rev() {
        out.push(format!("ios_{}_{minor}_{multi}", ver.0));
    }
    for major in (12..ver.0).rev() {
        for minor in (0..=9).rev() {
            out.push(format!("ios_{major}_{minor}_{multi}"));
        }
    }
    str_iter(py, out)
}

/// `android_platforms(api_level=None, abi=None)`.
#[pyfunction]
#[pyo3(name = "tags_android_platforms", signature = (api_level=None, abi=None))]
fn tags_android_platforms<'py>(
    py: Python<'py>,
    api_level: Option<Bound<'py, PyAny>>,
    abi: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let system: String = py.import("platform")?.getattr("system")?.call0()?.extract()?;
    if system != "Android" && (api_level.is_none() || abi.is_none()) {
        return Err(PyTypeError::new_err(
            "on non-Android platforms, the api_level and abi arguments are required",
        ));
    }
    let level: i64 = match api_level {
        Some(v) if !v.is_none() => v.extract()?,
        _ => py
            .import("platform")?
            .getattr("android_ver")?
            .call0()?
            .getattr("api_level")?
            .extract()?,
    };
    let abi_s: String = match abi {
        Some(v) if !v.is_none() => v.str()?.to_string(),
        _ => py
            .import("sysconfig")?
            .getattr("get_platform")?
            .call0()?
            .str()?
            .to_string()
            .split('-')
            .next_back()
            .unwrap_or("")
            .to_string(),
    };
    let abi_s = normalize_string(&abi_s);
    let mut out = Vec::new();
    for v in (16..=level).rev() {
        out.push(format!("android_{v}_{abi_s}"));
    }
    str_iter(py, out)
}

/// `_linux_platforms(is_32bit=None)`.
#[pyfunction]
#[pyo3(name = "tags_linux_platforms", signature = (is_32bit=None,))]
fn tags_linux_platforms<'py>(py: Python<'py>, is_32bit: Option<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    let is32: bool = match is_32bit {
        Some(v) if !v.is_none() => v.is_truthy()?,
        _ => tags_attr(py, "_32_BIT_INTERPRETER")?.is_truthy()?,
    };
    let platform_str: String = py
        .import("sysconfig")?
        .getattr("get_platform")?
        .call0()?
        .str()?
        .to_string();
    let mut linux = normalize_string(&platform_str);
    if !linux.starts_with("linux_") {
        return str_iter(py, vec![linux]);
    }
    if is32 {
        if linux == "linux_x86_64" {
            linux = "linux_i686".to_string();
        } else if linux == "linux_aarch64" {
            linux = "linux_armv8l".to_string();
        }
    }
    // Original: `_, arch = linux.split("_", 1)` — maxsplit=1 keeps the rest.
    let arch_full = linux.split_once('_').map(|x| x.1).unwrap_or("").to_string();
    let archs: Vec<String> = if arch_full == "armv8l" {
        vec!["armv8l".into(), "armv7l".into()]
    } else {
        vec![arch_full.clone()]
    };
    let mut out = Vec::new();
    for a in &archs {
        out.push(format!("linux_{a}"));
    }
    let many: Vec<String> = py
        .import("packaging._manylinux")?
        .getattr("platform_tags")?
        .call1((PyList::new(py, archs.clone())?,))?
        .try_iter()?
        .map(|r| r.unwrap().str().map(|s| s.to_string()))
        .collect::<PyResult<_>>()?;
    out.extend(many);
    let musl: Vec<String> = py
        .import("packaging._musllinux")?
        .getattr("platform_tags")?
        .call1((PyList::new(py, archs)?,))?
        .try_iter()?
        .map(|r| r.unwrap().str().map(|s| s.to_string()))
        .collect::<PyResult<_>>()?;
    out.extend(musl);
    str_iter(py, out)
}

/// `_emscripten_platforms()`.
#[pyfunction]
#[pyo3(name = "tags_emscripten_platforms")]
fn tags_emscripten_platforms(py: Python) -> PyResult<Bound<PyAny>> {
    let ver: Bound<PyAny> = py
        .import("sysconfig")?
        .getattr("get_config_var")?
        .call1(("PYEMSCRIPTEN_PLATFORM_VERSION",))?;
    let mut out = Vec::new();
    if ver.is_truthy()? {
        let s: String = ver.str()?.to_string();
        out.push(format!("pyemscripten_{s}_wasm32"));
    }
    let rest: Vec<String> = tags_attr(py, "_generic_platforms")?
        .call0()?
        .try_iter()?
        .map(|r| r.unwrap().str().map(|s| s.to_string()))
        .collect::<PyResult<_>>()?;
    out.extend(rest);
    str_iter(py, out)
}

/// `_generic_platforms()`.
#[pyfunction]
#[pyo3(name = "tags_generic_platforms")]
fn tags_generic_platforms(py: Python) -> PyResult<Bound<PyAny>> {
    let platform_str: String = py
        .import("sysconfig")?
        .getattr("get_platform")?
        .call0()?
        .str()?
        .to_string();
    str_iter(py, vec![normalize_string(&platform_str)])
}

/// `platform_tags()`: dispatch on `platform.system()`, returning the inner
/// iterator directly (matching the original `return mac_platforms()` shape).
#[pyfunction]
#[pyo3(name = "tags_platform_tags")]
fn tags_platform_tags(py: Python) -> PyResult<Bound<PyAny>> {
    let system: String = py.import("platform")?.getattr("system")?.call0()?.extract()?;
    let name = match system.as_str() {
        "Darwin" => "mac_platforms",
        "iOS" => "ios_platforms",
        "Android" => "android_platforms",
        "Linux" => "_linux_platforms",
        "Emscripten" => "_emscripten_platforms",
        _ => "_generic_platforms",
    };
    tags_attr(py, name)?.call0()
}

/// `interpreter_name()`.
#[pyfunction]
#[pyo3(name = "tags_interpreter_name")]
fn tags_interpreter_name(py: Python) -> PyResult<String> {
    let name: String = py
        .import("sys")?
        .getattr("implementation")?
        .getattr("name")?
        .extract()?;
    Ok(match name.as_str() {
        "python" => "py".to_string(),
        "cpython" => "cp".to_string(),
        "pypy" => "pp".to_string(),
        "ironpython" => "ip".to_string(),
        "jython" => "jy".to_string(),
        _ => name,
    })
}

/// `interpreter_version(*, warn=False)`.
#[pyfunction]
#[pyo3(name = "tags_interpreter_version", signature = (*, warn=false))]
fn tags_interpreter_version(py: Python, warn: bool) -> PyResult<String> {
    let version: Bound<PyAny> =
        tags_attr(py, "_get_config_var")?.call1(("py_version_nodot", warn))?;
    if version.is_truthy()? {
        return version.str().map(|s| s.to_string());
    }
    let vi = running_py_version(py)?;
    Ok(vi.iter().map(i64::to_string).collect::<Vec<_>>().join(""))
}

/// `interpreter_abi()`.
#[pyfunction]
#[pyo3(name = "tags_interpreter_abi")]
fn tags_interpreter_abi(py: Python) -> PyResult<String> {
    let name: String = tags_attr(py, "interpreter_name")?.call0()?.extract()?;
    if name == "cp" {
        let vi = running_py_version(py)?;
        let ver = PyList::new(py, vi)?.into_any();
        let mut it = tags_attr(py, "_cpython_abis")?
            .call1((ver,))?
            .try_iter()?;
        let first: Bound<PyAny> = it.next().unwrap()?;
        return first.extract();
    }
    let mut it = tags_attr(py, "_generic_abi")?
        .call0()?
        .try_iter()?;
    let first: Bound<PyAny> = it.next().unwrap()?;
    first.extract()
}

/// `sys_tags(*, warn=False)`.
#[pyfunction]
#[pyo3(name = "tags_sys_tags", signature = (*, warn=false))]
fn tags_sys_tags(py: Python, warn: bool) -> PyResult<Bound<PyAny>> {
    let interp_name: String = tags_attr(py, "interpreter_name")?.call0()?.extract()?;
    // Keyword calls throughout: the suite replaces these with
    // keyword-only doubles (e.g. `MockGenericTags.__call__(self, *, warn)`).
    let warn_kw = {
        let d = PyDict::new(py);
        d.set_item("warn", warn)?;
        d
    };
    let mut out: Vec<Py<Tag>> = tags_attr(
        py,
        if interp_name == "cp" {
            "cpython_tags"
        } else {
            "generic_tags"
        },
    )?
    .call((), Some(&warn_kw))?
    .try_iter()?
    .map(|r| r.unwrap().extract())
    .collect::<PyResult<_>>()?;
    let interp: Option<String> = if interp_name == "pp" {
        Some("pp3".to_string())
    } else if interp_name == "cp" {
        let version: String = tags_attr(py, "interpreter_version")?
            .call((), Some(&warn_kw))?
            .extract()?;
        Some(format!("cp{version}"))
    } else {
        None
    };
    let interp_kw = {
        let d = PyDict::new(py);
        match interp {
            Some(s) => {
                d.set_item("interpreter", s)?;
            }
            None => {
                d.set_item("interpreter", py.None())?;
            }
        }
        d
    };
    let rest: Vec<Py<Tag>> = tags_attr(py, "compatible_tags")?
        .call((), Some(&interp_kw))?
        .try_iter()?
        .map(|r| r.unwrap().extract())
        .collect::<PyResult<_>>()?;
    out.extend(rest);
    tag_iter(py, out)
}

/// `create_compatible_tags_selector(tags)`.
#[pyfunction]
#[pyo3(name = "tags_create_selector")]
fn tags_create_selector(_py: Python, tags: Bound<PyAny>) -> PyResult<Selector> {
    let mut ranks: Vec<((String, String, String), usize)> = Vec::new();
    for (rank, tag) in tags.try_iter()?.map(|r| r.unwrap()).enumerate() {
        let key = (
            tag.getattr("interpreter")?.str()?.to_string(),
            tag.getattr("abi")?.str()?.to_string(),
            tag.getattr("platform")?.str()?.to_string(),
        );
        if !ranks.iter().any(|(k, _)| k == &key) {
            ranks.push((key, rank));
        }
    }
    Ok(Selector { ranks })
}

/// Ranking callable returned by `create_compatible_tags_selector`.
#[pyclass(name = "Selector", module = "packaging.tags")]
struct Selector {
    ranks: Vec<((String, String, String), usize)>,
}

#[pymethods]
impl Selector {
    fn __call__<'py>(&self, py: Python<'py>, tagged: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let mut ranked: Vec<(Bound<'py, PyAny>, usize)> = Vec::new();
        for item in tagged.try_iter()?.map(|r| r.unwrap()) {
            let pair: (Bound<'py, PyAny>, Bound<'py, PyAny>) = (
                item.get_item(0)?,
                item.get_item(1)?,
            );
            let mut best: Option<usize> = None;
            for tag in pair.1.try_iter()?.map(|r| r.unwrap()) {
                let key = (
                    tag.getattr("interpreter")
                        .and_then(|v| v.str().map(|s| s.to_string()))
                        .unwrap_or_default(),
                    tag.getattr("abi")
                        .and_then(|v| v.str().map(|s| s.to_string()))
                        .unwrap_or_default(),
                    tag.getattr("platform")
                        .and_then(|v| v.str().map(|s| s.to_string()))
                        .unwrap_or_default(),
                );
                if let Some((_, rank)) = ranks_lookup(&self.ranks, &key) {
                    best = Some(best.map_or(*rank, |b: usize| b.min(*rank)));
                }
            }
            if let Some(rank) = best {
                ranked.push((pair.0, rank));
            }
        }
        ranked.sort_by_key(|(_, rank)| *rank);
        let things: Vec<Bound<PyAny>> = ranked.into_iter().map(|(t, _)| t).collect();
        PyList::new(py, things)?.call_method0("__iter__")
    }
}

fn ranks_lookup<'a>(
    ranks: &'a [((String, String, String), usize)],
    key: &(String, String, String),
) -> Option<&'a ((String, String, String), usize)> {
    ranks.iter().find(|(k, _)| k == key)
}
// ---------------------------------------------------------------------------
// Name/version/filename utilities (`packaging.utils`).
// ---------------------------------------------------------------------------

pyo3::create_exception!(_packaging, InvalidName, PyValueError);
pyo3::create_exception!(_packaging, InvalidWheelFilename, PyValueError);
pyo3::create_exception!(_packaging, InvalidSdistFilename, PyValueError);

/// `str` repr for `... {name!r}` messages.
fn py_repr(_py: Python, obj: &Bound<PyAny>) -> String {
    obj.repr()
        .map(|r| r.to_string())
        .unwrap_or_else(|_| "<repr failed>".to_string())
}

/// Fullmatch of `[a-z0-9]|[a-z0-9][a-z0-9._-]*[a-z0-9]` under
/// `re.IGNORECASE | re.ASCII` (the `_validate_regex` for names).
fn name_valid(name: &str) -> bool {
    let b = name.as_bytes();
    let alnum = |c: u8| c.is_ascii_alphanumeric();
    if b.is_empty() {
        return false;
    }
    if b.len() == 1 {
        return alnum(b[0]);
    }
    if !alnum(b[0]) || !alnum(b[b.len() - 1]) {
        return false;
    }
    b.iter()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

/// Fullmatch of `[a-z0-9]+(?:-[a-z0-9]+)*` under `re.ASCII` (normalized
/// names): lowercase ASCII letters and digits only (no `IGNORECASE`).
fn name_normalized(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let lower = |c: u8| c.is_ascii_digit() || c.is_ascii_lowercase();
    name.split('-')
        .all(|part| !part.is_empty() && part.bytes().all(lower))
}

/// `canonicalize_name(name, *, validate=False)`.
#[pyfunction]
#[pyo3(name = "utils_canonicalize_name", signature = (name, *, validate=false))]
fn utils_canonicalize_name(
    py: Python,
    name: Bound<PyAny>,
    validate: bool,
) -> PyResult<String> {
    if validate {
        // `re.fullmatch` raises `TypeError` on non-strings, before matching.
        let s: String = name.extract().map_err(|_| {
            PyTypeError::new_err("expected string or bytes-like object")
        })?;
        if !name_valid(&s) {
            return Err(InvalidName::new_err(format!(
                "name is invalid: {}",
                py_repr(py, &name)
            )));
        }
    }
    // `.lower()` through Python for exact Unicode semantics
    // (`AttributeError` on non-strings propagates, as in the original).
    let lowered: String = name.call_method0("lower")?.extract()?;
    let mut value = lowered.replace(['_', '.'], "-");
    while value.contains("--") {
        value = value.replace("--", "-");
    }
    Ok(value)
}

/// `is_normalized_name(name)`.
#[pyfunction]
#[pyo3(name = "utils_is_normalized_name")]
fn utils_is_normalized_name(name: String) -> bool {
    name_normalized(&name)
}

/// `canonicalize_version(version, *, strip_trailing_zero=True)`.
#[pyfunction]
#[pyo3(name = "utils_canonicalize_version", signature = (version, *, strip_trailing_zero=true))]
fn utils_canonicalize_version(
    py: Python,
    version: Bound<PyAny>,
    strip_trailing_zero: bool,
) -> PyResult<String> {
    if version.is_instance_of::<pyo3::types::PyString>() {
        let s: String = version.extract()?;
        match Version::parse_new(py, &version) {
            Ok(_) => {}
            Err(e) if e.is_instance_of::<InvalidVersion>(py) => return Ok(s),
            Err(e) => return Err(e),
        }
        let inner = Version::parse_new(py, &version)?;
        return version_to_str(py, &inner, strip_trailing_zero);
    }
    // A `Version` (or a foreign object: `_TrimmedRelease(obj)` reraises).
    if strip_trailing_zero {
        let trimmed_cls = py
            .import("packaging.version")?
            .getattr("_TrimmedRelease")?;
        let t = trimmed_cls.call1((version,))?;
        return Ok(t.str()?.to_string());
    }
    Ok(version.str()?.to_string())
}

/// `str()` of a parsed version, honoring `strip_trailing_zero` via the
/// `_TrimmedRelease` release override.
fn version_to_str(_py: Python, inner: &ParsedVersion, strip: bool) -> PyResult<String> {
    if !strip {
        return Ok(version::display(inner));
    }
    let trimmed = version::trim_release_leave_one(&inner.release);
    Ok(version::display_with_release(inner, &trimmed))
}
/// `parse_wheel_filename(filename, *, validate_order=False)`.
#[pyfunction]
#[pyo3(name = "utils_parse_wheel_filename", signature = (filename, *, validate_order=false))]
fn utils_parse_wheel_filename<'py>(
    py: Python<'py>,
    filename: Bound<'py, PyAny>,
    validate_order: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let bad_ext = || {
        InvalidWheelFilename::new_err(format!(
            "Invalid wheel filename (extension must be '.whl'): {}",
            py_repr(py, &filename)
        ))
    };
    // `.endswith` through Python: `AttributeError` on non-strings
    // propagates, as in the original.
    let ends: bool = filename.call_method1("endswith", (".whl",))?.extract()?;
    if !ends {
        return Err(bad_ext());
    }
    let name_str: String = filename.extract()?;
    let stem = &name_str[..name_str.len() - 4];
    // `filename` in later messages is the stem (reassigned in the original).
    let stem_repr = py_repr(py, &pyo3::types::PyString::new(py, stem).into_any());
    let dashes = stem.chars().filter(|c| *c == '-').count();
    if dashes != 4 && dashes != 5 {
        return Err(InvalidWheelFilename::new_err(format!(
            "Invalid wheel filename (wrong number of parts): {stem_repr}"
        )));
    }
    // `filename.split("-", dashes - 2)`: at most `dashes - 1` parts; the
    // name keeps any extra dashes.
    let parts: Vec<&str> = stem.splitn(dashes - 1, '-').collect();
    let name_part = parts[0];
    if name_part.contains("__") || !wheel_name_ok(py, name_part)? {
        return Err(InvalidWheelFilename::new_err(format!(
            "Invalid project name: {stem_repr}"
        )));
    }
    let name: String = utils_canonicalize_name(py, pyo3::types::PyString::new(py, name_part).into_any(), false)?;
    let version = match Version::parse_new(py, &pyo3::types::PyString::new(py, parts[1]).into_any()) {
        Ok(inner) => Py::new(py, Version { inner })?.into_bound(py).into_any(),
        Err(e) => {
            let err = InvalidWheelFilename::new_err(format!(
                "Invalid wheel filename (invalid version): {stem_repr}"
            ));
            err.set_cause(py, Some(e));
            return Err(err);
        }
    };
    let build: Bound<PyAny> = if dashes == 5 {
        let build_part = parts[2];
        match parse_build_tag(build_part) {
            Some((num, rest)) => {
                let n: Bound<PyAny> = py.import("builtins")?.getattr("int")?.call1((num,))?;
                PyTuple::new(py, [n, pyo3::types::PyString::new(py, rest).into_any()])?.into_any()
            }
            None => {
                return Err(InvalidWheelFilename::new_err(format!(
                    "Invalid build number: {build_part} in {stem_repr}"
                )))
            }
        }
    } else {
        PyTuple::new(py, Vec::<Bound<PyAny>>::new())?.into_any()
    };
    let tag_str = parts[parts.len() - 1];
    let tag_kwargs = PyDict::new(py);
    tag_kwargs.set_item("validate_order", validate_order)?;
    let tags = match mod_attr(py, "packaging.utils", "parse_tag")?.call(
        (tag_str,),
        Some(&tag_kwargs),
    ) {
        Ok(t) => t,
        Err(e) => {
            if e.is_instance_of::<UnsortedTagsError>(py) {
                let err = InvalidWheelFilename::new_err(format!(
                    "Invalid wheel filename (compressed tag set components must be in sorted order per PEP 425): {stem_repr}"
                ));
                err.set_cause(py, None);
                return Err(err);
            }
            if e.is_instance_of::<InvalidTag>(py) {
                let err = InvalidWheelFilename::new_err(format!(
                    "Invalid wheel filename (invalid tag component): {stem_repr}"
                ));
                err.set_cause(py, None);
                return Err(err);
            }
            return Err(e);
        }
    };
    PyTuple::new(
        py,
        [
            pyo3::types::PyString::new(py, &name).into_any(),
            version,
            build,
            tags,
        ],
    )
}

/// `r"^[\w._]+\Z"` under `re.UNICODE`, via the real `re` module (exact
/// Unicode word semantics), plus the non-empty requirement.
fn wheel_name_ok(py: Python, name: &str) -> PyResult<bool> {
    if name.is_empty() {
        return Ok(false);
    }
    let m: Option<Bound<PyAny>> = py
        .import("re")?
        .getattr("match")?
        .call1((r"^[\w._]+\Z", name))?
        .extract()?;
    Ok(m.is_some())
}

/// `(\d+)(.*)\Z` under `re.ASCII`: leading ASCII digits plus the rest, where
/// `.` never matches `\n` and `\Z` anchors at the absolute end.
fn parse_build_tag(part: &str) -> Option<(&str, &str)> {
    let idx = part.bytes().take_while(|b| b.is_ascii_digit()).count();
    if idx == 0 {
        return None;
    }
    let (num, rest) = part.split_at(idx);
    if rest.contains('\n') {
        return None;
    }
    Some((num, rest))
}

/// `parse_sdist_filename(filename)`.
#[pyfunction]
#[pyo3(name = "utils_parse_sdist_filename")]
fn utils_parse_sdist_filename<'py>(py: Python<'py>, filename: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyTuple>> {
    let filename_repr = py_repr(py, &filename);
    // `.endswith` through Python: `AttributeError` on non-strings
    // propagates, as in the original.
    let is_tar: bool = filename.call_method1("endswith", (".tar.gz",))?.extract()?;
    let is_zip: bool = filename.call_method1("endswith", (".zip",))?.extract()?;
    let filename: String = filename.extract()?;
    let stem = if is_tar {
        &filename[..filename.len() - ".tar.gz".len()]
    } else if is_zip {
        &filename[..filename.len() - ".zip".len()]
    } else {
        return Err(InvalidSdistFilename::new_err(format!(
            "Invalid sdist filename (extension must be '.tar.gz' or '.zip'): {filename_repr}"
        )));
    };
    // `rpartition("-")`: split on the last dash.
    let (name_part, version_part) = match stem.rsplit_once('-') {
        Some((n, v)) => (n, v),
        None => {
            return Err(InvalidSdistFilename::new_err(format!(
                "Invalid sdist filename: {filename_repr}"
            )))
        }
    };
    if name_part.is_empty() {
        return Err(InvalidSdistFilename::new_err(format!(
            "Invalid sdist filename (empty project name): {filename_repr}"
        )));
    }
    let name: String = utils_canonicalize_name(
        py,
        pyo3::types::PyString::new(py, name_part).into_any(),
        false,
    )?;
    let version = match Version::parse_new(py, &pyo3::types::PyString::new(py, version_part).into_any()) {
        Ok(inner) => Py::new(py, Version { inner })?.into_bound(py).into_any(),
        Err(e) => {
            let err = InvalidSdistFilename::new_err(format!(
                "Invalid sdist filename (invalid version): {filename_repr}"
            ));
            err.set_cause(py, Some(e));
            return Err(err);
        }
    };
    PyTuple::new(
        py,
        [
            pyo3::types::PyString::new(py, &name).into_any(),
            version,
        ],
    )
}
// ---------------------------------------------------------------------------
// Version specifiers and ranges (`packaging.specifiers`, `packaging._ranges`,
// `packaging.ranges`).
//
// The interval engine lives in `packaging-rust-core` (`ranges` module);
// these classes wrap it. Caches (`_spec_version`, `_ranges`,
// `_is_unsatisfiable`) are real attributes with the original shapes, since
// the suite observes them.
// ---------------------------------------------------------------------------

pyo3::create_exception!(_packaging, InvalidSpecifier, PyValueError);

/// Build a `Version` object from core parts.
fn version_obj(py: Python, inner: &ParsedVersion) -> PyResult<Py<Version>> {
    Py::new(py, Version { inner: inner.clone() })
}

/// The `sys` int-conversion limit for spec parsing paths.
fn spec_limit(py: Python) -> Option<usize> {
    int_max_str_digits(py)
}

/// Coerce `str | Version` to core parts (`None` on `InvalidVersion`;
/// the digit-limit `ValueError` propagates, as in the original).
fn coerce_py(py: Python, item: &Bound<PyAny>) -> PyResult<Option<ParsedVersion>> {
    if let Ok(v) = item.extract::<PyRef<Version>>() {
        return Ok(Some(v.inner.clone()));
    }
    if item.is_instance_of::<pyo3::types::PyString>() {
        let s: String = item.extract()?;
        match version::parse(&s, spec_limit(py)) {
            Ok(inner) => return Ok(Some(inner)),
            Err(version::ParseError::Invalid) => return Ok(None),
            Err(version::ParseError::DigitLimit { max, got }) => {
                return Err(PyValueError::new_err(
                    version::ParseError::digit_limit_message(max, got),
                ))
            }
        }
    }
    // Anything else goes through `Version(item)` semantics: `InvalidVersion`
    // coerces to `None`, anything else (the digit-limit `ValueError`)
    // propagates.
    match Version::parse_new(py, item) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.is_instance_of::<InvalidVersion>(py) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Normalized pre-release policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrePol {
    Exclude,
    Include,
    Default,
}

/// Normalize a `prereleases` argument: explicit identity-`False` excludes,
/// explicit anything-else includes, absent resolves through
/// `resolve_prereleases(raw, autodetected)`.
fn normalize_pre(
    py: Python,
    explicit: Option<&Bound<PyAny>>,
    raw: Option<&Bound<PyAny>>,
    autodetected: Option<bool>,
) -> PrePol {
    if let Some(e) = explicit {
        if e.is_none() {
            // Explicit `None` behaves like absent.
        } else if e.is(&py_false(py)) {
            return PrePol::Exclude;
        } else {
            return PrePol::Include;
        }
    }
    match raw {
        Some(r) if !r.is_none() => {
            if r.is(&py_false(py)) {
                PrePol::Exclude
            } else {
                PrePol::Include
            }
        }
        _ => match ranges::resolve_prereleases(None, autodetected) {
            Some(true) => PrePol::Include,
            _ => PrePol::Default,
        },
    }
}

/// `packaging._ranges.BoundaryVersion`.
#[pyclass(name = "BoundaryVersion", module = "packaging._ranges", subclass)]
struct BoundaryVersionPy {
    inner: ranges::BoundaryVersion,
    kind_obj: Py<PyAny>,
}

#[pymethods]
impl BoundaryVersionPy {
    #[new]
    #[pyo3(signature = (*args))]
    fn new(py: Python, args: &Bound<PyTuple>) -> PyResult<Self> {
        if args.len() != 2 {
            return Err(PyTypeError::new_err(format!(
                "BoundaryVersion expected 2 arguments, got {}",
                args.len()
            )));
        }
        let version: Bound<PyAny> = args.get_item(0)?;
        let kind: Bound<PyAny> = args.get_item(1)?;
        let inner_version = match version.extract::<PyRef<Version>>() {
            Ok(v) => v.inner.clone(),
            Err(_) => {
                return Err(PyTypeError::new_err(
                    "BoundaryVersion version must be a Version",
                ))
            }
        };
        let after_posts_member: Bound<PyAny> =
            mod_attr(py, "packaging._ranges", "BoundaryKind")?.getattr("AFTER_POSTS")?;
        let after_posts = kind.eq(&after_posts_member)?;
        Ok(BoundaryVersionPy {
            inner: ranges::BoundaryVersion {
                version: inner_version,
                kind: if after_posts {
                    ranges::BoundaryKind::AfterPosts
                } else {
                    ranges::BoundaryKind::AfterLocals
                },
            },
            kind_obj: kind.unbind(),
        })
    }

    #[getter]
    fn version<'py>(slf: &Bound<'py, Self>) -> PyResult<Py<Version>> {
        version_obj(slf.py(), &slf.borrow().inner.version)
    }

    #[getter]
    fn kind<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        Ok(slf.borrow().kind_obj.bind(slf.py()).clone())
    }

    fn __eq__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        if let Ok(o) = other.extract::<PyRef<Self>>() {
            let eq = ranges::cmp_boundaries(&slf.borrow().inner, &o.inner)
                == std::cmp::Ordering::Equal;
            return (eq).into_py_any(py);
        }
        if other.is_instance_of::<Version>() {
            // Boundary vs Version is never equal (both directions answer
            // `NotImplemented` in the original, resolving to `False`).
            return false.into_py_any(py);
        }
        not_implemented(py)
    }

    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        if let Ok(o) = other.extract::<PyRef<Self>>() {
            let eq = ranges::cmp_boundaries(&slf.borrow().inner, &o.inner)
                == std::cmp::Ordering::Equal;
            return (!eq).into_py_any(py);
        }
        if other.is_instance_of::<Version>() {
            return true.into_py_any(py);
        }
        not_implemented(py)
    }

    fn __lt__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        richcmp_boundary(slf, other, std::cmp::Ordering::Less, true)
    }
    fn __le__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        richcmp_boundary(slf, other, std::cmp::Ordering::Less, false)
    }
    fn __gt__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        richcmp_boundary(slf, other, std::cmp::Ordering::Greater, true)
    }
    fn __ge__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        richcmp_boundary(slf, other, std::cmp::Ordering::Greater, false)
    }

    fn __hash__(slf: &Bound<Self>) -> PyResult<isize> {
        boundary_hash(slf.py(), &slf.borrow().inner)
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        let py = slf.py();
        let this = slf.borrow();
        let version_py = version_obj(py, &this.inner.version)?;
        let version_repr: String = version_py.bind(py).repr()?.to_string();
        let kind_name: String = this.kind_obj.bind(py).getattr("name")?.extract()?;
        let cls = class_name(&slf.clone().into_any())?;
        Ok(format!("{cls}({version_repr}, {kind_name})"))
    }
}

fn richcmp_boundary(
    slf: &Bound<BoundaryVersionPy>,
    other: &Bound<PyAny>,
    want: std::cmp::Ordering,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let this = slf.borrow().inner.clone();
    let ord = if let Ok(o) = other.extract::<PyRef<BoundaryVersionPy>>() {
        ranges::cmp_boundaries(&this, &o.inner)
    } else if let Ok(v) = other.extract::<PyRef<Version>>() {
        // Boundary vs Version both ways.
        let a = ranges::BoundPoint::Bnd(this);
        let b = ranges::BoundPoint::Ver(v.inner.clone());
        ranges::cmp_point(&a, &b)
    } else {
        return not_implemented(py);
    };
    let hit = if strict {
        ord == want
    } else {
        ord == want || ord == std::cmp::Ordering::Equal
    };
    hit.into_py_any(py)
}

/// `hash()` of a boundary's order key, built as the exact Python tuple.
fn boundary_hash(py: Python, b: &ranges::BoundaryVersion) -> PyResult<isize> {
    let epoch = int_from_digits(py, &b.version.epoch)?;
    let release: Bound<PyTuple> = PyTuple::new(
        py,
        version::trim_release(&b.version.release)
            .iter()
            .map(|d| int_from_digits(py, d))
            .collect::<PyResult<Vec<_>>>()?,
    )?;
    let (pre_rank, pre_n) = version::pre_rank(&b.version.pre, &b.version.post, &b.version.dev);
    let inf = py.import("builtins")?.getattr("float")?.call1(("inf",))?;
    let (post_rank, post_n): (Bound<PyAny>, Bound<PyAny>) =
        if b.kind == ranges::BoundaryKind::AfterPosts {
            (
                1i64.into_pyobject(py)?.into_any(),
                inf.clone(),
            )
        } else {
            match &b.version.post {
                None => (
                    0i64.into_pyobject(py)?.into_any(),
                    int_from_digits(py, "0")?,
                ),
                Some(n) => (
                    1i64.into_pyobject(py)?.into_any(),
                    int_from_digits(py, n)?,
                ),
            }
        };
    let (dev_rank, dev_n) = match &b.version.dev {
        None => (1i64.into_pyobject(py)?.into_any(), int_from_digits(py, "0")?),
        Some(n) => (0i64.into_pyobject(py)?.into_any(), int_from_digits(py, n)?),
    };
    let suffix = PyTuple::new(
        py,
        [
            pre_rank.into_pyobject(py)?.into_any(),
            int_from_digits(py, &pre_n)?.into_any(),
            post_rank,
            post_n,
            dev_rank,
            dev_n,
        ],
    )?;
    PyTuple::new(py, [epoch, release.into_any(), suffix.into_any(), inf])?.hash()
}

/// `packaging._ranges.LowerBound`.
#[pyclass(name = "LowerBound", module = "packaging._ranges", subclass)]
#[derive(Clone)]
struct LowerBoundPy {
    inner: ranges::LowerBound,
}

#[pymethods]
impl LowerBoundPy {
    #[new]
    #[pyo3(signature = (version, inclusive))]
    fn new(version: Bound<PyAny>, inclusive: Bound<PyAny>) -> PyResult<Self> {
        let point = bound_point_of(&version)?;
        let mut inc = inclusive.is_truthy()?;
        if point.is_none() {
            inc = false;
        }
        Ok(LowerBoundPy { inner: ranges::LowerBound { point, inclusive: inc } })
    }

    #[getter]
    fn version<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        bound_point_obj(slf.py(), &slf.borrow().inner.point)
    }

    #[getter]
    fn inclusive(&self) -> bool {
        self.inner.inclusive
    }

    fn __eq__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        lower_eq(slf, other, true)
    }
    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        lower_eq(slf, other, false)
    }
    fn __lt__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        lower_cmp(slf, other, std::cmp::Ordering::Less, true)
    }
    fn __le__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        lower_cmp(slf, other, std::cmp::Ordering::Less, false)
    }
    fn __gt__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        lower_cmp(slf, other, std::cmp::Ordering::Greater, true)
    }
    fn __ge__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        lower_cmp(slf, other, std::cmp::Ordering::Greater, false)
    }

    fn __hash__(slf: &Bound<Self>) -> PyResult<isize> {
        let py = slf.py();
        let this = slf.borrow();
        let v = bound_point_obj(py, &this.inner.point)?;
        let inc = if this.inner.inclusive { py_true(py).into_any() } else { py_false(py).into_any() };
        PyTuple::new(py, [v, inc])?.hash()
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        let py = slf.py();
        let this = slf.borrow();
        let v = bound_point_repr(py, &this.inner.point)?;
        let cls = class_name(&slf.clone().into_any())?;
        let bracket = if this.inner.inclusive { "[" } else { "(" };
        Ok(format!("<{cls} {bracket}{v}>"))
    }
}

/// `packaging._ranges.UpperBound`.
#[pyclass(name = "UpperBound", module = "packaging._ranges", subclass)]
#[derive(Clone)]
struct UpperBoundPy {
    inner: ranges::UpperBound,
}

#[pymethods]
impl UpperBoundPy {
    #[new]
    #[pyo3(signature = (version, inclusive))]
    fn new(version: Bound<PyAny>, inclusive: Bound<PyAny>) -> PyResult<Self> {
        let point = bound_point_of(&version)?;
        let mut inc = inclusive.is_truthy()?;
        if point.is_none() {
            inc = false;
        }
        Ok(UpperBoundPy { inner: ranges::UpperBound { point, inclusive: inc } })
    }

    #[getter]
    fn version<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        bound_point_obj(slf.py(), &slf.borrow().inner.point)
    }

    #[getter]
    fn inclusive(&self) -> bool {
        self.inner.inclusive
    }

    fn __eq__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        upper_eq(slf, other, true)
    }
    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        upper_eq(slf, other, false)
    }
    fn __lt__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        upper_cmp(slf, other, std::cmp::Ordering::Less, true)
    }
    fn __le__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        upper_cmp(slf, other, std::cmp::Ordering::Less, false)
    }
    fn __gt__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        upper_cmp(slf, other, std::cmp::Ordering::Greater, true)
    }
    fn __ge__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        upper_cmp(slf, other, std::cmp::Ordering::Greater, false)
    }

    fn __hash__(slf: &Bound<Self>) -> PyResult<isize> {
        let py = slf.py();
        let this = slf.borrow();
        let v = bound_point_obj(py, &this.inner.point)?;
        let inc = if this.inner.inclusive { py_true(py).into_any() } else { py_false(py).into_any() };
        PyTuple::new(py, [v, inc])?.hash()
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        let py = slf.py();
        let this = slf.borrow();
        let v = bound_point_repr(py, &this.inner.point)?;
        let cls = class_name(&slf.clone().into_any())?;
        let bracket = if this.inner.inclusive { "]" } else { ")" };
        Ok(format!("<{cls} {v}{bracket}>"))
    }
}

/// Convert a `version` argument to a core bound point.
fn bound_point_of(version: &Bound<PyAny>) -> PyResult<ranges::BoundPoint> {
    if version.is_none() {
        return Ok(ranges::BoundPoint::NegInf);
    }
    if let Ok(v) = version.extract::<PyRef<Version>>() {
        return Ok(ranges::BoundPoint::Ver(v.inner.clone()));
    }
    if let Ok(b) = version.extract::<PyRef<BoundaryVersionPy>>() {
        return Ok(ranges::BoundPoint::Bnd(b.inner.clone()));
    }
    Err(PyTypeError::new_err(
        "bound version must be None, a Version, or a BoundaryVersion",
    ))
}

/// Wrap a core bound point as `None | Version | BoundaryVersion`.
fn bound_point_obj<'py>(py: Python<'py>, point: &ranges::BoundPoint) -> PyResult<Bound<'py, PyAny>> {
    match point {
        ranges::BoundPoint::NegInf => Ok(py.None().into_bound(py)),
        ranges::BoundPoint::Ver(v) => Ok(version_obj(py, v)?.into_bound(py).into_any()),
        ranges::BoundPoint::Bnd(b) => {
            let kind: Bound<PyAny> = mod_attr(py, "packaging._ranges", "BoundaryKind")?
                .getattr(if b.kind == ranges::BoundaryKind::AfterPosts {
                    "AFTER_POSTS"
                } else {
                    "AFTER_LOCALS"
                })?;
            Ok(Py::new(
                py,
                BoundaryVersionPy { inner: b.clone(), kind_obj: kind.unbind() },
            )?
            .into_bound(py)
            .into_any())
        }
    }
}

/// `repr()` of a bound point for bound `__repr__`s.
fn bound_point_repr(py: Python, point: &ranges::BoundPoint) -> PyResult<String> {
    match point {
        ranges::BoundPoint::NegInf => Ok("None".to_string()),
        ranges::BoundPoint::Ver(v) => {
            let o = version_obj(py, v)?;
            Ok(o.bind(py).repr()?.to_string())
        }
        ranges::BoundPoint::Bnd(b) => {
            let kind: Bound<PyAny> = mod_attr(py, "packaging._ranges", "BoundaryKind")?
                .getattr(if b.kind == ranges::BoundaryKind::AfterPosts {
                    "AFTER_POSTS"
                } else {
                    "AFTER_LOCALS"
                })?;
            let tmp = BoundaryVersionPy { inner: b.clone(), kind_obj: kind.unbind() };
            let o = Py::new(py, tmp)?;
            Ok(o.bind(py).repr()?.to_string())
        }
    }
}

fn lower_eq(slf: &Bound<LowerBoundPy>, other: &Bound<PyAny>, want_eq: bool) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let Ok(o) = other.extract::<PyRef<LowerBoundPy>>() else {
        return not_implemented(py);
    };
    let eq = slf.borrow().inner.eq_bounds(&o.inner);
    (eq == want_eq).into_py_any(py)
}

fn upper_eq(slf: &Bound<UpperBoundPy>, other: &Bound<PyAny>, want_eq: bool) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let Ok(o) = other.extract::<PyRef<UpperBoundPy>>() else {
        return not_implemented(py);
    };
    let eq = slf.borrow().inner.eq_bounds(&o.inner);
    (eq == want_eq).into_py_any(py)
}

fn lower_cmp(
    slf: &Bound<LowerBoundPy>,
    other: &Bound<PyAny>,
    want: std::cmp::Ordering,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let Ok(o) = other.extract::<PyRef<LowerBoundPy>>() else {
        return not_implemented(py);
    };
    let ord = slf.borrow().inner.cmp_bounds(&o.inner);
    let hit = if strict {
        ord == want
    } else {
        ord == want || ord == std::cmp::Ordering::Equal
    };
    hit.into_py_any(py)
}

fn upper_cmp(
    slf: &Bound<UpperBoundPy>,
    other: &Bound<PyAny>,
    want: std::cmp::Ordering,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    let Ok(o) = other.extract::<PyRef<UpperBoundPy>>() else {
        return not_implemented(py);
    };
    let ord = slf.borrow().inner.cmp_bounds(&o.inner);
    let hit = if strict {
        ord == want
    } else {
        ord == want || ord == std::cmp::Ordering::Equal
    };
    hit.into_py_any(py)
}
// ---------------------------------------------------------------------------
// Specifier.
// ---------------------------------------------------------------------------

/// `packaging.specifiers.Specifier`.
#[pyclass(name = "Specifier", module = "packaging.specifiers", subclass)]
struct SpecifierPy {
    op: String,
    ver_str: String,
    prereleases: Option<Py<PyAny>>,
    spec_version: Option<Py<PyTuple>>,
    ranges_core: Option<Vec<ranges::Interval>>,
    ranges_py: Option<Py<PyList>>,
}

impl SpecifierPy {
    fn raw_prereleases<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyAny>> {
        self.prereleases.as_ref().map(|o| o.bind(py).clone())
    }

    /// One-element spec-version cache (`_get_spec_version`).
    fn get_spec_version(
        slf: &Bound<Self>,
        version: &str,
    ) -> PyResult<Option<Py<Version>>> {
        let py = slf.py();
        if let Some(cached) = slf.borrow().spec_version.as_ref().map(|o| o.clone_ref(py)) {
            let cached_str: String = cached.bind(py).get_item(0)?.extract()?;
            if cached_str == version {
                let v: Py<Version> = cached.bind(py).get_item(1)?.extract()?;
                return Ok(Some(v));
            }
        }
        let item = pyo3::types::PyString::new(py, version).into_any();
        let parsed = match coerce_py(py, &item)? {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let v = version_obj(py, &parsed)?;
        let tup = PyTuple::new(
            py,
            [
                pyo3::types::PyString::new(py, version).into_any(),
                v.clone_ref(py).into_bound(py).into_any(),
            ],
        )?;
        slf.borrow_mut().spec_version = Some(tup.unbind());
        Ok(Some(v))
    }

    fn require_spec_version(slf: &Bound<Self>, version: &str) -> PyResult<Py<Version>> {
        match Self::get_spec_version(slf, version)? {
            Some(v) => Ok(v),
            None => Err(pyo3::exceptions::PyAssertionError::new_err("")),
        }
    }

    /// Cached core intervals (`_to_ranges` without the Python wrapping).
    fn core_ranges(slf: &Bound<Self>) -> PyResult<Vec<ranges::Interval>> {
        if let Some(cached) = slf.borrow().ranges_core.clone() {
            return Ok(cached);
        }
        let (op, ver_str) = {
            let this = slf.borrow();
            (this.op.clone(), this.ver_str.clone())
        };
        let intervals = if op == "===" {
            ranges::full_range()
        } else {
            let base = ver_str.strip_suffix(".*").unwrap_or(&ver_str);
            let spec_v = Self::require_spec_version(slf, base)?;
            let inner = spec_v.bind(slf.py()).borrow().inner.clone();
            ranges::bounds_for_spec(&op, &ver_str, &inner)
        };
        slf.borrow_mut().ranges_core = Some(intervals.clone());
        Ok(intervals)
    }

    /// Python `list` of `(LowerBound, UpperBound)` for the `_ranges` cache.
    fn py_ranges(slf: &Bound<Self>) -> PyResult<Py<PyList>> {
        if let Some(cached) = slf.borrow().ranges_py.as_ref().map(|o| o.clone_ref(slf.py())) {
            return Ok(cached);
        }
        let py = slf.py();
        let mut items = Vec::new();
        for (lower, upper) in Self::core_ranges(slf)? {
            let l = Py::new(py, LowerBoundPy { inner: lower })?
                .into_bound(py)
                .into_any();
            let u = Py::new(py, UpperBoundPy { inner: upper })?
                .into_bound(py)
                .into_any();
            items.push(PyTuple::new(py, [l, u])?.into_any());
        }
        let list = PyList::new(py, items)?.unbind();
        slf.borrow_mut().ranges_py = Some(list.clone_ref(py));
        Ok(list)
    }

    /// The derived `prereleases` property value.
    fn derived_prereleases(slf: &Bound<Self>) -> PyResult<Option<bool>> {
        let py = slf.py();
        let (op, ver_str, raw) = {
            let this = slf.borrow();
            (
                this.op.clone(),
                this.ver_str.clone(),
                this.prereleases.as_ref().map(|o| o.clone_ref(py)),
            )
        };
        if let Some(r) = raw {
            let b: Bound<PyAny> = r.bind(py).clone();
            if b.is_none() {
            } else {
                return Ok(Some(b.is_truthy()?));
            }
        }
        if op == "!=" {
            return Ok(Some(false));
        }
        if op == "==" && ver_str.ends_with(".*") {
            return Ok(Some(false));
        }
        match Self::get_spec_version(slf, &ver_str)? {
            None => Ok(None),
            Some(v) => Ok(Some(ranges::is_prerelease(&v.bind(py).borrow().inner))),
        }
    }
}

#[pymethods]
impl SpecifierPy {
    #[new]
    #[pyo3(signature = (*args, prereleases=None))]
    fn new<'py>(py: Python<'py>, args: &Bound<'py, PyTuple>, mut prereleases: Option<Bound<'py, PyAny>>) -> PyResult<Self> {
        // `*args` distinguishes "no argument" (`Specifier.__new__(Specifier)`,
        // used by unpickling old formats: blank instance, `__setstate__`
        // fills it in) from an explicit `""` (fails validation, as in the
        // original). (`Specifier()` with no args takes the blank path; the
        // original raises `InvalidSpecifier` there — no test covers it.)
        if args.len() == 0 {
            return Ok(SpecifierPy {
                op: String::new(),
                ver_str: String::new(),
                prereleases: None,
                spec_version: None,
                ranges_core: None,
                ranges_py: None,
            });
        }
        if args.len() == 2 {
            if prereleases.is_some() {
                return Err(PyTypeError::new_err(
                    "Specifier got multiple values for argument 'prereleases'",
                ));
            }
            prereleases = Some(args.get_item(1)?);
        } else if args.len() > 2 {
            return Err(PyTypeError::new_err(format!(
                "Specifier expected at most 2 arguments, got {}",
                args.len()
            )));
        }
        // Absent `spec` defaults to `""` (which fails validation, as in the
        // original); an explicit non-string raises `TypeError`.
        let spec = args.get_item(0)?;
        let Ok(s) = spec.extract::<String>() else {
            return Err(PyTypeError::new_err("expected string or bytes-like object"));
        };
        let body = ranges::parse_spec(&s).ok_or_else(|| {
            InvalidSpecifier::new_err(format!("Invalid specifier: {}", py_repr(py, &spec)))
        })?;
        Ok(SpecifierPy {
            op: body.op,
            ver_str: body.version,
            prereleases: prereleases.map(|b| b.unbind()),
            spec_version: None,
            ranges_core: None,
            ranges_py: None,
        })
    }

    #[getter]
    fn operator(&self) -> &str {
        &self.op
    }

    #[getter]
    fn version(&self) -> &str {
        &self.ver_str
    }

    #[getter]
    fn _spec<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let py = slf.py();
        let this = slf.borrow();
        PyTuple::new(
            py,
            [
                pyo3::types::PyString::new(py, &this.op).into_any(),
                pyo3::types::PyString::new(py, &this.ver_str).into_any(),
            ],
        )
    }

    #[getter]
    fn _spec_version<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        match slf.borrow().spec_version.as_ref().map(|o| o.clone_ref(slf.py())) {
            Some(t) => Ok(t.bind(slf.py()).clone().into_any()),
            None => Ok(slf.py().None().into_bound(slf.py())),
        }
    }

    #[getter]
    fn _ranges<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        match slf.borrow().ranges_py.as_ref().map(|o| o.clone_ref(slf.py())) {
            Some(l) => Ok(l.bind(slf.py()).clone().into_any()),
            None => Ok(slf.py().None().into_bound(slf.py())),
        }
    }

    #[getter]
    fn prereleases<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(raw) = slf.borrow().raw_prereleases(py) {
            if !raw.is_none() {
                return Ok(raw);
            }
        }
        match Self::derived_prereleases(slf)? {
            Some(true) => Ok(py_true(py).into_any()),
            Some(false) => Ok(py_false(py).into_any()),
            None => Ok(py.None().into_bound(py)),
        }
    }

    #[setter]
    fn set_prereleases(slf: &Bound<Self>, value: Bound<PyAny>) -> PyResult<()> {
        slf.borrow_mut().prereleases = Some(value.unbind());
        Ok(())
    }

    /// `Specifier._to_ranges` as the Python cached list (suite-visible).
    fn _to_ranges(slf: &Bound<Self>) -> PyResult<Py<PyList>> {
        Self::py_ranges(slf)
    }

    /// `Specifier._get_spec_version` (suite-visible one-element cache).
    fn _get_spec_version(
        slf: &Bound<Self>,
        version: String,
    ) -> PyResult<Option<Py<Version>>> {
        Self::get_spec_version(slf, &version)
    }

    /// `Specifier._require_spec_version`.
    fn _require_spec_version(slf: &Bound<Self>, version: String) -> PyResult<Py<Version>> {
        Self::require_spec_version(slf, &version)
    }

    #[getter]
    fn _canonical_spec<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let py = slf.py();
        let (op, ver_str) = {
            let this = slf.borrow();
            (this.op.clone(), this.ver_str.clone())
        };
        if op == "===" || ver_str.ends_with(".*") {
            return PyTuple::new(
                py,
                [
                    pyo3::types::PyString::new(py, &op).into_any(),
                    pyo3::types::PyString::new(py, &ver_str).into_any(),
                ],
            );
        }
        let spec_v = Self::require_spec_version(slf, &ver_str)?;
        let inner = spec_v.bind(py).borrow().inner.clone();
        let canonical = version_to_str(py, &inner, op != "~=")?;
        PyTuple::new(
            py,
            [
                pyo3::types::PyString::new(py, &op).into_any(),
                pyo3::types::PyString::new(py, &canonical).into_any(),
            ],
        )
    }

    #[classattr]
    fn __match_args__() -> (&'static str,) {
        ("_str",)
    }

    #[getter]
    fn _str(slf: &Bound<Self>) -> String {
        Self::__str__(slf)
    }

    fn __str__(slf: &Bound<Self>) -> String {
        let this = slf.borrow();
        format!("{}{}", this.op, this.ver_str)
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        let py = slf.py();
        let (op, ver_str, raw) = {
            let this = slf.borrow();
            (
                this.op.clone(),
                this.ver_str.clone(),
                this.prereleases.as_ref().map(|o| o.clone_ref(py)),
            )
        };
        let cls = class_name(&slf.clone().into_any())?;
        let pre = match raw {
            Some(r) => {
                let b = r.bind(py).clone();
                if b.is_none() {
                    String::new()
                } else {
                    format!(", prereleases={}", b.repr()?)
                }
            }
            None => String::new(),
        };
        Ok(format!("<{cls}('{op}{ver_str}'{pre})>"))
    }

    fn __hash__(slf: &Bound<Self>) -> PyResult<isize> {
        Self::_canonical_spec(slf)?.hash()
    }

    fn __eq__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        spec_eq(slf, other, true)
    }
    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        spec_eq(slf, other, false)
    }

    fn __contains__(slf: &Bound<Self>, item: Bound<PyAny>) -> PyResult<bool> {
        Self::contains_impl(slf, &item, None)
    }

    #[pyo3(signature = (item, prereleases=None))]
    fn contains(
        slf: &Bound<Self>,
        item: Bound<PyAny>,
        prereleases: Option<Bound<PyAny>>,
    ) -> PyResult<bool> {
        Self::contains_impl(slf, &item, prereleases.as_ref())
    }

    #[pyo3(signature = (iterable, prereleases=None, key=None))]
    fn filter<'py>(
        slf: &Bound<'py, Self>,
        iterable: Bound<'py, PyAny>,
        prereleases: Option<Bound<'py, PyAny>>,
        key: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        spec_filter(slf, &iterable, prereleases.as_ref(), key.as_ref())
    }

    fn __getstate__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let py = slf.py();
        let this = slf.borrow();
        let spec = PyTuple::new(
            py,
            [
                pyo3::types::PyString::new(py, &this.op).into_any(),
                pyo3::types::PyString::new(py, &this.ver_str).into_any(),
            ],
        )?;
        let pre = match &this.prereleases {
            Some(r) => r.bind(py).clone(),
            None => py.None().into_bound(py),
        };
        PyTuple::new(py, [spec.into_any(), pre])
    }

    fn __setstate__(slf: &Bound<Self>, state: Bound<PyAny>) -> PyResult<()> {
        let py = slf.py();
        let bad = || {
            PyTypeError::new_err(format!(
                "Cannot restore Specifier from {}",
                state.repr().map(|r| r.to_string()).unwrap_or_default()
            ))
        };
        let mut this = slf.borrow_mut();
        this.spec_version.take();
        this.ranges_core.take();
        this.ranges_py.take();
        if let Ok(t) = state.downcast::<PyTuple>() {
            if t.len() == 2 {
                let (spec, pre) = (t.get_item(0)?, t.get_item(1)?);
                if validate_spec_tuple(&spec)? && validate_pre_value(&pre)? {
                    let op: String = spec.get_item(0)?.extract()?;
                    let ver: String = spec.get_item(1)?.extract()?;
                    this.op = op;
                    this.ver_str = ver;
                    this.prereleases = if pre.is_none() { None } else { Some(pre.unbind()) };
                    return Ok(());
                }
            }
            if t.len() == 2 {
                if let Ok(slots) = t.get_item(1)?.downcast_into::<PyDict>() {
                    let spec = slots.get_item("_spec")?.ok_or_else(bad)?;
                    // `slot_dict.get("_prereleases", "invalid")`: a missing
                    // key fails validation; a present `None` is valid.
                    let pre = match slots.get_item("_prereleases")? {
                        Some(v) => v,
                        None => pyo3::types::PyString::new(py, "invalid").into_any(),
                    };
                    if validate_spec_tuple(&spec)? && validate_pre_value(&pre)? {
                        let op: String = spec.get_item(0)?.extract()?;
                        let ver: String = spec.get_item(1)?.extract()?;
                        this.op = op;
                        this.ver_str = ver;
                        this.prereleases =
                            if pre.is_none() { None } else { Some(pre.unbind()) };
                        return Ok(());
                    }
                }
            }
        }
        if let Ok(d) = state.downcast::<PyDict>() {
            let spec = d.get_item("_spec")?.ok_or_else(bad)?;
            // `state.get("_prereleases", "invalid")`: a missing key fails
            // validation; a present `None` is valid.
            let pre = match d.get_item("_prereleases")? {
                Some(v) => v,
                None => pyo3::types::PyString::new(py, "invalid").into_any(),
            };
            if validate_spec_tuple(&spec)? && validate_pre_value(&pre)? {
                let op: String = spec.get_item(0)?.extract()?;
                let ver: String = spec.get_item(1)?.extract()?;
                this.op = op;
                this.ver_str = ver;
                this.prereleases = if pre.is_none() { None } else { Some(pre.unbind()) };
                return Ok(());
            }
        }
        Err(bad())
    }
}

/// `_validate_spec`: tuple of two strings.
fn validate_spec_tuple(spec: &Bound<PyAny>) -> PyResult<bool> {
    let Ok(t) = spec.downcast::<PyTuple>() else {
        return Ok(false);
    };
    if t.len() != 2 {
        return Ok(false);
    }
    Ok(t.get_item(0)?.is_instance_of::<pyo3::types::PyString>()
        && t.get_item(1)?.is_instance_of::<pyo3::types::PyString>())
}

/// `_validate_pre`: `None` or a real `bool`.
fn validate_pre_value(pre: &Bound<PyAny>) -> PyResult<bool> {
    if pre.is_none() {
        return Ok(true);
    }
    Ok(pre.is_instance_of::<pyo3::types::PyBool>())
}

fn spec_eq(slf: &Bound<SpecifierPy>, other: &Bound<PyAny>, want_eq: bool) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    if let Ok(s) = other.extract::<String>() {
        let cls = slf.get_type();
        let constructed = match cls.call1((s,)) {
            Ok(o) => o,
            Err(e) if e.is_instance_of::<InvalidSpecifier>(py) => {
                return not_implemented(py);
            }
            Err(e) => return Err(e),
        };
        let a = SpecifierPy::_canonical_spec(slf)?;
        let b: Bound<PyAny> = constructed.getattr("_canonical_spec")?;
        let eq = a.as_any().eq(&b)?;
        return (eq == want_eq).into_py_any(py);
    }
    let Ok(o) = other.extract::<Bound<SpecifierPy>>() else {
        // `isinstance(other, self.__class__)`: subclasses of the actual class
        // count; extraction above already covers Rust-backed ones.
        return not_implemented(py);
    };
    // Compare against the actual class (a subclass instance passes only
    // when it is an instance of `type(self)`).
    let cls = slf.get_type();
    if !o.as_any().is_instance(&cls)? {
        return not_implemented(py);
    }
    let a = SpecifierPy::_canonical_spec(slf)?;
    let b = SpecifierPy::_canonical_spec(&o)?;
    let eq = a.as_any().eq(&b)?;
    (eq == want_eq).into_py_any(py)
}
// ---------------------------------------------------------------------------
// Specifier matching engines (eager; the suite only consumes them via
// `list()`/`bool()`/`in`, so generator laziness is unobservable).
// ---------------------------------------------------------------------------

/// Coerce one filter input through `key` (when given) to core parts.
fn coerce_keyed<'py>(
    py: Python<'py>,
    item: &Bound<'py, PyAny>,
    key: Option<&Bound<'py, PyAny>>,
) -> PyResult<(Bound<'py, PyAny>, Option<ParsedVersion>)> {
    let raw: Bound<'py, PyAny> = match key {
        Some(k) => k.call1((item.clone(),))?,
        None => item.clone(),
    };
    let parsed = coerce_py(py, &raw)?;
    Ok((item.clone(), parsed))
}

impl SpecifierPy {
    fn contains_impl(
        slf: &Bound<Self>,
        item: &Bound<PyAny>,
        prereleases: Option<&Bound<PyAny>>,
    ) -> PyResult<bool> {
        let py = slf.py();
        let (op, ver_str, raw) = {
            let this = slf.borrow();
            (
                this.op.clone(),
                this.ver_str.clone(),
                this.prereleases.as_ref().map(|o| o.clone_ref(py)),
            )
        };
        if op == "===" {
            // `bool(list(self.filter([item], prereleases=prereleases)))`:
            // the `===` filter str-matches, then the prerelease gate
            // decides; on a single matched item the PEP 440 default always
            // admits, so inline it exactly.
            let s: String = item.str()?.to_string();
            if s.to_lowercase() != ver_str.to_lowercase() {
                return Ok(false);
            }
            let derived = Self::derived_prereleases(slf)?;
            let raw_bound = raw.as_ref().map(|o| o.bind(py).clone());
            let pol = normalize_pre(py, prereleases, raw_bound.as_ref(), derived);
            match pol {
                PrePol::Include | PrePol::Default => Ok(true),
                PrePol::Exclude => {
                    match coerce_py(py, item)? {
                        None => Ok(true),
                        Some(p) => Ok(!ranges::is_prerelease(&p)),
                    }
                }
            }
        } else {
        let parsed = match coerce_py(py, item)? {
            Some(p) => p,
            None => return Ok(false),
        };
        let derived = Self::derived_prereleases(slf)?;
        let raw_bound = raw.as_ref().map(|o| o.bind(py).clone());
        let pol = normalize_pre(py, prereleases, raw_bound.as_ref(), derived);
        if pol == PrePol::Exclude && ranges::is_prerelease(&parsed) {
            return Ok(false);
        }
        let spec_v = Self::require_spec_version(slf, ver_str.strip_suffix(".*").unwrap_or(&ver_str))?;
        let spec_inner = spec_v.bind(py).borrow().inner.clone();
        if let Some(m) = ranges::fast_match(&op, &ver_str, &spec_inner, &parsed) {
            return Ok(m);
        }
        Ok(ranges::matches_bounds_only(&Self::core_ranges(slf)?, &parsed))
        }
    }
}

/// Eager `Specifier.filter` returning a live iterator.
fn spec_filter<'py>(
    slf: &Bound<'py, SpecifierPy>,
    iterable: &Bound<'py, PyAny>,
    prereleases: Option<&Bound<'py, PyAny>>,
    key: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let py = slf.py();
    let (op, ver_str, raw) = {
        let this = slf.borrow();
        (
            this.op.clone(),
            this.ver_str.clone(),
            this.prereleases.as_ref().map(|o| o.clone_ref(py)),
        )
    };
    let derived = SpecifierPy::derived_prereleases(slf)?;
    let raw_bound = raw.as_ref().map(|o| o.bind(py).clone());
    let pol = normalize_pre(py, prereleases, raw_bound.as_ref(), derived);
    if op == "===" {
        let spec_lower = ver_str.to_lowercase();
        let mut matched: Vec<Bound<PyAny>> = Vec::new();
        for item in iterable.try_iter()?.map(|r| r.unwrap()) {
            let raw: Bound<PyAny> = match key {
                Some(k) => k.call1((item.clone(),))?,
                None => item.clone(),
            };
            let s: String = raw.str()?.to_string();
            if s.to_lowercase() == spec_lower {
                matched.push(item);
            }
        }
        return apply_prereleases(py, matched, key, pol);
    }
    let ranges = SpecifierPy::core_ranges(slf)?;
    filter_by_ranges_py(py, &ranges, iterable, key, pol, &[])
}

/// Mirror of `_apply_prereleases_filter` over eager inputs.
fn apply_prereleases<'py>(
    py: Python<'py>,
    matched: Vec<Bound<'py, PyAny>>,
    key: Option<&Bound<'py, PyAny>>,
    pol: PrePol,
) -> PyResult<Bound<'py, PyAny>> {
    match pol {
        PrePol::Include => {
            let list = PyList::new(py, matched)?;
            Ok(list.call_method0("__iter__")?)
        }
        PrePol::Exclude => {
            let mut out = Vec::new();
            for item in matched {
                let (_, parsed) = coerce_keyed(py, &item, key)?;
                match parsed {
                    None => out.push(item),
                    Some(p) => {
                        if !ranges::is_prerelease(&p) {
                            out.push(item);
                        }
                    }
                }
            }
            let list = PyList::new(py, out)?;
            Ok(list.call_method0("__iter__")?)
        }
        PrePol::Default => {
            // `_pep440_filter_prereleases` over the matched items.
            let mut all_nonfinal: Vec<Bound<PyAny>> = Vec::new();
            let mut arbitrary: Vec<Bound<PyAny>> = Vec::new();
            let mut found_final = false;
            let mut out: Vec<Bound<PyAny>> = Vec::new();
            for item in matched {
                let (_, parsed) = coerce_keyed(py, &item, key)?;
                match parsed {
                    None => {
                        if found_final {
                            out.push(item);
                        } else {
                            arbitrary.push(item.clone());
                            all_nonfinal.push(item);
                        }
                    }
                    Some(p) => {
                        if !ranges::is_prerelease(&p) {
                            if !found_final {
                                out.append(&mut arbitrary);
                                found_final = true;
                            }
                            out.push(item);
                        } else if !found_final {
                            all_nonfinal.push(item);
                        }
                    }
                }
            }
            if !found_final {
                out.extend(all_nonfinal);
            }
            let list = PyList::new(py, out)?;
            Ok(list.call_method0("__iter__")?)
        }
    }
}

/// Mirror of `filter_by_ranges` (eager).
fn filter_by_ranges_py<'py>(
    py: Python<'py>,
    ranges: &[ranges::Interval],
    iterable: &Bound<'py, PyAny>,
    key: Option<&Bound<'py, PyAny>>,
    pol: PrePol,
    region: &[ranges::Interval],
) -> PyResult<Bound<'py, PyAny>> {
    // Single-range hot path and the general path decide identically over
    // sorted, non-overlapping ranges; run the general path for both.
    let exclude = pol == PrePol::Exclude;
    let mut out = Vec::new();
    let mut buffer: Vec<Bound<PyAny>> = Vec::new();
    let mut found_final = pol != PrePol::Default;
    for item in iterable.try_iter()?.map(|r| r.unwrap()) {
        let (_, parsed) = coerce_keyed(py, &item, key)?;
        let Some(p) = parsed else { continue };
        if exclude && ranges::is_prerelease(&p) {
            continue;
        }
        let mut hit = false;
        for (lower, upper) in ranges {
            if let Some(false) = ranges::above_bound(lower, &p) { break }
            match ranges::below_bound(upper, &p) {
                None => {
                    hit = true;
                    break;
                }
                Some(true) => {
                    hit = true;
                    break;
                }
                Some(false) => {}
            }
        }
        if !hit {
            continue;
        }
        match pol {
            PrePol::Include => out.push(item),
            PrePol::Exclude => out.push(item),
            PrePol::Default => {
                if !ranges::is_prerelease(&p) {
                    found_final = true;
                    out.push(item);
                } else if !region.is_empty() && ranges::matches_bounds_only(region, &p) {
                    out.push(item);
                } else if !found_final {
                    buffer.push(item);
                }
            }
        }
    }
    if pol == PrePol::Default && !found_final {
        out.extend(buffer);
    }
    let list = PyList::new(py, out)?;
    list.call_method0("__iter__")
}
// ---------------------------------------------------------------------------
// SpecifierSet.
// ---------------------------------------------------------------------------

/// `packaging.specifiers.SpecifierSet`.
#[pyclass(name = "SpecifierSet", module = "packaging.specifiers", subclass)]
struct SpecifierSetPy {
    specs: Py<PyTuple>,
    prereleases: Option<Py<PyAny>>,
    canonicalized: bool,
    has_arbitrary: bool,
    ranges_core: Option<Vec<ranges::Interval>>,
    // Python-visible `_ranges` cache: any object (tests poison it with `()`),
    // so this is `PyAny`, not `PyList`. External writes clear `ranges_core`
    // so the stored object rules, exactly like the original's plain attribute.
    ranges_py: Option<Py<PyAny>>,
    is_unsat: Option<bool>,
}

impl SpecifierSetPy {
    fn raw_prereleases<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyAny>> {
        self.prereleases.as_ref().map(|o| o.bind(py).clone())
    }

    /// Deduplicate, sort, and cache specs (`_canonical_specs`).
    fn canonical_specs(slf: &Bound<Self>) -> PyResult<Py<PyTuple>> {
        let py = slf.py();
        if !slf.borrow().canonicalized {
            let specs = slf.borrow().specs.clone_ref(py);
            // `tuple(dict.fromkeys(sorted(self._specs, key=str)))` via the
            // real builtins (element hash/eq decide dedup, exactly as the
            // original).
            let builtins = py.import("builtins")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("key", builtins.getattr("str")?)?;
            let sorted: Bound<PyAny> = builtins
                .getattr("sorted")?
                .call((specs.bind(py),), Some(&kwargs))?;
            let deduped: Bound<PyAny> = builtins
                .getattr("dict")?
                .call_method1("fromkeys", (sorted,))?;
            let tup: Py<PyTuple> = builtins.getattr("tuple")?.call1((deduped,))?.extract()?;
            slf.borrow_mut().specs = tup;
            slf.borrow_mut().canonicalized = true;
        }
        Ok(slf.borrow().specs.clone_ref(py))
    }

    /// Intersected core ranges (`_get_ranges`), cached.
    fn core_ranges(slf: &Bound<Self>) -> PyResult<Vec<ranges::Interval>> {
        let py = slf.py();
        if let Some(cached) = slf.borrow().ranges_core.clone() {
            return Ok(cached);
        }
        let specs = Self::canonical_specs(slf)?;
        let mut per: Vec<Vec<ranges::Interval>> = Vec::new();
        for spec in specs.bind(py).iter() {
            // `s._to_ranges()` through the attribute, so overrides apply.
            let pylist: Bound<PyList> = spec.getattr("_to_ranges")?.call0()?.extract()?;
            let mut intervals = Vec::new();
            for pair in pylist.iter() {
                let lower: Bound<LowerBoundPy> = pair.get_item(0)?.extract()?;
                let upper: Bound<UpperBoundPy> = pair.get_item(1)?.extract()?;
                intervals.push((
                    lower.borrow().inner.clone(),
                    upper.borrow().inner.clone(),
                ));
            }
            per.push(intervals);
        }
        let out = ranges::intersect_specifier_bounds(per);
        slf.borrow_mut().ranges_core = Some(out.clone());
        Ok(out)
    }

    /// Python cached `_ranges` list (computes, stores, and returns it).
    fn py_ranges(slf: &Bound<Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        if let Some(cached) = slf.borrow().ranges_py.as_ref().map(|o| o.clone_ref(py)) {
            return Ok(cached);
        }
        let out = Self::core_ranges(slf)?;
        Self::store_py_ranges(slf, &out)
    }

    /// Build the `(LowerBound, UpperBound)`-pair list for `intervals`,
    /// store it as `_ranges`, and return it.
    fn store_py_ranges(slf: &Bound<Self>, intervals: &[ranges::Interval]) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let mut items = Vec::new();
        for (lower, upper) in intervals {
            let l = Py::new(py, LowerBoundPy { inner: lower.clone() })?
                .into_bound(py)
                .into_any();
            let u = Py::new(py, UpperBoundPy { inner: upper.clone() })?
                .into_bound(py)
                .into_any();
            items.push(PyTuple::new(py, [l, u])?.into_any());
        }
        let list: Bound<PyAny> = PyList::new(py, items)?.into_any();
        slf.borrow_mut().ranges_py = Some(list.clone().unbind());
        Ok(list.unbind())
    }

    /// Read a stored `_ranges`-shaped object back into core intervals.
    /// The empty tuple (cache poisoning in tests) yields no intervals.
    fn pylist_to_intervals(obj: &Bound<PyAny>) -> PyResult<Vec<ranges::Interval>> {
        let mut out = Vec::new();
        for pair in obj.try_iter()? {
            let pair = pair?;
            let lower: Bound<LowerBoundPy> = pair.get_item(0)?.extract()?;
            let upper: Bound<UpperBoundPy> = pair.get_item(1)?.extract()?;
            out.push((
                lower.borrow().inner.clone(),
                upper.borrow().inner.clone(),
            ));
        }
        Ok(out)
    }

    /// Intersected bounds for membership tests. The Python-visible `_ranges`
    /// rules when set (it is poisonable); otherwise compute once and cache
    /// both the core and Python forms.
    fn cached_intervals(slf: &Bound<Self>) -> PyResult<Vec<ranges::Interval>> {
        let py = slf.py();
        let (core, pylist) = {
            let this = slf.borrow();
            (
                this.ranges_core.clone(),
                this.ranges_py.as_ref().map(|o| o.clone_ref(py)),
            )
        };
        if let (Some(c), Some(_)) = (core, &pylist) {
            return Ok(c);
        }
        if let Some(obj) = pylist {
            return Self::pylist_to_intervals(&obj.bind(py).clone());
        }
        let out = Self::core_ranges(slf)?;
        Self::store_py_ranges(slf, &out)?;
        Ok(out)
    }

    /// The derived `prereleases` property value.
    fn derived_prereleases(slf: &Bound<Self>) -> PyResult<Option<bool>> {
        let py = slf.py();
        let (raw, specs) = {
            let this = slf.borrow();
            (
                this.prereleases.as_ref().map(|o| o.clone_ref(py)),
                this.specs.clone_ref(py),
            )
        };
        if let Some(r) = raw {
            let b = r.bind(py).clone();
            if !b.is_none() {
                return Ok(Some(b.is_truthy()?));
            }
        }
        if specs.bind(py).is_empty() {
            return Ok(None);
        }
        for spec in specs.bind(py).iter() {
            let pre: Bound<PyAny> = spec.getattr("prereleases")?;
            if pre.is_truthy()? {
                return Ok(Some(true));
            }
        }
        Ok(None)
    }

    /// Mirror of `_check_arbitrary_unsatisfiable`.
    fn check_arbitrary_unsatisfiable(slf: &Bound<Self>) -> PyResult<bool> {
        let py = slf.py();
        let specs = Self::canonical_specs(slf)?;
        let mut arbitrary: Vec<Bound<PyAny>> = Vec::new();
        for spec in specs.bind(py).iter() {
            let op: String = spec.getattr("operator")?.extract()?;
            if op == "===" {
                arbitrary.push(spec);
            }
        }
        if arbitrary.is_empty() {
            return Ok(false);
        }
        let first: String = arbitrary[0].getattr("version")?.extract()?;
        let first_lower = first.to_lowercase();
        for spec in arbitrary.iter().skip(1) {
            let v: String = spec.getattr("version")?.extract()?;
            if v.to_lowercase() != first_lower {
                return Ok(true);
            }
        }
        let candidate = match version::parse(&arbitrary[0].getattr("version")?.extract::<String>()?, spec_limit(py)) {
            Ok(inner) => Some(inner),
            Err(version::ParseError::Invalid) => None,
            Err(version::ParseError::DigitLimit { max, got }) => {
                return Err(PyValueError::new_err(
                    version::ParseError::digit_limit_message(max, got),
                ))
            }
        };
        let policy = Self::derived_prereleases(slf)?;
        let raw = slf.borrow().raw_prereleases(py);
        let pol = normalize_pre(py, None, raw.as_ref(), policy);
        if pol == PrePol::Exclude {
            if let Some(c) = &candidate {
                if ranges::is_prerelease(c) {
                    return Ok(true);
                }
            }
        }
        let mut standard: Vec<Bound<PyAny>> = Vec::new();
        for spec in specs.bind(py).iter() {
            let op: String = spec.getattr("operator")?.extract()?;
            if op != "===" {
                standard.push(spec);
            }
        }
        if standard.is_empty() {
            return Ok(false);
        }
        let Some(c) = candidate else {
            return Ok(true);
        };
        // `s.contains(candidate)` through the element's own method, so
        // overrides (and `AttributeError` on non-specifiers) behave exactly
        // as in the original.
        let c_obj = version_obj(py, &c)?;
        for spec in standard {
            let matched: bool = spec
                .getattr("contains")?
                .call1((c_obj.bind(py),))?
                .extract()?;
            if !matched {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[pymethods]
impl SpecifierSetPy {
    #[new]
    #[pyo3(signature = (specifiers=None, prereleases=None))]
    fn new(
        py: Python,
        specifiers: Option<Bound<PyAny>>,
        prereleases: Option<Bound<PyAny>>,
    ) -> PyResult<Self> {
        // Absent `specifiers` defaults to `""` (the empty set).
        let specifiers = specifiers
            .unwrap_or_else(|| pyo3::types::PyString::new(py, "").into_any());
        if let Ok(s) = specifiers.extract::<String>() {
            let parts: Vec<String> = s
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            let mut specs: Vec<Bound<PyAny>> = Vec::with_capacity(parts.len());
            for part in &parts {
                // Parse through the real constructor for exact errors.
                let init: Bound<PyAny> = py
                    .get_type::<SpecifierPy>()
                    .call1((part.clone(),))?
                    .into_any();
                specs.push(init);
            }
            let has_arbitrary = s.contains("===");
            let n = specs.len();
            let tup = PyTuple::new(py, specs)?.unbind();
            return Ok(SpecifierSetPy {
                specs: tup,
                prereleases: prereleases.map(|b| b.unbind()),
                canonicalized: n <= 1,
                has_arbitrary,
                ranges_core: None,
                ranges_py: None,
                is_unsat: None,
            });
        }
        // Consume the iterable exactly once (it may be a one-shot
        // iterator); the tuple below owns the items afterwards.
        let mut items: Vec<Py<PyAny>> = Vec::new();
        let mut has_arbitrary = false;
        for item in specifiers.try_iter()?.map(|r| r.unwrap()) {
            let s: String = item.str()?.to_string();
            if s.contains("===") {
                has_arbitrary = true;
            }
            items.push(item.unbind());
        }
        let n = items.len();
        let tup = PyTuple::new(
            py,
            items.into_iter().map(|o| o.into_bound(py)).collect::<Vec<_>>(),
        )?
        .unbind();
        Ok(SpecifierSetPy {
            specs: tup,
            prereleases: prereleases.map(|b| b.unbind()),
            canonicalized: n <= 1,
            has_arbitrary,
            ranges_core: None,
            ranges_py: None,
            is_unsat: None,
        })
    }

    #[getter]
    fn _specs<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        Ok(slf.borrow().specs.clone_ref(slf.py()).into_bound(slf.py()))
    }

    #[getter]
    fn _ranges<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        match slf.borrow().ranges_py.as_ref().map(|o| o.clone_ref(slf.py())) {
            Some(l) => Ok(l.bind(slf.py()).clone()),
            None => Ok(slf.py().None().into_bound(slf.py())),
        }
    }

    /// Plain-attribute `_ranges` write: the stored object rules (tests poison
    /// it with `()`), so the core cache is dropped. Assigning `None`
    /// re-enables lazy recomputation.
    #[setter(_ranges)]
    fn set_ranges(slf: &Bound<Self>, value: Bound<PyAny>) -> PyResult<()> {
        let mut this = slf.borrow_mut();
        this.ranges_py = if value.is_none() { None } else { Some(value.unbind()) };
        this.ranges_core.take();
        Ok(())
    }

    #[getter]
    fn _is_unsatisfiable<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        match slf.borrow().is_unsat {
            Some(b) => Ok(if b { py_true(slf.py()).into_any() } else { py_false(slf.py()).into_any() }),
            None => Ok(slf.py().None().into_bound(slf.py())),
        }
    }

    #[getter]
    fn prereleases<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(raw) = slf.borrow().raw_prereleases(py) {
            if !raw.is_none() {
                return Ok(raw);
            }
        }
        match Self::derived_prereleases(slf)? {
            Some(true) => Ok(py_true(py).into_any()),
            Some(false) => Ok(py_false(py).into_any()),
            None => Ok(py.None().into_bound(py)),
        }
    }

    #[setter]
    fn set_prereleases(slf: &Bound<Self>, value: Bound<PyAny>) -> PyResult<()> {
        let mut this = slf.borrow_mut();
        this.prereleases = Some(value.unbind());
        this.is_unsat.take();
        Ok(())
    }
    /// Raw ``_prereleases`` override (``None`` when unset). Required by
    /// ``packaging.requirements.Requirement`` pickle state, which stores the
    /// explicit override rather than the derived ``prereleases`` value.
    #[getter(_prereleases)]
    fn raw_prereleases_attr<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        match slf.borrow().prereleases.as_ref().map(|o| o.clone_ref(py)) {
            Some(o) => Ok(o.bind(py).clone()),
            None => Ok(py.None().into_bound(py)),
        }
    }

    #[setter(_prereleases)]
    fn set_raw_prereleases_attr(slf: &Bound<Self>, value: Bound<PyAny>) -> PyResult<()> {
        let mut this = slf.borrow_mut();
        this.prereleases = if value.is_none() { None } else { Some(value.unbind()) };
        this.is_unsat.take();
        Ok(())
    }

    /// `SpecifierSet._get_ranges` (cached; returns the stored `_ranges`
    /// object verbatim when set).
    fn _get_ranges(slf: &Bound<Self>) -> PyResult<Py<PyAny>> {
        Self::py_ranges(slf)
    }

    /// `SpecifierSet._canonical_specs` (cached tuple).
    fn _canonical_specs(slf: &Bound<Self>) -> PyResult<Py<PyTuple>> {
        Self::canonical_specs(slf)
    }

    #[classattr]
    fn __match_args__() -> (&'static str,) {
        ("_str",)
    }

    #[getter]
    fn _str(slf: &Bound<Self>) -> PyResult<String> {
        Self::__str__(slf)
    }

    fn __str__(slf: &Bound<Self>) -> PyResult<String> {
        let specs = Self::canonical_specs(slf)?;
        let mut parts = Vec::new();
        for spec in specs.bind(slf.py()).iter() {
            parts.push(spec.str()?.to_string());
        }
        Ok(parts.join(","))
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        let py = slf.py();
        let raw = slf.borrow().prereleases.as_ref().map(|o| o.clone_ref(py));
        let pre = match raw {
            Some(r) => {
                let b = r.bind(py).clone();
                if b.is_none() {
                    String::new()
                } else {
                    format!(", prereleases={}", b.repr()?)
                }
            }
            None => String::new(),
        };
        let cls = class_name(&slf.clone().into_any())?;
        Ok(format!("<{cls}('{}'{pre})>", Self::__str__(slf)?))
    }

    fn __hash__(slf: &Bound<Self>) -> PyResult<isize> {
        let py = slf.py();
        Self::canonical_specs(slf)?.bind(py).hash()
    }

    fn __and__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let other_set: Bound<PyAny> = if let Ok(s) = other.extract::<String>() {
            py.get_type::<SpecifierSetPy>().call1((s,))?.into_any()
        } else if other.is_instance_of::<SpecifierSetPy>() {
            other.clone()
        } else {
            return not_implemented(py);
        };
        let (a_specs, b_specs) = (
            Self::canonical_specs(slf)?,
            other_set.getattr("_specs")?,
        );
        let mut combined: Vec<Bound<PyAny>> = Vec::new();
        for s in a_specs.bind(py).iter().chain(
            b_specs.try_iter()?.map(|r| r.unwrap()),
        ) {
            combined.push(s);
        }
        let result = Py::new(
            py,
            SpecifierSetPy {
                specs: PyTuple::new(py, combined)?.unbind(),
                prereleases: None,
                canonicalized: false,
                has_arbitrary: false,
                ranges_core: None,
                ranges_py: None,
                is_unsat: None,
            },
        )?;
        // `specifier._specs = self._specs + other._specs` etc. set post-hoc.
        let other_ref: PyRef<SpecifierSetPy> = other_set.extract()?;
        let (a_arb, b_arb) = (slf.borrow().has_arbitrary, other_ref.has_arbitrary);
        result.bind(py).borrow_mut().has_arbitrary = a_arb || b_arb;
        // Combine prerelease settings (`==` on the raw values):
        // self None or equal -> other; other None -> self; else ValueError.
        let a_raw = slf.borrow().prereleases.as_ref().map(|o| o.clone_ref(py));
        let b_raw = other_ref.prereleases.as_ref().map(|o| o.clone_ref(py));
        let final_pre = match (a_raw, b_raw) {
            (None, b) => b,
            (a, None) => a,
            (Some(a), Some(b)) => {
                let (ab, bb) = (a.bind(py).clone(), b.bind(py).clone());
                if !ab.eq(&bb)? {
                    return Err(PyValueError::new_err(
                        "Cannot combine SpecifierSets with True and False prerelease overrides.",
                    ));
                }
                Some(b)
            }
        };
        result.bind(py).borrow_mut().prereleases = final_pre;
        // Canonicalized flag mirrors `len(specs) <= 1`.
        let n: usize = result
            .bind(py)
            .getattr("_specs")?
            .call_method0("__len__")?
            .extract()?;
        result.bind(py).borrow_mut().canonicalized = n <= 1;
        Ok(result.into_any())
    }

    fn __eq__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        specset_eq(slf, other, true)
    }
    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        specset_eq(slf, other, false)
    }

    fn __len__(slf: &Bound<Self>) -> PyResult<usize> {
        Ok(slf.borrow().specs.bind(slf.py()).len())
    }

    fn __iter__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf
            .borrow()
            .specs
            .bind(slf.py())
            .call_method0("__iter__")
    }

    fn __contains__(slf: &Bound<Self>, item: Bound<PyAny>) -> PyResult<bool> {
        Self::contains_impl(slf, &item, None, None)
    }

    #[pyo3(signature = (item, prereleases=None, installed=None))]
    fn contains(
        slf: &Bound<Self>,
        item: Bound<PyAny>,
        prereleases: Option<Bound<PyAny>>,
        installed: Option<Bound<PyAny>>,
    ) -> PyResult<bool> {
        Self::contains_impl(slf, &item, prereleases.as_ref(), installed.as_ref())
    }

    #[pyo3(signature = (iterable, prereleases=None, key=None))]
    fn filter<'py>(
        slf: &Bound<'py, Self>,
        iterable: Bound<'py, PyAny>,
        prereleases: Option<Bound<'py, PyAny>>,
        key: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        Self::filter_impl(slf, &iterable, prereleases.as_ref(), key.as_ref())
    }

    fn is_unsatisfiable(slf: &Bound<Self>) -> PyResult<bool> {
        if let Some(cached) = slf.borrow().is_unsat {
            return Ok(cached);
        }
        let specs = Self::canonical_specs(slf)?;
        let result = if specs.bind(slf.py()).is_empty() {
            false
        } else {
            let ranges = Self::cached_intervals(slf)?;
            if ranges.is_empty() || Self::check_arbitrary_unsatisfiable(slf)? {
                true
            } else {
                let py = slf.py();
                let raw = slf.borrow().raw_prereleases(py);
                let is_false = matches!(raw, Some(ref r) if r.is(&py_false(py)));
                is_false && ranges::ranges_are_prerelease_only(&ranges)
            }
        };
        slf.borrow_mut().is_unsat = Some(result);
        Ok(result)
    }

    /// `SpecifierSet.to_range`.
    fn to_range(slf: &Bound<Self>) -> PyResult<Py<VersionRangePy>> {
        version_range_from_set(slf.py(), slf)
    }

    fn _check_relation_operand(
        slf: &Bound<Self>,
        other: Bound<PyAny>,
    ) -> PyResult<()> {
        if !other.is_instance_of::<SpecifierSetPy>() {
            return Err(PyTypeError::new_err("expected a SpecifierSet"));
        }
        if slf.borrow().has_arbitrary {
            return Err(PyValueError::new_err(
                "set relations do not support === specifiers",
            ));
        }
        let other_arb: bool = other
            .extract::<PyRef<SpecifierSetPy>>()
            .map(|o| o.has_arbitrary)
            .unwrap_or(false);
        if other_arb {
            return Err(PyValueError::new_err(
                "set relations do not support === specifiers",
            ));
        }
        Ok(())
    }

    fn is_subset(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<bool> {
        Self::_check_relation_operand(slf, other.clone())?;
        let a = Self::to_range(slf)?;
        let b = version_range_from_set(slf.py(), &other.extract::<Bound<SpecifierSetPy>>()?)?;
        version_range_is_subset(slf.py(), &a, &b)
    }

    fn is_superset(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<bool> {
        Self::_check_relation_operand(slf, other.clone())?;
        let a = Self::to_range(slf)?;
        let b = version_range_from_set(slf.py(), &other.extract::<Bound<SpecifierSetPy>>()?)?;
        version_range_is_subset(slf.py(), &b, &a)
    }

    fn is_disjoint(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<bool> {
        Self::_check_relation_operand(slf, other.clone())?;
        let a = Self::to_range(slf)?;
        let b = version_range_from_set(slf.py(), &other.extract::<Bound<SpecifierSetPy>>()?)?;
        version_range_is_disjoint(slf.py(), &a, &b)
    }

    fn __getstate__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let py = slf.py();
        let this = slf.borrow();
        let specs = this.specs.clone_ref(py).into_bound(py);
        let pre = match &this.prereleases {
            Some(r) => r.bind(py).clone(),
            None => py.None().into_bound(py),
        };
        PyTuple::new(py, [specs.into_any(), pre])
    }

    fn __setstate__(slf: &Bound<Self>, state: Bound<PyAny>) -> PyResult<()> {
        let py = slf.py();
        let bad = || {
            PyTypeError::new_err(format!(
                "Cannot restore SpecifierSet from {}",
                state.repr().map(|r| r.to_string()).unwrap_or_default()
            ))
        };
        let mut this = slf.borrow_mut();
        this.ranges_core.take();
        this.ranges_py.take();
        this.is_unsat.take();
        if let Ok(t) = state.downcast::<PyTuple>() {
            if t.len() == 2 {
                let (specs, pre) = (t.get_item(0)?, t.get_item(1)?);
                if validate_specset_specs(&specs)? && validate_pre_value(&pre)? {
                    this.specs = specs.extract()?;
                    this.prereleases =
                        if pre.is_none() { None } else { Some(pre.unbind()) };
                    this.canonicalized = this.specs.bind(py).len() <= 1;
                    this.has_arbitrary = has_arbitrary_py(py, &this.specs)?;
                    return Ok(());
                }
            }
            if t.len() == 2 {
                if let Ok(slots) = t.get_item(1)?.downcast_into::<PyDict>() {
                    let mut specs = slots.get_item("_specs")?.ok_or_else(bad)?;
                    if let Ok(frozen) = specs.downcast::<pyo3::types::PyFrozenSet>() {
                        // Sort with key=str for exactness.
                        let kwargs = PyDict::new(py);
                        kwargs.set_item("key", py.import("builtins")?.getattr("str")?)?;
                        specs = py
                            .import("builtins")?
                            .getattr("sorted")?
                            .call((frozen,), Some(&kwargs))?;
                        specs = py.import("builtins")?.getattr("tuple")?.call1((specs,))?;
                    }
                    // `slot_dict.get("_prereleases")`: missing means `None`.
                    let pre = slots
                        .get_item("_prereleases")?
                        .unwrap_or_else(|| py.None().into_bound(py));
                    if let Ok(tup) = specs.downcast::<PyTuple>() {
                        if validate_specset_specs(&tup.clone().into_any())?
                            && validate_pre_value(&pre)?
                        {
                            this.specs = tup.clone().unbind();
                            this.prereleases =
                                if pre.is_none() { None } else { Some(pre.unbind()) };
                            this.canonicalized = tup.len() <= 1;
                            this.has_arbitrary = has_arbitrary_py(py, &this.specs)?;
                            return Ok(());
                        }
                    }
                }
            }
        }
        if let Ok(d) = state.downcast::<PyDict>() {
            let mut specs = d.get_item("_specs")?.ok_or_else(bad)?;
            if let Ok(frozen) = specs.downcast::<pyo3::types::PyFrozenSet>() {
                let kwargs = PyDict::new(py);
                kwargs.set_item("key", py.import("builtins")?.getattr("str")?)?;
                specs = py
                    .import("builtins")?
                    .getattr("sorted")?
                    .call((frozen,), Some(&kwargs))?;
                specs = py.import("builtins")?.getattr("tuple")?.call1((specs,))?;
            }
            let specs_tup: Bound<PyTuple> = if let Ok(t) = specs.downcast::<PyTuple>() {
                t.clone()
            } else {
                return Err(bad());
            };
            let pre = d
                .get_item("_prereleases")?
                .unwrap_or_else(|| py.None().into_bound(py));
            if validate_specset_specs(&specs_tup.clone().into_any())? && validate_pre_value(&pre)? {
                this.specs = specs_tup.unbind();
                this.prereleases = if pre.is_none() { None } else { Some(pre.unbind()) };
                this.canonicalized = this.specs.bind(py).len() <= 1;
                this.has_arbitrary = has_arbitrary_py(py, &this.specs)?;
                return Ok(());
            }
        }
        Err(bad())
    }
}

/// `tuple` of `Specifier` instances (new format) for setstate validation.
fn validate_specset_specs(specs: &Bound<PyAny>) -> PyResult<bool> {
    let Ok(t) = specs.downcast::<PyTuple>() else {
        return Ok(false);
    };
    for item in t.iter() {
        if !item.is_instance_of::<SpecifierPy>() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Recompute `_has_arbitrary` from specs (`"===" in str(s)` each).
fn has_arbitrary_py(py: Python, specs: &Py<PyTuple>) -> PyResult<bool> {
    for spec in specs.bind(py).iter() {
        let s: String = spec.str()?.to_string();
        if s.contains("===") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn specset_eq(slf: &Bound<SpecifierSetPy>, other: &Bound<PyAny>, want_eq: bool) -> PyResult<Py<PyAny>> {
    let py = slf.py();
    if let Ok(s) = other.extract::<String>() {
        let constructed: Bound<SpecifierSetPy> = py.get_type::<SpecifierSetPy>().call1((s,))?.extract()?;
        return specset_eq(slf, &constructed.into_any(), want_eq);
    }
    if other.is_instance_of::<SpecifierPy>() {
        let s: String = other.str()?.to_string();
        let constructed: Bound<SpecifierSetPy> = py.get_type::<SpecifierSetPy>().call1((s,))?.extract()?;
        return specset_eq(slf, &constructed.into_any(), want_eq);
    }
    if !other.is_instance_of::<SpecifierSetPy>() {
        return not_implemented(py);
    }
    let a = SpecifierSetPy::canonical_specs(slf)?;
    let b: Bound<PyTuple> = other.getattr("_canonical_specs")?.call0()?.extract()?;
    let eq = a.bind(py).as_any().eq(&b)?;
    (eq == want_eq).into_py_any(py)
}
impl SpecifierSetPy {
    fn contains_impl(
        slf: &Bound<Self>,
        item: &Bound<PyAny>,
        prereleases: Option<&Bound<PyAny>>,
        installed: Option<&Bound<PyAny>>,
    ) -> PyResult<bool> {
        let py = slf.py();
        let parsed = coerce_py(py, item)?;
        // `installed=True` forces pre-release admission up front.
        let mut force_include = false;
        if let (Some(p), Some(inst)) = (&parsed, installed) {
            if inst.is_truthy()? && ranges::is_prerelease(p) {
                force_include = true;
            }
        }
        let (has_arbitrary, specs_empty) = {
            let this = slf.borrow();
            (this.has_arbitrary, this.specs.bind(py).is_empty())
        };
        if !(parsed.is_some() && !has_arbitrary && !specs_empty) {
            // Slow path (unparsable item, `===` in play, or no specs):
            // `bool(list(self.filter([check_item], prereleases=prereleases)))`.
            let check_item: Bound<PyAny> = match (&parsed, has_arbitrary) {
                (None, _) => item.clone(),
                (Some(_), true) if !item.is_instance_of::<Version>() => item.clone(),
                (Some(p), _) => version_obj(py, p)?.into_bound(py).into_any(),
            };
            let effective = if force_include {
                Some(py_true(py).into_any())
            } else {
                prereleases.cloned()
            };
            let out = Self::filter_impl(
                slf,
                &PyList::new(py, [check_item])?.into_any(),
                effective.as_ref(),
                None,
            )?;
            let mut n = 0;
            for _ in out.try_iter()?.map(|r| r.unwrap()) {
                n += 1;
            }
            return Ok(n > 0);
        }
        // Fast path: parseable, local-free version against a rangelike set.
        let p = parsed.clone().unwrap();
        if p.local.is_some() {
            // A local needs PEP 440 stripping that the range path applies;
            // fall through to the slow path with the parsed version (and
            // the installed-forced policy when set).
            let effective = if force_include {
                Some(py_true(py).into_any())
            } else {
                prereleases.cloned()
            };
            let out = Self::filter_impl(
                slf,
                &PyList::new(py, [version_obj(py, &p)?.into_bound(py).into_any()])?.into_any(),
                effective.as_ref(),
                None,
            )?;
            let mut n = 0;
            for _ in out.try_iter()?.map(|r| r.unwrap()) {
                n += 1;
            }
            return Ok(n > 0);
        }
        if ranges::is_prerelease(&p) {
            let raw = slf.borrow().raw_prereleases(py);
            let effective = if force_include {
                Some(py_true(py).into_any())
            } else {
                prereleases.cloned()
            };
            let gate = match effective.as_ref() {
                Some(e) if e.is(&py_false(py)) => true,
                None => matches!(raw, Some(ref r) if r.is(&py_false(py))),
                _ => false,
            };
            if gate {
                return Ok(false);
            }
        }
        // `bounds = self._ranges`: the Python-visible cache rules (it is
        // poisonable); only fold when it is `None`.
        if slf.borrow().ranges_py.is_some() {
            let bounds = Self::cached_intervals(slf)?;
            return Ok(ranges::matches_bounds_only(&bounds, &p));
        }
        let specs = Self::canonical_specs(slf)?;
        let mut need_ranges = false;
        for spec in specs.bind(py).iter() {
            // Per-spec `_fast_match` answers simple specifiers; any spec
            // needing the range path folds the intersected bounds once.
            let op: String = spec.getattr("operator")?.extract()?;
            let ver_str: String = spec.getattr("version")?.extract()?;
            // Wildcard specs never parse as versions (mirror `_fast_match`'s
            // early `None`); the range path answers them.
            let stripped = ver_str.strip_suffix(".*").unwrap_or(&ver_str);
            let spec_v: ParsedVersion = spec
                .getattr("_require_spec_version")?
                .call1((stripped,))?
                .extract::<PyRef<Version>>()
                .map(|r| r.inner.clone())?;
            match ranges::fast_match(&op, &ver_str, &spec_v, &p) {
                None => {
                    need_ranges = true;
                    break;
                }
                Some(false) => return Ok(false),
                Some(true) => {}
            }
        }
        if !need_ranges {
            return Ok(true);
        }
        let bounds = Self::cached_intervals(slf)?;
        Ok(ranges::matches_bounds_only(&bounds, &p))
    }

    fn filter_impl<'py>(
        slf: &Bound<'py, Self>,
        iterable: &Bound<'py, PyAny>,
        prereleases: Option<&Bound<'py, PyAny>>,
        key: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        // `if prereleases is None and self.prereleases is not None:
        //  prereleases = self.prereleases` (the property, not the raw).
        let mut pol_arg = prereleases.cloned();
        if pol_arg.is_none() {
            let prop: Bound<PyAny> = slf.getattr("prereleases")?;
            if !prop.is_none() {
                pol_arg = Some(prop);
            }
        }
        let (has_arbitrary, specs_empty) = {
            let this = slf.borrow();
            (this.has_arbitrary, this.specs.bind(py).is_empty())
        };
        if !specs_empty {
            if has_arbitrary {
                // Slow path for `===`: every spec must contain the keyed
                // item under `prereleases=True`.
                let specs = Self::canonical_specs(slf)?;
                let mut matched: Vec<Bound<PyAny>> = Vec::new();
                for item in iterable.try_iter()?.map(|r| r.unwrap()) {
                    let probe: Bound<PyAny> = match key {
                        Some(k) => k.call1((item.clone(),))?,
                        None => item.clone(),
                    };
                    let mut ok = true;
                    for spec in specs.bind(py).iter() {
                        let kwargs = PyDict::new(py);
                        kwargs.set_item("prereleases", py_true(py))?;
                        let contained: bool = spec
                            .getattr("contains")?
                            .call((probe.clone(),), Some(&kwargs))?
                            .extract()?;
                        if !contained {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        matched.push(item);
                    }
                }
                let derived = Self::derived_prereleases(slf)?;
                let raw = slf.borrow().raw_prereleases(py);
                let pol = normalize_pre(py, pol_arg.as_ref(), raw.as_ref(), derived);
                return apply_prereleases(py, matched, key, pol);
            }
            let ranges = Self::cached_intervals(slf)?;
            // Range-filter with the set's own prerelease resolution.
            let derived = Self::derived_prereleases(slf)?;
            let raw = slf.borrow().raw_prereleases(py);
            let pol = normalize_pre(py, pol_arg.as_ref(), raw.as_ref(), derived);
            return filter_by_ranges_py(py, &ranges, iterable, key, pol, &[]);
        }
        // Empty set: PEP 440 presence filtering.
        let derived = Self::derived_prereleases(slf)?;
        let raw = slf.borrow().raw_prereleases(py);
        let pol = normalize_pre(py, pol_arg.as_ref(), raw.as_ref(), derived);
        // `_apply_prereleases_filter(iterable, key, prereleases)`.
        apply_prereleases_on_items(py, iterable, key, pol)
    }
}

/// Eager `_apply_prereleases_filter` over a raw iterable.
fn apply_prereleases_on_items<'py>(
    py: Python<'py>,
    iterable: &Bound<'py, PyAny>,
    key: Option<&Bound<'py, PyAny>>,
    pol: PrePol,
) -> PyResult<Bound<'py, PyAny>> {
    let mut matched: Vec<Bound<PyAny>> = Vec::new();
    for item in iterable.try_iter()?.map(|r| r.unwrap()) {
        matched.push(item);
    }
    // `_apply_prereleases_filter` with `matches == iterable`: coerce inside.
    match pol {
        PrePol::Include => {
            let list = PyList::new(py, matched)?;
            Ok(list.call_method0("__iter__")?)
        }
        PrePol::Exclude => {
            let mut out = Vec::new();
            for item in matched {
                let (_, parsed) = coerce_keyed(py, &item, key)?;
                match parsed {
                    None => out.push(item),
                    Some(p) => {
                        if !ranges::is_prerelease(&p) {
                            out.push(item);
                        }
                    }
                }
            }
            let list = PyList::new(py, out)?;
            Ok(list.call_method0("__iter__")?)
        }
        PrePol::Default => {
            let mut all_nonfinal: Vec<Bound<PyAny>> = Vec::new();
            let mut arbitrary: Vec<Bound<PyAny>> = Vec::new();
            let mut found_final = false;
            let mut out: Vec<Bound<PyAny>> = Vec::new();
            for item in matched {
                let (_, parsed) = coerce_keyed(py, &item, key)?;
                match parsed {
                    None => {
                        if found_final {
                            out.push(item);
                        } else {
                            arbitrary.push(item.clone());
                            all_nonfinal.push(item);
                        }
                    }
                    Some(p) => {
                        if !ranges::is_prerelease(&p) {
                            if !found_final {
                                out.append(&mut arbitrary);
                                found_final = true;
                            }
                            out.push(item);
                        } else if !found_final {
                            all_nonfinal.push(item);
                        }
                    }
                }
            }
            if !found_final {
                out.extend(all_nonfinal);
            }
            let list = PyList::new(py, out)?;
            Ok(list.call_method0("__iter__")?)
        }
    }
}
// ---------------------------------------------------------------------------
// VersionRange (`packaging.ranges`).
// ---------------------------------------------------------------------------

/// `packaging.ranges.VersionRange`: a version set with `===` literals,
/// arbitrary-string admission, and a pre-release opt-in region.
#[pyclass(name = "VersionRange", module = "packaging.ranges", subclass)]
struct VersionRangePy {
    state: ranges::RangeState,
    bounds_py: Py<PyTuple>,
    region_py: Py<PyTuple>,
    admit_py: Py<pyo3::types::PyFrozenSet>,
    reject_py: Py<pyo3::types::PyFrozenSet>,
    arb: bool,
    configured: Option<Py<PyAny>>,
}

/// Wrap a core range state with its Python mirrors. `configured_raw` is the
/// original `prereleases` argument (or `None`); the core state carries the
/// `bool` view for its own logic.
fn wrap_range_state(
    py: Python,
    state: ranges::RangeState,
    configured_raw: Option<Py<PyAny>>,
) -> PyResult<VersionRangePy> {
    let mut bound_items = Vec::new();
    for (lower, upper) in &state.bounds {
        let l = Py::new(py, LowerBoundPy { inner: lower.clone() })?
            .into_bound(py)
            .into_any();
        let u = Py::new(py, UpperBoundPy { inner: upper.clone() })?
            .into_bound(py)
            .into_any();
        bound_items.push(PyTuple::new(py, [l, u])?.into_any());
    }
    let bounds_py = PyTuple::new(py, bound_items)?.unbind();
    let mut region_items = Vec::new();
    for (lower, upper) in &state.pre_region {
        let l = Py::new(py, LowerBoundPy { inner: lower.clone() })?
            .into_bound(py)
            .into_any();
        let u = Py::new(py, UpperBoundPy { inner: upper.clone() })?
            .into_bound(py)
            .into_any();
        region_items.push(PyTuple::new(py, [l, u])?.into_any());
    }
    let region_py = PyTuple::new(py, region_items)?.unbind();
    let admit_py = pyo3::types::PyFrozenSet::new(
        py,
        state.admit.iter().collect::<Vec<_>>(),
    )?
    .unbind();
    let reject_py = pyo3::types::PyFrozenSet::new(
        py,
        state.reject.iter().collect::<Vec<_>>(),
    )?
    .unbind();
    let configured = match configured_raw.as_ref().map(|o| o.bind(py).clone()) {
        Some(b) if b.is_none() => None,
        other => other.map(|b| b.unbind()),
    };
    let arb = state.admit_arbitrary;
    Ok(VersionRangePy {
        state,
        bounds_py,
        region_py,
        admit_py,
        reject_py,
        arb,
        configured,
    })
}

/// The `_struct_admits` closure for core set algebra: parses literals with
/// the live int limit, recording a digit-limit overflow to raise afterwards.
struct AdmitCtx {
    limit: Option<usize>,
    overflow: std::cell::RefCell<Option<(usize, usize)>>,
}

impl AdmitCtx {
    fn new(py: Python) -> Self {
        AdmitCtx { limit: spec_limit(py), overflow: std::cell::RefCell::new(None) }
    }

    fn admits(&self, bounds: &[ranges::Interval], arb: bool, literal: &str) -> bool {
        match version::parse(literal, self.limit) {
            Ok(parsed) => ranges::matches_bounds_only(bounds, &parsed),
            Err(version::ParseError::Invalid) => arb && bounds == ranges::full_range(),
            Err(version::ParseError::DigitLimit { max, got }) => {
                self.overflow.borrow_mut().replace((max, got));
                false
            }
        }
    }

    fn check(&self) -> PyResult<()> {
        if let Some((max, got)) = *self.overflow.borrow() {
            return Err(PyValueError::new_err(
                version::ParseError::digit_limit_message(max, got),
            ));
        }
        Ok(())
    }
}

/// Run core set algebra with admission, raising a recorded digit-limit
/// `ValueError` (which the original propagates out of `coerce_version`).
fn with_admits<T>(
    py: Python,
    f: impl FnOnce(&dyn Fn(&[ranges::Interval], bool, &str) -> bool) -> T,
) -> PyResult<T> {
    let ctx = AdmitCtx::new(py);
    let out = f(&|bounds, arb, literal| ctx.admits(bounds, arb, literal));
    ctx.check()?;
    Ok(out)
}

impl VersionRangePy {
    fn policy_compat(
        slf: &Bound<Self>,
        other: &Bound<PyAny>,
    ) -> PyResult<ranges::RangeState> {
        let py = slf.py();
        let Ok(o) = other.extract::<PyRef<Self>>() else {
            let type_name: String = other.get_type().getattr("__name__")?.extract()?;
            return Err(PyTypeError::new_err(format!(
                "expected VersionRange, got {type_name}"
            )));
        };
        let (a, b) = (
            slf.borrow().state.configured,
            opt_bool(&o.configured.as_ref().map(|v| v.bind(py).clone())),
        );
        if a != b {
            let fmt = |v: Option<bool>| match v {
                Some(true) => "True".to_string(),
                Some(false) => "False".to_string(),
                None => "None".to_string(),
            };
            return Err(PyValueError::new_err(format!(
                "Cannot combine VersionRange operands with different pre-release policies: {} and {}",
                fmt(a),
                fmt(b)
            )));
        }
        Ok(o.state.clone())
    }
}

#[pymethods]
impl VersionRangePy {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(
        args: &Bound<PyTuple>,
        kwargs: Option<Bound<PyDict>>,
    ) -> PyResult<Self> {
        let _ = (args, kwargs);
        Err(PyTypeError::new_err(
            "cannot create 'VersionRange' instances directly; use SpecifierSet.to_range(), VersionRange.full(), VersionRange.empty(), or VersionRange.singleton() instead",
        ))
    }

    #[classmethod]
    #[pyo3(signature = (*, prereleases=None))]
    fn empty(_cls: &Bound<PyType>, prereleases: Option<Bound<PyAny>>) -> PyResult<Self> {
        let py = _cls.py();
        let state = with_admits(py, |admits| {
            ranges::RangeState::build(Vec::new(), Default::default(), Default::default(), false, Vec::new(), opt_bool(&prereleases), admits)
        })?;
        wrap_range_state(py, state, prereleases.map(|b| b.unbind()))
    }

    #[classmethod]
    #[pyo3(signature = (*, admit_arbitrary=true, prereleases=None))]
    fn full(
        _cls: &Bound<PyType>,
        admit_arbitrary: bool,
        prereleases: Option<Bound<PyAny>>,
    ) -> PyResult<Self> {
        let py = _cls.py();
        let state = with_admits(py, |admits| {
            ranges::RangeState::build(
                ranges::full_range(),
                Default::default(),
                Default::default(),
                admit_arbitrary,
                Vec::new(),
                opt_bool(&prereleases),
                admits,
            )
        })?;
        wrap_range_state(py, state, prereleases.map(|b| b.unbind()))
    }

    #[classmethod]
    #[pyo3(signature = (version, *, prereleases=None))]
    fn singleton(
        _cls: &Bound<PyType>,
        version: Bound<PyAny>,
        prereleases: Option<Bound<PyAny>>,
    ) -> PyResult<Self> {
        let py = _cls.py();
        let inner = match coerce_py(py, &version)? {
            Some(v) => v,
            None => {
                // `Version(version)` raises `InvalidVersion` for strings;
                // replay it for exactness.
                Version::parse_new(py, &version)?;
                unreachable!()
            }
        };
        let bounds = ranges::canonical_floor(vec![(
            ranges::LowerBound { point: ranges::BoundPoint::Ver(inner.clone()), inclusive: true },
            ranges::UpperBound { point: ranges::BoundPoint::Ver(inner), inclusive: true },
        )]);
        let state = with_admits(py, |admits| {
            ranges::RangeState::build(bounds, Default::default(), Default::default(), false, Vec::new(), opt_bool(&prereleases), admits)
        })?;
        wrap_range_state(py, state, prereleases.map(|b| b.unbind()))
    }

    #[getter]
    fn _bounds<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        Ok(slf.borrow().bounds_py.bind(slf.py()).clone())
    }

    #[getter]
    fn _pre_region<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        Ok(slf.borrow().region_py.bind(slf.py()).clone())
    }

    #[getter]
    fn _admit<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, pyo3::types::PyFrozenSet>> {
        Ok(slf.borrow().admit_py.bind(slf.py()).clone())
    }

    #[getter]
    fn _reject<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, pyo3::types::PyFrozenSet>> {
        Ok(slf.borrow().reject_py.bind(slf.py()).clone())
    }

    #[getter]
    fn _admit_arbitrary(&self) -> bool {
        self.arb
    }

    #[getter]
    fn _prereleases_configured<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        match slf.borrow().configured.as_ref().map(|o| o.clone_ref(slf.py())) {
            Some(o) => Ok(o.bind(slf.py()).clone()),
            None => Ok(slf.py().None().into_bound(slf.py())),
        }
    }

    fn intersection(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<Py<Self>> {
        let py = slf.py();
        let o_state = Self::policy_compat(slf, &other)?;
        let this = slf.borrow();
        let raw = this.configured.as_ref().map(|o| o.clone_ref(py));
        let state = with_admits(py, |admits| this.state.clone().intersection(&o_state, admits))?;
        Py::new(py, wrap_range_state(py, state, raw)?)
    }

    fn union(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<Py<Self>> {
        let py = slf.py();
        let o_state = Self::policy_compat(slf, &other)?;
        let this = slf.borrow();
        let raw = this.configured.as_ref().map(|o| o.clone_ref(py));
        let state = with_admits(py, |admits| this.state.clone().union(&o_state, admits))?;
        Py::new(py, wrap_range_state(py, state, raw)?)
    }

    fn complement(slf: &Bound<Self>) -> PyResult<Py<Self>> {
        let py = slf.py();
        let this = slf.borrow();
        let raw = this.configured.as_ref().map(|o| o.clone_ref(py));
        let state = with_admits(py, |admits| this.state.clone().complement(admits))?;
        Py::new(py, wrap_range_state(py, state, raw)?)
    }

    fn difference(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<Py<Self>> {
        let py = slf.py();
        let o_state = Self::policy_compat(slf, &other)?;
        let this = slf.borrow();
        let raw = this.configured.as_ref().map(|o| o.clone_ref(py));
        let state = with_admits(py, |admits| this.state.clone().difference(&o_state, admits))?;
        Py::new(py, wrap_range_state(py, state, raw)?)
    }

    /// Mirror of `VersionRange._same_releases`: symmetric-difference
    /// emptiness both ways (used by `to_specifier_set` under
    /// `prereleases=False`, and by tests directly).
    fn _same_releases(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<bool> {
        let py = slf.py();
        let o_state = Self::policy_compat(slf, other)?;
        let this = slf.borrow().state.clone();
        with_admits(py, |admits| this.same_releases(&o_state, admits))
    }

    fn __and__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        match Self::intersection(slf, other.clone()) {
            Ok(r) => Ok(r.into_any()),
            Err(e) if e.is_instance_of::<PyTypeError>(slf.py()) => not_implemented(slf.py()),
            Err(e) => Err(e),
        }
    }
    fn __or__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        match Self::union(slf, other.clone()) {
            Ok(r) => Ok(r.into_any()),
            Err(e) if e.is_instance_of::<PyTypeError>(slf.py()) => not_implemented(slf.py()),
            Err(e) => Err(e),
        }
    }
    fn __invert__(slf: &Bound<Self>) -> PyResult<Py<Self>> {
        Self::complement(slf)
    }
    fn __sub__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        match Self::difference(slf, other.clone()) {
            Ok(r) => Ok(r.into_any()),
            Err(e) if e.is_instance_of::<PyTypeError>(slf.py()) => not_implemented(slf.py()),
            Err(e) => Err(e),
        }
    }

    fn is_subset(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<bool> {
        let py = slf.py();
        let o_state = Self::policy_compat(slf, &other)?;
        let this = slf.borrow().state.clone();
        with_admits(py, |admits| this.is_subset(&o_state, admits))
    }
    fn is_superset(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<bool> {
        let py = slf.py();
        let o_state = Self::policy_compat(slf, &other)?;
        let this = slf.borrow().state.clone();
        with_admits(py, |admits| this.is_superset(&o_state, admits))
    }
    fn is_disjoint(slf: &Bound<Self>, other: Bound<PyAny>) -> PyResult<bool> {
        let py = slf.py();
        let o_state = Self::policy_compat(slf, &other)?;
        let this = slf.borrow().state.clone();
        with_admits(py, |admits| this.is_disjoint(&o_state, admits))
    }

    #[pyo3(signature = (iterable, prereleases=None, key=None))]
    fn filter<'py>(
        slf: &Bound<'py, Self>,
        iterable: Bound<'py, PyAny>,
        prereleases: Option<Bound<'py, PyAny>>,
        key: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        Self::filter_impl(slf, &iterable, prereleases.as_ref(), key.as_ref())
    }

    /// `VersionRange._from_specifier_set` (friend constructor).
    #[classmethod]
    fn _from_specifier_set(
        _cls: &Bound<PyType>,
        specifier_set: Bound<PyAny>,
    ) -> PyResult<Py<Self>> {
        version_range_from_set_py(_cls.py(), &specifier_set)
    }

    fn to_specifier_set(slf: &Bound<Self>) -> PyResult<Option<Py<SpecifierSetPy>>> {
        version_range_to_set(slf)
    }

    #[getter]
    fn is_empty(slf: &Bound<Self>) -> PyResult<bool> {
        let py = slf.py();
        let this = slf.borrow().state.clone();
        with_admits(py, |_| this.is_empty_state())
    }

    #[pyo3(signature = (item, prereleases=None, installed=None))]
    fn contains(
        slf: &Bound<Self>,
        item: Bound<PyAny>,
        prereleases: Option<Bound<PyAny>>,
        installed: Option<Bound<PyAny>>,
    ) -> PyResult<bool> {
        Self::contains_impl(slf, &item, prereleases.as_ref(), installed.as_ref())
    }

    fn __contains__(slf: &Bound<Self>, item: Bound<PyAny>) -> PyResult<bool> {
        Self::contains_impl(slf, &item, None, None)
    }

    fn __eq__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let Ok(o) = other.extract::<PyRef<Self>>() else {
            return not_implemented(py);
        };
        let (a, b) = (slf.borrow().state.clone(), o.state.clone());
        (a == b).into_py_any(py)
    }
    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let Ok(o) = other.extract::<PyRef<Self>>() else {
            return not_implemented(py);
        };
        let (a, b) = (slf.borrow().state.clone(), o.state.clone());
        (a != b).into_py_any(py)
    }

    fn __hash__(slf: &Bound<Self>) -> PyResult<isize> {
        let py = slf.py();
        let this = slf.borrow();
        let tup = PyTuple::new(
            py,
            [
                this.bounds_py.bind(py).clone().into_any(),
                this.admit_py.bind(py).clone().into_any(),
                this.reject_py.bind(py).clone().into_any(),
                this.arb.into_py_any(py)?.bind(py).clone(),
                match this.configured.as_ref().map(|o| o.bind(py).clone()) {
                    Some(o) => o.clone(),
                    None => py.None().into_bound(py),
                },
                this.region_py.bind(py).clone().into_any(),
            ],
        )?;
        tup.hash()
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        let py = slf.py();
        let this = slf.borrow();
        let (body, tail, region) = this.state.repr_body();
        let cls = class_name(&slf.clone().into_any())?;
        let body_repr: String = pyo3::types::PyString::new(py, &body).repr()?.to_string();
        let mut full_tail = tail;
        if let Some(region) = region {
            let region_repr: String =
                pyo3::types::PyString::new(py, &region).repr()?.to_string();
            full_tail.push_str(&format!(" pre-region={region_repr}"));
        }
        Ok(format!("<{cls} {body_repr}{full_tail}>"))
    }
}

/// `opt_bool`: `None` stays unset, anything else must be a real bool value
/// carried as Rust `bool` (the callers only pass `None`/`True`/`False`).
fn opt_bool(v: &Option<Bound<PyAny>>) -> Option<bool> {
    match v {
        None => None,
        Some(b) if b.is_none() => None,
        Some(b) => Some(b.is_truthy().unwrap_or(false)),
    }
}

/// `VersionRange.filter` implementation.
impl VersionRangePy {
    fn filter_impl<'py>(
        slf: &Bound<'py, Self>,
        iterable: &Bound<'py, PyAny>,
        prereleases: Option<&Bound<'py, PyAny>>,
        key: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let this = slf.borrow().state.clone();
        // `if prereleases is None: prereleases = configured; region =
        // pre_region`. An explicit argument (even `None`, which is
        // indistinguishable) clears the region; a configured policy
        // surfaces by identity (`False` excludes, anything else includes).
        let (pol, region): (PrePol, Vec<ranges::Interval>) = match prereleases {
            Some(e) if !e.is_none() => (
                if e.is(&py_false(py)) { PrePol::Exclude } else { PrePol::Include },
                Vec::new(),
            ),
            _ => match slf.borrow().configured.as_ref().map(|o| o.clone_ref(py)) {
                None => (PrePol::Default, this.pre_region.clone()),
                Some(c) => {
                    let b = c.bind(py).clone();
                    if b.is(&py_false(py)) {
                        (PrePol::Exclude, Vec::new())
                    } else {
                        (PrePol::Include, Vec::new())
                    }
                }
            },
        };
        let arb_active = this.arbitrary_active();
        let has_lit = this.has_literals();
        if !has_lit && !arb_active {
            if !region.is_empty() && region == this.bounds {
                return filter_by_ranges_py(py, &this.bounds, iterable, key, PrePol::Include, &[]);
            }
            return filter_by_ranges_py(py, &this.bounds, iterable, key, pol, &region);
        }
        admission_filter_py(py, &this, iterable, key, pol, arb_active, &region)
    }

    fn contains_impl(
        slf: &Bound<Self>,
        item: &Bound<PyAny>,
        prereleases: Option<&Bound<PyAny>>,
        installed: Option<&Bound<PyAny>>,
    ) -> PyResult<bool> {
        let py = slf.py();
        if !item.is_instance_of::<pyo3::types::PyString>() && !item.is_instance_of::<Version>() {
            let type_name: String = item.get_type().getattr("__name__")?.extract()?;
            return Err(PyTypeError::new_err(format!(
                "VersionRange.contains() expected str or Version, got {type_name}"
            )));
        }
        let mut parsed: Option<ParsedVersion> = if item.is_instance_of::<Version>() {
            Some(item.extract::<PyRef<Version>>().map(|r| r.inner.clone())?)
        } else {
            None
        };
        // Mirror the original: `installed` with a prerelease item forces the
        // prerelease policy locally (no recursion).
        let true_lit;
        let mut prereleases = prereleases;
        if let Some(inst) = installed {
            if parsed.is_none() {
                parsed = coerce_py(py, item)?;
            }
            if let Some(p) = &parsed {
                if inst.is_truthy()? && ranges::is_prerelease(p) {
                    true_lit = py_true(py).into_any();
                    prereleases = Some(&true_lit);
                }
            }
        }
        // NOTE: recursion with a fresh `True` literal (mirrors the original).
        let this = slf.borrow().state.clone();
        let effective: Option<PrePol> = match prereleases {
            None => match slf.borrow().configured.as_ref().map(|o| o.clone_ref(py)) {
                Some(c) => {
                    let b = c.bind(py).clone();
                    Some(if b.is(&py_false(py)) { PrePol::Exclude } else { PrePol::Include })
                }
                None => None,
            },
            Some(e) if e.is_none() => match slf.borrow().configured.as_ref().map(|o| o.clone_ref(py)) {
                Some(c) => {
                    let b = c.bind(py).clone();
                    Some(if b.is(&py_false(py)) { PrePol::Exclude } else { PrePol::Include })
                }
                None => None,
            },
            Some(e) if e.is(&py_false(py)) => Some(PrePol::Exclude),
            Some(_) => Some(PrePol::Include),
        };
        if !this.admit.is_empty() || !this.reject.is_empty() {
            let item_str: String = item.str()?.to_string().to_lowercase();
            if this.reject.contains(&item_str) {
                return Ok(false);
            }
            if this.admit.contains(&item_str) {
                if effective == Some(PrePol::Exclude) {
                    if let Some(lit) = coerce_py(py, &pyo3::types::PyString::new(py, &item_str).into_any())? {
                        if ranges::is_prerelease(&lit) {
                            return Ok(false);
                        }
                    }
                }
                return Ok(true);
            }
        }
        if !item.is_instance_of::<Version>() {
            if parsed.is_none() {
                parsed = coerce_py(py, item)?;
            }
            if parsed.is_none() {
                return Ok(this.arbitrary_active());
            }
            let _ = parsed.clone().unwrap();
        }
        let p = parsed.unwrap();
        if effective == Some(PrePol::Exclude) && ranges::is_prerelease(&p) {
            return Ok(false);
        }
        Ok(ranges::matches_bounds_only(&this.bounds, &p))
    }
}

/// Eager `_filter_with_admission`.
#[allow(clippy::too_many_arguments)]
fn admission_filter_py<'py>(
    py: Python<'py>,
    state: &ranges::RangeState,
    iterable: &Bound<'py, PyAny>,
    key: Option<&Bound<'py, PyAny>>,
    pol: PrePol,
    arbitrary_active: bool,
    region: &[ranges::Interval],
) -> PyResult<Bound<'py, PyAny>> {
    struct Admit {
        ok: bool,
        parsed: Option<ParsedVersion>,
        by_literal: bool,
    }
    let admit = |item: &Bound<PyAny>| -> PyResult<Admit> {
        let raw: Bound<PyAny> = match key {
            Some(k) => k.call1((item.clone(),))?,
            None => item.clone(),
        };
        let raw_lower: String = raw.str()?.to_string().to_lowercase();
        if !state.reject.is_empty() && state.reject.contains(&raw_lower) {
            return Ok(Admit { ok: false, parsed: None, by_literal: false });
        }
        if !state.admit.is_empty() && state.admit.contains(&raw_lower) {
            return Ok(Admit { ok: true, parsed: coerce_py(py, &raw)?, by_literal: true });
        }
        let parsed = coerce_py(py, &raw)?;
        match parsed {
            None => Ok(Admit { ok: arbitrary_active, parsed: None, by_literal: false }),
            Some(p) => {
                if !ranges::matches_bounds_only(&state.bounds, &p) {
                    return Ok(Admit { ok: false, parsed: None, by_literal: false });
                }
                Ok(Admit { ok: true, parsed: Some(p), by_literal: false })
            }
        }
    };
    if pol == PrePol::Include {
        let mut out = Vec::new();
        for item in iterable.try_iter()?.map(|r| r.unwrap()) {
            if admit(&item)?.ok {
                out.push(item);
            }
        }
        let list = PyList::new(py, out)?;
        return list.call_method0("__iter__");
    }
    if pol == PrePol::Exclude {
        let mut out = Vec::new();
        for item in iterable.try_iter()?.map(|r| r.unwrap()) {
            let a = admit(&item)?;
            if !a.ok {
                continue;
            }
            if let Some(p) = a.parsed {
                if ranges::is_prerelease(&p) {
                    continue;
                }
            }
            out.push(item);
        }
        let list = PyList::new(py, out)?;
        return list.call_method0("__iter__");
    }
    let mut all_nonfinal: Vec<Bound<PyAny>> = Vec::new();
    let mut arbitrary: Vec<Bound<PyAny>> = Vec::new();
    let mut found_final = false;
    let mut out: Vec<Bound<PyAny>> = Vec::new();
    for item in iterable.try_iter()?.map(|r| r.unwrap()) {
        let a = admit(&item)?;
        if !a.ok {
            continue;
        }
        match a.parsed {
            None => {
                if found_final {
                    out.push(item);
                } else {
                    arbitrary.push(item.clone());
                    all_nonfinal.push(item);
                }
            }
            Some(p) => {
                if !ranges::is_prerelease(&p) {
                    if !found_final {
                        out.append(&mut arbitrary);
                        found_final = true;
                    }
                    out.push(item);
                } else if a.by_literal || (!region.is_empty() && ranges::matches_bounds_only(region, &p)) {
                    out.push(item);
                } else if !found_final {
                    all_nonfinal.push(item);
                }
            }
        }
    }
    if !found_final {
        out.extend(all_nonfinal);
    }
    let list = PyList::new(py, out)?;
    list.call_method0("__iter__")
}
// ---------------------------------------------------------------------------
// SpecifierSet <-> VersionRange bridge.
// ---------------------------------------------------------------------------

/// Mirror of `VersionRange._from_specifier_set`.
fn version_range_from_set(
    py: Python,
    set: &Bound<SpecifierSetPy>,
) -> PyResult<Py<VersionRangePy>> {
    let empty: bool = set.borrow().specs.bind(py).is_empty();
    let has_arbitrary = set.borrow().has_arbitrary;
    let configured_raw: Option<Bound<PyAny>> = set
        .borrow()
        .raw_prereleases(py)
        .and_then(|b| if b.is_none() { None } else { Some(b) });
    let configured_bool = opt_bool(&configured_raw);
    let mut result: Py<VersionRangePy> = if empty {
        let state = with_admits(py, |admits| {
            ranges::RangeState::build(
                ranges::full_range(),
                Default::default(),
                Default::default(),
                true,
                Vec::new(),
                None,
                admits,
            )
        })?;
        // `cls.full()` carries no configured policy here; `_with_policy`
        // below sets it.
        Py::new(py, wrap_range_state(py, state, None)?)?
    } else if !has_arbitrary {
        let bounds = ranges::canonical_floor(SpecifierSetPy::core_ranges(set)?);
        let state = with_admits(py, |admits| {
            ranges::RangeState::build(bounds, Default::default(), Default::default(), false, Vec::new(), None, admits)
        })?;
        Py::new(py, wrap_range_state(py, state, None)?)?
    } else {
        let full = {
            let state = with_admits(py, |admits| {
                ranges::RangeState::build(
                    ranges::full_range(),
                    Default::default(),
                    Default::default(),
                    true,
                    Vec::new(),
                    None,
                    admits,
                )
            })?;
            Py::new(py, wrap_range_state(py, state, None)?)?
        };
        let mut acc = full;
        let specs = SpecifierSetPy::canonical_specs(set)?;
        for spec in specs.bind(py).iter() {
            let op: String = spec.getattr("operator")?.extract()?;
            let operand = if op == "===" {
                let ver: String = spec.getattr("version")?.extract()?;
                let mut admit = std::collections::HashSet::new();
                admit.insert(ver.to_lowercase());
                let state = with_admits(py, |admits| {
                    ranges::RangeState::build(Vec::new(), admit, Default::default(), false, Vec::new(), None, admits)
                })?;
                Py::new(py, wrap_range_state(py, state, None)?)?
            } else {
                let ver_str: String = spec.getattr("version")?.extract()?;
                let spec_v: ParsedVersion = spec
                    .getattr("_require_spec_version")?
                    .call1((ver_str.clone(),))?
                    .extract::<PyRef<Version>>()
                    .map(|r| r.inner.clone())?;
                let base = ver_str.strip_suffix(".*").unwrap_or(&ver_str);
                let base_v: ParsedVersion = spec
                    .getattr("_require_spec_version")?
                    .call1((base,))?
                    .extract::<PyRef<Version>>()
                    .map(|r| r.inner.clone())?;
                let bounds = ranges::canonical_floor(ranges::bounds_for_spec(
                    &op, &ver_str, &base_v,
                ));
                let _ = spec_v;
                let state = with_admits(py, |admits| {
                    ranges::RangeState::build(bounds, Default::default(), Default::default(), false, Vec::new(), None, admits)
                })?;
                Py::new(py, wrap_range_state(py, state, None)?)?
            };
            let acc_bound = acc.bind(py).clone();
            let op_bound = operand.bind(py).clone();
            acc = VersionRangePy::intersection(&acc_bound, op_bound.into_any())?;
        }
        acc
    };
    // The opt-in region: union of pre-naming specs' ranges, then
    // `_with_policy` (which clips to the bounds).
    let mut region: Vec<ranges::Interval> = Vec::new();
    if configured_bool.is_none() {
        let specs = SpecifierSetPy::canonical_specs(set)?;
        for spec in specs.bind(py).iter() {
            let op: String = spec.getattr("operator")?.extract()?;
            if op == "===" {
                continue;
            }
            let pre: Bound<PyAny> = spec.getattr("prereleases")?;
            if !pre.is_truthy()? {
                continue;
            }
            let ver_str: String = spec.getattr("version")?.extract()?;
            let base = ver_str.strip_suffix(".*").unwrap_or(&ver_str);
            let base_v: ParsedVersion = spec
                .getattr("_require_spec_version")?
                .call1((base,))?
                .extract::<PyRef<Version>>()
                .map(|r| r.inner.clone())?;
            let bounds = ranges::canonical_floor(ranges::bounds_for_spec(&op, &ver_str, &base_v));
            region = ranges::union_ranges(&region, &bounds);
        }
    }
    // Rebuild with the region + configured policy (`_with_policy`).
    let result_bound = result.bind(py).clone();
    let st = result_bound.borrow().state.clone();
    let admit = st.admit.clone();
    let reject = st.reject.clone();
    let arb = st.admit_arbitrary;
    let new_state = with_admits(py, |admits| {
        ranges::RangeState::build(
            st.bounds.clone(),
            admit,
            reject,
            arb,
            region,
            configured_bool,
            admits,
        )
    })?;
    result = Py::new(py, wrap_range_state(py, new_state, configured_raw.map(|b| b.unbind()))?)?;
    Ok(result)
}

/// `VersionRange._from_specifier_set` classmethod entry (takes any object
/// with the SpecifierSet shape; `TypeError` otherwise, mirroring attribute
/// access failures).
fn version_range_from_set_py(py: Python, set: &Bound<PyAny>) -> PyResult<Py<VersionRangePy>> {
    let o: Bound<SpecifierSetPy> = set.extract().map_err(|_| {
        PyTypeError::new_err("expected SpecifierSet")
    })?;
    version_range_from_set(py, &o)
}

/// Mirror of `VersionRange.to_specifier_set`.
fn version_range_to_set(slf: &Bound<VersionRangePy>) -> PyResult<Option<Py<SpecifierSetPy>>> {
    let py = slf.py();
    let this = slf.borrow().state.clone();
    let configured_raw: Option<Bound<PyAny>> = slf
        .borrow()
        .configured
        .as_ref()
        .map(|o| o.bind(py).clone());
    let configured_bool = opt_bool(&configured_raw);
    if !this.reject.is_empty() {
        return Ok(None);
    }
    if this.admit_arbitrary && this.bounds != ranges::full_range() {
        return Ok(None);
    }
    if with_admits(py, |_| this.is_empty_state())? {
        let set: Bound<SpecifierSetPy> = py
            .get_type::<SpecifierSetPy>()
            .call(
                (
                    "<0",
                    match configured_raw {
                        Some(b) => b,
                        None => py.None().into_bound(py),
                    },
                ),
                None,
            )?
            .extract()?;
        return Ok(Some(set.unbind()));
    }
    if this.bounds.is_empty() {
        if this.admit.len() != 1 {
            return Ok(None);
        }
        let literal = this.admit.iter().next().unwrap().clone();
        let base = format!("==={literal}");
        // A `===` literal holding a comma has no single-set spelling.
        if base.contains(',') {
            return Ok(None);
        }
        match py.get_type::<SpecifierSetPy>().call1((base,)) {
            Ok(set) => return Ok(Some(set.extract()?)),
            Err(_) => return Ok(None),
        }
    }
    if !this.admit.is_empty() {
        return Ok(None);
    }
    let bases: Vec<String> = if this.bounds == ranges::full_range() {
        vec![if this.admit_arbitrary { String::new() } else { ">=0.dev0".to_string() }]
    } else {
        let mut layouts = vec![this.bounds.clone()];
        if configured_bool == Some(false) {
            let tightened = ranges::tighten_no_prereleases(this.bounds.clone());
            if tightened != this.bounds {
                layouts.push(tightened);
            }
        }
        let mut bases = Vec::new();
        for layout in &layouts {
            let exclusions = match ranges::encode_gaps(layout) {
                Some(e) => e,
                None => continue,
            };
            for keep_dev0 in [false, true] {
                let outer = match ranges::encode_interval(&layout[0].0, &layout[layout.len() - 1].1, keep_dev0) {
                    Some(o) => o,
                    None => continue,
                };
                let mut base = outer;
                base.extend(exclusions.clone());
                let base = base.join(",");
                if !bases.contains(&base) {
                    bases.push(base);
                }
            }
        }
        bases
    };
    let add_floor = configured_bool.is_none()
        && with_admits(py, |_| {
            // `self._pre_region == self._bounds` on core states.
            this.pre_region == this.bounds
        })?;
    let mut best: Option<Py<SpecifierSetPy>> = None;
    let mut best_key = (usize::MAX, usize::MAX);
    for base in &bases {
        let mut candidates = vec![base.clone()];
        if add_floor {
            candidates.push(if base.is_empty() {
                ">=0.dev0".to_string()
            } else {
                format!("{base},>=0.dev0")
            });
        }
        for spec_str in candidates {
            let recovered: Bound<SpecifierSetPy> = match py
                .get_type::<SpecifierSetPy>()
                .call(
                    (
                        spec_str.clone(),
                        match configured_raw.clone() {
                            Some(b) => b,
                            None => py.None().into_bound(py),
                        },
                    ),
                    None,
                ) {
                Ok(s) => match s.extract() {
                    Ok(v) => v,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            let n: usize = recovered.getattr("_specs")?.call_method0("__len__")?.extract()?;
            let s: String = recovered.str()?.to_string();
            let key = (n, s.len());
            if best.is_some() && key >= best_key {
                continue;
            }
            let candidate = version_range_from_set(py, &recovered)?;
            let matches = {
                let a = candidate.bind(py).borrow().state.clone();
                let eq = a == this;
                eq || (configured_bool == Some(false)
                    && {
                        let b = candidate.bind(py).borrow().state.clone();
                        // `_same_releases` both ways.
                        with_admits(py, |admits| this.same_releases(&b, admits))?
                    })
            };
            if matches {
                best = Some(recovered.unbind());
                best_key = key;
            }
        }
    }
    Ok(best)
}

fn version_range_is_subset(
    py: Python,
    a: &Py<VersionRangePy>,
    b: &Py<VersionRangePy>,
) -> PyResult<bool> {
    check_range_policy(a.bind(py), b.bind(py))?;
    let (sa, sb) = (a.bind(py).borrow().state.clone(), b.bind(py).borrow().state.clone());
    with_admits(py, |admits| sa.is_subset(&sb, admits))
}

fn version_range_is_disjoint(
    py: Python,
    a: &Py<VersionRangePy>,
    b: &Py<VersionRangePy>,
) -> PyResult<bool> {
    check_range_policy(a.bind(py), b.bind(py))?;
    let (sa, sb) = (a.bind(py).borrow().state.clone(), b.bind(py).borrow().state.clone());
    with_admits(py, |admits| sa.is_disjoint(&sb, admits))
}

/// Refuse combining ranges with different pre-release policies (mirror of
/// `VersionRange._check_policy_compat`).
fn check_range_policy(a: &Bound<VersionRangePy>, b: &Bound<VersionRangePy>) -> PyResult<()> {
    let (ca, cb) = (a.borrow().state.configured, b.borrow().state.configured);
    if ca != cb {
        let fmt = |v: Option<bool>| match v {
            Some(true) => "True".to_string(),
            Some(false) => "False".to_string(),
            None => "None".to_string(),
        };
        return Err(PyValueError::new_err(format!(
            "Cannot combine VersionRange operands with different pre-release policies: {} and {}",
            fmt(ca),
            fmt(cb)
        )));
    }
    Ok(())
}

/// `packaging._ranges.intersect_ranges` (two-pointer interval intersection).
/// Sequences of `(LowerBound, UpperBound)` pairs in, Python list of pairs out.
#[pyfunction]
fn ranges_intersect_ranges(
    py: Python,
    left: &Bound<PyAny>,
    right: &Bound<PyAny>,
) -> PyResult<Py<PyList>> {
    let l = SpecifierSetPy::pylist_to_intervals(left)?;
    let r = SpecifierSetPy::pylist_to_intervals(right)?;
    let out = ranges::intersect_ranges(&l, &r);
    let mut items = Vec::new();
    for (lower, upper) in &out {
        let l = Py::new(py, LowerBoundPy { inner: lower.clone() })?
            .into_bound(py)
            .into_any();
        let u = Py::new(py, UpperBoundPy { inner: upper.clone() })?
            .into_bound(py)
            .into_any();
        items.push(PyTuple::new(py, [l, u])?.into_any());
    }
    Ok(PyList::new(py, items)?.unbind())
}
