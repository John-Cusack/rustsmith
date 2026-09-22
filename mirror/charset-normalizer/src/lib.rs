//! Stage-1 Rust mirror of `Ousret/charset_normalizer`.
//!
//! Behavior-identical port per `PORTING.md`. Language-model tables are frozen
//! data (tables_*, byte-identical, never re-tuned). Logging is observable
//! behavior (test_logging.py): every logger call is reproduced via the
//! `charset_normalizer` logger with identical level + message.
//!
//! License: MIT (preserved from the original).

use pyo3::exceptions::{PyFileNotFoundError, PyOSError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

pub(crate) mod tables_constant;
pub(crate) mod tables_ucd;
pub(crate) mod tables_codecs;
pub(crate) mod decoders;
pub(crate) mod utils;
pub(crate) mod models;
pub(crate) mod md;
pub(crate) mod cd;
pub(crate) mod api;
pub(crate) mod legacy;

use api::{DetectOptions, Log};

// ---------------------------------------------------------------------------
// Python logging sink (`Log` impl over the `charset_normalizer` logger).
// ---------------------------------------------------------------------------
struct PyLogger<'py> {
    logger: Bound<'py, PyAny>,
}

impl<'py> PyLogger<'py> {
    fn get(py: Python<'py>) -> PyResult<Self> {
        let logging = py.import("logging")?;
        let logger = logging
            .call_method1("getLogger", ("charset_normalizer",))?;
        Ok(Self { logger })
    }
}

impl Log for PyLogger<'_> {
    fn trace(&self, msg: String) {
        // TRACE == 5 (`constant.py:999`); `caplog` renders it as "Level 5".
        let _ = self.logger.call_method1("log", (5u8, msg));
    }
    fn debug(&self, msg: String) {
        let _ = self.logger.call_method1("debug", (msg,));
    }
}

