//! State pyclasses (mirrors of `StateBlock`, `StateInline`, `StateCore`).
//!
//! Each wraps the shared core handle plus the originating `MarkdownIt`
//! object (`md`, verbatim attribute). `env` is a live view: the getter
//! materializes the core env, the setter merges writes back.

use std::cell::RefCell;
use std::rc::Rc;

use markdown_it_rust_core::{SharedBlockState, SharedCoreState, SharedInlineState};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::host::PyHost;
use crate::host_convert::{env_from_py, env_to_py};
use crate::markdown::PyMarkdownIt;
use crate::token::PyToken;

fn current_md(host: &Rc<PyHost>) -> Option<Py<PyAny>> {
    host.current_md_nogil()
}

// ---------------------------------------------------------------------------
// StateBlock
// ---------------------------------------------------------------------------

#[pyclass(name = "StateBlock", unsendable)]
pub struct PyStateBlock {
    inner: SharedBlockState,
    md: Option<Py<PyAny>>,
}

impl PyStateBlock {
    pub fn wrap(inner: SharedBlockState, host: &Rc<PyHost>) -> Self {
        let md = current_md(host);
        Self { inner, md }
    }

    pub fn shared(&self) -> SharedBlockState {
        self.inner.clone()
    }
}

#[pymethods]
impl PyStateBlock {
    #[new]
    #[pyo3(signature = (src, md, env, tokens))]
    fn new(
        py: Python,
        src: String,
        md: Bound<PyAny>,
        env: Bound<PyDict>,
        tokens: Vec<Bound<PyToken>>,
    ) -> PyResult<Self> {
        let md_obj: Bound<PyMarkdownIt> = md.downcast_into().map_err(|_| {
            PyTypeError::new_err("md must be a MarkdownIt instance")
        })?;
        let (options, host) = {
            let m = md_obj.borrow();
            (m.options_rc(), m.host_rc())
        };
        let mut env_data = Default::default();
        env_from_py(py, &mut env_data, &env);
        let toks = tokens.into_iter().map(|t| t.borrow().shared()).collect();
        let code_enabled = host.block.borrow().ruler.is_active("code");
        let inner = Rc::new(RefCell::new(
            markdown_it_rust_core::rules_block::StateBlock::new(
                &src,
                options,
                host.handle(),
                env_data,
                toks,
                code_enabled,
            ),
        ));
        Ok(Self { inner, md: Some(md_obj.unbind().into_any()) })
    }

    fn __repr__(&self) -> String {
        let st = self.inner.borrow();
        format!(
            "StateBlock(line={},level={},tokens={})",
            st.line,
            st.level,
            st.tokens.len()
        )
    }

    #[getter]
    fn get_src(&self) -> String {
        self.inner.borrow().src.iter().collect()
    }

    #[setter]
    fn set_src(&self, v: String) {
        // Verbatim: only `_src` is replaced; line caches go stale.
        let mut st = self.inner.borrow_mut();
        let chars: Vec<char> = v.chars().collect();
        let mut byte_of = Vec::with_capacity(chars.len() + 1);
        for (byte_idx, _) in v.char_indices() {
            byte_of.push(byte_idx);
        }
        byte_of.push(v.len());
        st.src = chars;
        st.src_text = v;
        st.src_byte_of = byte_of;
    }

    #[getter]
    fn get_srcCharCode(&self, py: Python) -> PyResult<Py<PyAny>> {
        use pyo3::exceptions::PyDeprecationWarning;
        PyErr::warn(
            py,
            &py.get_type::<PyDeprecationWarning>(),
            c"StateBase.srcCharCode is deprecated. Use StateBase.src instead.",
            2,
        )?;
        let codes: Vec<u32> = self.inner.borrow().src.iter().map(|&c| c as u32).collect();
        Ok(codes.into_pyobject(py)?.into_any().unbind())
    }

    #[getter]
    fn get_md(&self, py: Python) -> PyResult<Py<PyAny>> {
        match &self.md {
            Some(m) => Ok(m.clone_ref(py)),
            None => Ok(py.None()),
        }
    }

    #[setter]
    fn set_md(&mut self, v: Py<PyAny>) {
        self.md = Some(v);
    }

    #[getter]
    fn get_env(&self, py: Python) -> Py<PyDict> {
        env_to_py(py, &self.inner.borrow().env)
    }

    #[setter]
    fn set_env(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        env_from_py(py, &mut self.inner.borrow_mut().env, &v);
        Ok(())
    }

