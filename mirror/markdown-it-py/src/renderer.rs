//! `RendererHTML` pyclass (mirror of `markdown_it/renderer.py:RendererHTML`).
//!
//! `rules` is a live Python dict populated with bound methods (verbatim
//! `__init__`); the core render loop consults it per token. Default rule
//! bodies run the core implementations; `image` mutates `alt` verbatim.

use std::cell::RefCell;
use std::rc::Rc;

use markdown_it_rust_core::render::RenderOpts;
use markdown_it_rust_core::SharedOptions;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::host::PyHost;
use crate::token::PyToken;

const RULE_NAMES: &[&str] = &[
    "list_item_open",
    "code_inline",
    "code_block",
    "fence",
    "image",
    "hardbreak",
    "softbreak",
    "text",
    "html_block",
    "html_inline",
];

#[pyclass(name = "RendererHTML", unsendable, subclass)]
pub struct PyRendererHTML {
    #[pyo3(get)]
    rules: Py<PyDict>,
    host: Rc<PyHost>,
}

impl PyRendererHTML {
    pub fn new_with(py: Python, host: Rc<PyHost>, rules: Py<PyDict>) -> PyResult<Self> {
        let _ = py;
        Ok(Self { rules, host })
    }

    /// Populate `rules` with bound methods (verbatim `__init__`).
    ///
    /// The ten defaults resolve through `getattr`, so subclass overrides
    /// win automatically; remaining `dir()` members that are Python methods
    /// (subclass additions like `strong_open`) are picked up via an
    /// `inspect.ismethod` scan, mirroring `inspect.getmembers`.
    pub fn populate_rules(slf: &Bound<Self>) -> PyResult<()> {
        let rules: Bound<PyDict> = slf.getattr("rules")?.downcast_into()?;
        for name in RULE_NAMES {
            if !rules.contains(name)? {
                rules.set_item(name, slf.getattr(name)?)?;
            }
        }
        let py = slf.py();
        let inspect = py.import("inspect")?;
        for name in slf.dir()? {
            let name: String = name.extract()?;
            if name.starts_with("render") || name.starts_with('_') {
                continue;
            }
            if rules.contains(&name)? {
                continue;
            }
            let obj = slf.getattr(&name)?;
            let is_method: bool = inspect.call_method1("ismethod", (obj.clone(),))?.extract()?;
            if is_method {
                rules.set_item(name, obj)?;
            }
        }
        Ok(())
    }

    /// Mirror instance rules into the host view (additive; never removes
    /// `add_render_rule` entries).
    fn sync_rules(&self, py: Python) {
        let host_dict = self.host.rules_dict.bind(py);
        for (k, v) in self.rules.bind(py).iter() {
            host_dict.set_item(k, v).ok();
        }
    }

    fn core_tokens(&self, tokens: &Bound<PyAny>) -> PyResult<Vec<markdown_it_rust_core::SharedToken>> {
        let mut out = Vec::new();
        for item in tokens.try_iter()? {
            let t: Bound<PyToken> = item?.downcast_into()?;
            out.push(t.borrow().shared());
        }
        Ok(out)
    }
}

#[pymethods]
impl PyRendererHTML {
    #[new]
    #[pyo3(signature = (parser=None))]
    fn new(py: Python, parser: Option<Bound<PyAny>>) -> PyResult<Self> {
        // A standalone renderer (verbatim `__init__(self, parser=None)`).
        // Without a parser there is no host: rules still populate, but
        // rendering falls back to pure-Python method calls below.
        let _ = parser;
        let rules = PyDict::new(py);
        // Standalone host (MarkdownIt wires the shared one; see `new_with`).
        let block = Rc::new(RefCell::new(markdown_it_rust_core::rules_block::ParserBlock::new()));
        let inline = Rc::new(RefCell::new(markdown_it_rust_core::rules_inline::ParserInline::new()));
        let linkify = Rc::new(RefCell::new(None));
        let highlight = Rc::new(RefCell::new(None));
        let rules_dict = rules.clone().unbind();
        let host = PyHost::new(py, block, inline, linkify, rules_dict, highlight)?;
        Ok(Self { rules: rules.unbind(), host })
    }