// ---------------------------------------------------------------------------
// `from_bytes` and friends.
// ---------------------------------------------------------------------------
fn parse_cp_list(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Vec<String>>> {
    match obj {
        None => Ok(None),
        Some(o) if o.is_none() => Ok(None),
        Some(o) => {
            let mut out = Vec::new();
            for item in o.try_iter()? {
                let s: String = item?.extract()?;
                out.push(s);
            }
            Ok(Some(out))
        }
    }
}

fn detect_options(
    steps: usize,
    chunk_size: usize,
    threshold: f64,
    cp_isolation: Option<&Bound<'_, PyAny>>,
    cp_exclusion: Option<&Bound<'_, PyAny>>,
    preemptive_behaviour: bool,
    language_threshold: f64,
    enable_fallback: bool,
) -> PyResult<DetectOptions> {
    Ok(DetectOptions {
        steps,
        chunk_size,
        threshold,
        cp_isolation: parse_cp_list(cp_isolation)?,
        cp_exclusion: parse_cp_list(cp_exclusion)?,
        preemptive: preemptive_behaviour,
        language_threshold,
        enable_fallback,
    })
}

fn run_detection(
    py: Python<'_>,
    data: &[u8],
    opts: &DetectOptions,
    explain: bool,
) -> PyResult<models::CharsetMatches> {
    let logger = PyLogger::get(py)?;
    if explain {
        // Mirror api.py:87-90 + per-return cleanup (net effect): attach the
        // shared handler, drop to TRACE, restore afterwards.
        let handler = py
            .import("charset_normalizer")?
            .getattr("api")?
            .getattr("explain_handler")?;
        let prev_level: i32 = logger.logger.getattr("level")?.extract()?;
        logger.logger.call_method1("addHandler", (handler.clone(),))?;
        logger.logger.call_method1("setLevel", (5u8,))?;
        let out = api::from_bytes(data, opts, &logger);
        logger.logger.call_method1("removeHandler", (handler,))?;
        logger.logger.call_method1("setLevel", (prev_level,))?;
        Ok(out)
    } else {
        Ok(api::from_bytes(data, opts, &logger))
    }
}

#[pyfunction]
#[pyo3(signature = (data, steps = 5, chunk_size = 512, threshold = 0.2, cp_isolation = None, cp_exclusion = None, preemptive_behaviour = true, explain = false, language_threshold = 0.1, enable_fallback = true))]
#[allow(clippy::too_many_arguments)]
fn from_bytes(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    steps: usize,
    chunk_size: usize,
    threshold: f64,
    cp_isolation: Option<&Bound<'_, PyAny>>,
    cp_exclusion: Option<&Bound<'_, PyAny>>,
    preemptive_behaviour: bool,
    explain: bool,
    language_threshold: f64,
    enable_fallback: bool,
) -> PyResult<PyMatches> {
    // TypeError on non-bytes, mirroring api.py:80-85.
    let bytes: Vec<u8> = data.extract().map_err(|_| {
        let tname = data
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_default();
        PyTypeError::new_err(format!(
            "Expected object of type bytes or bytearray, got: {}",
            tname
        ))
    })?;
    let opts = detect_options(
        steps, chunk_size, threshold, cp_isolation, cp_exclusion,
        preemptive_behaviour, language_threshold, enable_fallback,
    )?;
    let out = run_detection(py, &bytes, &opts, explain)?;
    Ok(PyMatches { inner: out })
}

#[pyfunction]
#[pyo3(signature = (fp, steps = 5, chunk_size = 512, threshold = 0.2, cp_isolation = None, cp_exclusion = None, preemptive_behaviour = true, explain = false, language_threshold = 0.1, enable_fallback = true))]
#[allow(clippy::too_many_arguments)]
fn from_fp(
    py: Python<'_>,
    fp: &Bound<'_, PyAny>,
    steps: usize,
    chunk_size: usize,
    threshold: f64,
    cp_isolation: Option<&Bound<'_, PyAny>>,
    cp_exclusion: Option<&Bound<'_, PyAny>>,
    preemptive_behaviour: bool,
    explain: bool,
    language_threshold: f64,
    enable_fallback: bool,
) -> PyResult<PyMatches> {
    // Never closes fp (api.py: from_fp contract).
    let data: Vec<u8> = fp.call_method0("read")?.extract()?;
    let opts = detect_options(
        steps, chunk_size, threshold, cp_isolation, cp_exclusion,
        preemptive_behaviour, language_threshold, enable_fallback,
    )?;
    let out = run_detection(py, &data, &opts, explain)?;
    Ok(PyMatches { inner: out })
}

#[pyfunction]
#[pyo3(signature = (path, steps = 5, chunk_size = 512, threshold = 0.2, cp_isolation = None, cp_exclusion = None, preemptive_behaviour = true, explain = false, language_threshold = 0.1, enable_fallback = true))]
#[allow(clippy::too_many_arguments)]
fn from_path(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    steps: usize,
    chunk_size: usize,
    threshold: f64,
    cp_isolation: Option<&Bound<'_, PyAny>>,
    cp_exclusion: Option<&Bound<'_, PyAny>>,
    preemptive_behaviour: bool,
    explain: bool,
    language_threshold: f64,
    enable_fallback: bool,
) -> PyResult<PyMatches> {
    // str | bytes | PathLike, opened rb (api.py: from_path).
    let p: std::path::PathBuf = if let Ok(s) = path.extract::<String>() {
        s.into()
    } else if let Ok(b) = path.extract::<Vec<u8>>() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            std::ffi::OsString::from_vec(b).into()
        }
        #[cfg(not(unix))]
        {
            String::from_utf8_lossy(&b).into_owned().into()
        }
    } else if let Ok(s) = path.call_method0("__fspath__")?.extract::<String>() {
        s.into()
    } else {
        return Err(PyTypeError::new_err("expected str, bytes or path-like object"));
    };
    let data = std::fs::read(&p).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            PyFileNotFoundError::new_err(e.to_string())
        } else {
            PyOSError::new_err(e.to_string())
        }
    })?;
    let opts = detect_options(
        steps, chunk_size, threshold, cp_isolation, cp_exclusion,
        preemptive_behaviour, language_threshold, enable_fallback,
    )?;
    let out = run_detection(py, &data, &opts, explain)?;
    Ok(PyMatches { inner: out })
}