    #[getter]
    fn get_tokens(&self, py: Python) -> PyResult<Vec<Py<PyToken>>> {
        Ok(self
            .inner
            .borrow()
            .tokens
            .clone()
            .into_iter()
            .map(|t| Py::new(py, PyToken::wrap(t)).unwrap())
            .collect())
    }

    #[setter]
    fn set_tokens(&self, py: Python, v: Vec<Bound<PyToken>>) -> PyResult<()> {
        let _ = py;
        self.inner.borrow_mut().tokens =
            v.into_iter().map(|t| t.borrow().shared()).collect();
        Ok(())
    }

    #[getter]
    fn get_bMarks(&self) -> Vec<usize> {
        self.inner.borrow().b_marks.clone()
    }

    #[setter]
    fn set_bMarks(&self, v: Vec<usize>) {
        self.inner.borrow_mut().b_marks = v;
    }

    #[getter]
    fn get_eMarks(&self) -> Vec<usize> {
        self.inner.borrow().e_marks.clone()
    }

    #[setter]
    fn set_eMarks(&self, v: Vec<usize>) {
        self.inner.borrow_mut().e_marks = v;
    }

    #[getter]
    fn get_tShift(&self) -> Vec<usize> {
        self.inner.borrow().t_shift.clone()
    }

    #[setter]
    fn set_tShift(&self, v: Vec<usize>) {
        self.inner.borrow_mut().t_shift = v;
    }

    #[getter]
    fn get_sCount(&self) -> Vec<usize> {
        self.inner.borrow().s_count.clone()
    }

    #[setter]
    fn set_sCount(&self, v: Vec<i64>) {
        // Verbatim: plain ints; the `-1` paragraph-continuation marker wraps
        // (two's complement, as in core).
        self.inner.borrow_mut().s_count = v.into_iter().map(|x| x as usize).collect();
    }

    #[getter]
    fn get_bsCount(&self) -> Vec<usize> {
        self.inner.borrow().bs_count.clone()
    }

    #[setter]
    fn set_bsCount(&self, v: Vec<usize>) {
        self.inner.borrow_mut().bs_count = v;
    }

    #[getter]
    fn get_blkIndent(&self) -> usize {
        self.inner.borrow().blk_indent
    }

    #[setter]
    fn set_blkIndent(&self, v: usize) {
        self.inner.borrow_mut().blk_indent = v;
    }

    #[getter]
    fn get_line(&self) -> usize {
        self.inner.borrow().line
    }

    #[setter]
    fn set_line(&self, v: usize) {
        self.inner.borrow_mut().line = v;
    }

    #[getter]
    fn get_lineMax(&self) -> usize {
        self.inner.borrow().line_max
    }

    #[setter]
    fn set_lineMax(&self, v: usize) {
        self.inner.borrow_mut().line_max = v;
    }

    #[getter]
    fn get_tight(&self) -> bool {
        self.inner.borrow().tight
    }

    #[setter]
    fn set_tight(&self, v: bool) {
        self.inner.borrow_mut().tight = v;
    }

    #[getter]
    fn get_ddIndent(&self) -> i64 {
        self.inner.borrow().dd_indent
    }

    #[setter]
    fn set_ddIndent(&self, v: i64) {
        self.inner.borrow_mut().dd_indent = v;
    }

    #[getter]
    fn get_listIndent(&self) -> i64 {
        self.inner.borrow().list_indent
    }

    #[setter]
    fn set_listIndent(&self, v: i64) {
        self.inner.borrow_mut().list_indent = v;
    }

    #[getter]
    fn get_parentType(&self) -> String {
        self.inner.borrow().parent_type.clone()
    }

    #[setter]
    fn set_parentType(&self, v: String) {
        self.inner.borrow_mut().parent_type = v;
    }

    #[getter]
    fn get_level(&self) -> usize {
        self.inner.borrow().level
    }

    #[setter]
    fn set_level(&self, v: usize) {
        self.inner.borrow_mut().level = v;
    }

    #[getter]
    fn get_code_enabled(&self) -> bool {
        self.inner.borrow().code_enabled
    }

    fn push(&self, py: Python, ttype: &str, tag: &str, nesting: i8) -> PyResult<Py<PyToken>> {
        Py::new(py, PyToken::wrap(self.inner.borrow_mut().push(ttype, tag, nesting)))
    }

    fn isEmpty(&self, line: usize) -> bool {
        self.inner.borrow().is_empty(line)
    }

