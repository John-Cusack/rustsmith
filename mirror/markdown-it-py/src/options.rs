//! `OptionsDict` pyclass (mirror of `markdown_it/utils.py:OptionsDict`).
//!
//! A plain dict with attribute access. `dict` is the truth; validated
//! scalars mirror into the shared core options at set time through Python
//! truthiness (matching use-site semantics). Non-scalar values (e.g. a
//! `highlight` callable, arbitrary plugin keys) live in the dict only;
//! `highlight` additionally rides a cell shared with the host.

use std::cell::RefCell;
use std::rc::Rc;

use markdown_it_rust_core::options_env::{ExtraVal, QuotesVal};
use markdown_it_rust_core::SharedOptions;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict};

use crate::host_convert::extra_val_from_py;

#[pyclass(name = "OptionsDict", unsendable)]
pub struct PyOptionsDict {
    shared: SharedOptions,
    dict: Py<PyDict>,
    highlight: Rc<RefCell<Option<Py<PyAny>>>>,
}

impl PyOptionsDict {
    pub fn wrap(shared_in: SharedOptions) -> Self {
        // On-demand wrapper around a live core options handle (used for the
        // `options` argument of render-rule overrides). The dict view is
        // materialized from the mirror; the highlight cell is fresh (empty).
        let shared = shared_in.clone();
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            let o = shared.borrow();
            dict.set_item("maxNesting", o.max_nesting).ok();
            dict.set_item("html", o.html).ok();
            dict.set_item("linkify", o.linkify).ok();
            dict.set_item("typographer", o.typographer).ok();
            match &o.quotes {
                QuotesVal::Str(s) => {
                    dict.set_item("quotes", s.clone()).ok();
                }
                QuotesVal::List(v) => {
                    dict.set_item("quotes", v.clone()).ok();
                }
            }
            dict.set_item("xhtmlOut", o.xhtml_out).ok();
            dict.set_item("breaks", o.breaks).ok();
            dict.set_item("langPrefix", o.lang_prefix.clone()).ok();
            for (k, v) in &o.extras {
                let val = match v {
                    ExtraVal::Bool(b) => PyBool::new(py, *b).to_owned().into_any().unbind(),
                    ExtraVal::Int(i) => i.into_pyobject(py).unwrap().into_any().unbind(),
                    ExtraVal::Float(f) => f.into_pyobject(py).unwrap().into_any().unbind(),
                    ExtraVal::Str(s) => s.into_pyobject(py).unwrap().into_any().unbind(),
                    ExtraVal::Null => py.None(),
                };
                dict.set_item(k, val).ok();
            }
            Self { shared: shared.clone(), dict: dict.unbind(), highlight: Rc::new(RefCell::new(None)) }
        })
    }

    pub fn new_with(
        shared: SharedOptions,
        dict: Py<PyDict>,
        highlight: Rc<RefCell<Option<Py<PyAny>>>>,
    ) -> Self {
        Self { shared, dict, highlight }
    }

    pub fn shared(&self) -> SharedOptions {
        self.shared.clone()
    }

    pub fn dict<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        self.dict.bind(py).clone()
    }

    pub fn highlight_cell(&self) -> Rc<RefCell<Option<Py<PyAny>>>> {
        self.highlight.clone()
    }

    pub(crate) fn mirror_key(&self, py: Python, key: &str, value: &Bound<PyAny>) {
        self.mirror(py, key, value);
    }

    /// Mirror one key into core (best-effort; the dict is always exact).
    fn mirror(&self, py: Python, key: &str, value: &Bound<PyAny>) {
        let mut o = self.shared.borrow_mut();
        match key {
            "maxNesting" => {
                // Verbatim: no validation at set (`True` behaves as `1`);
                // non-ints keep the old mirror.
                if let Ok(i) = value.extract::<i64>() {
                    o.max_nesting = i;
                }
            }
            "html" => {
                if let Ok(b) = truthy(py, value) {
                    o.html = b;
                }
            }
            "linkify" => {
                if let Ok(b) = truthy(py, value) {
                    o.linkify = b;
                }
            }
            "typographer" => {
                if let Ok(b) = truthy(py, value) {
                    o.typographer = b;
                }
            }
            "quotes" => {
                if let Ok(s) = value.extract::<String>() {
                    o.quotes = QuotesVal::Str(s);
                } else if let Ok(seq) = value.extract::<Vec<String>>() {
                    o.quotes = QuotesVal::List(seq);
                }
            }
            "xhtmlOut" => {
                if let Ok(b) = truthy(py, value) {
                    o.xhtml_out = b;
                }
            }
            "breaks" => {
                if let Ok(b) = truthy(py, value) {
                    o.breaks = b;
                }
            }
            "langPrefix" => {
                if let Ok(s) = value.extract::<String>() {
                    o.lang_prefix = s;
                }
            }
            "highlight" => {
                *self.highlight.borrow_mut() =
                    if value.is_none() { None } else { Some(value.clone().unbind()) };
            }
            _ => {
                match extra_val_from_py(py, value) {
                    Ok(v) => {
                        o.extras.insert(key.to_string(), v);
                    }
                    Err(_) => {
                        o.extras.remove(key);
                    }
                }
            }
        }
    }
}

