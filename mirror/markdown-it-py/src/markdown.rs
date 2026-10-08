//! `MarkdownIt` pyclass (mirror of `markdown_it/main.py`).
//!
//! Owns the three core parsers, the options, the linkify cell, the renderer,
//! and the shared [`PyHost`]. Chainable methods (`use`, `enable`,
//! `disable`, `configure`) return the *same* object verbatim. Presets arrive
//! as plain dicts from the verbatim-Python `presets` shims.

use std::cell::RefCell;
use std::rc::Rc;

use markdown_it_rust_core::SharedOptions;
use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use crate::host::PyHost;
use crate::host_convert::{env_from_py, env_to_user};
use crate::options::PyOptionsDict;
use crate::parsers::{PyParserBlock, PyParserCore, PyParserInline};
use crate::renderer::PyRendererHTML;
use crate::states::PyStateCore;
use crate::token::PyToken;

const PRESET_NAMES: &[&str] = &["default", "js-default", "zero", "commonmark", "gfm-like", "gfm-like2"];

#[pyclass(name = "MarkdownIt", unsendable)]
pub struct PyMarkdownIt {
    options_obj: Py<PyOptionsDict>,
    options: SharedOptions,
    linkify: Rc<RefCell<Option<Py<PyAny>>>>,
    highlight: Rc<RefCell<Option<Py<PyAny>>>>,
    block: Py<PyParserBlock>,
    inline: Py<PyParserInline>,
    core: Py<PyParserCore>,
    renderer: Py<PyAny>,
    host: Rc<PyHost>,
    utils_mod: Py<PyAny>,
    helpers_mod: Py<PyAny>,
}

impl PyMarkdownIt {
    pub fn options_rc(&self) -> SharedOptions {
        self.options.clone()
    }

    pub fn host_rc(&self) -> Rc<PyHost> {
        self.host.clone()
    }

    pub fn shallow_clone(&self, py: Python) -> Self {
        Self {
            options_obj: self.options_obj.clone_ref(py),
            options: self.options.clone(),
            linkify: self.linkify.clone(),
            highlight: self.highlight.clone(),
            block: self.block.clone_ref(py),
            inline: self.inline.clone_ref(py),
            core: self.core.clone_ref(py),
            renderer: self.renderer.clone_ref(py),
            host: self.host.clone(),
            utils_mod: self.utils_mod.clone_ref(py),
            helpers_mod: self.helpers_mod.clone_ref(py),
        }
    }

    fn apply_rule_op(
        &self,
        py: Python,
        names: &Bound<PyAny>,
        ignore_invalid: bool,
        op: &str,
    ) -> PyResult<Vec<String>> {
        let list: Vec<String> = if let Ok(s) = names.extract::<String>() {
            vec![s]
        } else {
            names.extract()?
        };
        // Verbatim: per-chain calls ignore invalid names; only the final
        // missed check honors `ignoreInvalid`.
        let mut result = Vec::new();
        for chain in ["core", "block", "inline"] {
            let parser = self.get_chain(py, chain)?;
            let ruler = parser.getattr("ruler")?;
            let found: Vec<String> =
                ruler.call_method1(op, (list.clone(), true))?.extract()?;
            result.extend(found);
        }
        let ruler2 = self.inline.bind(py).getattr("ruler2")?;
        let found: Vec<String> =
            ruler2.call_method1(op, (list.clone(), true))?.extract()?;
        result.extend(found);
        let missed: Vec<String> = list.into_iter().filter(|n| !result.contains(n)).collect();
        if !missed.is_empty() && !ignore_invalid {
            return Err(PyValueError::new_err(format!(
                "MarkdownIt. Failed to {op} unknown rule(s): {missed:?}"
            )));
        }
        Ok(result)
    }

    fn get_chain<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        match name {
            "inline" => Ok(self.inline.bind(py).clone().into_any()),
            "block" => Ok(self.block.bind(py).clone().into_any()),
            "core" => Ok(self.core.bind(py).clone().into_any()),
            "renderer" => Ok(self.renderer.bind(py).clone().into_any()),
            _ => Err(PyKeyError::new_err(name.to_string())),
        }
    }

    fn checked_env<'py>(&self, py: Python<'py>, env: Option<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyDict>> {
        match env {
            None => Ok(PyDict::new(py)),
            Some(e) if e.is_none() => Ok(PyDict::new(py)),
            Some(e) => {
                // Verbatim `isinstance(env, MutableMapping)`.
                let collections = py.import("collections.abc")?;
                let mapping: Bound<pyo3::types::PyType> =
                    collections.getattr("MutableMapping")?.downcast_into()?;
                if !e.is_instance(&mapping)? {
                    return Err(PyTypeError::new_err(format!(
                        "Input data should be a MutableMapping, not {e:?}"
                    )));
                }
                e.downcast_into::<PyDict>()
                    .map_err(|_| PyTypeError::new_err("env must be a dict"))
            }
        }
    }
}

