//! `pygments._pygments`: compiled half of the Stage-1 mirror.
//!
//! Thin PyO3 translation over the reusable [`pygments_rust_core`] engine.
//! All Python-visible behavior (option handling, fallbacks, exceptions,
//! token streams) stays in `pygments/*.py`; this module only translates
//! between Python objects and core string kernels.
//!
//! The whole port holds **zero `unsafe` blocks**: every function takes
//! `&str`/`Vec<String>` by value through PyO3's safe extractors and
//! returns owned values.

use pyo3::prelude::*;
use pyo3::types::PyString;

/// HTML-escape `&`, `<`, `>`, `"`, `'`.
/// Kernel for `formatters/html.py::escape_html` and
/// `formatters/svg.py::escape_html`.
#[pyfunction]
fn escape_html_text(text: &Bound<'_, PyString>) -> PyResult<String> {
    Ok(pygments_rust_core::escape_html_text(
        &text.to_string_lossy(),
    ))
}

/// Collapse `\r\n` → `\n` and lone `\r` → `\n`.
/// Kernel for the newline head of `Lexer._preprocess_lexer_input`.
#[pyfunction]
fn normalize_newlines(text: &Bound<'_, PyString>) -> PyResult<String> {
    Ok(pygments_rust_core::normalize_newlines(
        &text.to_string_lossy(),
    ))
}

/// Expand tabs (when `tabsize > 0`) and ensure a trailing newline.
/// Kernel for the tail of `Lexer._preprocess_lexer_input`.
#[pyfunction]
#[pyo3(signature = (text, tabsize, ensurenl))]
fn expand_tabs_and_ensure(
    text: &Bound<'_, PyString>,
    tabsize: usize,
    ensurenl: bool,
) -> PyResult<String> {
    Ok(pygments_rust_core::expand_tabs_and_ensure(
        &text.to_string_lossy(),
        tabsize,
        ensurenl,
    ))
}

/// Build a regex matching any of the given literal strings.
/// Kernel for `regexopt.regex_opt`.
#[pyfunction]
#[pyo3(signature = (strings, prefix="", suffix=""))]
fn regex_opt(strings: Vec<String>, prefix: &str, suffix: &str) -> PyResult<String> {
    Ok(pygments_rust_core::regex_opt(&strings, prefix, suffix))
}

/// `pygments._pygments`: compiled half of the Stage-1 mirror.
#[pymodule]
fn _pygments(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(escape_html_text, m)?)?;
    m.add_function(wrap_pyfunction!(normalize_newlines, m)?)?;
    m.add_function(wrap_pyfunction!(expand_tabs_and_ensure, m)?)?;
    m.add_function(wrap_pyfunction!(regex_opt, m)?)?;
    Ok(())
}