#[pyfunction]
#[pyo3(signature = (obj, steps = 5, chunk_size = 512, threshold = 0.2, cp_isolation = None, cp_exclusion = None, preemptive_behaviour = true, explain = false, language_threshold = 0.1, enable_fallback = false))]
#[allow(clippy::too_many_arguments)]
fn is_binary(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
    steps: usize,
    chunk_size: usize,
    threshold: f64,
    cp_isolation: Option<&Bound<'_, PyAny>>,
    cp_exclusion: Option<&Bound<'_, PyAny>>,
    preemptive_behaviour: bool,
    explain: bool,
    language_threshold: f64,
    enable_fallback: bool,
) -> PyResult<bool> {
    // Dispatch mirrors api.py: is_binary (str/PathLike -> path, bytes -> bytes,
    // else file-like). `not guesses`.
    let opts = detect_options(
        steps, chunk_size, threshold, cp_isolation, cp_exclusion,
        preemptive_behaviour, language_threshold, enable_fallback,
    )?;
    let logger = PyLogger::get(py)?;
    let out = if obj.extract::<String>().is_ok() || obj.hasattr("__fspath__")? {
        let p: std::path::PathBuf = if let Ok(s) = obj.extract::<String>() {
            s.into()
        } else {
            obj.call_method0("__fspath__")?.extract::<String>()?.into()
        };
        let data = std::fs::read(&p).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PyFileNotFoundError::new_err(e.to_string())
            } else {
                PyOSError::new_err(e.to_string())
            }
        })?;
        api::from_bytes(&data, &opts, &logger)
    } else if let Ok(data) = obj.extract::<Vec<u8>>() {
        api::from_bytes(&data, &opts, &logger)
    } else {
        let data: Vec<u8> = obj.call_method0("read")?.extract()?;
        api::from_bytes(&data, &opts, &logger)
    };
    Ok(out.is_empty())
}

// ---------------------------------------------------------------------------
// `CharsetMatch` / `CharsetMatches` Python classes.
// ---------------------------------------------------------------------------
#[pyclass(name = "CharsetMatch")]
#[derive(Clone)]
struct PyMatch {
    inner: models::CharsetMatch,
}

#[pymethods]
impl PyMatch {
    #[new]
    #[pyo3(signature = (payload, guessed_encoding, mean_mess_ratio, has_sig_or_bom, languages, decoded_payload = None, preemptive_declaration = None))]
    fn new(
        payload: Vec<u8>,
        guessed_encoding: String,
        mean_mess_ratio: f64,
        has_sig_or_bom: bool,
        languages: Vec<(String, f64)>,
        decoded_payload: Option<String>,
        preemptive_declaration: Option<String>,
    ) -> Self {
        Self {
            inner: models::CharsetMatch::new(
                payload,
                guessed_encoding,
                mean_mess_ratio,
                has_sig_or_bom,
                languages,
                decoded_payload,
                preemptive_declaration,
            ),
        }
    }

