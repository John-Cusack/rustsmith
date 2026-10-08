//! Pure-Rust markdown-it core (Stage-1 mirror of `executablebooks/markdown-it-py`).
//!
//! No Python dependency: all dynamic seams (plugin rule callables, `mdurl`,
//! `linkify-it-py`, `highlight` callbacks, render-rule overrides, live `env`
//! dicts) cross the [`Host`] trait, implemented once by the PyO3 binding.
//! Sharing model: token streams and parser states live behind
//! `Rc<RefCell<..>>` handles ([`SharedToken`], [`SharedBlockState`],
//! [`SharedInlineState`], [`SharedCoreState`]) so Python plugin rules mutate
//! the live parse through the binding's pyclass wrappers. Core functions
//! borrow briefly and never hold a borrow across a [`Host`] call.

use std::cell::RefCell;
use std::rc::Rc;

pub mod common_utils;
pub mod entities_data;
pub mod options_env;
pub mod punycode;
pub mod rules_block;
pub mod rules_core;
pub mod rules_inline;
pub mod token;
pub use token::{AttrVal, MetaVal, SharedToken, Token};

pub mod render;
pub mod ruler;
pub mod helpers;
/// Live block-parser state shared with Python plugin rules.
pub type SharedBlockState = Rc<RefCell<rules_block::StateBlock>>;
/// Live inline-parser state shared with Python plugin rules.
pub type SharedInlineState = Rc<RefCell<rules_inline::StateInline>>;
/// Live core-parser state shared with Python plugin rules.
pub type SharedCoreState = Rc<RefCell<rules_core::StateCore>>;
/// Parser options shared with the live `OptionsDict`.
pub type SharedOptions = Rc<RefCell<options_env::OptionsData>>;

/// Reference definition stored in `env["references"]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub title: String,
    pub href: String,
    pub map: Option<(usize, usize)>,
}

/// Duplicate reference record for `env["duplicate_refs"]`.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateRef {
    pub href: String,
    pub label: String,
    pub map: Option<(usize, usize)>,
    pub title: String,
}

