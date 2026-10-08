//! Parser + ruler pyclasses.
//!
//! `PyRuler` wraps one core ruler (inline has two: `ruler`/`ruler2`); rule
//! functions cross as registry ids, `alt` lists through `options["alt"]`.
//! Builtin rules surface as [`PyBuiltinRule`] callables (chain + name).
//! `terminator_re` is a real compiled `re.Pattern` (built via Python `re`
//! for exact escaping), cached per parser with `is`-stable identity.

use std::cell::RefCell;
use std::rc::Rc;

use markdown_it_rust_core::rules_block::{BlockKind, BlockRule};
use markdown_it_rust_core::rules_core::{CoreKind, CoreRule};
use markdown_it_rust_core::rules_inline::{Inline2Kind, Inline2Rule, InlineKind, InlineRule};
use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::host::PyHost;
use crate::host_convert::{env_from_py, env_to_user};
use crate::markdown::PyMarkdownIt;
use crate::states::{PyStateBlock, PyStateCore, PyStateInline};
use crate::token::PyToken;

#[derive(Clone)]
pub enum RulerTarget {
    Block(Rc<RefCell<markdown_it_rust_core::rules_block::ParserBlock>>),
    InlineMain(Rc<RefCell<markdown_it_rust_core::rules_inline::ParserInline>>),
    Inline2(Rc<RefCell<markdown_it_rust_core::rules_inline::ParserInline>>),
    Core(Rc<RefCell<markdown_it_rust_core::rules_core::ParserCore>>),
}

#[pyclass(name = "Ruler", unsendable)]
pub struct PyRuler {
    target: RulerTarget,
    host: Rc<PyHost>,
}

impl PyRuler {
    pub fn new(target: RulerTarget, host: Rc<PyHost>) -> Self {
        Self { target, host }
    }

    fn alt_of(options: Option<Bound<PyAny>>) -> PyResult<Vec<String>> {
        match options {
            None => Ok(Vec::new()),
            Some(o) if o.is_none() => Ok(Vec::new()),
            Some(o) => {
                let d = o.downcast::<PyDict>().map_err(|_| {
                    PyTypeError::new_err("options must be a dict with optional 'alt'")
                })?;
                match d.get_item("alt")? {
                    Some(alt) => alt.extract(),
                    None => Ok(Vec::new()),
                }
            }
        }
    }

    fn builtin_fn(&self, py: Python, chain: &str, name: &str) -> PyResult<Py<PyAny>> {
        Ok(Py::new(py, PyBuiltinRule { chain: chain.to_string(), name: name.to_string(), host: Some(self.host.clone()) })?
            .into_any())
    }

    fn rule_to_py(&self, py: Python, chain: &str, name: &str, is_python: Option<u64>) -> PyResult<Py<PyAny>> {
        match is_python {
            Some(id) => self.host.lookup(py, id).ok_or_else(|| PyTypeError::new_err("unknown rule id")),
            None => self.builtin_fn(py, chain, name),
        }
    }
}

