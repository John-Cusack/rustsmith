//! [`PyHost`]: the core [`Host`] trait over live Python objects.
//!
//! Holds the three core parser `Rc`s (shared with the `Parser*` pyclasses,
//! so ruler mutations stay live), the Python-callable registry (rule
//! functions keyed by registration id, render overrides cached by object
//! identity), the shared `linkify`/`highlight` cells, the shared `rules`
//! dict, and the cached `mdurl` module. No back-pointer to `MarkdownIt`:
//! no reference cycle.
//!
//! `Host` is implemented on `Rc<PyHost>` (a local trait on a composed type
//! is legal) so every method can mint the `Rc<dyn Host>` core functions
//! require without raw pointers.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use markdown_it_rust_core::options_env::EnvData;
use markdown_it_rust_core::{
    Host, LinkifyMatch, SharedCoreState, SharedInlineState, SharedToken,
};
use markdown_it_rust_core::{SharedBlockState, SharedOptions};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};

use crate::host_convert::{env_from_py, env_to_py, normalize_link_py};
use crate::states::{PyStateBlock, PyStateCore, PyStateInline};
use crate::token::PyToken;

pub struct PyHost {
    pub(crate) block: Rc<RefCell<markdown_it_rust_core::rules_block::ParserBlock>>,
    pub(crate) inline: Rc<RefCell<markdown_it_rust_core::rules_inline::ParserInline>>,
    registry: RefCell<HashMap<u64, Py<PyAny>>>,
    ptr_to_id: RefCell<HashMap<usize, u64>>,
    next_id: Cell<u64>,
    pub(crate) linkify: Rc<RefCell<Option<Py<PyAny>>>>,
    pub(crate) rules_dict: Py<PyDict>,
    pub(crate) highlight: Rc<RefCell<Option<Py<PyAny>>>>,
    pub(crate) mdurl: Py<PyModule>,
    pub(crate) current_md: RefCell<Option<Py<PyAny>>>,
}

impl PyHost {
    pub fn set_current_md(&self, md: Option<Py<PyAny>>) {
        *self.current_md.borrow_mut() = md;
    }

    pub fn current_md(&self, py: Python) -> Option<Py<PyAny>> {
        self.current_md.borrow().as_ref().map(|o| o.clone_ref(py))
    }

    /// `current_md` without a `Python` token (single-threaded callers only).
    pub(crate) fn current_md_nogil(&self) -> Option<Py<PyAny>> {
        Python::with_gil(|py| self.current_md(py))
    }

    pub fn new(
        py: Python,
        block: Rc<RefCell<markdown_it_rust_core::rules_block::ParserBlock>>,
        inline: Rc<RefCell<markdown_it_rust_core::rules_inline::ParserInline>>,
        linkify: Rc<RefCell<Option<Py<PyAny>>>>,
        rules_dict: Py<PyDict>,
        highlight: Rc<RefCell<Option<Py<PyAny>>>>,
    ) -> PyResult<Rc<Self>> {
        let mdurl = py.import("mdurl")?;
        Ok(Rc::new(Self {
            block,
            inline,
            registry: RefCell::new(HashMap::new()),
            ptr_to_id: RefCell::new(HashMap::new()),
            next_id: Cell::new(1),
            linkify,
            rules_dict,
            highlight,
            mdurl: mdurl.unbind(),
            current_md: RefCell::new(None),
        }))
    }

    /// Register a Python callable, returning its core id.
    pub fn intern(&self, obj: Py<PyAny>) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        self.registry.borrow_mut().insert(id, obj);
        id
    }

    /// Intern by object identity (render overrides consulted per token).
    fn intern_ptr(&self, obj: &Bound<PyAny>) -> u64 {
        let ptr = obj.as_ptr() as usize;
        if let Some(&id) = self.ptr_to_id.borrow().get(&ptr) {
            return id;
        }
        let id = self.intern(obj.clone().unbind());
        self.ptr_to_id.borrow_mut().insert(ptr, id);
        id
    }

    pub(crate) fn lookup(&self, py: Python, id: u64) -> Option<Py<PyAny>> {
        self.registry.borrow().get(&id).map(|o| o.clone_ref(py))
    }

}

fn wrap_block(py: Python, state: &SharedBlockState, host: &Rc<PyHost>) -> PyResult<Py<PyStateBlock>> {
    Py::new(py, PyStateBlock::wrap(state.clone(), host))
}

fn wrap_inline(
    py: Python,
    state: &SharedInlineState,
    host: &Rc<PyHost>,
) -> PyResult<Py<PyStateInline>> {
    Py::new(py, PyStateInline::wrap(state.clone(), host))
}

