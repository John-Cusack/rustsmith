//! Inline layer (mirror of `markdown_it/parser_inline.py` and
//! `markdown_it/rules_inline/state_inline.py`).
//!
//! [`StateInline`] carries the pending-text buffer (`_pending` +
//! `_pending_buffer`, verbatim semantics), the delimiter stacks (shared
//! `Rc` handles so `tokens_meta` aliases the live list, verbatim), and the
//! `src` triple (chars/text/byte offsets). [`ParserInline`] owns both rulers
//! plus the terminator set; the `text` rule runs inside
//! [`ParserInline::tokenize`] so per-char terminator checks never cross the
//! host boundary.

pub mod autolink;
pub mod backticks;
pub mod balance_pairs;
pub mod emphasis;
pub mod entity;
pub mod escape;
pub mod fragments_join;
pub mod html_inline;
pub mod image;
pub mod link;
pub mod linkify;
pub mod newline;
pub mod strikethrough;
pub mod text;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::common_utils::{is_md_ascii_punct, is_punct_char, is_white_space};
use crate::options_env::EnvData;
use crate::ruler::Ruler;
use crate::token::SharedToken;
use crate::{Host, SharedOptions};

use super::rules_block::MdRef;

#[derive(Debug, Clone)]
pub struct Delimiter {
    pub marker: u32,
    pub length: i64,
    pub token: usize,
    pub end: i64,
    pub open: bool,
    pub close: bool,
    pub level: Option<bool>,
}

pub type SharedDelims = Rc<RefCell<Vec<Delimiter>>>;

#[derive(Debug, Clone, Copy)]
pub struct Scanned {
    pub can_open: bool,
    pub can_close: bool,
    pub length: usize,
}

