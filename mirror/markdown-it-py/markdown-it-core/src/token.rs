//! `Token` (mirror of `markdown_it/token.py`).
//!
//! Mutable value type shared through [`SharedToken`] (`Rc<RefCell<..>>`) so
//! core rules and Python plugin code observe the same token stream. `attrs`
//! and `meta` stay insertion-ordered pair vecs (dict semantics: unique keys;
//! equality is order-insensitive, matching Python `dict ==`).

use std::cell::RefCell;
use std::rc::Rc;

/// Attribute value (`dict[str, str | int | float]` in Python).
#[derive(Debug, Clone, PartialEq)]
pub enum AttrVal {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// Metadata value (JSON-shaped subset of Python `meta` dicts).
#[derive(Debug, Clone, PartialEq)]
pub enum MetaVal {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

/// Shared token handle: the aliasing unit between core and Python.
pub type SharedToken = Rc<RefCell<Token>>;

pub fn shared_token(token: Token) -> SharedToken {
    Rc::new(RefCell::new(token))
}

#[derive(Debug, Clone)]
pub struct Token {
    pub typ: String,
    pub tag: String,
    pub nesting: i8,
    pub attrs: Vec<(String, AttrVal)>,
    pub map: Option<(usize, usize)>,
    pub level: usize,
    pub children: Option<Vec<SharedToken>>,
    pub content: String,
    pub markup: String,
    pub info: String,
    pub meta: Vec<(String, MetaVal)>,
    pub block: bool,
    pub hidden: bool,
}

impl Token {
    pub fn new(typ: &str, tag: &str, nesting: i8) -> Self {
        Self {
            typ: typ.to_string(),
            tag: tag.to_string(),
            nesting,
            attrs: Vec::new(),
            map: None,
            level: 0,
            children: None,
            content: String::new(),
            markup: String::new(),
            info: String::new(),
            meta: Vec::new(),
            block: false,
            hidden: false,
        }
    }

    /// Order-insensitive equality (Python dataclass `==` over dict fields).
    pub fn token_eq(&self, other: &Token) -> bool {
        self.typ == other.typ
            && self.tag == other.tag
            && self.nesting == other.nesting
            && unordered_eq(&self.attrs, &other.attrs)
            && self.map == other.map
            && self.level == other.level
            && children_eq(&self.children, &other.children)
            && self.content == other.content
            && self.markup == other.markup
            && self.info == other.info
            && unordered_eq(&self.meta, &other.meta)
            && self.block == other.block
            && self.hidden == other.hidden
    }

    pub fn attr_index(&self, name: &str) -> i64 {
        for (i, (k, _)) in self.attrs.iter().enumerate() {
            if k == name {
                return i as i64;
            }
        }
        -1
    }

    pub fn attr_get(&self, name: &str) -> Option<AttrVal> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    pub fn attr_set(&mut self, name: &str, value: AttrVal) {
        if let Some(slot) = self.attrs.iter_mut().find(|(k, _)| k == name) {
            slot.1 = value;
        } else {
            self.attrs.push((name.to_string(), value));
        }
    }

    pub fn attr_items(&self) -> &[(String, AttrVal)] {
        &self.attrs
    }

    pub fn attr_push(&mut self, name: &str, value: AttrVal) {
        self.attr_set(name, value);
    }

    /// `attrJoin`: appends through a space, creating the attr when missing.
    /// The `TypeError` message reproduces the original verbatim, including
    /// its literal `'name'` (upstream bug, pinned by parity).
    pub fn attr_join(&mut self, name: &str, value: &str) -> Result<(), String> {
        match self.attrs.iter_mut().find(|(k, _)| k == name) {
            Some((_, AttrVal::Str(cur))) => {
                cur.push(' ');
                cur.push_str(value);
                Ok(())
            }
            Some((_, other)) => Err(format!(
                "existing attr 'name' is not a str: {}",
                attr_val_repr(other)
            )),
            None => {
                self.attrs.push((name.to_string(), AttrVal::Str(value.to_string())));
                Ok(())
            }
        }
    }
}

fn attr_val_repr(v: &AttrVal) -> String {
    match v {
        AttrVal::Str(s) => s.clone(),
        AttrVal::Int(i) => i.to_string(),
        AttrVal::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{f:.1}")
            } else {
                format!("{f}")
            }
        }
        // Verbatim `str(value)`: Python `True`/`False`.
        AttrVal::Bool(b) => {
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
    }
}

fn unordered_eq<T: PartialEq>(a: &[(String, T)], b: &[(String, T)]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2))
}

fn children_eq(a: &Option<Vec<SharedToken>>, b: &Option<Vec<SharedToken>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            x.len() == y.len()
                && x.iter().zip(y.iter()).all(|(t1, t2)| t1.borrow().token_eq(&t2.borrow()))
        }
        _ => false,
    }
}

impl MetaVal {
    /// Python truthiness of a metadata value.
    pub fn truthy(&self) -> bool {
        match self {
            MetaVal::Null => false,
            MetaVal::Bool(b) => *b,
            MetaVal::Int(i) => *i != 0,
            MetaVal::Float(f) => *f != 0.0,
            MetaVal::Str(s) => !s.is_empty(),
        }
    }
}