    #[getter]
    fn get___output__(&self, py: Python) -> PyResult<Py<PyAny>> {
        let _ = py;
        Ok(pyo3::types::PyString::new(py, "html").into_any().unbind())
    }

    fn render(slf: &Bound<Self>, py: Python, tokens: Bound<PyAny>, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        // Instances built outside MarkdownIt (subclasses, custom classes):
        // populate defaults + added methods, then mirror into the host view
        // (both idempotent, so per-render cost is one dict scan).
        PyRendererHTML::populate_rules(slf)?;
        let this = slf.borrow();
        this.sync_rules(py);
        let toks = this.core_tokens(&tokens)?;
        // Resolve options: prefer the live shared handle when the caller
        // passes our OptionsDict, else snapshot the mapping.
        let (opts, shared) = this.resolve_options(py, &options)?;
        let env_data = crate::host_convert::env_snapshot_to_core(py, &env)?;
        let host_dyn = this.host.handle();
        Ok(markdown_it_rust_core::render::render(&toks, &opts, &env_data, &host_dyn, &shared))
    }

    fn renderInline(slf: &Bound<Self>, py: Python, tokens: Bound<PyAny>, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        PyRendererHTML::populate_rules(slf)?;
        let this = slf.borrow();
        this.sync_rules(py);
        let toks = this.core_tokens(&tokens)?;
        let (opts, shared) = this.resolve_options(py, &options)?;
        let env_data = crate::host_convert::env_snapshot_to_core(py, &env)?;
        let host_dyn = this.host.handle();
        Ok(markdown_it_rust_core::render::render_inline(&toks, &opts, &env_data, &host_dyn, &shared))
    }

    fn renderToken(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, _env: Bound<PyAny>) -> PyResult<String> {
        let toks = self.core_tokens(&tokens)?;
        let (opts, _) = self.resolve_options(py, &options)?;
        Ok(markdown_it_rust_core::render::render_token(&toks, idx, &opts))
    }

    fn renderInlineAsText(&self, py: Python, tokens: Bound<PyAny>, options: Bound<PyAny>, _env: Bound<PyAny>) -> PyResult<String> {
        let toks = self.core_tokens(&tokens)?;
        let (opts, _) = self.resolve_options(py, &options)?;
        Ok(markdown_it_rust_core::render::render_inline_as_text(&toks, &opts))
    }

    fn renderAttrs(&self, py: Python, token: &Bound<PyToken>) -> PyResult<String> {
        let _ = py;
        Ok(markdown_it_rust_core::render::render_attrs(&token.borrow().shared().borrow()))
    }

    // Default rule bodies (bound-method targets in `rules`).

    fn list_item_open(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "list_item_open", tokens, idx, options, env)
    }

    fn code_inline(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "code_inline", tokens, idx, options, env)
    }

    fn code_block(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "code_block", tokens, idx, options, env)
    }

    fn fence(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "fence", tokens, idx, options, env)
    }

    fn image(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "image", tokens, idx, options, env)
    }

    fn hardbreak(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "hardbreak", tokens, idx, options, env)
    }

    fn softbreak(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "softbreak", tokens, idx, options, env)
    }

    fn text(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "text", tokens, idx, options, env)
    }

    fn html_block(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "html_block", tokens, idx, options, env)
    }

    fn html_inline(&self, py: Python, tokens: Bound<PyAny>, idx: usize, options: Bound<PyAny>, env: Bound<PyAny>) -> PyResult<String> {
        self.dispatch(py, "html_inline", tokens, idx, options, env)
    }
}

