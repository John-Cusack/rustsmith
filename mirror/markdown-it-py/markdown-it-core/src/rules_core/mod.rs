//! Core layer (mirror of `markdown_it/parser_core.py`,
//! `markdown_it/rules_core/state_core.py`, and `markdown_it/rules_core/*.py`).

pub mod linkify;
pub mod normalize;
pub mod replacements;
pub mod smartquotes;
pub mod text_join;

use std::rc::Rc;
use std::sync::LazyLock;

use regex::Regex;

use crate::options_env::EnvData;
use crate::ruler::Ruler;
use crate::token::SharedToken;
use crate::{Host, SharedOptions};

use super::rules_block::MdRef;

#[derive(Debug, Clone)]
pub enum CoreRule {
    Builtin(CoreKind),
    Python(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreKind {
    Normalize,
    Block,
    Inline,
    Linkify,
    Replacements,
    Smartquotes,
    TextJoin,
}

pub struct StateCore {
    pub src: String,
    pub md: MdRef,
    pub env: EnvData,
    pub tokens: Vec<SharedToken>,
    pub inline_mode: bool,
}

impl StateCore {
    pub fn new(src: &str, options: SharedOptions, host: Rc<dyn Host>, env: EnvData, tokens: Vec<SharedToken>) -> Self {
        Self { src: src.to_string(), md: MdRef { options, host }, env, tokens, inline_mode: false }
    }
}

pub struct ParserCore {
    pub ruler: Ruler<CoreRule>,
}

impl ParserCore {
    pub fn new() -> Self {
        let mut ruler = Ruler::new();
        ruler.push("normalize", CoreRule::Builtin(CoreKind::Normalize), &[]);
        ruler.push("block", CoreRule::Builtin(CoreKind::Block), &[]);
        ruler.push("inline", CoreRule::Builtin(CoreKind::Inline), &[]);
        ruler.push("linkify", CoreRule::Builtin(CoreKind::Linkify), &[]);
        ruler.push("replacements", CoreRule::Builtin(CoreKind::Replacements), &[]);
        ruler.push("smartquotes", CoreRule::Builtin(CoreKind::Smartquotes), &[]);
        ruler.push("text_join", CoreRule::Builtin(CoreKind::TextJoin), &[]);
        Self { ruler }
    }

    /// Snapshot the core chain (brief borrow).
    pub fn snapshot(&mut self) -> Vec<CoreRule> {
        self.ruler.get_rules("")
    }

    pub fn process(&mut self, state: &crate::SharedCoreState, host: &Rc<dyn Host>) {
        let rules = self.snapshot();
        process_with(&rules, state, host);
    }
}

impl Default for ParserCore {
    fn default() -> Self {
        Self::new()
    }
}

/// Core chain over a pre-cloned snapshot (no parser borrow across calls).
pub fn process_with(rules: &[CoreRule], state: &crate::SharedCoreState, host: &Rc<dyn Host>) {
    for rule in rules {
        match rule {
            CoreRule::Builtin(kind) => run_core_builtin(*kind, state, host),
            CoreRule::Python(id) => host.call_core_rule(*id, state),
        }
    }
}

pub fn run_core_builtin(kind: CoreKind, state: &crate::SharedCoreState, host: &Rc<dyn Host>) {
    match kind {
        CoreKind::Normalize => normalize::normalize(state),
        CoreKind::Block => host.core_block_parse(state),
        CoreKind::Inline => inline_rule(state, host),
        CoreKind::Linkify => linkify::linkify(state, host),
        CoreKind::Replacements => replacements::replace(state),
        CoreKind::Smartquotes => smartquotes::smartquotes(state),
        CoreKind::TextJoin => text_join::text_join(state),
    }
}

/// Core `inline` rule: inline-parse every inline token's content.
fn inline_rule(state: &crate::SharedCoreState, host: &Rc<dyn Host>) {
    let tokens = state.borrow().tokens.clone();
    for token in &tokens {
        let is_inline = token.borrow().typ == "inline";
        if !is_inline {
            continue;
        }
        if token.borrow().children.is_none() {
            token.borrow_mut().children = Some(Vec::new());
        }
        host.core_inline_parse(state, token);
    }
}

/// Core `block` rule helper: split off for reuse.
pub static NEWLINES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\r\n?|\n").unwrap());
pub static NULL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("\0").unwrap());