/// Dynamic host operations. Object-safe so cores hold `Rc<dyn Host>`.
/// Every method taking a shared state wraps it for the Python callable and
/// unwraps the result; `&mut` borrows are never held across these calls.
pub trait Host {
    /// Run the block rule chain over a nested range (blockquote/list).
    fn tokenize_block(&self, state: &SharedBlockState, start: usize, end: usize);
    /// Call a Python block rule by registry id.
    fn call_block_rule(
        &self,
        id: u64,
        state: &SharedBlockState,
        start: usize,
        end: usize,
        silent: bool,
    ) -> bool;
    /// Call a Python inline rule by registry id.
    fn call_inline_rule(&self, id: u64, state: &SharedInlineState, silent: bool) -> bool;
    /// Call a Python inline2 (post-processing) rule by registry id.
    fn call_inline2_rule(&self, id: u64, state: &SharedInlineState);
    /// Call a Python core rule by registry id.
    fn call_core_rule(&self, id: u64, state: &SharedCoreState);
    /// `md.normalizeLink` (delegates to the Python `mdurl` module).
    fn normalize_link(&self, url: &str) -> String;
    /// `md.normalizeLinkText` (delegates to the Python `mdurl` module).
    fn normalize_link_text(&self, url: &str) -> String;
    /// `md.validateLink` with an optional Python validator callable id.
    fn validate_link(&self, url: &str, validator: Option<u64>) -> bool;
    /// `md.linkify.pretest(text)`; `None` when linkify is not installed.
    fn linkify_pretest(&self, text: &str) -> Option<bool>;
    /// `md.linkify.test(schema, text)`; `None` when linkify is not installed.
    /// `md.linkify.test(text)`; `None` when linkify is not installed.
    fn linkify_test(&self, text: &str) -> Option<bool>;
    /// `md.linkify.match_at_start(fragment)`; `None` when not installed or no match.
    fn linkify_match_at_start(&self, fragment: &str) -> Option<LinkifyMatch>;
    /// `md.linkify.match(text)`; empty when not installed.
    fn linkify_match(&self, text: &str) -> Vec<LinkifyMatch>;
    /// Whether a linkify implementation is installed at all.
    fn has_linkify(&self) -> bool;
    /// `options.highlight(content, lang, attrs)`; `None` when unset.
    fn call_highlight(&self, content: &str, lang: &str, attrs: &str) -> Option<String>;
    /// Render-rule override id for a token type, if `renderer.rules` has one.
    fn render_override(&self, token_type: &str) -> Option<u64>;
    /// Call a render-rule override `(tokens, idx, options, env) -> str`.
    fn call_render_rule(
        &self,
        id: u64,
        tokens: &[SharedToken],
        idx: usize,
        options: &SharedOptions,
        env: &options_env::EnvData,
    ) -> String;
    /// `ParserInline.skipToken` on a live state (used by `parseLinkLabel`).
    fn inline_skip_token(&self, state: &SharedInlineState);
    /// `HTML_TAG_RE.match(src, pos)` through the live
    /// `markdown_it.rules_inline.html_inline` module (verbatim dispatch, so
    /// monkeypatched patterns apply). Returns the matched tag text.
    fn html_tag_match(&self, src: &str, pos: usize) -> Option<String>;
    /// Nested inline tokenize on a live state (link rule; live ruler, no
    /// post-processing rules).
    fn inline_tokenize(&self, state: &SharedInlineState);
    /// Fresh full inline parse of image alt content (new state sharing
    /// options/host; env cloned in and merged back).
    fn inline_parse_content(
        &self,
        parent: &SharedInlineState,
        content: &str,
    ) -> Vec<SharedToken>;
    /// Silent block-terminator check over a named chain (`paragraph`,
    /// `reference`, `blockquote`, `list`).
    fn block_terminates(
        &self,
        chain: &str,
        state: &SharedBlockState,
        line: usize,
        end: usize,
    ) -> bool;
    /// Core `block` rule: top-level block parse appending to the core
    /// state's token stream.
    fn core_block_parse(&self, state: &SharedCoreState);
    /// Core `inline` rule: inline-parse one token's content into its children.
    fn core_inline_parse(&self, state: &SharedCoreState, token: &SharedToken);
    /// Materialize the live `env` dict for a Python rule call.
    fn env_snapshot(&self, env: &options_env::EnvData) -> EnvSnapshot;
    /// Merge a Python rule's `env` writes back into core env data.
    fn env_merge_back(&self, env: &mut options_env::EnvData, snapshot: EnvSnapshot);
}

/// Panic payload for Python `TypeError`s raised verbatim from core paths
/// (e.g. `attrJoin` on a non-string attr). Caught by the binding.
#[derive(Debug)]
pub struct HostTypeError(pub String);

/// Panic payload for Python `UnicodeError`s (punycode decode failures in
/// `normalizeLinkText`, verbatim unsuppressed). Caught by the binding.
#[derive(Debug)]
pub struct HostUnicodeError(pub String);

/// Raise a verbatim Python `UnicodeError` (caught at the binding boundary).
pub fn panic_unicode_error(msg: String) -> ! {
    std::panic::panic_any(HostUnicodeError(msg))
}

/// Raise a verbatim Python `TypeError` (caught at the binding boundary).
pub fn panic_type_error(msg: String) -> ! {
    std::panic::panic_any(HostTypeError(msg))
}

/// Host for states allocated without a parser (`StateInline.__new__`
/// without `__init__`): pure-data methods work; any host callback panics
/// (unreachable — bare states never parse).
#[derive(Debug, Clone, Copy)]
pub struct NullHost;

