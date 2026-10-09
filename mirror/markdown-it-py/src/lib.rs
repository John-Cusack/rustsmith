//! PyO3 binding over `markdown-it-rust-core` (`markdown_it._markdown_it`).
//!
//! Layout mirrors the original package: every `markdown_it.*` Python module
//! is a thin shim re-exporting the names defined here, except `tree.py`,
//! `cli/`, `presets/`, and `utils.read_fixture_file`, which stay verbatim
//! Python (navigational/IO glue with no hot path).

//! NOTE: `non_snake_case` is allowed crate-wide because every public name
//! mirrors the upstream Python API verbatim (`renderInline`, `isStrSpace`,
//! `validateLink`, ...); renaming any of them breaks compatibility.
#![allow(non_snake_case)]

mod host;
mod host_convert;
mod markdown;
mod options;
mod parsers;
mod renderer;
mod states;
mod token;

use markdown_it_rust_core::rules_block::fence::FenceConfig;
use pyo3::prelude::*;

use markdown::{PyMarkdownIt, PyResetGuard};
use options::PyOptionsDict;
use parsers::{PyBuiltinRule, PyParserBlock, PyParserCore, PyParserInline, PyRuler};
use renderer::PyRendererHTML;
use states::{PyStateBlock, PyStateCore, PyStateInline};
use token::PyToken;

#[allow(non_snake_case)]
#[pymodule]
fn _markdown_it(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyToken>()?;
    m.add_class::<PyStateBlock>()?;
    m.add_class::<PyStateInline>()?;
    m.add_class::<PyStateCore>()?;
    m.add_class::<PyRuler>()?;
    m.add_class::<PyBuiltinRule>()?;
    m.add_class::<PyParserBlock>()?;
    m.add_class::<PyParserInline>()?;
    m.add_class::<PyParserCore>()?;
    m.add_class::<PyRendererHTML>()?;
    m.add_class::<PyOptionsDict>()?;
    m.add_class::<PyMarkdownIt>()?;
    m.add_class::<PyFenceRule>()?;
    m.add_class::<PyResetGuard>()?;
    m.add_function(wrap_pyfunction!(make_fence_rule, m)?)?;
    m.add_function(wrap_pyfunction!(normalizeLink, m)?)?;
    m.add_function(wrap_pyfunction!(normalizeLinkText, m)?)?;
    m.add_function(wrap_pyfunction!(validateLink, m)?)?;
    m.add_function(wrap_pyfunction!(parseLinkDestination, m)?)?;
    m.add_function(wrap_pyfunction!(parseLinkTitle, m)?)?;
    for (name, func) in [
        ("isSpace", wrap_pyfunction!(isSpace, m)?),
        ("isStrSpace", wrap_pyfunction!(isStrSpace, m)?),
        ("isWhiteSpace", wrap_pyfunction!(isWhiteSpace, m)?),
        ("isPunctChar", wrap_pyfunction!(isPunctChar, m)?),
        ("isMdAsciiPunct", wrap_pyfunction!(isMdAsciiPunct, m)?),
        ("mdTrim", wrap_pyfunction!(mdTrim, m)?),
        ("normalizeReference", wrap_pyfunction!(normalizeReference, m)?),
        ("isLinkOpen", wrap_pyfunction!(isLinkOpen, m)?),
        ("isLinkClose", wrap_pyfunction!(isLinkClose, m)?),
        ("escapeHtml", wrap_pyfunction!(escapeHtml, m)?),
        ("escapeRE", wrap_pyfunction!(escapeRE, m)?),
        ("unescapeAll", wrap_pyfunction!(unescapeAll, m)?),
        ("stripEscape", wrap_pyfunction!(stripEscape, m)?),
        ("isValidEntityCode", wrap_pyfunction!(isValidEntityCode, m)?),
        ("fromCodePoint", wrap_pyfunction!(fromCodePoint, m)?),
        ("replaceEntityPattern", wrap_pyfunction!(replaceEntityPattern, m)?),
        ("to_ascii", wrap_pyfunction!(to_ascii, m)?),
        ("to_unicode", wrap_pyfunction!(to_unicode, m)?),
    ] {
        m.add(name, func)?;
    }
    // Rule-namespace singletons (default builtins as callables).
    for (chain, names) in [
        ("block", vec!["table", "code", "fence", "blockquote", "hr", "list", "reference", "html_block", "heading", "lheading", "paragraph"]),
        ("inline", vec!["text", "linkify", "newline", "escape", "backticks", "strikethrough", "emphasis", "link", "image", "autolink", "html_inline", "entity"]),
        ("inline2", vec!["balance_pairs", "strikethrough", "emphasis", "fragments_join"]),
        ("core", vec!["normalize", "block", "inline", "linkify", "replacements", "smartquotes", "text_join"]),
    ] {
        for name in names {
            m.add(format!("{chain}_{name}").as_str(), PyBuiltinRule::bare(chain, name))?;
        }
    }
    Ok(())
}