impl PyMarkdownIt {
    fn apply_config(
        &self,
        py: Python,
        presets: Bound<PyAny>,
        options_update: Option<Bound<PyAny>>,
    ) -> PyResult<Py<PyOptionsDict>> {
        // Verbatim `configure` (preset dicts come from the shims). Returns
        // the fresh options object; callers swap the visible field.
        let config: Bound<PyDict> = if let Ok(name) = presets.extract::<String>() {
            if !PRESET_NAMES.contains(&name.as_str()) {
                return Err(PyKeyError::new_err(format!(
                    "Wrong `markdown-it` preset '{name}', check name"
                )));
            }
            let presets_mod = py.import("markdown_it.presets")?;
            presets_mod.call_method1("get_preset", (name,))?.downcast_into::<PyDict>()?
        } else {
            match presets.downcast_into::<PyDict>() {
                Ok(d) => d,
                Err(_) => {
                    return Err(PyValueError::new_err("Wrong `markdown-it` config, can't be empty"));
                }
            }
        };
        if config.len() == 0 {
            return Err(PyValueError::new_err("Wrong `markdown-it` config, can't be empty"));
        }
        let mut options: Bound<PyDict> = match config.get_item("options")? {
            Some(o) => o.downcast_into()?,
            None => PyDict::new(py),
        };
        if let Some(update) = options_update {
            if !update.is_none() {
                let is_mapping = if update.downcast::<PyDict>().is_ok() {
                    true
                } else {
                    let collections = py.import("collections.abc")?;
                    let mapping: Bound<pyo3::types::PyType> =
                        collections.getattr("Mapping")?.downcast_into()?;
                    update.is_instance(&mapping)?
                };
                if !is_mapping {
                    return Err(PyTypeError::new_err(format!(
                        "options_update should be a mapping: {}",
                        update.repr()?
                    )));
                }
                // Verbatim `{**options, **options_update}` (options_update wins).
                let merged = PyDict::new(py);
                for (k, v) in options.iter() {
                    merged.set_item(k, v)?;
                }
                if let Ok(d) = update.downcast::<PyDict>() {
                    for (k, v) in d.iter() {
                        merged.set_item(k, v)?;
                    }
                } else {
                    merged.call_method1("update", (update.clone(),))?;
                }
                options = merged;
            }
        }
        // `self.set(options)` verbatim: fresh dict, reset mirror, re-mirror.
        // The visible `options_obj` swaps at the end (returned to caller).
        let fresh_out: Py<PyOptionsDict>;
        {
            let dict = PyDict::new(py);
            for (k, v) in options.iter() {
                dict.set_item(k, v)?;
            }
            let fresh = Py::new(
                py,
                PyOptionsDict::new_with(self.options.clone(), dict.unbind(), self.highlight.clone()),
            )?;
            let keys: Vec<String> = fresh
                .bind(py)
                .call_method0("keys")?
                .try_iter()?
                .filter_map(|k| k.ok()?.extract().ok())
                .collect();
            *self.options.borrow_mut() = Default::default();
            *self.highlight.borrow_mut() = None;
            for k in keys {
                let v: Bound<PyAny> = fresh.bind(py).call_method1("__getitem__", (k.clone(),))?;
                fresh.borrow(py).mirror_key(py, &k, &v);
            }
            fresh_out = fresh.clone_ref(py);
        }
        if let Ok(Some(components)) = config.get_item("components") {
            let components: Bound<PyDict> = components.downcast_into()?;
            for (name, component) in components.iter() {
                let name: String = name.extract()?;
                let component: Bound<PyDict> = component.downcast_into()?;
                match name.as_str() {
                    "core" => {
                        let parser = self.core.clone_ref(py);
                        if let Ok(Some(rules)) = component.get_item("rules") {
                            if !rules.is_none() {
                                let list: Vec<String> = rules.extract()?;
                                parser.bind(py).borrow_mut().inner.borrow_mut().ruler.enable_only(&list, false).map_err(PyKeyError::new_err)?;
                            }
                        }
                    }
                    "block" => {
                        let parser = self.block.clone_ref(py);
                        if let Ok(Some(rules)) = component.get_item("rules") {
                            if !rules.is_none() {
                                let list: Vec<String> = rules.extract()?;
                                parser.bind(py).borrow_mut().inner.borrow_mut().ruler.enable_only(&list, false).map_err(PyKeyError::new_err)?;
                            }
                        }
                    }
                    "inline" => {
                        let parser = self.inline.clone_ref(py);
                        if let Ok(Some(rules)) = component.get_item("rules") {
                            if !rules.is_none() {
                                let list: Vec<String> = rules.extract()?;
                                parser.bind(py).borrow_mut().inner.borrow_mut().ruler.enable_only(&list, false).map_err(PyKeyError::new_err)?;
                            }
                        }
                        if let Ok(Some(rules2)) = component.get_item("rules2") {
                            if !rules2.is_none() {
                                let list: Vec<String> = rules2.extract()?;
                                parser.bind(py).borrow_mut().inner.borrow_mut().ruler2.enable_only(&list, false).map_err(PyKeyError::new_err)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(fresh_out)
    }
}

#[pymethods]
impl PyMarkdownIt {
    #[new]
    #[pyo3(signature = (config=None, options_update=None, *, renderer_cls=None))]
    fn new(
        py: Python,
        config: Option<Bound<PyAny>>,
        options_update: Option<Bound<PyAny>>,
        renderer_cls: Option<Bound<PyAny>>,
    ) -> PyResult<Self> {
        // Verbatim default `config="commonmark"`.
        let config = match config {
            Some(c) if !c.is_none() => c,
            _ => pyo3::types::PyString::new(py, "commonmark").into_any(),
        };
        let options: SharedOptions = Rc::new(RefCell::new(Default::default()));
        let block_core = Rc::new(RefCell::new(markdown_it_rust_core::rules_block::ParserBlock::new()));
        let inline_core = Rc::new(RefCell::new(markdown_it_rust_core::rules_inline::ParserInline::new()));
        let core_core = Rc::new(RefCell::new(markdown_it_rust_core::rules_core::ParserCore::new()));
        let linkify: Rc<RefCell<Option<Py<PyAny>>>> = Rc::new(RefCell::new(None));
        let highlight: Rc<RefCell<Option<Py<PyAny>>>> = Rc::new(RefCell::new(None));
        let rules_dict = PyDict::new(py).unbind();
        let host = PyHost::new(
            py,
            block_core.clone(),
            inline_core.clone(),
            linkify.clone(),
            rules_dict,
            highlight.clone(),
        )?;
        // linkify-it-py, when installed (verbatim `main.__init__`).
        if let Ok(linkify_mod) = py.import("linkify_it") {
            if let Ok(cls) = linkify_mod.getattr("LinkifyIt") {
                if let Ok(obj) = cls.call0() {
                    *linkify.borrow_mut() = Some(obj.unbind());
                }
            }
        }
        let options_dict = PyDict::new(py);
        let options_obj = Py::new(
            py,
            PyOptionsDict::new_with(options.clone(), options_dict.unbind(), highlight.clone()),
        )?;
        let block = Py::new(py, PyParserBlock::new(block_core, host.clone()))?;
        let inline = Py::new(py, PyParserInline::new(inline_core, host.clone()))?;
        let core = Py::new(py, PyParserCore::new(core_core, host.clone()))?;
        // Renderer (default; a custom class swaps in below with `md`).
        // The host reads this same dict object: overrides land live.
        let renderer: Py<PyAny> = {
            let r = Py::new(
                py,
                PyRendererHTML::new_with(py, host.clone(), host.rules_dict.clone_ref(py))?,
            )?;
            PyRendererHTML::populate_rules(r.bind(py))?;
            r.into_any()
        };
        // Verbatim: `self.utils = common.utils`, `self.helpers = helpers`.
        let utils_mod = py.import("markdown_it.common.utils")?;
        let helpers_mod = py.import("markdown_it.helpers")?;
        let this = Self {
            options_obj,
            options,
            linkify,
            highlight,
            block,
            inline,
            core,
            renderer,
            host,
            utils_mod: utils_mod.into_any().unbind(),
            helpers_mod: helpers_mod.into_any().unbind(),
        };
        // `self.utils = utils; self.helpers = helpers` are plain attributes.
        let fresh = this.apply_config(py, config, options_update)?;
        let mut this = this;
        this.options_obj = fresh;
        let slf = Py::new(py, this)?;
        // Verbatim `renderer_cls(self)`: custom renderers receive the instance.
        if let Some(cls) = renderer_cls {
            if !cls.is_none() {
                let custom: Bound<PyAny> = cls.call1((slf.clone_ref(py),))?;
                slf.borrow_mut(py).renderer = custom.unbind();
            }
        }
        // Return the inner value (handles are shared with `slf`, which drops).
        let out = slf.borrow(py).shallow_clone(py);
        drop(slf);
        Ok(out)
    }

    fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
        // Verbatim: `{module}.{qualname}()`.
        let cls = slf.get_type();
        let module: String = cls.getattr("__module__")?.extract()?;
        let name: String = cls.getattr("__qualname__")?.extract()?;
        Ok(format!("{module}.{name}()"))
    }

    #[getter]
    fn get_utils(&self, py: Python) -> Py<PyAny> {
        self.utils_mod.clone_ref(py)
    }

    #[getter]
    fn get_helpers(&self, py: Python) -> Py<PyAny> {
        self.helpers_mod.clone_ref(py)
    }

    #[getter]
    fn get_options(&self, py: Python) -> Py<PyOptionsDict> {
        self.options_obj.clone_ref(py)
    }

    #[getter]
    fn get_linkify(&self, py: Python) -> Option<Py<PyAny>> {
        self.linkify.borrow().as_ref().map(|o| o.clone_ref(py))
    }

    #[setter]
    fn set_linkify(&self, v: Bound<PyAny>) -> PyResult<()> {
        let v = if v.is_none() { None } else { Some(v) };
        *self.linkify.borrow_mut() = v.filter(|o| !o.is_none()).map(|o| o.unbind());
        Ok(())
    }

    #[getter]
    fn get_inline(&self, py: Python) -> Py<PyParserInline> {
        self.inline.clone_ref(py)
    }

    #[getter]
    fn get_block(&self, py: Python) -> Py<PyParserBlock> {
        self.block.clone_ref(py)
    }

    #[getter]
    fn get_core(&self, py: Python) -> Py<PyParserCore> {
        self.core.clone_ref(py)
    }

    #[getter]
    fn get_renderer(&self, py: Python) -> Py<PyAny> {
        self.renderer.clone_ref(py)
    }

    #[setter]
    fn set_renderer(&mut self, v: Py<PyAny>) {
        self.renderer = v;
    }

    fn __getitem__(&self, py: Python, name: &str) -> PyResult<Py<PyAny>> {
        Ok(self.get_chain(py, name)?.unbind())
    }

    fn set(slf: &Bound<Self>, py: Python, options: Bound<PyAny>) -> PyResult<()> {
        // Verbatim: replace the whole OptionsDict.
        let dict = PyDict::new(py);
        if let Ok(d) = options.downcast::<PyDict>() {
            for (k, v) in d.iter() {
                dict.set_item(k, v)?;
            }
        } else {
            dict.call_method1("update", (options.clone(),))?;
        }
        let this = slf.borrow();
        let fresh = Py::new(
            py,
            PyOptionsDict::new_with(this.options.clone(), dict.unbind(), this.highlight.clone()),
        )?;
        let keys: Vec<String> = fresh
            .bind(py)
            .call_method0("keys")?
            .try_iter()?
            .filter_map(|k| k.ok()?.extract().ok())
            .collect();
        *this.options.borrow_mut() = Default::default();
        *this.highlight.borrow_mut() = None;
        for k in keys {
            let v: Bound<PyAny> = fresh.bind(py).call_method1("__getitem__", (k.clone(),))?;
            fresh.borrow(py).mirror_key(py, &k, &v);
        }
        drop(this);
        slf.borrow_mut().options_obj = fresh;
        Ok(())
    }


    #[pyo3(signature = (presets, options_update=None))]
    fn configure<'py>(
        slf: &'py Bound<'py, Self>,
        py: Python<'py>,
        presets: Bound<'py, PyAny>,
        options_update: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, Self>> {
        let fresh = slf.borrow().apply_config(py, presets, options_update)?;
        slf.borrow_mut().options_obj = fresh;
        Ok(slf.clone())
    }

    fn get_all_rules(&self, py: Python) -> PyResult<Py<PyAny>> {
        let d = PyDict::new(py);
        d.set_item("core", self.core.bind(py).getattr("ruler")?.call_method0("get_all_rules")?)?;
        d.set_item("block", self.block.bind(py).getattr("ruler")?.call_method0("get_all_rules")?)?;
        d.set_item("inline", self.inline.bind(py).getattr("ruler")?.call_method0("get_all_rules")?)?;
        d.set_item("inline2", self.inline.bind(py).getattr("ruler2")?.call_method0("get_all_rules")?)?;
        Ok(d.into_any().unbind())
    }

    fn get_active_rules(&self, py: Python) -> PyResult<Py<PyAny>> {
        let d = PyDict::new(py);
        d.set_item("core", self.core.bind(py).getattr("ruler")?.call_method0("get_active_rules")?)?;
        d.set_item("block", self.block.bind(py).getattr("ruler")?.call_method0("get_active_rules")?)?;
        d.set_item("inline", self.inline.bind(py).getattr("ruler")?.call_method0("get_active_rules")?)?;
        d.set_item("inline2", self.inline.bind(py).getattr("ruler2")?.call_method0("get_active_rules")?)?;
        Ok(d.into_any().unbind())
    }

    #[allow(non_snake_case)]
    #[pyo3(signature = (names, ignoreInvalid=false))]
    fn enable<'py>(slf: &'py Bound<'py, Self>, py: Python<'py>, names: Bound<'py, PyAny>, ignoreInvalid: bool) -> PyResult<Bound<'py, Self>> {
        slf.borrow().apply_rule_op(py, &names, ignoreInvalid, "enable")?;
        Ok(slf.clone())
    }

    #[allow(non_snake_case)]
    #[pyo3(signature = (names, ignoreInvalid=false))]
    fn disable<'py>(slf: &'py Bound<'py, Self>, py: Python<'py>, names: Bound<'py, PyAny>, ignoreInvalid: bool) -> PyResult<Bound<'py, Self>> {
        slf.borrow().apply_rule_op(py, &names, ignoreInvalid, "disable")?;
        Ok(slf.clone())
    }

    fn reset_rules(slf: &Bound<Self>, py: Python) -> PyResult<Py<PyAny>> {
        // Verbatim context manager: snapshot active rules, restore on exit.
        let saved_any = slf.borrow().get_active_rules(py)?;
        let saved: Bound<PyDict> = saved_any.bind(py).clone().downcast_into()?;
        let guard = Py::new(py, PyResetGuard { md: slf.clone().unbind(), saved: saved.unbind() })?;
        Ok(guard.into_any())
    }

    #[pyo3(signature = (name, function, fmt="html"))]
    fn add_render_rule(&self, py: Python, name: String, function: Bound<PyAny>, fmt: &str) -> PyResult<()> {
        // Verbatim: only when `renderer.__output__ == fmt`.
        let output: String = self.renderer.bind(py).getattr("__output__")?.extract()?;
        if output != fmt {
            return Ok(());
        }
        let bound = function.call_method1("__get__", (self.renderer.bind(py),))?;
        self.host.rules_dict.bind(py).set_item(name, bound)?;
        Ok(())
    }

    #[pyo3(signature = (plugin, *params, **options))]
    fn r#use<'py>(
        slf: &'py Bound<'py, Self>,
        py: Python<'py>,
        plugin: Bound<'py, PyAny>,
        params: &Bound<'py, PyTuple>,
        options: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, Self>> {
        let mut args: Vec<Bound<PyAny>> = vec![slf.clone().into_any()];
        for p in params.iter() {
            args.push(p);
        }
        let args = PyTuple::new(py, args)?;
        match options {
            Some(kw) => {
                plugin.call(args, Some(kw))?;
            }
            None => {
                plugin.call1(args)?;
            }
        }
        Ok(slf.clone())
    }

    #[pyo3(signature = (src, env=None))]
    fn parse(slf: &Bound<Self>, py: Python, src: Bound<PyAny>, env: Option<Bound<PyAny>>) -> PyResult<Vec<Py<PyToken>>> {
        let src: String = src.extract().map_err(|_| {
            PyTypeError::new_err(format!("Input data should be a string, not {src:?}"))
        })?;
        let this = slf.borrow();
        let env_dict = this.checked_env(py, env)?;
        this.host.set_current_md(Some(slf.clone().into_any().unbind()));
        let result = this.parse_inner(py, &src, &env_dict);
        // Panic payloads become the verbatim Python errors.
        let mapped = result.map_err(|e| map_panic(py, e));
        this.host.set_current_md(None);
        mapped
    }

    #[pyo3(signature = (src, env=None))]
    fn render(slf: &Bound<Self>, py: Python, src: Bound<PyAny>, env: Option<Bound<PyAny>>) -> PyResult<Py<PyAny>> {
        // One env dict shared between parse and render (verbatim).
        let src_s: String = src.extract().map_err(|_| {
            PyTypeError::new_err(format!("Input data should be a string, not {src:?}"))
        })?;
        let this = slf.borrow();
        let env_dict = this.checked_env(py, env)?;
        this.host.set_current_md(Some(slf.clone().into_any().unbind()));
        let tokens = this.parse_inner(py, &src_s, &env_dict).map_err(|e| map_panic(py, e))?;
        let out = this.render_tokens(py, tokens, &env_dict);
        this.host.set_current_md(None);
        out
    }

    #[pyo3(signature = (src, env=None))]
    fn parseInline(slf: &Bound<Self>, py: Python, src: Bound<PyAny>, env: Option<Bound<PyAny>>) -> PyResult<Vec<Py<PyToken>>> {
        let src_s: String = src.extract().map_err(|_| {
            PyTypeError::new_err(format!("Input data should be a string, not {src:?}"))
        })?;
        let this = slf.borrow();
        let env_dict = this.checked_env(py, env)?;
        this.host.set_current_md(Some(slf.clone().into_any().unbind()));
        let result = this.parse_inline_inner(py, &src_s, &env_dict).map_err(|e| map_panic(py, e));
        this.host.set_current_md(None);
        result
    }

    #[pyo3(signature = (src, env=None))]
    fn renderInline(slf: &Bound<Self>, py: Python, src: Bound<PyAny>, env: Option<Bound<PyAny>>) -> PyResult<Py<PyAny>> {
        let src_s: String = src.extract().map_err(|_| {
            PyTypeError::new_err(format!("Input data should be a string, not {src:?}"))
        })?;
        let this = slf.borrow();
        let env_dict = this.checked_env(py, env)?;
        this.host.set_current_md(Some(slf.clone().into_any().unbind()));
        let tokens = this.parse_inline_inner(py, &src_s, &env_dict).map_err(|e| map_panic(py, e))?;
        let out = this.render_tokens(py, tokens, &env_dict);
        this.host.set_current_md(None);
        out
    }

    fn validateLink(&self, py: Python, url: String) -> PyResult<bool> {
        let _ = py;
        Ok(self.host.handle().validate_link(&url, None))
    }

    fn normalizeLink(&self, py: Python, url: String) -> PyResult<String> {
        let _ = py;
        Ok(self.host.handle().normalize_link(&url))
    }

    fn normalizeLinkText(&self, py: Python, link: String) -> PyResult<String> {
        let _ = py;
        Ok(self.host.handle().normalize_link_text(&link))
    }
}

impl PyMarkdownIt {
    fn parse_inner(
        &self,
        py: Python,
        src: &str,
        env_dict: &Bound<PyDict>,
    ) -> Result<Vec<Py<PyToken>>, PanicPayload> {
        let input_keys: Vec<String> =
            env_dict.iter().filter_map(|(k, _)| k.extract().ok()).collect();
        let mut env_data = Default::default();
        env_from_py(py, &mut env_data, env_dict);
        let host_dyn = self.host.handle();
        let state = Rc::new(RefCell::new(markdown_it_rust_core::rules_core::StateCore::new(
            src,
            self.options.clone(),
            host_dyn.clone(),
            env_data,
            Vec::new(),
        )));
        let wrapper = Py::new(py, PyStateCore::wrap(state.clone(), &self.host))
            .map_err(PanicPayload::Py)?;
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.core.bind(py).borrow().inner.borrow_mut().process(&wrapper.borrow(py).shared(), &host_dyn);
        }));
        if let Err(payload) = r {
            return Err(PanicPayload::Core(payload));
        }
        let st = state.borrow();
        env_to_user(py, &st.env, env_dict, &input_keys);
        Ok(st
            .tokens
            .clone()
            .into_iter()
            .map(|t| Py::new(py, PyToken::wrap(t)).unwrap())
            .collect())
    }

    fn parse_inline_inner(
        &self,
        py: Python,
        src: &str,
        env_dict: &Bound<PyDict>,
    ) -> Result<Vec<Py<PyToken>>, PanicPayload> {
        let input_keys: Vec<String> =
            env_dict.iter().filter_map(|(k, _)| k.extract().ok()).collect();
        let mut env_data = Default::default();
        env_from_py(py, &mut env_data, env_dict);
        let host_dyn = self.host.handle();
        let state = Rc::new(RefCell::new(markdown_it_rust_core::rules_core::StateCore::new(
            src,
            self.options.clone(),
            host_dyn.clone(),
            env_data,
            Vec::new(),
        )));
        state.borrow_mut().inline_mode = true;
        let wrapper = Py::new(py, PyStateCore::wrap(state.clone(), &self.host))
            .map_err(PanicPayload::Py)?;
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.core.bind(py).borrow().inner.borrow_mut().process(&wrapper.borrow(py).shared(), &host_dyn);
        }));
        if let Err(payload) = r {
            return Err(PanicPayload::Core(payload));
        }
        let st = state.borrow();
        env_to_user(py, &st.env, env_dict, &input_keys);
        Ok(st
            .tokens
            .clone()
            .into_iter()
            .map(|t| Py::new(py, PyToken::wrap(t)).unwrap())
            .collect())
    }

    fn render_tokens(
        &self,
        py: Python,
        tokens: Vec<Py<PyToken>>,
        env_dict: &Bound<PyDict>,
    ) -> PyResult<Py<PyAny>> {
        // Always dispatch through the loaded renderer (default or custom).
        let list = PyList::empty(py);
        for t in &tokens {
            list.append(t)?;
        }
        let options = self.options_obj.clone_ref(py);
        self.renderer.bind(py).call_method1("render", (list, options, env_dict.clone()))
            .map(|v| v.unbind())
    }
}