fn truthy(py: Python, v: &Bound<PyAny>) -> PyResult<bool> {
    let _ = py;
    v.is_truthy()
}

#[pymethods]
impl PyOptionsDict {
    #[new]
    fn new(py: Python, options: Bound<PyAny>) -> PyResult<Self> {
        let shared = Rc::new(RefCell::new(Default::default()));
        let dict = PyDict::new(py);
        // Verbatim `dict(options)`.
        if let Ok(d) = options.downcast::<PyDict>() {
            for (k, v) in d.iter() {
                dict.set_item(k, v)?;
            }
        } else {
            dict.call_method1("update", (options.clone(),))?;
        }
        let highlight = Rc::new(RefCell::new(None));
        let this = Self { shared, dict: dict.unbind(), highlight };
        // Mirror every initial key.
        let keys: Vec<String> = this
            .dict
            .bind(py)
            .keys()
            .iter()
            .filter_map(|k| k.extract().ok())
            .collect();
        for k in keys {
            if let Ok(Some(v)) = this.dict.bind(py).get_item(&k) {
                this.mirror(py, &k, &v);
            }
        }
        Ok(this)
    }

    fn __getitem__(&self, py: Python, key: String) -> PyResult<Py<PyAny>> {
        self.dict
            .bind(py)
            .get_item(&key)?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key))
            .map(|v| v.unbind())
    }

    fn __setitem__(&self, py: Python, key: String, value: Bound<PyAny>) -> PyResult<()> {
        self.mirror(py, &key, &value);
        self.dict.bind(py).set_item(&key, &value)?;
        Ok(())
    }

    fn __delitem__(&self, py: Python, key: String) -> PyResult<()> {
        self.dict.bind(py).del_item(&key)?;
        // Mirror falls back to the default for known keys.
        let mut o = self.shared.borrow_mut();
        match key.as_str() {
            "maxNesting" => o.max_nesting = 20,
            "html" => o.html = true,
            "linkify" => o.linkify = false,
            "typographer" => o.typographer = false,
            "quotes" => o.quotes = QuotesVal::Str("“”‘’".to_string()),
            "xhtmlOut" => o.xhtml_out = true,
            "breaks" => o.breaks = false,
            "langPrefix" => o.lang_prefix = "language-".to_string(),
            "highlight" => *self.highlight.borrow_mut() = None,
            _ => {
                o.extras.remove(&key);
            }
        }
        Ok(())
    }

    fn __iter__(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.dict.bind(py).call_method0("__iter__")?.unbind())
    }

    fn __len__(&self, py: Python) -> usize {
        self.dict.bind(py).len()
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        self.dict.bind(py).repr()?.extract()
    }

    fn __str__(&self, py: Python) -> PyResult<String> {
        self.dict.bind(py).str()?.extract()
    }

    fn __contains__(&self, py: Python, key: String) -> PyResult<bool> {
        self.dict.bind(py).contains(&key)
    }

    #[pyo3(signature = (key, default=None))]
    fn get(&self, py: Python, key: String, default: Option<Bound<PyAny>>) -> PyResult<Py<PyAny>> {
        match self.dict.bind(py).get_item(&key)? {
            Some(v) => Ok(v.unbind()),
            None => Ok(default.map(|d| d.unbind()).unwrap_or_else(|| py.None())),
        }
    }

    fn keys(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.dict.bind(py).call_method0("keys")?.unbind())
    }

    fn items(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.dict.bind(py).call_method0("items")?.unbind())
    }

    fn values(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.dict.bind(py).call_method0("values")?.unbind())
    }

    // Attribute access (verbatim properties).

    #[getter]
    fn get_maxNesting(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "maxNesting".to_string())
    }

    #[setter]
    fn set_maxNesting(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "maxNesting".to_string(), v)
    }

    #[getter]
    fn get_html(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "html".to_string())
    }

    #[setter]
    fn set_html(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "html".to_string(), v)
    }

    #[getter]
    fn get_linkify(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "linkify".to_string())
    }

    #[setter]
    fn set_linkify(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "linkify".to_string(), v)
    }

    #[getter]
    fn get_typographer(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "typographer".to_string())
    }

    #[setter]
    fn set_typographer(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "typographer".to_string(), v)
    }

    #[getter]
    fn get_quotes(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "quotes".to_string())
    }

    #[setter]
    fn set_quotes(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "quotes".to_string(), v)
    }

    #[getter]
    fn get_xhtmlOut(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "xhtmlOut".to_string())
    }

    #[setter]
    fn set_xhtmlOut(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "xhtmlOut".to_string(), v)
    }

    #[getter]
    fn get_breaks(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "breaks".to_string())
    }

    #[setter]
    fn set_breaks(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "breaks".to_string(), v)
    }

    #[getter]
    fn get_langPrefix(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "langPrefix".to_string())
    }

    #[setter]
    fn set_langPrefix(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "langPrefix".to_string(), v)
    }

    #[getter]
    fn get_highlight(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.__getitem__(py, "highlight".to_string())
    }

    #[setter]
    fn set_highlight(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        self.__setitem__(py, "highlight".to_string(), v)
    }
}