#[derive(Debug, Clone)]
pub enum InlineRule {
    Builtin(InlineKind),
    Python(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKind {
    Text,
    Linkify,
    Newline,
    Escape,
    Backticks,
    Strikethrough,
    Emphasis,
    Link,
    Image,
    Autolink,
    HtmlInline,
    Entity,
}

#[derive(Debug, Clone)]
pub enum Inline2Rule {
    Builtin(Inline2Kind),
    Python(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inline2Kind {
    BalancePairs,
    Strikethrough,
    Emphasis,
    FragmentsJoin,
}

pub struct StateInline {
    pub src: Vec<char>,
    pub src_text: String,
    pub src_byte_of: Vec<usize>,
    pub md: MdRef,
    pub env: EnvData,
    pub tokens: Vec<SharedToken>,
    pub tokens_meta: Vec<Option<SharedDelims>>,
    pub pos: usize,
    pub pos_max: usize,
    pub level: usize,
    pending: String,
    pending_buffer: Vec<String>,
    pub pending_level: usize,
    pub cache: HashMap<usize, usize>,
    pub delimiters: SharedDelims,
    prev_delimiters: Vec<SharedDelims>,
    pub backticks: HashMap<usize, usize>,
    pub backticks_scanned: bool,
    pub link_level: i64,
    html_terminators: Option<HashMap<String, i64>>,
}

impl StateInline {
    pub fn new(
        src: &str,
        options: SharedOptions,
        host: Rc<dyn Host>,
        env: EnvData,
        tokens: Vec<SharedToken>,
    ) -> Self {
        let chars: Vec<char> = src.chars().collect();
        let mut byte_of: Vec<usize> = Vec::with_capacity(chars.len() + 1);
        for (byte_idx, _) in src.char_indices() {
            byte_of.push(byte_idx);
        }
        byte_of.push(src.len());
        let pos_max = chars.len();
        let meta = vec![None; tokens.len()];
        Self {
            src: chars,
            src_text: src.to_string(),
            src_byte_of: byte_of,
            md: MdRef { options, host },
            env,
            tokens,
            tokens_meta: meta,
            pos: 0,
            pos_max,
            level: 0,
            pending: String::new(),
            pending_buffer: Vec::new(),
            pending_level: 0,
            cache: HashMap::new(),
            delimiters: Rc::new(RefCell::new(Vec::new())),
            prev_delimiters: Vec::new(),
            backticks: HashMap::new(),
            backticks_scanned: false,
            link_level: 0,
            html_terminators: None,
        }
    }

    /// `&str` window over a char range (O(1) borrow, no copy).
    pub fn text_range(&self, start: usize, end: usize) -> &str {
        let last = self.src_byte_of.len() - 1;
        let s = self.src_byte_of[start.min(last)];
        let e = self.src_byte_of[end.min(last)];
        &self.src_text[s..e]
    }

    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        match self.src_byte_of.binary_search(&byte_idx) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    /// `pending` getter: materialize the buffer (verbatim).
    pub fn pending(&mut self) -> String {
        if !self.pending_buffer.is_empty() {
            let mut text = std::mem::take(&mut self.pending);
            text.push_str(&self.pending_buffer.concat());
            self.pending_buffer.clear();
            self.pending = text;
        }
        self.pending.clone()
    }

    /// `pending` setter: replace everything (works pre-`__init__`, never
    /// shares the buffer).
    pub fn set_pending(&mut self, value: String) {
        self.pending = value;
        self.pending_buffer = Vec::new();
    }

    pub fn append_pending(&mut self, text: &str) {
        self.pending_buffer.push(text.to_string());
    }

    pub fn pending_len(&mut self) -> usize {
        self.pending().chars().count()
    }

    /// Raw `_pending` (no materialization) for the `_pending` test probe.
    pub fn pending_raw(&self) -> &str {
        &self.pending
    }

    /// Raw `_pending_buffer` for the `_pending_buffer` test probe.
    pub fn pending_buffer_raw(&self) -> &[String] {
        &self.pending_buffer
    }

    pub fn push_pending(&mut self) -> SharedToken {
        let content = self.pending();
        let token = crate::Token::new("text", "", 0);
        let shared = Rc::new(RefCell::new(token));
        {
            let mut t = shared.borrow_mut();
            t.content = content;
            t.level = self.pending_level;
        }
        self.tokens.push(shared.clone());
        self.pending = String::new();
        self.pending_buffer = Vec::new();
        shared
    }

    pub fn push(&mut self, ttype: &str, tag: &str, nesting: i8) -> SharedToken {
        if !self.pending_buffer.is_empty() || !self.pending.is_empty() {
            // Verbatim: `if self.pending:` (materializes).
            let p = self.pending();
            if !p.is_empty() {
                self.push_pending();
            } else {
                self.pending_buffer.clear();
            }
        }
        let token = crate::Token::new(ttype, tag, nesting);
        let shared = Rc::new(RefCell::new(token));
        let mut token_meta: Option<SharedDelims> = None;
        if nesting < 0 {
            self.level = self.level.saturating_sub(1);
            self.delimiters = self.prev_delimiters.pop().unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));
        }
        shared.borrow_mut().level = self.level;
        if nesting > 0 {
            self.level += 1;
            self.prev_delimiters.push(self.delimiters.clone());
            self.delimiters = Rc::new(RefCell::new(Vec::new()));
            token_meta = Some(self.delimiters.clone());
        }
        self.pending_level = self.level;
        self.tokens.push(shared.clone());
        self.tokens_meta.push(token_meta);
        shared
    }

    pub fn scan_delims(&self, start: usize, can_split_word: bool) -> Scanned {
        let pos_max = self.pos_max;
        let marker = self.src[start];
        let last_char = if start > 0 { self.src[start - 1] } else { ' ' };
        let mut pos = start;
        while pos < pos_max && self.src[pos] == marker {
            pos += 1;
        }
        let count = pos - start;
        let next_char = if pos < pos_max { self.src[pos] } else { ' ' };
        let is_last_punct =
            is_md_ascii_punct(last_char as u32) || is_punct_char(last_char);
        let is_next_punct =
            is_md_ascii_punct(next_char as u32) || is_punct_char(next_char);
        let is_last_ws = is_white_space(last_char as u32);
        let is_next_ws = is_white_space(next_char as u32);
        let left_flanking =
            !(is_next_ws || (is_next_punct && !(is_last_ws || is_last_punct)));
        let right_flanking =
            !(is_last_ws || (is_last_punct && !(is_next_ws || is_next_punct)));
        let can_open =
            left_flanking && (can_split_word || !right_flanking || is_last_punct);
        let can_close =
            right_flanking && (can_split_word || !left_flanking || is_next_punct);
        Scanned { can_open, can_close, length: count }
    }

    pub fn html_terminator_last(&mut self, term: &str) -> i64 {
        if self.html_terminators.is_none() {
            self.html_terminators = Some(HashMap::new());
        }
        if let Some(cache) = self.html_terminators.as_ref() {
            if let Some(&last) = cache.get(term) {
                return last;
            }
        }
        let last = match self.src_text.rfind(term) {
            Some(byte_idx) => self.byte_to_char(byte_idx) as i64,
            None => -1,
        };
        self.html_terminators.as_mut().unwrap().insert(term.to_string(), last);
        last
    }

    /// Bare allocation (`StateInline.__new__` without `__init__`): default
    /// options/env/tokens with a [`NullHost`](crate::NullHost). Only
    /// host-free methods (pending) are meaningful on it, verbatim.
    pub fn bare(src: &str) -> Self {
        Self::new(
            src,
            Rc::new(RefCell::new(crate::options_env::OptionsData::default())),
            Rc::new(crate::NullHost),
            EnvData::default(),
            Vec::new(),
        )
    }

    /// Shallow copy with an isolated pending buffer (verbatim `__copy__`).
    pub fn copy_shallow(&mut self) -> StateInline {
        let text = self.pending();
        StateInline {
            src: self.src.clone(),
            src_text: self.src_text.clone(),
            src_byte_of: self.src_byte_of.clone(),
            md: self.md.clone(),
            env: self.env.clone(),
            tokens: self.tokens.clone(),
            tokens_meta: self.tokens_meta.clone(),
            pos: self.pos,
            pos_max: self.pos_max,
            level: self.level,
            pending: text,
            pending_buffer: Vec::new(),
            pending_level: self.pending_level,
            cache: self.cache.clone(),
            delimiters: self.delimiters.clone(),
            prev_delimiters: self.prev_delimiters.clone(),
            backticks: self.backticks.clone(),
            backticks_scanned: self.backticks_scanned,
            link_level: self.link_level,
            html_terminators: self.html_terminators.clone(),
        }
    }
}

pub struct ParserInline {
    pub ruler: Ruler<InlineRule>,
    pub ruler2: Ruler<Inline2Rule>,
    pub extra_terminators: HashSet<char>,
}

impl ParserInline {
    pub fn new() -> Self {
        use InlineKind::*;
        let mut ruler = Ruler::new();
        ruler.push("text", InlineRule::Builtin(Text), &[]);
        ruler.push("linkify", InlineRule::Builtin(Linkify), &[]);
        ruler.push("newline", InlineRule::Builtin(Newline), &[]);
        ruler.push("escape", InlineRule::Builtin(Escape), &[]);
        ruler.push("backticks", InlineRule::Builtin(Backticks), &[]);
        ruler.push("strikethrough", InlineRule::Builtin(Strikethrough), &[]);
        ruler.push("emphasis", InlineRule::Builtin(Emphasis), &[]);
        ruler.push("link", InlineRule::Builtin(Link), &[]);
        ruler.push("image", InlineRule::Builtin(Image), &[]);
        ruler.push("autolink", InlineRule::Builtin(Autolink), &[]);
        ruler.push("html_inline", InlineRule::Builtin(HtmlInline), &[]);
        ruler.push("entity", InlineRule::Builtin(Entity), &[]);
        let mut ruler2 = Ruler::new();
        ruler2.push("balance_pairs", Inline2Rule::Builtin(Inline2Kind::BalancePairs), &[]);
        ruler2.push("strikethrough", Inline2Rule::Builtin(Inline2Kind::Strikethrough), &[]);
        ruler2.push("emphasis", Inline2Rule::Builtin(Inline2Kind::Emphasis), &[]);
        ruler2.push("fragments_join", Inline2Rule::Builtin(Inline2Kind::FragmentsJoin), &[]);
        Self { ruler, ruler2, extra_terminators: HashSet::new() }
    }

    pub fn is_terminator(&self, ch: char) -> bool {
        self.extra_terminators.contains(&ch) || text::is_default_terminator(ch)
    }

    /// Owned terminator snapshot for rule calls (no parser borrow crosses
    /// Python re-entry).
    pub fn terminator_snapshot(&self) -> HashSet<char> {
        let mut set = text::default_terminators();
        set.extend(self.extra_terminators.iter().copied());
        set
    }

    pub fn add_terminator_char(&mut self, ch: char) {
        if !text::is_default_terminator(ch) {
            self.extra_terminators.insert(ch);
        }
    }

    /// Snapshot the default chain + terminators (brief borrow).
    pub fn snapshot(&mut self) -> (Vec<InlineRule>, std::collections::HashSet<char>) {
        (self.ruler.get_rules(""), self.terminator_snapshot())
    }

    /// Snapshot the post-processing chain (brief borrow).
    pub fn snapshot2(&mut self) -> Vec<Inline2Rule> {
        self.ruler2.get_rules("")
    }

    pub fn skip_token(&mut self, state: &crate::SharedInlineState, host: &Rc<dyn Host>) {
        let (rules, terms) = self.snapshot();
        skip_token_with(&rules, &terms, state, host);
    }

    pub fn tokenize(&mut self, state: &crate::SharedInlineState, host: &Rc<dyn Host>) {
        let (rules, terms) = self.snapshot();
        tokenize_with(&rules, &terms, state, host);
    }

    pub fn parse_fresh(
        &mut self,
        src: &str,
        options: SharedOptions,
        host: Rc<dyn Host>,
        env: EnvData,
    ) -> Vec<SharedToken> {
        let state = Rc::new(RefCell::new(StateInline::new(src, options, host.clone(), env, Vec::new())));
        self.tokenize(&state, &host);
        let rules2 = self.ruler2.get_rules("");
        for rule in &rules2 {
            run_inline2_rule(rule, &state, &host);
        }
        let tokens = state.borrow().tokens.clone();
        tokens
    }
}

impl Default for ParserInline {
    fn default() -> Self {
        Self::new()
    }
}


/// `skipToken` over pre-cloned snapshots (no parser borrow across calls).
pub fn skip_token_with(
    rules: &[InlineRule],
    terms: &std::collections::HashSet<char>,
    state: &crate::SharedInlineState,
    host: &Rc<dyn Host>,
) {
    let pos = state.borrow().pos;
    // NOTE: the lookup borrow must end before any `borrow_mut` (an `if
    // let` scrutinee borrow lives into the body and would panic).
    let cached = state.borrow().cache.get(&pos).copied();
    if let Some(cached) = cached {
        state.borrow_mut().pos = cached;
        return;
    }
    let max_nesting = state.borrow().md.max_nesting();
    let mut ok = false;
    if (state.borrow().level as i64) < max_nesting {
        for rule in rules {
            state.borrow_mut().level += 1;
            ok = run_inline_rule(rule, state, true, host, terms);
            state.borrow_mut().level -= 1;
            if ok {
                break;
            }
        }
    } else {
        // NOTE: read before the mutable borrow (same-statement
        // read/write on one cell panics).
        let pos_max = state.borrow().pos_max;
        state.borrow_mut().pos = pos_max;
    }
    if !ok {
        state.borrow_mut().pos += 1;
    }
    let end = state.borrow().pos;
    state.borrow_mut().cache.insert(pos, end);
}

/// `tokenize` over pre-cloned snapshots (no parser borrow across calls).
pub fn tokenize_with(
    rules: &[InlineRule],
    terms: &std::collections::HashSet<char>,
    state: &crate::SharedInlineState,
    host: &Rc<dyn Host>,
) {
    let max_nesting = state.borrow().md.max_nesting();
    loop {
        let (pos, end) = {
            let st = state.borrow();
            (st.pos, st.pos_max)
        };
        if pos >= end {
            break;
        }
        let mut ok = false;
        if (state.borrow().level as i64) < max_nesting {
            for rule in rules {
                ok = run_inline_rule(rule, state, false, host, terms);
                if ok {
                    break;
                }
            }
        }
        if ok {
            if state.borrow().pos >= end {
                break;
            }
            continue;
        }
        {
            let mut st = state.borrow_mut();
            let ch = st.src[st.pos];
            st.pos += 1;
            st.append_pending(&ch.to_string());
        }
    }
    if !state.borrow_mut().pending().is_empty() {
        state.borrow_mut().push_pending();
    }
}

pub fn run_inline_rule(
    rule: &InlineRule,
    state: &crate::SharedInlineState,
    silent: bool,
    host: &Rc<dyn Host>,
    terms: &HashSet<char>,
) -> bool {
    match rule {
        InlineRule::Builtin(kind) => run_inline_builtin(*kind, state, silent, host, terms),
        InlineRule::Python(id) => host.call_inline_rule(*id, state, silent),
    }
}

pub fn run_inline_builtin(
    kind: InlineKind,
    state: &crate::SharedInlineState,
    silent: bool,
    host: &Rc<dyn Host>,
    terms: &HashSet<char>,
) -> bool {
    match kind {
        InlineKind::Text => text::text(state, silent, terms),
        InlineKind::Linkify => linkify::linkify(state, silent, host),
        InlineKind::Newline => newline::newline(state, silent),
        InlineKind::Escape => escape::escape(state, silent),
        InlineKind::Backticks => backticks::backtick(state, silent),
        InlineKind::Strikethrough => strikethrough::tokenize(state, silent),
        InlineKind::Emphasis => emphasis::tokenize(state, silent),
        InlineKind::Link => link::link(state, silent, host),
        InlineKind::Image => image::image(state, silent, host),
        InlineKind::Autolink => autolink::autolink(state, silent, host),
        InlineKind::HtmlInline => html_inline::html_inline(state, silent, host),
        InlineKind::Entity => entity::entity(state, silent),
    }
}

pub fn run_inline2_rule(rule: &Inline2Rule, state: &crate::SharedInlineState, host: &Rc<dyn Host>) {
    match rule {
        Inline2Rule::Builtin(kind) => match kind {
            Inline2Kind::BalancePairs => balance_pairs::link_pairs(state),
            Inline2Kind::Strikethrough => strikethrough::post_process(state),
            Inline2Kind::Emphasis => emphasis::post_process(state),
            Inline2Kind::FragmentsJoin => fragments_join::fragments_join(state),
        },
        Inline2Rule::Python(id) => host.call_inline2_rule(*id, state),
    }
}