impl PyRendererHTML {
    /// Run the core default rule body for `name` (used by the bound
    /// methods above; the core loop itself only calls Python overrides).
    fn dispatch(
        &self,
        py: Python,
        name: &str,
        tokens: Bound<PyAny>,
        idx: usize,
        options: Bound<PyAny>,
        env: Bound<PyAny>,
    ) -> PyResult<String> {
        let toks = self.core_tokens(&tokens)?;
        let (opts, _) = self.resolve_options(py, &options)?;
        let env_data = crate::host_convert::env_snapshot_to_core(py, &env)?;
        let host_dyn = self.host.handle();
        let out = match name {
            "list_item_open" => markdown_it_rust_core::render::rule_list_item_open(&toks, idx, &opts, &host_dyn),
            "code_inline" => markdown_it_rust_core::render::rule_code_inline(&toks, idx, &opts),
            "code_block" => markdown_it_rust_core::render::rule_code_block(&toks, idx, &opts),
            "fence" => markdown_it_rust_core::render::rule_fence(&toks, idx, &opts, &host_dyn),
            "image" => markdown_it_rust_core::render::rule_image(&toks, idx, &opts, &env_data, &host_dyn),
            "hardbreak" => markdown_it_rust_core::render::rule_hardbreak(&toks, idx, &opts),
            "softbreak" => markdown_it_rust_core::render::rule_softbreak(&toks, idx, &opts),
            "text" => markdown_it_rust_core::render::rule_text(&toks, idx, &opts),
            "html_block" => markdown_it_rust_core::render::rule_html_block(&toks, idx, &opts),
            "html_inline" => markdown_it_rust_core::render::rule_html_inline(&toks, idx, &opts),
            _ => markdown_it_rust_core::render::render_token(&toks, idx, &opts),
        };
        Ok(out)
    }

    /// Resolve render options: the live shared handle when possible.
    fn resolve_options(
        &self,
        py: Python,
        options: &Bound<PyAny>,
    ) -> PyResult<(RenderOpts, SharedOptions)> {
        use crate::options::PyOptionsDict;
        if let Ok(o) = options.downcast::<PyOptionsDict>() {
            let shared = o.borrow().shared();
            let shared2 = shared.clone();
            let b = shared.borrow();
            return Ok((
                RenderOpts {
                    xhtml_out: b.xhtml_out,
                    breaks: b.breaks,
                    lang_prefix: b.lang_prefix.clone(),
                    tasklists_editable: b.extra_bool("tasklists_editable", false),
                },
                shared2,
            ));
        }
        // Snapshot a foreign mapping into a mirrored handle so overrides
        // observe the same values (verbatim attribute reads).
        let get_bool = |k: &str| -> bool {
            options.getattr(k).ok().and_then(|v| v.is_truthy().ok()).unwrap_or(false)
        };
        let get_str = |k: &str| -> Option<String> {
            options.getattr(k).ok()?.extract().ok()
        };
        let get_int = |k: &str| -> Option<i64> {
            options.getattr(k).ok()?.extract().ok()
        };
        let shared: SharedOptions = Rc::new(RefCell::new(Default::default()));
        {
            let mut o = shared.borrow_mut();
            if let Some(v) = get_int("maxNesting") {
                o.max_nesting = v;
            }
            o.html = get_bool("html");
            o.linkify = get_bool("linkify");
            o.typographer = get_bool("typographer");
            if let Some(s) = get_str("quotes") {
                o.quotes = markdown_it_rust_core::options_env::QuotesVal::Str(s);
            }
            o.xhtml_out = get_bool("xhtmlOut");
            o.breaks = get_bool("breaks");
            if let Some(s) = get_str("langPrefix") {
                o.lang_prefix = s;
            }
            let _ = py;
        }
        let b = shared.borrow();
        let shared2 = shared.clone();
        Ok((
            RenderOpts {
                xhtml_out: b.xhtml_out,
                breaks: b.breaks,
                lang_prefix: b.lang_prefix.clone(),
                tasklists_editable: options
                    .call_method1("get", ("tasklists_editable", false))
                    .ok()
                    .and_then(|v| v.is_truthy().ok())
                    .unwrap_or(false),
            },
            shared2,
        ))
    }
}