#[pymethods]
impl PyRuler {
    #[pyo3(signature = (name, func, options=None))]
    fn at(&self, py: Python, name: &str, func: Bound<PyAny>, options: Option<Bound<PyAny>>) -> PyResult<()> {
        let alt = Self::alt_of(options)?;
        let id = self.host.intern(func.unbind());
        let _ = py;
        match &self.target {
            RulerTarget::Block(p) => p
                .borrow_mut()
                .ruler
                .at(name, BlockRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::InlineMain(p) => p
                .borrow_mut()
                .ruler
                .at(name, InlineRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::Inline2(p) => p
                .borrow_mut()
                .ruler2
                .at(name, Inline2Rule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::Core(p) => p
                .borrow_mut()
                .ruler
                .at(name, CoreRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
        }
    }

    #[pyo3(signature = (before, name, func, options=None))]
    fn before(
        &self,
        py: Python,
        before: &str,
        name: &str,
        func: Bound<PyAny>,
        options: Option<Bound<PyAny>>,
    ) -> PyResult<()> {
        let alt = Self::alt_of(options)?;
        let id = self.host.intern(func.unbind());
        let _ = py;
        match &self.target {
            RulerTarget::Block(p) => p
                .borrow_mut()
                .ruler
                .before(before, name, BlockRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::InlineMain(p) => p
                .borrow_mut()
                .ruler
                .before(before, name, InlineRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::Inline2(p) => p
                .borrow_mut()
                .ruler2
                .before(before, name, Inline2Rule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::Core(p) => p
                .borrow_mut()
                .ruler
                .before(before, name, CoreRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
        }
    }

    #[pyo3(signature = (after, name, func, options=None))]
    fn after(
        &self,
        py: Python,
        after: &str,
        name: &str,
        func: Bound<PyAny>,
        options: Option<Bound<PyAny>>,
    ) -> PyResult<()> {
        let alt = Self::alt_of(options)?;
        let id = self.host.intern(func.unbind());
        let _ = py;
        match &self.target {
            RulerTarget::Block(p) => p
                .borrow_mut()
                .ruler
                .after(after, name, BlockRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::InlineMain(p) => p
                .borrow_mut()
                .ruler
                .after(after, name, InlineRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::Inline2(p) => p
                .borrow_mut()
                .ruler2
                .after(after, name, Inline2Rule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
            RulerTarget::Core(p) => p
                .borrow_mut()
                .ruler
                .after(after, name, CoreRule::Python(id), &alt)
                .map_err(PyKeyError::new_err),
        }
    }

    #[pyo3(signature = (name, func, options=None))]
    fn push(&self, py: Python, name: &str, func: Bound<PyAny>, options: Option<Bound<PyAny>>) -> PyResult<()> {
        let alt: Vec<&str> = Vec::new();
        let opt_alt = Self::alt_of(options)?;
        let alt_refs: Vec<&str> = opt_alt.iter().map(|s| s.as_str()).collect();
        let _ = alt;
        let id = self.host.intern(func.unbind());
        let _ = py;
        match &self.target {
            RulerTarget::Block(p) => p.borrow_mut().ruler.push(name, BlockRule::Python(id), &alt_refs),
            RulerTarget::InlineMain(p) => p.borrow_mut().ruler.push(name, InlineRule::Python(id), &alt_refs),
            RulerTarget::Inline2(p) => p.borrow_mut().ruler2.push(name, Inline2Rule::Python(id), &alt_refs),
            RulerTarget::Core(p) => p.borrow_mut().ruler.push(name, CoreRule::Python(id), &alt_refs),
        }
        Ok(())
    }

    #[allow(non_snake_case)]
    #[pyo3(signature = (names, ignoreInvalid=false))]
    fn enable(&self, names: Bound<PyAny>, ignoreInvalid: bool) -> PyResult<Vec<String>> {
        let names = to_str_list(&names)?;
        let r = match &self.target {
            RulerTarget::Block(p) => p.borrow_mut().ruler.enable(&names, ignoreInvalid),
            RulerTarget::InlineMain(p) => p.borrow_mut().ruler.enable(&names, ignoreInvalid),
            RulerTarget::Inline2(p) => p.borrow_mut().ruler2.enable(&names, ignoreInvalid),
            RulerTarget::Core(p) => p.borrow_mut().ruler.enable(&names, ignoreInvalid),
        };
        r.map_err(PyKeyError::new_err)
    }

    #[allow(non_snake_case)]
    #[pyo3(signature = (names, ignoreInvalid=false))]
    fn enableOnly(&self, names: Bound<PyAny>, ignoreInvalid: bool) -> PyResult<Vec<String>> {
        let names = to_str_list(&names)?;
        let r = match &self.target {
            RulerTarget::Block(p) => p.borrow_mut().ruler.enable_only(&names, ignoreInvalid),
            RulerTarget::InlineMain(p) => p.borrow_mut().ruler.enable_only(&names, ignoreInvalid),
            RulerTarget::Inline2(p) => p.borrow_mut().ruler2.enable_only(&names, ignoreInvalid),
            RulerTarget::Core(p) => p.borrow_mut().ruler.enable_only(&names, ignoreInvalid),
        };
        r.map_err(PyKeyError::new_err)
    }

    #[allow(non_snake_case)]
    #[pyo3(signature = (names, ignoreInvalid=false))]
    fn disable(&self, names: Bound<PyAny>, ignoreInvalid: bool) -> PyResult<Vec<String>> {
        let names = to_str_list(&names)?;
        let r = match &self.target {
            RulerTarget::Block(p) => p.borrow_mut().ruler.disable(&names, ignoreInvalid),
            RulerTarget::InlineMain(p) => p.borrow_mut().ruler.disable(&names, ignoreInvalid),
            RulerTarget::Inline2(p) => p.borrow_mut().ruler2.disable(&names, ignoreInvalid),
            RulerTarget::Core(p) => p.borrow_mut().ruler.disable(&names, ignoreInvalid),
        };
        r.map_err(PyKeyError::new_err)
    }

    #[pyo3(signature = (chain=""))]
    fn getRules(&self, py: Python, chain: &str) -> PyResult<Vec<Py<PyAny>>> {
        match &self.target {
            RulerTarget::Block(p) => {
                let rules = p.borrow_mut().ruler.get_rules(chain);
                rules.into_iter().map(|r| self.block_rule_to_py(py, &r)).collect()
            }
            RulerTarget::InlineMain(p) => {
                let rules = p.borrow_mut().ruler.get_rules(chain);
                rules.into_iter().map(|r| self.inline_rule_to_py(py, &r)).collect()
            }
            RulerTarget::Inline2(p) => {
                let rules = p.borrow_mut().ruler2.get_rules(chain);
                rules.into_iter().map(|r| self.inline2_rule_to_py(py, &r)).collect()
            }
            RulerTarget::Core(p) => {
                let rules = p.borrow_mut().ruler.get_rules(chain);
                rules.into_iter().map(|r| self.core_rule_to_py(py, &r)).collect()
            }
        }
    }

    fn get_all_rules(&self) -> Vec<String> {
        match &self.target {
            RulerTarget::Block(p) => p.borrow().ruler.get_all_rules(),
            RulerTarget::InlineMain(p) => p.borrow().ruler.get_all_rules(),
            RulerTarget::Inline2(p) => p.borrow().ruler2.get_all_rules(),
            RulerTarget::Core(p) => p.borrow().ruler.get_all_rules(),
        }
    }

    fn get_active_rules(&self) -> Vec<String> {
        match &self.target {
            RulerTarget::Block(p) => p.borrow().ruler.get_active_rules(),
            RulerTarget::InlineMain(p) => p.borrow().ruler.get_active_rules(),
            RulerTarget::Inline2(p) => p.borrow().ruler2.get_active_rules(),
            RulerTarget::Core(p) => p.borrow().ruler.get_active_rules(),
        }
    }
}

impl PyRuler {
    fn block_rule_to_py(&self, py: Python, rule: &BlockRule) -> PyResult<Py<PyAny>> {
        match rule {
            BlockRule::Python(id) => self.rule_to_py(py, "block", "", Some(*id)),
            BlockRule::Builtin(k) => self.rule_to_py(py, "block", block_kind_name(*k), None),
        }
    }

    fn inline_rule_to_py(&self, py: Python, rule: &InlineRule) -> PyResult<Py<PyAny>> {
        match rule {
            InlineRule::Python(id) => self.rule_to_py(py, "inline", "", Some(*id)),
            InlineRule::Builtin(k) => self.rule_to_py(py, "inline", inline_kind_name(*k), None),
        }
    }

    fn inline2_rule_to_py(&self, py: Python, rule: &Inline2Rule) -> PyResult<Py<PyAny>> {
        match rule {
            Inline2Rule::Python(id) => self.rule_to_py(py, "inline2", "", Some(*id)),
            Inline2Rule::Builtin(k) => self.rule_to_py(py, "inline2", inline2_kind_name(*k), None),
        }
    }

    fn core_rule_to_py(&self, py: Python, rule: &CoreRule) -> PyResult<Py<PyAny>> {
        match rule {
            CoreRule::Python(id) => self.rule_to_py(py, "core", "", Some(*id)),
            CoreRule::Builtin(k) => self.rule_to_py(py, "core", core_kind_name(*k), None),
        }
    }
}

fn to_str_list(v: &Bound<PyAny>) -> PyResult<Vec<String>> {
    if let Ok(s) = v.extract::<String>() {
        return Ok(vec![s]);
    }
    v.extract::<Vec<String>>()
        .map_err(|_| PyTypeError::new_err("rule names must be a string or list of strings"))
}

pub fn block_kind_name(k: BlockKind) -> &'static str {
    match k {
        BlockKind::Table => "table",
        BlockKind::Code => "code",
        BlockKind::Fence => "fence",
        BlockKind::Blockquote => "blockquote",
        BlockKind::Hr => "hr",
        BlockKind::List => "list",
        BlockKind::Reference => "reference",
        BlockKind::HtmlBlock => "html_block",
        BlockKind::Heading => "heading",
        BlockKind::Lheading => "lheading",
        BlockKind::Paragraph => "paragraph",
    }
}

pub fn inline_kind_name(k: InlineKind) -> &'static str {
    match k {
        InlineKind::Text => "text",
        InlineKind::Linkify => "linkify",
        InlineKind::Newline => "newline",
        InlineKind::Escape => "escape",
        InlineKind::Backticks => "backticks",
        InlineKind::Strikethrough => "strikethrough",
        InlineKind::Emphasis => "emphasis",
        InlineKind::Link => "link",
        InlineKind::Image => "image",
        InlineKind::Autolink => "autolink",
        InlineKind::HtmlInline => "html_inline",
        InlineKind::Entity => "entity",
    }
}

pub fn inline2_kind_name(k: Inline2Kind) -> &'static str {
    match k {
        Inline2Kind::BalancePairs => "balance_pairs",
        Inline2Kind::Strikethrough => "strikethrough",
        Inline2Kind::Emphasis => "emphasis",
        Inline2Kind::FragmentsJoin => "fragments_join",
    }
}

pub fn core_kind_name(k: CoreKind) -> &'static str {
    match k {
        CoreKind::Normalize => "normalize",
        CoreKind::Block => "block",
        CoreKind::Inline => "inline",
        CoreKind::Linkify => "linkify",
        CoreKind::Replacements => "replacements",
        CoreKind::Smartquotes => "smartquotes",
        CoreKind::TextJoin => "text_join",
    }
}

/// A builtin rule as a Python callable (`getRules` results, `rules_*`
/// module namespaces). Dispatches on (chain, name) against the state's
/// live host.
#[pyclass(name = "BuiltinRule", unsendable)]
pub struct PyBuiltinRule {
    chain: String,
    name: String,
    host: Option<Rc<PyHost>>,
}

impl PyBuiltinRule {
    /// Module-level singleton (no owning ruler; the state's live host
    /// serves rule execution, default terminators serve `text`).
    pub fn bare(chain: &str, name: &str) -> Self {
        Self { chain: chain.to_string(), name: name.to_string(), host: None }
    }
}

#[pymethods]
impl PyBuiltinRule {
    #[pyo3(signature = (*args))]
    fn __call__(
        &self,
        py: Python,
        args: &Bound<pyo3::types::PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        // Rule execution uses the *state's* live host; the terminator set
        // for `text` comes from the owning ruler when present (defaults for
        // module singletons).
        let terms = match &self.host {
            Some(h) => h.inline.borrow().terminator_snapshot(),
            None => markdown_it_rust_core::rules_inline::text::default_terminators(),
        };
        match self.chain.as_str() {
            "block" => {
                let state: Bound<PyStateBlock> = args.get_item(0)?.downcast_into()?;
                let start: usize = args.get_item(1)?.extract()?;
                let end: usize = args.get_item(2)?.extract()?;
                let silent: bool = args.get_item(3)?.extract()?;
                let kind = block_kind_by_name(&self.name).ok_or_else(|| {
                    PyTypeError::new_err(format!("unknown block rule {}", self.name))
                })?;
                let shared = state.borrow().shared();
                let host_dyn = shared.borrow().md.host.clone();
                let r = markdown_it_rust_core::rules_block::call_builtin(
                    kind,
                    &shared,
                    start,
                    end,
                    silent,
                    &host_dyn,
                );
                Ok(pyo3::types::PyBool::new(py, r).to_owned().into_any().unbind())
            }
            "inline" => {
                let state: Bound<PyStateInline> = args.get_item(0)?.downcast_into()?;
                let silent: bool = args.get_item(1)?.extract()?;
                let kind = inline_kind_by_name(&self.name).ok_or_else(|| {
                    PyTypeError::new_err(format!("unknown inline rule {}", self.name))
                })?;
                let shared = state.borrow().shared();
                let host_dyn = shared.borrow().md.host.clone();
                let r = markdown_it_rust_core::rules_inline::run_inline_builtin(
                    kind,
                    &shared,
                    silent,
                    &host_dyn,
                    &terms,
                );
                Ok(pyo3::types::PyBool::new(py, r).to_owned().into_any().unbind())
            }
            "inline2" => {
                let state: Bound<PyStateInline> = args.get_item(0)?.downcast_into()?;
                let kind = inline2_kind_by_name(&self.name).ok_or_else(|| {
                    PyTypeError::new_err(format!("unknown inline2 rule {}", self.name))
                })?;
                let shared = state.borrow().shared();
                let host_dyn = shared.borrow().md.host.clone();
                let rule = markdown_it_rust_core::rules_inline::Inline2Rule::Builtin(kind);
                markdown_it_rust_core::rules_inline::run_inline2_rule(
                    &rule,
                    &shared,
                    &host_dyn,
                );
                Ok(py.None())
            }
            _ => Err(PyTypeError::new_err("unknown rule chain")),
        }
    }
}

fn block_kind_by_name(name: &str) -> Option<BlockKind> {
    Some(match name {
        "table" => BlockKind::Table,
        "code" => BlockKind::Code,
        "fence" => BlockKind::Fence,
        "blockquote" => BlockKind::Blockquote,
        "hr" => BlockKind::Hr,
        "list" => BlockKind::List,
        "reference" => BlockKind::Reference,
        "html_block" => BlockKind::HtmlBlock,
        "heading" => BlockKind::Heading,
        "lheading" => BlockKind::Lheading,
        "paragraph" => BlockKind::Paragraph,
        _ => return None,
    })
}

fn inline_kind_by_name(name: &str) -> Option<InlineKind> {
    Some(match name {
        "text" => InlineKind::Text,
        "linkify" => InlineKind::Linkify,
        "newline" => InlineKind::Newline,
        "escape" => InlineKind::Escape,
        "backticks" => InlineKind::Backticks,
        "strikethrough" => InlineKind::Strikethrough,
        "emphasis" => InlineKind::Emphasis,
        "link" => InlineKind::Link,
        "image" => InlineKind::Image,
        "autolink" => InlineKind::Autolink,
        "html_inline" => InlineKind::HtmlInline,
        "entity" => InlineKind::Entity,
        _ => return None,
    })
}

fn inline2_kind_by_name(name: &str) -> Option<Inline2Kind> {
    Some(match name {
        "balance_pairs" => Inline2Kind::BalancePairs,
        "strikethrough" => Inline2Kind::Strikethrough,
        "emphasis" => Inline2Kind::Emphasis,
        "fragments_join" => Inline2Kind::FragmentsJoin,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// ParserBlock
// ---------------------------------------------------------------------------

#[pyclass(name = "ParserBlock", unsendable)]
pub struct PyParserBlock {
    pub(crate) inner: Rc<RefCell<markdown_it_rust_core::rules_block::ParserBlock>>,
    host: Rc<PyHost>,
}

impl PyParserBlock {
    pub fn new(inner: Rc<RefCell<markdown_it_rust_core::rules_block::ParserBlock>>, host: Rc<PyHost>) -> Self {
        Self { inner, host }
    }

    pub fn rc(&self) -> Rc<RefCell<markdown_it_rust_core::rules_block::ParserBlock>> {
        self.inner.clone()
    }
}

#[pymethods]
impl PyParserBlock {
    #[getter]
    fn get_ruler(&self, py: Python) -> PyResult<Py<PyRuler>> {
        let _ = py;
        Py::new(py, PyRuler::new(RulerTarget::Block(self.inner.clone()), self.host.clone()))
    }

    fn tokenize(&self, state: &Bound<PyStateBlock>, start: usize, end: usize) -> PyResult<()> {
        let host_dyn = self.host.handle();
        let rules = self.inner.borrow_mut().default_chain();
        markdown_it_rust_core::rules_block::tokenize_with(
            &rules,
            &state.borrow().shared(),
            start,
            end,
            &host_dyn,
        );
        Ok(())
    }

    fn parse(
        &self,
        py: Python,
        src: String,
        md: &Bound<PyMarkdownIt>,
        env: &Bound<PyDict>,
        out_tokens: Vec<Bound<PyToken>>,
    ) -> PyResult<Option<Vec<Py<PyToken>>>> {
        if src.is_empty() {
            return Ok(None);
        }
        let (options, host) = {
            let m = md.borrow();
            (m.options_rc(), m.host_rc())
        };
        let mut env_data = Default::default();
        env_from_py(py, &mut env_data, env);
        let toks = out_tokens.into_iter().map(|t| t.borrow().shared()).collect();
        let code_enabled = self.inner.borrow().ruler.is_active("code");
        let host_dyn = host.handle();
        let state = Rc::new(RefCell::new(
            markdown_it_rust_core::rules_block::StateBlock::new(
                &src, options, host_dyn.clone(), env_data, toks, code_enabled,
            ),
        ));
        let rules = self.inner.borrow_mut().default_chain();
        let end = state.borrow().line_max;
        markdown_it_rust_core::rules_block::tokenize_with(&rules, &state, 0, end, &host_dyn);
        let input_keys: Vec<String> = env.iter().filter_map(|(k, _)| k.extract().ok()).collect();
        let st = state.borrow();
        env_to_user(py, &st.env, env, &input_keys);
        Ok(Some(
            st.tokens
                .clone()
                .into_iter()
                .map(|t| Py::new(py, PyToken::wrap(t)).unwrap())
                .collect(),
        ))
    }
}

// ---------------------------------------------------------------------------
// ParserInline
// ---------------------------------------------------------------------------

#[pyclass(name = "ParserInline", unsendable)]
pub struct PyParserInline {
    pub(crate) inner: Rc<RefCell<markdown_it_rust_core::rules_inline::ParserInline>>,
    host: Rc<PyHost>,
    terminator_cache: RefCell<Option<Py<PyAny>>>,
}

impl PyParserInline {
    pub fn new(inner: Rc<RefCell<markdown_it_rust_core::rules_inline::ParserInline>>, host: Rc<PyHost>) -> Self {
        Self { inner, host, terminator_cache: RefCell::new(None) }
    }

    pub fn rc(&self) -> Rc<RefCell<markdown_it_rust_core::rules_inline::ParserInline>> {
        self.inner.clone()
    }

    fn build_terminator_re(&self, py: Python) -> PyResult<Py<PyAny>> {
        // Exact pattern via Python `re` (same escaping as the original).
        let re = py.import("re")?;
        let mut chars: Vec<String> = markdown_it_rust_core::rules_inline::text::default_terminators()
            .into_iter()
            .map(|c| c.to_string())
            .collect();
        let mut extras: Vec<String> = self.inner.borrow().extra_terminators.iter().map(|c| c.to_string()).collect();
        chars.append(&mut extras);
        chars.sort();
        let joined: String = chars.concat();
        let escaped: String = re.call_method1("escape", (joined,))?.extract()?;
        let pattern = format!("[{escaped}]");
        Ok(re.call_method1("compile", (pattern,))?.unbind().into_any())
    }
}

#[pymethods]
impl PyParserInline {
    #[getter]
    fn get_ruler(&self, py: Python) -> PyResult<Py<PyRuler>> {
        let _ = py;
        Py::new(py, PyRuler::new(RulerTarget::InlineMain(self.inner.clone()), self.host.clone()))
    }

    #[getter]
    fn get_ruler2(&self, py: Python) -> PyResult<Py<PyRuler>> {
        let _ = py;
        Py::new(py, PyRuler::new(RulerTarget::Inline2(self.inner.clone()), self.host.clone()))
    }

    #[getter]
    fn get_terminator_re(&self, py: Python) -> PyResult<Py<PyAny>> {
        if let Some(cached) = self.terminator_cache.borrow().as_ref().map(|o| o.clone_ref(py)) {
            return Ok(cached);
        }
        let compiled = self.build_terminator_re(py)?;
        *self.terminator_cache.borrow_mut() = Some(compiled.clone_ref(py));
        Ok(compiled)
    }

    #[getter]
    fn get__extra_terminator_chars(&self, py: Python) -> PyResult<Py<PyAny>> {
        let set = pyo3::types::PySet::empty(py)?;
        for c in &self.inner.borrow().extra_terminators {
            set.add(c.to_string())?;
        }
        Ok(set.into_any().unbind())
    }

    fn add_terminator_char(&self, py: Python, ch: Bound<PyAny>) -> PyResult<()> {
        let s: String = ch.extract()?;
        let mut chars = s.chars();
        let c = chars.next().ok_or_else(|| PyTypeError::new_err("expected a single character"))?;
        if chars.next().is_some() {
            return Err(PyTypeError::new_err("expected a single character"));
        }
        let is_new = !markdown_it_rust_core::rules_inline::text::is_default_terminator(c)
            && !self.inner.borrow().extra_terminators.contains(&c);
        if is_new {
            self.inner.borrow_mut().add_terminator_char(c);
            // Rebuild the cached pattern (verbatim: new object).
            let compiled = self.build_terminator_re(py)?;
            *self.terminator_cache.borrow_mut() = Some(compiled);
        }
        Ok(())
    }

    fn skipToken(&self, state: &Bound<PyStateInline>) -> PyResult<()> {
        let host_dyn = self.host.handle();
        self.inner.borrow_mut().skip_token(&state.borrow().shared(), &host_dyn);
        Ok(())
    }

    fn tokenize(&self, state: &Bound<PyStateInline>) -> PyResult<()> {
        let host_dyn = self.host.handle();
        self.inner.borrow_mut().tokenize(&state.borrow().shared(), &host_dyn);
        Ok(())
    }

    fn parse(
        &self,
        py: Python,
        src: String,
        md: &Bound<PyMarkdownIt>,
        env: &Bound<PyDict>,
        tokens: Vec<Bound<PyToken>>,
    ) -> PyResult<Vec<Py<PyToken>>> {
        let (options, _host) = {
            let m = md.borrow();
            (m.options_rc(), m.host_rc())
        };
        let mut env_data = Default::default();
        env_from_py(py, &mut env_data, env);
        let toks = tokens.into_iter().map(|t| t.borrow().shared()).collect();
        let host_dyn = self.host.handle();
        let state = Rc::new(RefCell::new(
            markdown_it_rust_core::rules_inline::StateInline::new(
                &src, options, host_dyn.clone(), env_data, toks,
            ),
        ));
        self.inner.borrow_mut().tokenize(&state, &host_dyn);
        let rules2 = self.inner.borrow_mut().ruler2.get_rules("");
        for rule in &rules2 {
            markdown_it_rust_core::rules_inline::run_inline2_rule(rule, &state, &host_dyn);
        }
        let input_keys: Vec<String> = env.iter().filter_map(|(k, _)| k.extract().ok()).collect();
        let st = state.borrow();
        env_to_user(py, &st.env, env, &input_keys);
        Ok(st
            .tokens
            .clone()
            .into_iter()
            .map(|t| Py::new(py, PyToken::wrap(t)).unwrap())
            .collect())
    }
}

// ---------------------------------------------------------------------------
// ParserCore
// ---------------------------------------------------------------------------

#[pyclass(name = "ParserCore", unsendable)]
pub struct PyParserCore {
    pub(crate) inner: Rc<RefCell<markdown_it_rust_core::rules_core::ParserCore>>,
    host: Rc<PyHost>,
}

impl PyParserCore {
    pub fn new(inner: Rc<RefCell<markdown_it_rust_core::rules_core::ParserCore>>, host: Rc<PyHost>) -> Self {
        Self { inner, host }
    }

    pub fn rc(&self) -> Rc<RefCell<markdown_it_rust_core::rules_core::ParserCore>> {
        self.inner.clone()
    }
}

#[pymethods]
impl PyParserCore {
    #[getter]
    fn get_ruler(&self, py: Python) -> PyResult<Py<PyRuler>> {
        let _ = py;
        Py::new(py, PyRuler::new(RulerTarget::Core(self.inner.clone()), self.host.clone()))
    }

    fn process(&self, state: &Bound<PyStateCore>) -> PyResult<()> {
        let host_dyn = self.host.handle();
        self.inner.borrow_mut().process(&state.borrow().shared(), &host_dyn);
        Ok(())
    }
}