/// `make_fence_rule` factory verbatim (`markdown_it/rules_block/fence.py`):
/// markers/token type/exact-match/info restrictions/min length.
#[pyclass(name = "FenceRule", unsendable)]
struct PyFenceRule {
    config: FenceConfig,
}

#[pymethods]
impl PyFenceRule {
    #[pyo3(signature = (state, start, end, silent))]
    fn __call__(
        &self,
        state: &Bound<PyStateBlock>,
        start: usize,
        end: usize,
        silent: bool,
    ) -> PyResult<bool> {
        Ok(markdown_it_rust_core::rules_block::fence::fence_with(
            &state.borrow().shared(),
            start,
            end,
            silent,
            &self.config,
        ))
    }
}

#[pyfunction]
#[pyo3(signature = (*, markers=None, token_type="fence", exact_match=false, disallow_marker_in_info=None, min_markers=3))]
fn make_fence_rule(
    markers: Option<Vec<String>>,
    token_type: &str,
    exact_match: bool,
    disallow_marker_in_info: Option<Vec<String>>,
    min_markers: usize,
) -> PyFenceRule {
    PyFenceRule {
        config: FenceConfig {
            markers: markers
                .unwrap_or_else(|| vec!["~".to_string(), "`".to_string()])
                .into_iter()
                .filter_map(|s| {
                    let mut chars = s.chars();
                    let c = chars.next()?;
                    if chars.next().is_some() {
                        None
                    } else {
                        Some(c)
                    }
                })
                .collect(),
            token_type: token_type.to_string(),
            exact_match,
            disallow_marker_in_info: disallow_marker_in_info
                .unwrap_or_else(|| vec!["`".to_string()])
                .into_iter()
                .filter_map(|s| {
                    let mut chars = s.chars();
                    let c = chars.next()?;
                    if chars.next().is_some() {
                        None
                    } else {
                        Some(c)
                    }
                })
                .collect(),
            min_markers,
        },
    }
}

/// `normalizeLink` as a module function (mirror of
/// `markdown_it/common/normalize_url.py`).
#[pyfunction]
fn normalizeLink(py: Python, url: &str) -> PyResult<String> {
    let mdurl = py.import("mdurl")?;
    Ok(crate::host_convert::normalize_link_py(py, &mdurl.unbind(), url, false))
}

/// `normalizeLinkText` as a module function.
#[pyfunction]
fn normalizeLinkText(py: Python, url: &str) -> PyResult<String> {
    let mdurl = py.import("mdurl")?;
    Ok(crate::host_convert::normalize_link_py(py, &mdurl.unbind(), url, true))
}

/// `validateLink` as a module function.
#[pyfunction]
#[pyo3(signature = (url, validator=None))]
fn validateLink(url: &str, validator: Option<pyo3::Bound<pyo3::PyAny>>) -> PyResult<bool> {
    if let Some(v) = validator {
        if !v.is_none() {
            return v
                .call1((url,))
                .map(|r| r.extract::<bool>().unwrap_or(false));
        }
    }
    Ok(markdown_it_rust_core::common_utils::validate_link_default(url))
}

