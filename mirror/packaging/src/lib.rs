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
            obj.str()?.to_string()
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
            obj.str()?.to_string()
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
        return Ok(hit.into_py_any(py)?);
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
        return Ok((eq == want_eq).into_py_any(py)?);
    }
    if is_base_version(py, other)? {
        let a = slf.borrow().key_tuple(py)?;
        let b: Bound<PyAny> = other.getattr("_key")?;
        let eq = a.as_any().eq(&b)?;
        return Ok((eq == want_eq).into_py_any(py)?);
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
    Ok(hit.into_py_any(py)?)
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
    // Pattern matching (`__match_args__ == ("_str",)`, mirroring the original).
    m.getattr("Version")?
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
    Ok(PyList::new(py, items)?.call_method0("__iter__")?)
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
    Ok(Py::new(py, Tag { interpreter: i, abi: a, platform: p, hash })?)
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
                return Ok(args.get_item(i)?);
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
        Ok((this.hash == o.hash
            && this.platform == o.platform
            && this.abi == o.abi
            && this.interpreter == o.interpreter)
            .into_py_any(py)?)
    }

    fn __ne__(slf: &Bound<Self>, other: &Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let Ok(o) = other.extract::<PyRef<Self>>() else {
            return py.NotImplemented().into_py_any(py);
        };
        let this = slf.borrow();
        Ok((this.hash != o.hash
            || this.platform != o.platform
            || this.abi != o.abi
            || this.interpreter != o.interpreter)
            .into_py_any(py)?)
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
    Ok(list.call_method0("__iter__")?)
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
        if version > (10, 5) || version < (10, 4) {
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
    if (10, 0) <= version && version < (11, 0) {
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
    for major in ((12)..ver.0).rev() {
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
            .last()
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
    let arch_full = linux.splitn(2, '_').nth(1).unwrap_or("").to_string();
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
        Ok(PyList::new(py, things)?.call_method0("__iter__")?)
    }
}

fn ranks_lookup<'a>(
    ranks: &'a [((String, String, String), usize)],
    key: &(String, String, String),
) -> Option<&'a ((String, String, String), usize)> {
    ranks.iter().find(|(k, _)| k == key)
}