fn wrap_core(py: Python, state: &SharedCoreState, host: &Rc<PyHost>) -> PyResult<Py<PyStateCore>> {
    Py::new(py, PyStateCore::wrap(state.clone(), host))
}

fn wrap_tokens(py: Python, tokens: &[SharedToken]) -> Py<PyList> {
    let list = PyList::empty(py);
    for t in tokens {
        if let Ok(obj) = Py::new(py, PyToken::wrap(t.clone())) {
            list.append(obj).ok();
        }
    }
    list.unbind()
}

/// Cloneable host handle for core entry points (orphan-rule-safe).
#[derive(Clone)]
pub struct HostHandle(pub Rc<PyHost>);

impl PyHost {
    pub fn handle(self: &Rc<Self>) -> Rc<dyn Host> {
        Rc::new(HostHandle(self.clone())) as Rc<dyn Host>
    }
}

impl Host for HostHandle {
    fn tokenize_block(&self, state: &SharedBlockState, start: usize, end: usize) {
        let host: Rc<dyn Host> = Rc::new(self.clone());
        let rules = self.0.block.borrow_mut().default_chain();
        markdown_it_rust_core::rules_block::tokenize_with(&rules, state, start, end, &host);
    }

    fn block_terminates(
        &self,
        chain: &str,
        state: &SharedBlockState,
        line: usize,
        end: usize,
    ) -> bool {
        let host: Rc<dyn Host> = Rc::new(self.clone());
        let rules = self.0.block.borrow_mut().chain(chain);
        markdown_it_rust_core::rules_block::terminates_with(&rules, state, line, end, &host)
    }

    fn core_block_parse(&self, state: &SharedCoreState) {
        let host: Rc<dyn Host> = Rc::new(self.clone());
        // Verbatim core `block` rule: inline mode emits a single inline token.
        if state.borrow().inline_mode {
            let token = markdown_it_rust_core::token::shared_token({
                let mut t = markdown_it_rust_core::Token::new("inline", "", 0);
                t.content = state.borrow().src.clone();
                t.map = Some((0, 1));
                t.children = Some(Vec::new());
                t
            });
            state.borrow_mut().tokens.push(token);
            return;
        }
        let (src, options, env, tokens) = {
            let mut st = state.borrow_mut();
            let env = std::mem::take(&mut st.env);
            let tokens = std::mem::take(&mut st.tokens);
            (st.src.clone(), st.md.options.clone(), env, tokens)
        };
        if src.is_empty() {
            let mut st = state.borrow_mut();
            st.env = env;
            st.tokens = tokens;
            return;
        }
        let code_enabled = self.0.block.borrow().ruler.is_active("code");
        let bstate = Rc::new(RefCell::new(
            markdown_it_rust_core::rules_block::StateBlock::new(
                &src,
                options,
                host.clone(),
                env,
                tokens,
                code_enabled,
            ),
        ));
        let end = bstate.borrow().line_max;
        let rules = self.0.block.borrow_mut().default_chain();
        markdown_it_rust_core::rules_block::tokenize_with(&rules, &bstate, 0, end, &host);
        let mut b = bstate.borrow_mut();
        let mut st = state.borrow_mut();
        st.env = std::mem::take(&mut b.env);
        st.tokens = std::mem::take(&mut b.tokens);
    }

    fn core_inline_parse(&self, state: &SharedCoreState, token: &SharedToken) {
        let host: Rc<dyn Host> = Rc::new(self.clone());
        let (content, options, env, children) = {
            let st = state.borrow();
            let t = token.borrow();
            (
                t.content.clone(),
                st.md.options.clone(),
                st.env.clone(),
                t.children.clone().unwrap_or_default(),
            )
        };
        let istate = Rc::new(RefCell::new(
            markdown_it_rust_core::rules_inline::StateInline::new(
                &content,
                options,
                host.clone(),
                env,
                children,
            ),
        ));
        let (rules, terms) = self.0.inline.borrow_mut().snapshot();
        markdown_it_rust_core::rules_inline::tokenize_with(&rules, &terms, &istate, &host);
        let rules2 = self.0.inline.borrow_mut().snapshot2();
        for rule in &rules2 {
            markdown_it_rust_core::rules_inline::run_inline2_rule(rule, &istate, &host);
        }
        let mut i = istate.borrow_mut();
        token.borrow_mut().children = Some(std::mem::take(&mut i.tokens));
        state.borrow_mut().env = std::mem::take(&mut i.env);
    }

