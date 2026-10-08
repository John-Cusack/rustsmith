//! `Token` pyclass (mirror of `markdown_it/token.py`).
//!
//! Wraps the shared core handle; equality is value-based (order-insensitive
//! for `attrs`/`meta`, matching `dict ==`). `as_dict`/`from_dict` convert
//! the upstream `{}`/`None`/pairs shapes verbatim (map and attr pairs are
//! `[a, b]`/`[k, v]` *lists*, as `data_regression` pins).

use std::cell::RefCell;
use std::rc::Rc;

use markdown_it_rust_core::token::{shared_token, AttrVal, MetaVal, SharedToken};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::host_convert::{attr_val_from_py, attr_val_to_py, meta_val_from_py, meta_val_to_py};

#[pyclass(name = "Token", unsendable)]
pub struct PyToken {
    inner: SharedToken,
}

impl PyToken {
    pub fn shared(&self) -> SharedToken {
        self.inner.clone()
    }

    pub fn wrap(inner: SharedToken) -> Self {
        Self { inner }
    }

    fn convert_attrs(py: Python, attrs: &Bound<PyAny>) -> PyResult<Vec<(String, AttrVal)>> {
        if attrs.is_none() {
            return Ok(Vec::new());
        }
        if let Ok(dict) = attrs.downcast::<PyDict>() {
            let mut out = Vec::new();
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                out.push((key, attr_val_from_py(py, &v)?));
            }
            return Ok(out);
        }
        if let Ok(list) = attrs.downcast::<PyList>() {
            // Upstream form: list of [key, value] lists.
            let mut out = Vec::new();
            for item in list.iter() {
                let pair: Vec<Bound<PyAny>> = item.extract()?;
                if pair.len() != 2 {
                    return Err(PyTypeError::new_err("attr pair must be [key, value]"));
                }
                let key: String = pair[0].extract()?;
                out.push((key, attr_val_from_py(py, &pair[1])?));
            }
            return Ok(out);
        }
        Err(PyTypeError::new_err("attrs must be a dict, a list of pairs, or None"))
    }

    fn convert_meta(py: Python, meta: &Bound<PyAny>) -> PyResult<Vec<(String, MetaVal)>> {
        if meta.is_none() {
            return Ok(Vec::new());
        }
        let dict = meta
            .downcast::<PyDict>()
            .map_err(|_| PyTypeError::new_err("meta must be a dict or None"))?;
        let mut out = Vec::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            out.push((key, meta_val_from_py(py, &v)?));
        }
        Ok(out)
    }

    fn from_core_dict(
        py: Python,
        item: &Bound<PyAny>,
    ) -> PyResult<markdown_it_rust_core::Token> {
        let dict = item
            .downcast::<PyDict>()
            .map_err(|_| PyTypeError::new_err("from_dict() requires a mapping"))?;
        let get = |k: &str| -> PyResult<Bound<PyAny>> {
            dict.get_item(k)?
                .ok_or_else(|| PyTypeError::new_err(format!("missing key {k:?}")))
        };
        let mut token = markdown_it_rust_core::Token::new(
            &get("type")?.extract::<String>()?,
            &get("tag")?.extract::<String>()?,
            get("nesting")?.extract::<i8>()?,
        );
        token.attrs = Self::convert_attrs(py, &get("attrs")?)?;
        let map = get("map")?;
        if !map.is_none() {
            token.map = Some(extract_map(&map)?);
        }
        token.level = get("level")?.extract()?;
        let children = get("children")?;
        if !children.is_none() {
            let mut out = Vec::new();
            for sub in children
                .downcast::<PyList>()
                .map_err(|_| PyTypeError::new_err("children must be a list or None"))?
                .iter()
            {
                if let Ok(tok) = sub.downcast::<PyToken>() {
                    out.push(tok.borrow().shared());
                } else {
                    out.push(shared_token(Self::from_core_dict(py, &sub)?));
                }
            }
            token.children = Some(out);
        }
        token.content = get("content")?.extract()?;
        token.markup = get("markup")?.extract()?;
        token.info = get("info")?.extract()?;
        token.meta = Self::convert_meta(py, &get("meta")?)?;
        token.block = get("block")?.extract()?;
        token.hidden = get("hidden")?.extract()?;
        Ok(token)
    }
}

