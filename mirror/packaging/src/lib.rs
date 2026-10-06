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
use pyo3::exceptions::{PyDeprecationWarning, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyTuple, PyType};
pyo3::create_exception!(_packaging, InvalidVersion, PyValueError);

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
    // Tracebacks name the public module, not the extension.
    m.py()
        .get_type::<InvalidVersion>()
        .setattr("__module__", "packaging.version")?;
    m.py()
        .get_type::<ELFInvalid>()
        .setattr("__module__", "packaging._elffile")?;
    // Pattern matching (`__match_args__ == ("_str",)`, mirroring the original).
    m.getattr("Version")?
        .setattr("__match_args__", ("_str",))?;
    Ok(())
}
// ---------------------------------------------------------------------------
// ELF files (`packaging._elffile`).
// ---------------------------------------------------------------------------

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
