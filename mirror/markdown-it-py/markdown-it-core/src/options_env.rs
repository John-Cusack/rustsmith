//! Parser options and `env` data (mirror of `OptionsDict` storage in
//! `markdown_it/utils.py` plus the `env` contracts in `main.py`).
//!
//! Validation lives in the binding's `OptionsDict` pyclass (it must raise
//! the exact Python errors); this module is the validated plain-data store
//! shared through `Rc<RefCell<..>>` so mid-parse option writes are visible
//! to nested parses.

use std::collections::HashMap;

use crate::{DuplicateRef, Reference};

/// Quote pairs for smartquotes: a 4-char string or a 4-list.
#[derive(Debug, Clone)]
pub enum QuotesVal {
    Str(String),
    List(Vec<String>),
}

impl QuotesVal {
    pub fn at(&self, idx: usize) -> String {
        match self {
            QuotesVal::Str(s) => s.chars().nth(idx).map(|c| c.to_string()).unwrap_or_default(),
            QuotesVal::List(v) => v.get(idx).cloned().unwrap_or_default(),
        }
    }
}

/// Validated parser options. Known fields are typed; every other key
/// (e.g. `tasklists_editable`, `tasklists`, `alerts`,
/// `strikethrough_single_tilde`, `store_labels`, `inline_definitions`,
/// plugin keys) rides `extras`.
#[derive(Debug, Clone)]
pub struct OptionsData {
    pub max_nesting: i64,
    pub html: bool,
    pub linkify: bool,
    pub typographer: bool,
    pub quotes: QuotesVal,
    pub xhtml_out: bool,
    pub breaks: bool,
    pub lang_prefix: String,
    /// `highlight` callable registry id (`None` when unset/None).
    pub highlight: Option<u64>,
    pub extras: HashMap<String, ExtraVal>,
}

/// Option value kinds for known slots (highlight excluded: registry id).
#[derive(Debug, Clone, PartialEq)]
pub enum ExtraVal {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Null,
}

impl Default for OptionsData {
    fn default() -> Self {
        Self {
            max_nesting: 20,
            html: true,
            linkify: false,
            typographer: false,
            quotes: QuotesVal::Str("“”‘’".to_string()),
            xhtml_out: true,
            breaks: false,
            lang_prefix: "language-".to_string(),
            highlight: None,
            extras: HashMap::new(),
        }
    }
}

impl OptionsData {
    /// Python-truthiness read of an extra key (verbatim `.get(key,
    /// default)` use-sites: `1`, `"false"`, and other truthy values count).
    pub fn extra_bool(&self, key: &str, default: bool) -> bool {
        match self.extras.get(key) {
            Some(v) => extra_truthy(v).unwrap_or(default),
            None => default,
        }
    }
}

/// Live `env` mapping threaded through parse/render. `references` and
/// `duplicate_refs` are first-class (link/reference rules); any other
/// JSON-shaped plugin key rides `extra`, and non-JSON plugin values ride
/// the binding side-table (see `Host::env_snapshot`).
#[derive(Debug, Clone, Default)]
pub struct EnvData {
    pub references: HashMap<String, Reference>,
    pub duplicate_refs: Vec<DuplicateRef>,
    pub extra: HashMap<String, EnvValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnvValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<EnvValue>),
    Map(Vec<(String, EnvValue)>),
}

/// Python truthiness over mirrored extra values.
fn extra_truthy(v: &ExtraVal) -> Option<bool> {
    match v {
        ExtraVal::Bool(b) => Some(*b),
        ExtraVal::Int(i) => Some(*i != 0),
        ExtraVal::Float(f) => Some(*f != 0.0),
        ExtraVal::Str(s) => Some(!s.is_empty()),
        ExtraVal::Null => Some(false),
    }
}