    fn skipEmptyLines(&self, from_pos: usize) -> usize {
        self.inner.borrow().skip_empty_lines(from_pos)
    }

    fn skipSpaces(&self, pos: usize) -> usize {
        self.inner.borrow().skip_spaces(pos)
    }

    fn skipSpacesBack(&self, pos: usize, minimum: usize) -> usize {
        self.inner.borrow().skip_spaces_back(pos, minimum)
    }

    fn skipChars(&self, pos: usize, code: u32) -> usize {
        self.inner.borrow().skip_chars(pos, code)
    }

    fn skipCharsStr(&self, pos: usize, ch: char) -> usize {
        self.inner.borrow().skip_chars_str(pos, ch)
    }

    fn skipCharsBack(&self, pos: usize, code: u32, minimum: usize) -> usize {
        self.inner.borrow().skip_chars_back(pos, code, minimum)
    }

    fn skipCharsStrBack(&self, pos: usize, ch: char, minimum: usize) -> usize {
        self.inner.borrow().skip_chars_str_back(pos, ch, minimum)
    }

    fn getLines(&self, begin: usize, end: usize, indent: usize, keep_last_lf: bool) -> String {
        self.inner.borrow().get_lines(begin, end, indent, keep_last_lf)
    }

    fn is_code_block(&self, line: usize) -> bool {
        self.inner.borrow().is_code_block(line)
    }
}

// ---------------------------------------------------------------------------
// StateInline
// ---------------------------------------------------------------------------

#[pyclass(name = "StateInline", unsendable)]
pub struct PyStateInline {
    inner: SharedInlineState,
    md: Option<Py<PyAny>>,
}

impl PyStateInline {
    pub fn wrap(inner: SharedInlineState, host: &Rc<PyHost>) -> Self {
        let md = current_md(host);
        Self { inner, md }
    }

    pub fn shared(&self) -> SharedInlineState {
        self.inner.clone()
    }
}

#[pymethods]
impl PyStateInline {
    #[new]
    #[allow(non_snake_case)]
    #[pyo3(signature = (src=None, md=None, env=None, outTokens=None))]
    fn new(
        py: Python,
        src: Option<String>,
        md: Option<Bound<PyAny>>,
        env: Option<Bound<PyDict>>,
        outTokens: Option<Vec<Bound<PyToken>>>,
    ) -> PyResult<Self> {
        // Verbatim: `__new__` without `__init__` also works (allocation
        // initializes everything); the bare instance supports `pending`.
        let src = src.unwrap_or_default();
        if let Some(md_obj) = md {
            let md_it: Bound<PyMarkdownIt> = md_obj.downcast_into().map_err(|_| {
                PyTypeError::new_err("md must be a MarkdownIt instance")
            })?;
            let (options, host) = {
                let m = md_it.borrow();
                (m.options_rc(), m.host_rc())
            };
            let mut env_data = Default::default();
            if let Some(e) = env {
                env_from_py(py, &mut env_data, &e);
            }
            let toks = outTokens
                .unwrap_or_default()
                .into_iter()
                .map(|t| t.borrow().shared())
                .collect();
            let inner = Rc::new(RefCell::new(
                markdown_it_rust_core::rules_inline::StateInline::new(
                    &src,
                    options,
                    host.handle(),
                    env_data,
                    toks,
                ),
            ));
            return Ok(Self { inner, md: Some(md_it.unbind().into_any()) });
        }
        // Bare allocation (`StateInline.__new__(StateInline)`): defaults.
        // No host is needed: pending works core-locally.
        let inner = Rc::new(RefCell::new(
            markdown_it_rust_core::rules_inline::StateInline::bare(&src),
        ));
        Ok(Self { inner, md: None })
    }

    fn __repr__(&self) -> String {
        let st = self.inner.borrow();
        format!("StateInline(pos=[{} of {}], token={})", st.pos, st.pos_max, st.tokens.len())
    }

    fn __copy__(&self, py: Python) -> PyResult<Py<PyStateInline>> {
        let mut st = self.inner.borrow_mut();
        let copied = st.copy_shallow();
        drop(st);
        Py::new(py, PyStateInline { inner: Rc::new(RefCell::new(copied)), md: self.md.as_ref().map(|o| o.clone_ref(py)) })
    }

    #[getter]
    fn get_src(&self) -> String {
        self.inner.borrow().src.iter().collect()
    }

