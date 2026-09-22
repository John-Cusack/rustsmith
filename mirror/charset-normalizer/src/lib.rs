//! Stage-1 Rust mirror of `Ousret/charset_normalizer` (pure-Python charset detection).
//!
//! Behavior-identical port per `PORTING.md`: same module boundary
//! (`charset_normalizer._charset_normalizer`), same public names, same
//! detection outcomes. No redesign: brute-force decode-all + mess/coherence
//! scoring ships; Stage-2 candidates live in `candidate_pool`, not here.
//!
//! Language-model tables (FREQUENCIES, UNICODE_RANGES, ENCODING_MARKS) are
//! data: ported byte-identical, never re-tuned.
//!
//! License: MIT (preserved from the original; see NOTICE).

use pyo3::prelude::*;
use pyo3::types::PyList;

// ---------------------------------------------------------------------------
// Pure-Rust core (no Python API): deterministic, miri-testable.
// Detection outcomes must match the original exactly; oracle parity +
// held-out divergence gate this.
// ---------------------------------------------------------------------------

/// Placeholder detector core: workers replace with the full decode-all +
/// mess_ratio/coherence_ratio port. The scaffold compiles and exposes the
/// public surface so `maturin develop` + oracle grading run end to end.
fn detect_stub(data: &[u8]) -> (&'static str, f64, f64) {
    if data.is_empty() {
        return ("utf_8", 0.0, 0.0);
    }
    if data.iter().all(|b| *b < 0x80) {
        return ("ascii", 0.0, 0.0);
    }
    ("utf_8", 0.0, 0.0)
}

#[pyclass]
struct CharsetMatch {
    encoding: String,
    chaos: f64,
    coherence: f64,
}

#[pymethods]
impl CharsetMatch {
    #[getter]
    fn encoding(&self) -> &str {
        &self.encoding
    }
    #[getter]
    fn chaos(&self) -> f64 {
        self.chaos
    }
    #[getter]
    fn coherence(&self) -> f64 {
        self.coherence
    }
}

#[pyfunction]
#[pyo3(signature = (data, steps = 5, chunk_size = 512, threshold = 0.2, cp_isolation = None, cp_exclusion = None, preemptive_behaviour = true, explain = false, language_threshold = 0.1, enable_fallback = true))]
#[allow(clippy::too_many_arguments)]
fn from_bytes(
    py: Python<'_>,
    data: Vec<u8>,
    steps: usize,
    chunk_size: usize,
    threshold: f64,
    cp_isolation: Option<Vec<String>>,
    cp_exclusion: Option<Vec<String>>,
    preemptive_behaviour: bool,
    explain: bool,
    language_threshold: f64,
    enable_fallback: bool,
) -> PyResult<Py<PyList>> {
    let _ = (steps, chunk_size, threshold, cp_isolation, cp_exclusion, preemptive_behaviour, explain, language_threshold, enable_fallback);
    let (enc, chaos, coh) = py.allow_threads(|| detect_stub(&data));
    Python::with_gil(|py| {
        let list = PyList::empty(py);
        let m = Py::new(py, CharsetMatch { encoding: enc.to_string(), chaos, coherence: coh })?;
        list.append(m)?;
        Ok(list.into())
    })
}

#[pyfunction]
#[pyo3(signature = (s, threshold = 0.2))]
fn mess_ratio(s: &str, threshold: f64) -> f64 {
    let _ = (s, threshold);
    0.0
}

#[pyfunction]
fn coherence_ratio(py: Python<'_>, _s: &str) -> PyResult<Py<PyList>> {
    Ok(PyList::empty(py).into())
}

#[pyfunction]
fn from_path(_path: String) -> PyResult<String> {
    Err(pyo3::exceptions::PyNotImplementedError::new_err("port: from_path not yet implemented"))
}

#[pyfunction]
fn from_fp(_fp: Py<PyAny>) -> PyResult<String> {
    Err(pyo3::exceptions::PyNotImplementedError::new_err("port: from_fp not yet implemented"))
}

#[pyfunction]
fn is_binary(_data: Vec<u8>) -> bool {
    false
}

#[pyfunction]
fn detect(_data: Vec<u8>) -> PyResult<String> {
    Err(pyo3::exceptions::PyNotImplementedError::new_err("port: detect not yet implemented"))
}

#[pymodule]
fn _charset_normalizer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<CharsetMatch>()?;
    m.add_function(wrap_pyfunction!(from_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(from_path, m)?)?;
    m.add_function(wrap_pyfunction!(from_fp, m)?)?;
    m.add_function(wrap_pyfunction!(is_binary, m)?)?;
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    m.add_function(wrap_pyfunction!(mess_ratio, m)?)?;
    m.add_function(wrap_pyfunction!(coherence_ratio, m)?)?;
    Ok(())
}