    #[getter]
    fn encoding(&self) -> &str {
        self.inner.encoding()
    }
    #[getter]
    fn encoding_aliases(&self) -> Vec<String> {
        self.inner.encoding_aliases()
    }
    #[getter]
    fn bom(&self) -> bool {
        self.inner.bom()
    }
    #[getter]
    fn byte_order_mark(&self) -> bool {
        self.inner.byte_order_mark()
    }
    #[getter]
    fn languages(&self) -> Vec<(String, f64)> {
        self.inner.languages.clone()
    }
    #[getter]
    fn language(&self) -> String {
        self.inner.language()
    }
    #[getter]
    fn chaos(&self) -> f64 {
        self.inner.chaos()
    }
    #[getter]
    fn coherence(&self) -> f64 {
        self.inner.coherence()
    }
    #[getter]
    fn percent_chaos(&self) -> f64 {
        self.inner.percent_chaos()
    }
    #[pyo3(signature = (encoding = None))]
    fn output(&mut self, encoding: Option<String>) -> PyResult<Vec<u8>> {
        let encoding = encoding.unwrap_or_else(|| "utf_8".to_string());
        self.inner
            .output(&encoding)
            .map_err(|e| PyValueError::new_err(format!("{:?}", e)))
    }
    fn __str__(&mut self) -> PyResult<String> {
        self.inner
            .decoded_str()
            .map(|s| s.to_string())
            .map_err(|e| PyValueError::new_err(format!("{:?}", e)))
    }
    #[getter]
    fn _string(&self) -> Option<String> {
        self.inner.decoded.clone()
    }
    #[getter]
    fn raw(&self) -> &[u8] {
        self.inner.raw()
    }
    #[getter]
    fn has_submatch(&self) -> bool {
        self.inner.has_submatch()
    }
    #[getter]
    fn alphabets(&mut self) -> Vec<String> {
        self.inner.alphabets().to_vec()
    }
    #[getter]
    fn could_be_from_charset(&self) -> Vec<String> {
        self.inner.could_be_from_charset()
    }
    #[getter]
    fn fingerprint(&self) -> u64 {
        self.inner.fingerprint()
    }
    #[getter]
    fn multi_byte_usage(&self) -> f64 {
        self.inner.multi_byte_usage()
    }
    fn __repr__(&self) -> String {
        format!("<CharsetMatch '{}' {}>", self.inner.encoding(), self.inner.fingerprint())
    }
    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: pyo3::basic::CompareOp) -> PyResult<bool> {
        // models.py __eq__/__lt__: Match-vs-Match and Match-vs-str.
        if let Ok(o) = other.extract::<PyRef<'_, PyMatch>>() {
            let ord = self.inner.partial_cmp(&o.inner);
            return Ok(match op {
                pyo3::basic::CompareOp::Eq => self.inner == o.inner,
                pyo3::basic::CompareOp::Ne => self.inner != o.inner,
                pyo3::basic::CompareOp::Lt => ord == Some(std::cmp::Ordering::Less),
                pyo3::basic::CompareOp::Le => matches!(ord, Some(std::cmp::Ordering::Less) | Some(std::cmp::Ordering::Equal)),
                pyo3::basic::CompareOp::Gt => ord == Some(std::cmp::Ordering::Greater),
                pyo3::basic::CompareOp::Ge => matches!(ord, Some(std::cmp::Ordering::Greater) | Some(std::cmp::Ordering::Equal)),
            });
        }
        if let Ok(s) = other.extract::<String>() {
            return Ok(match op {
                pyo3::basic::CompareOp::Eq => self.inner.eq_str(&s),
                pyo3::basic::CompareOp::Ne => !self.inner.eq_str(&s),
                _ => {
                    return Err(PyValueError::new_err(
                        "'<' not supported between instances of 'CharsetMatch' and 'str'",
                    ))
                }
            });
        }
        // models.py `__eq__` is total (other types compare unequal, never
        // raise); `__lt__` raises `ValueError` on non-matches.
        Ok(match op {
            pyo3::basic::CompareOp::Eq => false,
            pyo3::basic::CompareOp::Ne => true,
            _ => {
                return Err(PyValueError::new_err(
                    "comparison not supported between instances of 'CharsetMatch' and non-match",
                ))
            }
        })
    }
}

#[pyclass(name = "CharsetMatches")]
#[derive(Clone, Default)]
struct PyMatches {
    inner: models::CharsetMatches,
}

#[pymethods]
impl PyMatches {
    #[new]
    #[pyo3(signature = (results = None))]
    fn new(results: Option<Vec<PyRef<'_, PyMatch>>>) -> Self {
        let v = results.map(|r| r.iter().map(|m| m.inner.clone()).collect());
        Self {
            inner: models::CharsetMatches::new(v),
        }
    }
    fn best(&self) -> Option<PyMatch> {
        self.inner.best().map(|m| PyMatch { inner: m.clone() })
    }
    fn first(&self) -> Option<PyMatch> {
        self.inner.first().map(|m| PyMatch { inner: m.clone() })
    }
    fn append(&mut self, item: PyRef<'_, PyMatch>) -> PyResult<()> {
        self.inner
            .append(item.inner.clone())
            .map_err(|e| PyValueError::new_err(format!("{:?}", e)))
    }
    fn __len__(&self) -> usize {
        self.inner.len()
    }
    fn __bool__(&self) -> bool {
        !self.inner.is_empty()
    }
    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        if let Ok(i) = key.extract::<isize>() {
            let len = self.inner.len() as isize;
            let idx = if i < 0 { len + i } else { i };
            if idx < 0 || idx >= len {
                return Err(pyo3::exceptions::PyIndexError::new_err("list index out of range"));
            }
            let m = self.inner.get(idx as usize).unwrap().clone();
            return Ok(Py::new(py, PyMatch { inner: m })?.into_any());
        }
        if let Ok(s) = key.extract::<String>() {
            if let Some(m) = self.inner.get_by_encoding(&s) {
                return Ok(Py::new(py, PyMatch { inner: m.clone() })?.into_any());
            }
            return Err(pyo3::exceptions::PyKeyError::new_err(s));
        }
        Err(PyTypeError::new_err("indices must be integers or strings"))
    }
    fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let items: Vec<Py<PyMatch>> = slf
            .inner
            .iter()
            .map(|m| Py::new(py, PyMatch { inner: m.clone() }).unwrap())
            .collect();
        Ok(PyList::new(py, items)?.into_any().try_iter()?.into_any().unbind())
    }
}