#[pymethods]
impl PyToken {
    #[new]
    #[pyo3(signature = (r#type, tag, nesting, attrs=None, map=None, level=0, children=None, content=None, markup=None, info=None, meta=None, block=false, hidden=false))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn new(
        py: Python,
        r#type: String,
        tag: String,
        nesting: i8,
        attrs: Option<Bound<PyAny>>,
        map: Option<Bound<PyAny>>,
        level: usize,
        children: Option<Bound<PyAny>>,
        content: Option<String>,
        markup: Option<String>,
        info: Option<String>,
        meta: Option<Bound<PyAny>>,
        block: bool,
        hidden: bool,
    ) -> PyResult<Self> {
        let content = content.unwrap_or_default();
        let markup = markup.unwrap_or_default();
        let info = info.unwrap_or_default();
        let mut token = markdown_it_rust_core::Token::new(&r#type, &tag, nesting);
        if let Some(a) = attrs {
            token.attrs = Self::convert_attrs(py, &a)?;
        }
        if let Some(m) = map {
            if !m.is_none() {
                token.map = Some(extract_map(&m)?);
            }
        }
        token.level = level;
        if let Some(c) = children {
            if !c.is_none() {
                let list = c.downcast::<PyList>().map_err(|_| {
                    PyTypeError::new_err("children must be a list of Token or None")
                })?;
                let mut out = Vec::new();
                for item in list.iter() {
                    let t: Bound<PyToken> = item.downcast_into()?;
                    out.push(t.borrow().shared());
                }
                token.children = Some(out);
            }
        }
        token.content = content;
        token.markup = markup;
        token.info = info;
        if let Some(m) = meta {
            token.meta = Self::convert_meta(py, &m)?;
        }
        token.block = block;
        token.hidden = hidden;
        Ok(Self { inner: shared_token(token) })
    }

    fn __repr__(&self) -> String {
        let t = self.inner.borrow();
        format!(
            "Token(type={:?}, tag={:?}, nesting={:?}, attrs={:?}, map={:?}, level={:?}, children={:?}, content={:?}, markup={:?}, info={:?}, meta={:?}, block={:?}, hidden={:?})",
            t.typ,
            t.tag,
            t.nesting,
            t.attrs,
            t.map,
            t.level,
            t.children.as_ref().map(|c| c.len()),
            t.content,
            t.markup,
            t.info,
            t.meta,
            t.block,
            t.hidden
        )
    }

    fn __eq__(&self, _py: Python, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(o) = other.downcast::<PyToken>() {
            return Ok(self.inner.borrow().token_eq(&o.borrow().inner.borrow()));
        }
        Ok(false)
    }

    fn __ne__(&self, py: Python, other: &Bound<PyAny>) -> PyResult<bool> {
        Ok(!self.__eq__(py, other)?)
    }

    fn __hash__(&self) -> PyResult<isize> {
        Err(PyTypeError::new_err("unhashable type: 'Token'"))
    }

    #[getter]
    fn get_type(&self) -> String {
        self.inner.borrow().typ.clone()
    }

    #[setter]
    fn set_type(&self, v: String) {
        self.inner.borrow_mut().typ = v;
    }

    #[getter]
    fn get_tag(&self) -> String {
        self.inner.borrow().tag.clone()
    }

    #[setter]
    fn set_tag(&self, v: String) {
        self.inner.borrow_mut().tag = v;
    }

    #[getter]
    fn get_nesting(&self) -> i8 {
        self.inner.borrow().nesting
    }

    #[setter]
    fn set_nesting(&self, v: i8) {
        self.inner.borrow_mut().nesting = v;
    }

    #[getter]
    fn get_attrs(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        for (k, v) in self.inner.borrow().attrs.clone() {
            dict.set_item(k, attr_val_to_py(py, &v)?)?;
        }
        Ok(dict.into_any().unbind())
    }

    #[setter]
    fn set_attrs(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        let attrs = Self::convert_attrs(py, &v)?;
        self.inner.borrow_mut().attrs = attrs;
        Ok(())
    }

    #[getter]
    fn get_map(&self, py: Python) -> PyResult<Py<PyAny>> {
        match self.inner.borrow().map {
            Some((a, b)) => Ok(vec![a, b].into_pyobject(py)?.into_any().unbind()),
            None => Ok(py.None()),
        }
    }

    #[setter]
    fn set_map(&self, v: Bound<PyAny>) -> PyResult<()> {
        if v.is_none() {
            self.inner.borrow_mut().map = None;
            return Ok(());
        }
        // Verbatim: plain `[a, b]` lists (or tuples).
        self.inner.borrow_mut().map = Some(extract_map(&v)?);
        Ok(())
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
    fn get_children(&self, py: Python) -> PyResult<Option<Vec<Py<PyToken>>>> {
        Ok(self.inner.borrow().children.clone().map(|kids| {
            kids.into_iter()
                .map(|t| Py::new(py, PyToken::wrap(t)).unwrap())
                .collect()
        }))
    }

    #[setter]
    fn set_children(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        let _ = py;
        if v.is_none() {
            self.inner.borrow_mut().children = None;
            return Ok(());
        }
        let list = v.downcast::<PyList>().map_err(|_| {
            PyTypeError::new_err("children must be a list of Token or None")
        })?;
        let mut out = Vec::new();
        for item in list.iter() {
            let t: Bound<PyToken> = item.downcast_into()?;
            out.push(t.borrow().shared());
        }
        self.inner.borrow_mut().children = Some(out);
        Ok(())
    }

    #[getter]
    fn get_content(&self) -> String {
        self.inner.borrow().content.clone()
    }

    #[setter]
    fn set_content(&self, v: String) {
        self.inner.borrow_mut().content = v;
    }

    #[getter]
    fn get_markup(&self) -> String {
        self.inner.borrow().markup.clone()
    }

    #[setter]
    fn set_markup(&self, v: String) {
        self.inner.borrow_mut().markup = v;
    }

    #[getter]
    fn get_info(&self) -> String {
        self.inner.borrow().info.clone()
    }

    #[setter]
    fn set_info(&self, v: String) {
        self.inner.borrow_mut().info = v;
    }

    #[getter]
    fn get_meta(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        for (k, v) in self.inner.borrow().meta.clone() {
            dict.set_item(k, meta_val_to_py(py, &v)?)?;
        }
        Ok(dict.into_any().unbind())
    }

    #[setter]
    fn set_meta(&self, py: Python, v: Bound<PyAny>) -> PyResult<()> {
        // Verbatim: assignment replaces the whole mapping.
        let meta = Self::convert_meta(py, &v)?;
        self.inner.borrow_mut().meta = meta;
        Ok(())
    }

    #[getter]
    fn get_block(&self) -> bool {
        self.inner.borrow().block
    }

    #[setter]
    fn set_block(&self, v: bool) {
        self.inner.borrow_mut().block = v;
    }

    #[getter]
    fn get_hidden(&self) -> bool {
        self.inner.borrow().hidden
    }

    #[setter]
    fn set_hidden(&self, v: bool) {
        self.inner.borrow_mut().hidden = v;
    }

    fn attrIndex(&self, py: Python, name: &str) -> PyResult<i64> {
        emit_user_warning(
            py,
            "Token.attrIndex should not be used, since Token.attrs is a dictionary",
        )?;
        Ok(self.inner.borrow().attr_index(name))
    }

    fn attrItems(&self, py: Python) -> PyResult<Vec<(String, Py<PyAny>)>> {
        let mut out = Vec::new();
        for (k, v) in self.inner.borrow().attrs.clone() {
            out.push((k, attr_val_to_py(py, &v)?));
        }
        Ok(out)
    }

    fn attrPush(&self, py: Python, data: &Bound<PyAny>) -> PyResult<()> {
        let pair: Vec<Bound<PyAny>> = data.extract()?;
        if pair.len() != 2 {
            return Err(PyTypeError::new_err("attrPush requires [name, value]"));
        }
        let name: String = pair[0].extract()?;
        let v = attr_val_from_py(py, &pair[1])?;
        self.inner.borrow_mut().attr_push(&name, v);
        Ok(())
    }

    fn attrSet(&self, py: Python, name: &str, value: Bound<PyAny>) -> PyResult<()> {
        let v = attr_val_from_py(py, &value)?;
        self.inner.borrow_mut().attr_set(name, v);
        Ok(())
    }

    fn attrGet(&self, py: Python, name: &str) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .borrow()
            .attr_get(name)
            .map(|v| attr_val_to_py(py, &v))
            .transpose()
    }

    fn attrJoin(&self, name: &str, value: &str) -> PyResult<()> {
        self.inner
            .borrow_mut()
            .attr_join(name, value)
            .map_err(PyTypeError::new_err)
    }

    #[pyo3(signature = (**kwargs))]
    fn copy(&self, py: Python, kwargs: Option<&Bound<PyDict>>) -> PyResult<Py<PyToken>> {
        // Shallow copy with optional field overrides (`dc.replace` verbatim).
        let mut token = self.inner.borrow().clone();
        if let Some(kw) = kwargs {
            for (k, v) in kw.iter() {
                let key: String = k.extract()?;
                match key.as_str() {
                    "type" => token.typ = v.extract()?,
                    "tag" => token.tag = v.extract()?,
                    "nesting" => token.nesting = v.extract()?,
                    "attrs" => token.attrs = Self::convert_attrs(py, &v)?,
                    "map" => {
                        token.map = if v.is_none() { None } else { Some(extract_map(&v)?) };
                    }
                    "level" => token.level = v.extract()?,
                    "children" => {
                        token.children = if v.is_none() {
                            None
                        } else {
                            let list =
                                v.downcast::<PyList>().map_err(|_| {
                                    PyTypeError::new_err("children must be a list of Token or None")
                                })?;
                            let mut out = Vec::new();
                            for item in list.iter() {
                                let t: Bound<PyToken> = item.downcast_into()?;
                                out.push(t.borrow().shared());
                            }
                            Some(out)
                        };
                    }
                    "content" => token.content = v.extract()?,
                    "markup" => token.markup = v.extract()?,
                    "info" => token.info = v.extract()?,
                    "meta" => token.meta = Self::convert_meta(py, &v)?,
                    "block" => token.block = v.extract()?,
                    "hidden" => token.hidden = v.extract()?,
                    other => {
                        return Err(PyTypeError::new_err(format!("unexpected field {other:?}")));
                    }
                }
            }
        }
        Py::new(py, PyToken { inner: Rc::new(RefCell::new(token)) })
    }

    #[pyo3(signature = (*, children=true, as_upstream=true, meta_serializer=None, filter=None, dict_factory=None))]
    #[allow(clippy::too_many_arguments)]
    fn as_dict(
        &self,
        py: Python,
        children: bool,
        as_upstream: bool,
        meta_serializer: Option<Bound<PyAny>>,
        filter: Option<Bound<PyAny>>,
        dict_factory: Option<Bound<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        // Verbatim construction: ordered pairs, then `dict_factory(items)`,
        // then an optional second factory call over filtered items.
        let t = self.inner.borrow();
        let mut items: Vec<(String, Bound<PyAny>)> = Vec::new();
        items.push(("type".to_string(), t.typ.clone().into_pyobject(py)?.into_any()));
        items.push(("tag".to_string(), t.tag.clone().into_pyobject(py)?.into_any()));
        items.push(("nesting".to_string(), t.nesting.into_pyobject(py)?.into_any()));
        if as_upstream {
            if t.attrs.is_empty() {
                items.push(("attrs".to_string(), py.None().into_bound(py)));
            } else {
                // Upstream shape: list of [key, value] lists.
                let pairs = PyList::empty(py);
                for (k, v) in &t.attrs {
                    pairs.append(vec![
                        k.clone().into_pyobject(py)?.into_any(),
                        attr_val_to_py(py, v)?.bind(py).clone(),
                    ])?;
                }
                items.push(("attrs".to_string(), pairs.into_any()));
            }
        } else {
            let ad = PyDict::new(py);
            for (k, v) in &t.attrs {
                ad.set_item(k, attr_val_to_py(py, v)?)?;
            }
            items.push(("attrs".to_string(), ad.into_any()));
        }
        match t.map {
            Some((a, b)) => items.push((
                "map".to_string(),
                vec![a.into_pyobject(py)?.into_any(), b.into_pyobject(py)?.into_any()]
                    .into_pyobject(py)?
                    .into_any(),
            )),
            None => items.push(("map".to_string(), py.None().into_bound(py))),
        }
        items.push(("level".to_string(), t.level.into_pyobject(py)?.into_any()));
        if children {
            if let Some(kids) = &t.children {
                let out = PyList::empty(py);
                for k in kids {
                    let child = Py::new(py, PyToken::wrap(k.clone()))?;
                    let kwargs = PyDict::new(py);
                    kwargs.set_item("children", true)?;
                    kwargs.set_item("as_upstream", as_upstream)?;
                    if let Some(f) = &filter {
                        kwargs.set_item("filter", f.clone())?;
                    }
                    if let Some(f) = &dict_factory {
                        kwargs.set_item("dict_factory", f.clone())?;
                    }
                    if let Some(f) = &meta_serializer {
                        kwargs.set_item("meta_serializer", f.clone())?;
                    }
                    out.append(child.bind(py).call_method("as_dict", (), Some(&kwargs))?)?;
                }
                items.push(("children".to_string(), out.into_any()));
            } else {
                items.push(("children".to_string(), py.None().into_bound(py)));
            }
        } else if let Some(kids) = &t.children {
            let list = PyList::empty(py);
            for k in kids {
                list.append(Py::new(py, PyToken::wrap(k.clone()))?)?;
            }
            items.push(("children".to_string(), list.into_any()));
        } else {
            items.push(("children".to_string(), py.None().into_bound(py)));
        }
        items.push((
            "content".to_string(),
            t.content.clone().into_pyobject(py)?.into_any(),
        ));
        items.push(("markup".to_string(), t.markup.clone().into_pyobject(py)?.into_any()));
        items.push(("info".to_string(), t.info.clone().into_pyobject(py)?.into_any()));
        if let Some(ser) = &meta_serializer {
            let md = PyDict::new(py);
            for (k, v) in &t.meta {
                md.set_item(k, meta_val_to_py(py, v)?)?;
            }
            items.push(("meta".to_string(), ser.call1((md,))?));
        } else {
            let md = PyDict::new(py);
            for (k, v) in &t.meta {
                md.set_item(k, meta_val_to_py(py, v)?)?;
            }
            items.push(("meta".to_string(), md.into_any()));
        }
        items.push(("block".to_string(), pyo3::types::PyBool::new(py, t.block).to_owned().into_any()));
        items.push(("hidden".to_string(), pyo3::types::PyBool::new(py, t.hidden).to_owned().into_any()));
        drop(t);
        fn build<'py>(
            py: Python<'py>,
            items: &[Bound<'py, PyAny>],
            dict_factory: &Option<Bound<'py, PyAny>>,
        ) -> PyResult<Bound<'py, PyDict>> {
            // Items arrive flattened as [k0, v0, k1, v1, ...].
            let pairs = PyList::empty(py);
            let mut it = items.iter();
            while let (Some(k), Some(v)) = (it.next(), it.next()) {
                pairs.append(vec![k.clone(), v.clone()])?;
            }
            if let Some(f) = dict_factory {
                Ok(f.call1((pairs,))?.downcast_into()?)
            } else {
                let d = PyDict::new(py);
                let mut it = items.iter();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    let key: String = k.extract()?;
                    d.set_item(key, v)?;
                }
                Ok(d)
            }
        }
        // Flatten pairs for the factory call.
        let mut flat: Vec<Bound<PyAny>> = Vec::new();
        for (k, v) in &items {
            flat.push(k.clone().into_pyobject(py)?.into_any());
            flat.push(v.clone());
        }
        let mut mapping = build(py, &flat, &dict_factory)?;
        if let Some(f) = &filter {
            let mut kept: Vec<Bound<PyAny>> = Vec::new();
            for (k, v) in mapping.iter() {
                if f.call1((k.clone(), v.clone()))?.extract::<bool>()? {
                    kept.push(k);
                    kept.push(v);
                }
            }
            mapping = build(py, &kept, &dict_factory)?;
        }
        Ok(mapping.into_any().unbind())
    }

    #[classmethod]
    fn from_dict(
        _cls: &Bound<pyo3::types::PyType>,
        py: Python,
        dct: Bound<PyAny>,
    ) -> PyResult<Py<PyToken>> {
        // Verbatim: `cls(**dct)` with children converted recursively.
        let token = Self::from_core_dict(py, &dct)?;
        Py::new(py, PyToken { inner: shared_token(token) })
    }
}

/// `map` accepts `[line_begin, line_end]` lists (verbatim) or tuples.
fn extract_map(v: &Bound<PyAny>) -> PyResult<(usize, usize)> {
    let seq: Vec<usize> = v.extract()?;
    if seq.len() != 2 {
        return Err(PyTypeError::new_err("map must be a [line_begin, line_end] pair"));
    }
    Ok((seq[0], seq[1]))
}

pub fn emit_user_warning(py: Python, msg: &str) -> PyResult<()> {
    PyErr::warn(py, &py.get_type::<pyo3::exceptions::PyUserWarning>(), std::ffi::CString::new(msg).unwrap().as_c_str(), 2)
}