    #[setter]
    fn set_src(&self, v: String) {
        let mut st = self.inner.borrow_mut();
        let chars: Vec<char> = v.chars().collect();
        let mut byte_of = Vec::with_capacity(chars.len() + 1);
        for (byte_idx, _) in v.char_indices() {
            byte_of.push(byte_idx);
        }
        byte_of.push(v.len());
        st.src = chars;
        st.src_text = v;
        st.src_byte_of = byte_of;
    }

    #[getter]
    fn get_srcCharCode(&self, py: Python) -> PyResult<Py<PyAny>> {
        use pyo3::exceptions::PyDeprecationWarning;
        PyErr::warn(
            py,
            &py.get_type::<PyDeprecationWarning>(),
            c"StateBase.srcCharCode is deprecated. Use StateBase.src instead.",
            2,
        )?;
        let codes: Vec<u32> = self.inner.borrow().src.iter().map(|&c| c as u32).collect();
        Ok(codes.into_pyobject(py)?.into_any().unbind())
    }

    #[getter]
    fn get_md(&self, py: Python) -> PyResult<Py<PyAny>> {
        match &self.md {
            Some(m) => Ok(m.clone_ref(py)),
            None => Ok(py.None()),
        }
    }

    #[getter]
    fn get_env(&self, py: Python) -> Py<PyDict> {
        env_to_py(py, &self.inner.borrow().env)
    }

    #[setter]
    fn set_env(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        env_from_py(py, &mut self.inner.borrow_mut().env, &v);
        Ok(())
    }

    #[getter]
    fn get_tokens(&self, py: Python) -> PyResult<Vec<Py<PyToken>>> {
        Ok(self
            .inner
            .borrow()
            .tokens
            .clone()
            .into_iter()
            .map(|t| Py::new(py, PyToken::wrap(t)).unwrap())
            .collect())
    }

    #[setter]
    fn set_tokens(&self, py: Python, v: Vec<Bound<PyToken>>) -> PyResult<()> {
        let _ = py;
        self.inner.borrow_mut().tokens =
            v.into_iter().map(|t| t.borrow().shared()).collect();
        Ok(())
    }

    #[getter]
    fn get_pos(&self) -> usize {
        self.inner.borrow().pos
    }

    #[setter]
    fn set_pos(&self, v: usize) {
        self.inner.borrow_mut().pos = v;
    }

    #[getter]
    fn get_posMax(&self) -> usize {
        self.inner.borrow().pos_max
    }

    #[setter]
    fn set_posMax(&self, v: usize) {
        self.inner.borrow_mut().pos_max = v;
    }

    #[getter]
    fn get_level(&self) -> usize {
        self.inner.borrow().level
    }

    #[setter]
    fn set_level(&self, v: usize) {
        self.inner.borrow_mut().level = v;
    }

    #[getter]
    fn get_pending(&self) -> String {
        self.inner.borrow_mut().pending()
    }

    #[setter]
    fn set_pending(&self, v: String) {
        self.inner.borrow_mut().set_pending(v);
    }

    #[getter]
    fn get__pending(&self) -> String {
        self.inner.borrow().pending_raw().to_string()
    }

    #[getter]
    fn get__pending_buffer(&self, py: Python) -> PyResult<Vec<String>> {
        let _ = py;
        Ok(self.inner.borrow().pending_buffer_raw().to_vec())
    }

    #[getter]
    fn get_pendingLevel(&self) -> usize {
        self.inner.borrow().pending_level
    }

    #[setter]
    fn set_pendingLevel(&self, v: usize) {
        self.inner.borrow_mut().pending_level = v;
    }

    #[getter]
    fn get_linkLevel(&self) -> i64 {
        self.inner.borrow().link_level
    }

    #[setter]
    fn set_linkLevel(&self, v: i64) {
        self.inner.borrow_mut().link_level = v;
    }

    #[getter]
    fn get_cache(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.borrow().cache {
            dict.set_item(k, v)?;
        }
        Ok(dict.into_any().unbind())
    }

    #[setter]
    fn set_cache(&self, v: Bound<PyDict>) -> PyResult<()> {
        let mut map = std::collections::HashMap::new();
        for (k, val) in v.iter() {
            map.insert(k.extract::<usize>()?, val.extract::<usize>()?);
        }
        self.inner.borrow_mut().cache = map;
        Ok(())
    }

    fn append_pending(&self, text: &str) {
        self.inner.borrow_mut().append_pending(text);
    }