/// Panic payloads crossing the parse boundary.
enum PanicPayload {
    Py(PyErr),
    Core(Box<dyn std::any::Any + Send>),
}

/// Map core panic payloads to the verbatim Python errors.
fn map_panic(_py: Python, payload: PanicPayload) -> PyErr {
    match payload {
        PanicPayload::Py(e) => e,
        PanicPayload::Core(any) => {
            if let Some(missing) = any.downcast_ref::<markdown_it_rust_core::rules_inline::linkify::LinkifyMissing>() {
                let _ = missing;
                return pyo3::exceptions::PyModuleNotFoundError::new_err(
                    "Linkify enabled but not installed.",
                );
            }
            if let Some(te) = any.downcast_ref::<markdown_it_rust_core::HostTypeError>() {
                return PyTypeError::new_err(te.0.clone());
            }
            if let Some(ue) = any.downcast_ref::<markdown_it_rust_core::HostUnicodeError>() {
                return pyo3::exceptions::PyUnicodeError::new_err(ue.0.clone());
            }
            pyo3::exceptions::PyRuntimeError::new_err("internal error")
        }
    }
}

/// `reset_rules` context manager (verbatim save/restore).
#[pyclass(name = "ResetRulesGuard", unsendable)]
pub struct PyResetGuard {
    md: Py<PyMarkdownIt>,
    saved: Py<PyDict>,
}

#[pymethods]
impl PyResetGuard {
    fn __enter__(slf: &Bound<Self>) -> PyResult<Py<PyMarkdownIt>> {
        Ok(slf.borrow().md.clone_ref(slf.py()))
    }

    fn __exit__(
        &self,
        py: Python,
        _exc_type: Bound<PyAny>,
        _exc: Bound<PyAny>,
        _tb: Bound<PyAny>,
    ) -> PyResult<bool> {
        // Restore each chain via enableOnly (verbatim; `inline2` last).
        let md = self.md.bind(py);
        let saved = self.saved.bind(py);
        for chain in ["core", "block", "inline"] {
            let rules: Vec<String> = saved.get_item(chain)?.unwrap().extract()?;
            let parser: Bound<PyAny> = md.call_method1("__getitem__", (chain,))?;
            parser.getattr("ruler")?.call_method1("enableOnly", (rules,))?;
        }
        let rules2: Vec<String> = saved.get_item("inline2")?.unwrap().extract()?;
        md.getattr("inline")?.getattr("ruler2")?.call_method1("enableOnly", (rules2,))?;
        Ok(false)
    }
}