// ---------------------------------------------------------------------------
// `CharInfo` class (`md._char_info` surface).
// ---------------------------------------------------------------------------
#[pyclass(name = "CharInfo")]
#[derive(Clone)]
struct PyCharInfo {
    inner: md::CharInfo,
}

#[pymethods]
impl PyCharInfo {
    #[getter]
    fn character(&self) -> char {
        self.inner.character
    }
    #[getter]
    fn printable(&self) -> bool {
        self.inner.printable
    }
    #[getter]
    fn alpha(&self) -> bool {
        self.inner.alpha
    }
    #[getter]
    fn upper(&self) -> bool {
        self.inner.upper
    }
    #[getter]
    fn lower(&self) -> bool {
        self.inner.lower
    }
    #[getter]
    fn space(&self) -> bool {
        self.inner.space
    }
    #[getter]
    fn digit(&self) -> bool {
        self.inner.digit
    }
    #[getter]
    fn is_ascii(&self) -> bool {
        self.inner.is_ascii
    }
    #[getter]
    fn case_variable(&self) -> bool {
        self.inner.case_variable
    }
    #[getter]
    fn flags(&self) -> u32 {
        self.inner.flags
    }
    #[getter]
    fn accentuated(&self) -> bool {
        self.inner.accentuated
    }
    #[getter]
    fn latin(&self) -> bool {
        self.inner.latin
    }
    #[getter]
    fn is_cjk(&self) -> bool {
        self.inner.is_cjk
    }
    #[getter]
    fn is_katakana(&self) -> bool {
        self.inner.is_katakana
    }
    #[getter]
    fn is_halfwidth_katakana(&self) -> bool {
        self.inner.is_halfwidth_katakana
    }
    #[getter]
    fn is_arabic(&self) -> bool {
        self.inner.is_arabic
    }
    #[getter]
    fn is_ligature(&self) -> bool {
        self.inner.is_ligature
    }
    #[getter]
    fn is_superscript(&self) -> bool {
        self.inner.is_superscript
    }
    #[getter]
    fn is_sentence_open_punctuation(&self) -> bool {
        self.inner.is_sentence_open_punctuation
    }
    #[getter]
    fn is_glyph(&self) -> bool {
        self.inner.is_glyph
    }
    #[getter]
    fn punct(&self) -> bool {
        self.inner.punct
    }
    #[getter]
    fn sym(&self) -> bool {
        self.inner.sym
    }
    #[getter]
    fn range(&self) -> Option<&str> {
        self.inner.range
    }
    #[getter]
    fn sep(&self) -> bool {
        self.inner.sep
    }
    #[getter]
    fn emoticon(&self) -> bool {
        self.inner.emoticon
    }
    #[getter]
    fn safe(&self) -> bool {
        self.inner.safe
    }
    #[getter]
    fn common_cjk(&self) -> bool {
        self.inner.common_cjk
    }
    #[getter]
    fn unaccented(&self) -> char {
        self.inner.unaccented
    }
}

// ---------------------------------------------------------------------------
// Scalar functions.
// ---------------------------------------------------------------------------
#[pyfunction]
#[pyo3(signature = (decoded_sequence, maximum_threshold = 0.2, debug = false))]
fn mess_ratio(decoded_sequence: &str, maximum_threshold: f64, debug: bool) -> f64 {
    let _ = debug;
    md::mess_ratio(decoded_sequence, maximum_threshold)
}
#[pyfunction]
fn char_info(s: &str) -> PyResult<PyCharInfo> {
    let c = s
        .chars()
        .next()
        .ok_or_else(|| PyValueError::new_err("expected a single character"))?;
    Ok(PyCharInfo {
        inner: md::char_info(c),
    })
}

#[pyfunction]
fn is_suspiciously_successive_range(a: Option<String>, b: Option<String>) -> bool {
    md::is_suspiciously_successive_range(a.as_deref(), b.as_deref())
}

#[pyfunction]
#[pyo3(signature = (decoded_sequence, threshold = 0.1, lg_inclusion = None))]
fn coherence_ratio(
    decoded_sequence: &str,
    threshold: f64,
    lg_inclusion: Option<String>,
) -> Vec<(String, f64)> {
    cd::coherence_ratio(decoded_sequence, threshold, lg_inclusion.as_deref())
}

#[pyfunction]
fn encoding_languages(name: &str) -> Vec<String> {
    cd::encoding_languages(name)
}