    fn pushPending(&self, py: Python) -> PyResult<Py<PyToken>> {
        Py::new(py, PyToken::wrap(self.inner.borrow_mut().push_pending()))
    }

    fn push(&self, py: Python, ttype: &str, tag: &str, nesting: i8) -> PyResult<Py<PyToken>> {
        Py::new(py, PyToken::wrap(self.inner.borrow_mut().push(ttype, tag, nesting)))
    }

    fn scanDelims(&self, start: usize, can_split_word: bool) -> (bool, bool, usize) {
        let s = self.inner.borrow().scan_delims(start, can_split_word);
        (s.can_open, s.can_close, s.length)
    }

    fn html_terminator_last(&self, term: &str) -> i64 {
        self.inner.borrow_mut().html_terminator_last(term)
    }
}

// ---------------------------------------------------------------------------
// StateCore
// ---------------------------------------------------------------------------

#[pyclass(name = "StateCore", unsendable)]
pub struct PyStateCore {
    inner: SharedCoreState,
    md: Option<Py<PyAny>>,
}

impl PyStateCore {
    pub fn wrap(inner: SharedCoreState, host: &Rc<PyHost>) -> Self {
        let md = current_md(host);
        Self { inner, md }
    }

    pub fn shared(&self) -> SharedCoreState {
        self.inner.clone()
    }
}

#[pymethods]
impl PyStateCore {
    #[new]
    #[pyo3(signature = (src, md, env, tokens=None))]
    fn new(
        py: Python,
        src: String,
        md: Bound<PyAny>,
        env: Bound<PyDict>,
        tokens: Option<Vec<Bound<PyToken>>>,
    ) -> PyResult<Self> {
        let md_it: Bound<PyMarkdownIt> = md.downcast_into().map_err(|_| {
            PyTypeError::new_err("md must be a MarkdownIt instance")
        })?;
        let (options, host) = {
            let m = md_it.borrow();
            (m.options_rc(), m.host_rc())
        };
        let mut env_data = Default::default();
        env_from_py(py, &mut env_data, &env);
        let toks = tokens
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.borrow().shared())
            .collect();
        let inner = Rc::new(RefCell::new(
            markdown_it_rust_core::rules_core::StateCore::new(
                &src,
                options,
                host.handle(),
                env_data,
                toks,
            ),
        ));
        Ok(Self { inner, md: Some(md_it.unbind().into_any()) })
    }

    #[getter]
    fn get_src(&self) -> String {
        self.inner.borrow().src.clone()
    }

    #[setter]
    fn set_src(&self, v: String) {
        self.inner.borrow_mut().src = v;
    }

    #[getter]
    fn get_srcCharCode(&self, py: Python) -> PyResult<Py<PyAny>> {
        use pyo3::exceptions::PyDeprecationWarning;
        PyErr::warn(
            py,
            &py.get_type::<PyDeprecationWarning>(),
            c"StateBase.srcCharCode is deprecated. Use StateBase.src instead.",
            2,
        )?;
        let codes: Vec<u32> = self.inner.borrow().src.chars().map(|c| c as u32).collect();
        Ok(codes.into_pyobject(py)?.into_any().unbind())
    }

    #[getter]
    fn get_md(&self, py: Python) -> PyResult<Py<PyAny>> {
        match &self.md {
            Some(m) => Ok(m.clone_ref(py)),
            None => Ok(py.None()),
        }
    }

    #[getter]
    fn get_env(&self, py: Python) -> Py<PyDict> {
        env_to_py(py, &self.inner.borrow().env)
    }

    #[setter]
    fn set_env(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        env_from_py(py, &mut self.inner.borrow_mut().env, &v);
        Ok(())
    }

    #[getter]
    fn get_tokens(&self, py: Python) -> PyResult<Vec<Py<PyToken>>> {
        Ok(self
            .inner
            .borrow()
            .tokens
            .clone()
            .into_iter()
            .map(|t| Py::new(py, PyToken::wrap(t)).unwrap())
            .collect())
    }

    #[setter]
    fn set_tokens(&self, py: Python, v: Vec<Bound<PyToken>>) -> PyResult<()> {
        let _ = py;
        self.inner.borrow_mut().tokens =
            v.into_iter().map(|t| t.borrow().shared()).collect();
        Ok(())
    }

    #[getter]
    fn get_inlineMode(&self) -> bool {
        self.inner.borrow().inline_mode
    }

    #[setter]
    fn set_inlineMode(&self, v: bool) {
        self.inner.borrow_mut().inline_mode = v;
    }
}