/// `parseLinkDestination` as a module function (returns `(ok, pos, str)`).
#[pyfunction]
fn parseLinkDestination(s: &str, pos: usize, maximum: usize) -> (bool, usize, String) {
    let chars: Vec<char> = s.chars().collect();
    let r = markdown_it_rust_core::helpers::parse_link_destination(&chars, pos, maximum);
    (r.ok, r.pos, r.text)
}

/// `parseLinkTitle` as a module function (returns `(ok, can_continue, pos, str)`).
#[pyfunction]
fn parseLinkTitle(s: &str, pos: usize, maximum: usize) -> (bool, bool, usize, String) {
    let chars: Vec<char> = s.chars().collect();
    let r = markdown_it_rust_core::helpers::parse_link_title(&chars, pos, maximum, None);
    (r.ok, r.can_continue, r.pos, r.text)
}

/// Pure text utilities (mirror of `markdown_it/common/utils.py`), exposed
/// for `markdown_it.common.utils` imports.
#[pyfunction]
#[pyo3(signature = (code=None))]
fn isSpace(code: Option<u32>) -> bool {
    markdown_it_rust_core::common_utils::is_space(code)
}

#[pyfunction]
#[pyo3(signature = (ch=None))]
fn isStrSpace(ch: Option<char>) -> bool {
    markdown_it_rust_core::common_utils::is_str_space(ch)
}

#[pyfunction]
fn isWhiteSpace(code: u32) -> bool {
    markdown_it_rust_core::common_utils::is_white_space(code)
}

#[pyfunction]
fn isPunctChar(ch: char) -> bool {
    markdown_it_rust_core::common_utils::is_punct_char(ch)
}

#[pyfunction]
fn isMdAsciiPunct(ch: u32) -> bool {
    markdown_it_rust_core::common_utils::is_md_ascii_punct(ch)
}

#[pyfunction]
fn mdTrim(s: &str) -> String {
    markdown_it_rust_core::common_utils::md_trim(s)
}

#[pyfunction]
fn normalizeReference(s: &str) -> String {
    markdown_it_rust_core::common_utils::normalize_reference(s)
}

#[pyfunction]
fn isLinkOpen(s: &str) -> bool {
    markdown_it_rust_core::common_utils::is_link_open(s)
}

#[pyfunction]
fn isLinkClose(s: &str) -> bool {
    markdown_it_rust_core::common_utils::is_link_close(s)
}

#[pyfunction]
fn escapeHtml(s: &str) -> String {
    markdown_it_rust_core::common_utils::escape_html(s)
}

#[pyfunction]
fn escapeRE(s: &str) -> String {
    markdown_it_rust_core::common_utils::escape_re(s)
}

#[pyfunction]
fn unescapeAll(s: &str) -> String {
    markdown_it_rust_core::common_utils::unescape_all(s)
}

#[pyfunction]
fn stripEscape(s: &str) -> String {
    markdown_it_rust_core::common_utils::strip_escape(s)
}

#[pyfunction]
fn isValidEntityCode(c: u32) -> bool {
    markdown_it_rust_core::common_utils::is_valid_entity_code(c)
}

#[pyfunction]
fn fromCodePoint(c: u32) -> PyResult<String> {
    markdown_it_rust_core::common_utils::from_code_point(c)
        .map(|ch| ch.to_string())
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid code point"))
}

#[pyfunction]
fn replaceEntityPattern(matched: &str, name: &str) -> String {
    markdown_it_rust_core::common_utils::replace_entity_pattern(matched, name)
}

/// `to_ascii` for IDNA hostnames (mirror of `markdown_it/_punycode.py`).
#[pyfunction]
fn to_ascii(s: &str) -> String {
    markdown_it_rust_core::punycode::to_ascii(s)
}

/// `to_unicode` for IDNA hostnames (decode errors raise `UnicodeError`).
#[pyfunction]
fn to_unicode(s: &str) -> PyResult<String> {
    markdown_it_rust_core::punycode::to_unicode(s)
        .map_err(|e| pyo3::exceptions::PyUnicodeError::new_err(e.0))
}