#[pyfunction]
fn characters_popularity_compare(language: &str, ordered: Vec<String>) -> f64 {
    let chars: Vec<char> = ordered
        .iter()
        .flat_map(|s| s.chars().next())
        .collect();
    cd::characters_popularity_compare(language, &chars)
}

#[pyfunction]
fn filter_alt_coherence_matches(results: Vec<(String, f64)>) -> Vec<(String, f64)> {
    cd::filter_alt_coherence_matches(&results)
}

#[pyfunction]
fn get_target_features(language: &str) -> (bool, bool) {
    cd::get_target_features(language)
}

#[pyfunction]
fn cp_similarity(a: &str, b: &str) -> f64 {
    utils::cp_similarity(a, b)
}

#[pyfunction]
fn is_accentuated(s: &str) -> PyResult<bool> {
    let c = s
        .chars()
        .next()
        .ok_or_else(|| PyValueError::new_err("expected a single character"))?;
    Ok(utils::is_accentuated(c))
}

#[pyfunction]
fn mb_encoding_languages(name: &str) -> Vec<String> {
    cd::mb_encoding_languages(name)
}

#[pyfunction]
#[pyo3(signature = (cp_name, strict = true))]
fn iana_name(cp_name: &str, strict: bool) -> PyResult<String> {
    utils::iana_name(cp_name, strict).map_err(PyValueError::new_err)
}

#[pyfunction]
fn is_multi_byte_encoding(name: &str) -> bool {
    utils::is_multi_byte_encoding(name)
}

#[pyfunction]
fn unicode_range(s: &str) -> PyResult<Option<String>> {
    let c = s.chars().next().ok_or_else(|| {
        PyValueError::new_err("expected a single character")
    })?;
    Ok(utils::unicode_range(c).map(|r| r.to_string()))
}

#[pyfunction]
fn any_specified_encoding(data: Vec<u8>) -> Option<String> {
    utils::any_specified_encoding(&data, 8192).map(|s| s.to_string())
}

#[pyfunction]
#[pyo3(signature = (name = "charset_normalizer", level = 20, format_string = "%(asctime)s | %(levelname)s | %(message)s"))]
fn set_logging_handler(name: &str, level: i32, format_string: &str) -> PyResult<()> {
    // utils.py:345-355, executed for real (test_logging.py).
    Python::with_gil(|py| {
        let logging = py.import("logging")?;
        let logger = logging.call_method1("getLogger", (name,))?;
        logger.call_method1("setLevel", (level,))?;
        let handler_cls = logging.getattr("StreamHandler")?;
        let handler = handler_cls.call0()?;
        let fmt_cls = logging.getattr("Formatter")?;
        let fmt = fmt_cls.call1((format_string,))?;
        handler.call_method1("setFormatter", (fmt,))?;
        logger.call_method1("addHandler", (handler,))?;
        Ok(())
    })
}

#[pyfunction]
#[pyo3(signature = (data, should_rename_legacy = false))]
fn detect(py: Python<'_>, data: Vec<u8>, should_rename_legacy: bool) -> PyResult<Py<PyDict>> {
    let r = legacy::detect(&data, should_rename_legacy);
    let d = PyDict::new(py);
    match r.encoding {
        Some(e) => {
            d.set_item("encoding", e)?;
        }
        None => {
            d.set_item("encoding", py.None())?;
        }
    }
    d.set_item("language", r.language)?;
    match r.confidence {
        Some(c) => {
            d.set_item("confidence", c)?;
        }
        None => {
            d.set_item("confidence", py.None())?;
        }
    }
    Ok(d.into())
}

#[pyfunction]
#[pyo3(signature = (question, default = "yes"))]
fn query_yes_no(py: Python<'_>, question: &str, default: &str) -> PyResult<bool> {
    // cli/__main__.py:16-28, interactive via builtins.input.
    let prompt = if default == "yes" { " [Y/n] " } else { " [y/N] " };
    let builtins = py.import("builtins")?;
    loop {
        let full = format!("{}{}", question, prompt);
        let answer: String = builtins
            .call_method1("input", (full,))?
            .extract::<String>()?
            .trim()
            .to_lowercase();
        if let Some(v) = legacy::parse_yes_no_answer(&answer, default == "yes") {
            return Ok(v);
        }
        println!("Please respond with 'y' or 'n'.");
    }
}