impl Host for NullHost {
    fn tokenize_block(&self, _: &SharedBlockState, _: usize, _: usize) {
        panic!("bare state cannot parse")
    }
    fn block_terminates(&self, _: &str, _: &SharedBlockState, _: usize, _: usize) -> bool {
        panic!("bare state cannot parse")
    }
    fn core_block_parse(&self, _: &SharedCoreState) {
        panic!("bare state cannot parse")
    }
    fn core_inline_parse(&self, _: &SharedCoreState, _: &SharedToken) {
        panic!("bare state cannot parse")
    }
    fn call_block_rule(&self, _: u64, _: &SharedBlockState, _: usize, _: usize, _: bool) -> bool {
        panic!("bare state cannot parse")
    }
    fn call_inline_rule(&self, _: u64, _: &SharedInlineState, _: bool) -> bool {
        panic!("bare state cannot parse")
    }
    fn call_inline2_rule(&self, _: u64, _: &SharedInlineState) {
        panic!("bare state cannot parse")
    }
    fn call_core_rule(&self, _: u64, _: &SharedCoreState) {
        panic!("bare state cannot parse")
    }
    fn normalize_link(&self, _: &str) -> String {
        panic!("bare state cannot parse")
    }
    fn normalize_link_text(&self, _: &str) -> String {
        panic!("bare state cannot parse")
    }
    fn validate_link(&self, _: &str, _: Option<u64>) -> bool {
        panic!("bare state cannot parse")
    }
    fn has_linkify(&self) -> bool {
        false
    }
    fn linkify_pretest(&self, _: &str) -> Option<bool> {
        None
    }
    fn linkify_test(&self, _: &str) -> Option<bool> {
        None
    }
    fn linkify_match_at_start(&self, _: &str) -> Option<LinkifyMatch> {
        None
    }
    fn linkify_match(&self, _: &str) -> Vec<LinkifyMatch> {
        Vec::new()
    }
    fn call_highlight(&self, _: &str, _: &str, _: &str) -> Option<String> {
        None
    }
    fn render_override(&self, _: &str) -> Option<u64> {
        None
    }
    fn call_render_rule(
        &self,
        _: u64,
        _: &[SharedToken],
        _: usize,
        _: &SharedOptions,
        _: &options_env::EnvData,
    ) -> String {
        panic!("bare state cannot parse")
    }
    fn inline_skip_token(&self, _: &SharedInlineState) {
        panic!("bare state cannot parse")
    }
    fn html_tag_match(&self, _: &str, _: usize) -> Option<String> {
        panic!("bare state cannot parse")
    }
    fn inline_tokenize(&self, _: &SharedInlineState) {
        panic!("bare state cannot parse")
    }
    fn inline_parse_content(&self, _: &SharedInlineState, _: &str) -> Vec<SharedToken> {
        panic!("bare state cannot parse")
    }
    fn env_snapshot(&self, _: &options_env::EnvData) -> EnvSnapshot {
        EnvSnapshot { entries: Vec::new() }
    }
    fn env_merge_back(&self, _: &mut options_env::EnvData, _: EnvSnapshot) {}
}

/// A linkify-it match, converted at the binding boundary.
#[derive(Debug, Clone)]
pub struct LinkifyMatch {
    pub schema: String,
    pub index: i64,
    pub last_index: i64,
    pub raw: String,
    pub text: String,
    pub url: String,
}

/// Opaque live-env snapshot handed to Python rules (binding-defined shape).
/// Core only ferries it; the binding converts to/from a real dict.
#[derive(Debug, Clone, Default)]
pub struct EnvSnapshot {
    pub entries: Vec<(String, EnvSnapshotValue)>,
}

/// Snapshot value kinds (JSON-shaped core values; non-JSON plugin values
/// ride the binding side-table keyed by name).
#[derive(Debug, Clone)]
pub enum EnvSnapshotValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    List(Vec<EnvSnapshotValue>),
    Map(Vec<(String, EnvSnapshotValue)>),
    Opaque(String),
}