    fn call_block_rule(
        &self,
        id: u64,
        state: &SharedBlockState,
        start: usize,
        end: usize,
        silent: bool,
    ) -> bool {
        Python::with_gil(|py| {
            let Ok(obj) = wrap_block(py, state, &self.0) else {
                return false;
            };
            let env = env_to_py(py, &state.borrow().env);
            obj.bind(py).setattr("env", &env).ok();
            let func = match self.0.lookup(py, id) {
                Some(f) => f,
                None => return false,
            };
            let r = func.bind(py).call1((obj.clone_ref(py), start, end, silent));
            if let Ok(env_back) = obj.bind(py).getattr("env") {
                env_from_py(py, &mut state.borrow_mut().env, &env_back);
            }
            r.map(|v| v.is_truthy().unwrap_or(false)).unwrap_or(false)
        })
    }

    fn call_inline_rule(&self, id: u64, state: &SharedInlineState, silent: bool) -> bool {
        Python::with_gil(|py| {
            let Ok(obj) = wrap_inline(py, state, &self.0) else {
                return false;
            };
            let env = env_to_py(py, &state.borrow().env);
            obj.bind(py).setattr("env", &env).ok();
            let func = match self.0.lookup(py, id) {
                Some(f) => f,
                None => return false,
            };
            let r = func.bind(py).call1((obj.clone_ref(py), silent));
            if let Ok(env_back) = obj.bind(py).getattr("env") {
                env_from_py(py, &mut state.borrow_mut().env, &env_back);
            }
            r.map(|v| v.is_truthy().unwrap_or(false)).unwrap_or(false)
        })
    }

    fn call_inline2_rule(&self, id: u64, state: &SharedInlineState) {
        Python::with_gil(|py| {
            let Ok(obj) = wrap_inline(py, state, &self.0) else {
                return;
            };
            let env = env_to_py(py, &state.borrow().env);
            obj.bind(py).setattr("env", &env).ok();
            if let Some(func) = self.0.lookup(py, id) {
                let _ = func.bind(py).call1((obj.clone_ref(py),));
            }
            if let Ok(env_back) = obj.bind(py).getattr("env") {
                env_from_py(py, &mut state.borrow_mut().env, &env_back);
            }
        });
    }

    fn call_core_rule(&self, id: u64, state: &SharedCoreState) {
        Python::with_gil(|py| {
            let Ok(obj) = wrap_core(py, state, &self.0) else {
                return;
            };
            let env = env_to_py(py, &state.borrow().env);
            obj.bind(py).setattr("env", &env).ok();
            if let Some(func) = self.0.lookup(py, id) {
                let _ = func.bind(py).call1((obj.clone_ref(py),));
            }
            if let Ok(env_back) = obj.bind(py).getattr("env") {
                env_from_py(py, &mut state.borrow_mut().env, &env_back);
            }
        });
    }

    fn normalize_link(&self, url: &str) -> String {
        Python::with_gil(|py| normalize_link_py(py, &self.0.mdurl, url, false))
    }

    fn normalize_link_text(&self, url: &str) -> String {
        Python::with_gil(|py| normalize_link_py(py, &self.0.mdurl, url, true))
    }

    fn validate_link(&self, url: &str, validator: Option<u64>) -> bool {
        if let Some(id) = validator {
            return Python::with_gil(|py| {
                self.0.lookup(py, id)
                    .and_then(|f| f.bind(py).call1((url,)).ok())
                    .and_then(|v| v.extract::<bool>().ok())
                    .unwrap_or(false)
            });
        }
        markdown_it_rust_core::common_utils::validate_link_default(url)
    }

    fn has_linkify(&self) -> bool {
        self.0.linkify.borrow().is_some()
    }

    fn linkify_pretest(&self, text: &str) -> Option<bool> {
        Python::with_gil(|py| {
            let obj = self.0.linkify.borrow().as_ref().map(|o| o.clone_ref(py))?;
            obj.bind(py).call_method1("pretest", (text,)).ok()?.extract().ok()
        })
    }

    fn linkify_test(&self, text: &str) -> Option<bool> {
        Python::with_gil(|py| {
            let obj = self.0.linkify.borrow().as_ref().map(|o| o.clone_ref(py))?;
            obj.bind(py).call_method1("test", (text,)).ok()?.extract().ok()
        })
    }

    fn linkify_match_at_start(&self, fragment: &str) -> Option<LinkifyMatch> {
        Python::with_gil(|py| {
            let obj = self.0.linkify.borrow().as_ref().map(|o| o.clone_ref(py))?;
            let m = obj.bind(py).call_method1("match_at_start", (fragment,)).ok()?;
            if m.is_none() {
                return None;
            }
            match_to_core(&m).ok()
        })
    }

    fn linkify_match(&self, text: &str) -> Vec<LinkifyMatch> {
        Python::with_gil(|py| {
            let Some(obj) = self.0.linkify.borrow().as_ref().map(|o| o.clone_ref(py)) else {
                return Vec::new();
            };
            let Ok(m) = obj.bind(py).call_method1("match", (text,)) else {
                return Vec::new();
            };
            if m.is_none() {
                return Vec::new();
            }
            let Ok(list) = m.downcast::<PyList>() else {
                return Vec::new();
            };
            list.iter().filter_map(|item| match_to_core(&item).ok()).collect()
        })
    }