#[pyfunction]
#[pyo3(signature = (argv = None))]
fn cli_detect(py: Python<'_>, argv: Option<Vec<String>>) -> PyResult<i32> {
    let args: Vec<String> = match argv {
        Some(a) => a,
        None => {
            let sys = py.import("sys")?;
            let all: Vec<String> = sys.getattr("argv")?.extract()?;
            all.into_iter().skip(1).collect()
        }
    };
    match legacy::cli_detect(&args) {
        legacy::CliOutcome::Return(code) => Ok(code),
        legacy::CliOutcome::Raise(code) => {
            Err(pyo3::exceptions::PySystemExit::new_err(code))
        }
        legacy::CliOutcome::Version => {
            println!("{}", legacy::default_version_text());
            Err(pyo3::exceptions::PySystemExit::new_err(0))
        }
    }
}

// ---------------------------------------------------------------------------
// Constant values (`constant.py` data surface used by tests).
// ---------------------------------------------------------------------------
#[pyfunction]
fn too_big_sequence() -> usize {
    tables_constant::TOO_BIG_SEQUENCE
}

#[pyfunction]
fn too_small_sequence() -> usize {
    tables_constant::TOO_SMALL_SEQUENCE
}

#[pyfunction]
fn trace_level() -> u32 {
    tables_constant::TRACE_LEVEL
}

#[pyfunction]
fn iana_supported() -> Vec<String> {
    tables_constant::IANA_SUPPORTED
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[pyfunction]
fn re_pattern() -> &'static str {
    tables_constant::RE_ENCODING_INDICATION
}
#[pymodule]
fn _charset_normalizer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMatch>()?;
    m.add_class::<PyMatches>()?;
    m.add_class::<PyCharInfo>()?;
    m.add_function(wrap_pyfunction!(from_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(from_fp, m)?)?;
    m.add_function(wrap_pyfunction!(from_path, m)?)?;
    m.add_function(wrap_pyfunction!(is_binary, m)?)?;
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    m.add_function(wrap_pyfunction!(mess_ratio, m)?)?;
    m.add_function(wrap_pyfunction!(char_info, m)?)?;
    m.add_function(wrap_pyfunction!(is_suspiciously_successive_range, m)?)?;
    m.add_function(wrap_pyfunction!(coherence_ratio, m)?)?;
    m.add_function(wrap_pyfunction!(characters_popularity_compare, m)?)?;
    m.add_function(wrap_pyfunction!(filter_alt_coherence_matches, m)?)?;
    m.add_function(wrap_pyfunction!(get_target_features, m)?)?;
    m.add_function(wrap_pyfunction!(cp_similarity, m)?)?;
    m.add_function(wrap_pyfunction!(is_accentuated, m)?)?;
    m.add_function(wrap_pyfunction!(encoding_languages, m)?)?;
    m.add_function(wrap_pyfunction!(mb_encoding_languages, m)?)?;
    m.add_function(wrap_pyfunction!(iana_name, m)?)?;
    m.add_function(wrap_pyfunction!(is_multi_byte_encoding, m)?)?;
    m.add_function(wrap_pyfunction!(unicode_range, m)?)?;
    m.add_function(wrap_pyfunction!(any_specified_encoding, m)?)?;
    m.add_function(wrap_pyfunction!(set_logging_handler, m)?)?;
    m.add_function(wrap_pyfunction!(query_yes_no, m)?)?;
    m.add_function(wrap_pyfunction!(cli_detect, m)?)?;
    m.add_function(wrap_pyfunction!(too_big_sequence, m)?)?;
    m.add_function(wrap_pyfunction!(too_small_sequence, m)?)?;
    m.add_function(wrap_pyfunction!(trace_level, m)?)?;
    m.add_function(wrap_pyfunction!(iana_supported, m)?)?;
    m.add_function(wrap_pyfunction!(re_pattern, m)?)?;
    // `explain_handler`: a real StreamHandler (api.py:33-36); the shim
    // re-exports this object and `explain=True` attaches/detaches it.
    let py = m.py();
    let logging = py.import("logging")?;
    let handler = logging.getattr("StreamHandler")?.call0()?;
    let fmt = logging
        .getattr("Formatter")?
        .call1(("%(asctime)s | %(levelname)s | %(message)s",))?;
    handler.call_method1("setFormatter", (fmt,))?;
    m.add("explain_handler", handler)?;
    m.add("__version__", "3.5.1")?;
    Ok(())
}