    fn call_highlight(&self, content: &str, lang: &str, attrs: &str) -> Option<String> {
        Python::with_gil(|py| {
            let hl = self.0.highlight.borrow().as_ref().map(|o| o.clone_ref(py))?;
            let r = hl.bind(py).call1((content, lang, attrs)).ok()?;
            if r.is_none() {
                return None;
            }
            r.extract().ok()
        })
    }

    fn render_override(&self, token_type: &str) -> Option<u64> {
        Python::with_gil(|py| {
            let obj = self.0.rules_dict.bind(py).get_item(token_type).ok()??;
            Some(self.0.intern_ptr(&obj))
        })
    }

    fn call_render_rule(
        &self,
        id: u64,
        tokens: &[SharedToken],
        idx: usize,
        options: &SharedOptions,
        env: &EnvData,
    ) -> String {
        Python::with_gil(|py| {
            let list = wrap_tokens(py, tokens);
            let options_obj = Py::new(py, crate::options::PyOptionsDict::wrap(options.clone()));
            let env_dict = env_to_py(py, env);
            let (Ok(list), Ok(options_obj)) = (Ok::<_, PyErr>(list), options_obj) else {
                return String::new();
            };
            self.0.lookup(py, id)
                .and_then(|f| f.bind(py).call1((list, idx, options_obj, env_dict)).ok())
                .and_then(|v| v.extract::<String>().ok())
                .unwrap_or_default()
        })
    }

    fn html_tag_match(&self, src: &str, pos: usize) -> Option<String> {
        Python::with_gil(|py| {
            let module = py.import("markdown_it.rules_inline.html_inline").ok()?;
            let re = module.getattr("HTML_TAG_RE").ok()?;
            let m = re.call_method("match", (src, pos), None).ok()?;
            if m.is_none() {
                return None;
            }
            m.getattr("group").ok()?.call1((0,)).ok()?.extract().ok()
        })
    }

    fn inline_skip_token(&self, state: &SharedInlineState) {
        let host: Rc<dyn Host> = Rc::new(self.clone());
        let (rules, terms) = self.0.inline.borrow_mut().snapshot();
        markdown_it_rust_core::rules_inline::skip_token_with(&rules, &terms, state, &host);
    }

    fn inline_tokenize(&self, state: &SharedInlineState) {
        let host: Rc<dyn Host> = Rc::new(self.clone());
        let (rules, terms) = self.0.inline.borrow_mut().snapshot();
        markdown_it_rust_core::rules_inline::tokenize_with(&rules, &terms, state, &host);
    }

    fn inline_parse_content(
        &self,
        parent: &SharedInlineState,
        content: &str,
    ) -> Vec<SharedToken> {
        let host: Rc<dyn Host> = Rc::new(self.clone());
        let (options, env) = {
            let st = parent.borrow();
            (st.md.options.clone(), st.env.clone())
        };
        let istate = Rc::new(RefCell::new(
            markdown_it_rust_core::rules_inline::StateInline::new(
                content,
                options,
                host.clone(),
                env,
                Vec::new(),
            ),
        ));
        let (rules, terms) = self.0.inline.borrow_mut().snapshot();
        markdown_it_rust_core::rules_inline::tokenize_with(&rules, &terms, &istate, &host);
        let rules2 = self.0.inline.borrow_mut().snapshot2();
        for rule in &rules2 {
            markdown_it_rust_core::rules_inline::run_inline2_rule(rule, &istate, &host);
        }
        let mut i = istate.borrow_mut();
        parent.borrow_mut().env = std::mem::take(&mut i.env);
        std::mem::take(&mut i.tokens)
    }

    fn env_snapshot(&self, env: &EnvData) -> markdown_it_rust_core::EnvSnapshot {
        crate::host_convert::env_to_snapshot(env, &[])
    }

    fn env_merge_back(
        &self,
        _env: &mut EnvData,
        _snapshot: markdown_it_rust_core::EnvSnapshot,
    ) {
    }
}

fn match_to_core(m: &Bound<PyAny>) -> PyResult<LinkifyMatch> {
    let get_str = |k: &str| -> PyResult<String> {
        let v = m.getattr(k)?;
        if v.is_none() {
            return Ok(String::new());
        }
        v.extract()
    };
    Ok(LinkifyMatch {
        schema: get_str("schema")?,
        index: m.getattr("index")?.extract()?,
        last_index: m.getattr("last_index")?.extract()?,
        raw: get_str("raw")?,
        text: get_str("text")?,
        url: get_str("url")?,
    })
}
