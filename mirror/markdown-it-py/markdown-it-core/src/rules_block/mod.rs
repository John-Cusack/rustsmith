//! Block layer (mirror of `markdown_it/parser_block.py`,
//! `markdown_it/ruler.py` block usage, and
//! `markdown_it/rules_block/state_block.py`).
//!
//! [`StateBlock`] owns the line caches plus `src` in three aligned forms:
//! `src` (`Vec<char>`, Python-`str` indexing), `src_text` (`String`, regex
//! windows), and `src_byte_of` (byte offset per char index, sentinel `len`).
//! [`ParserBlock`] owns the block [`Ruler`](crate::ruler::Ruler); rule
//! snapshots are cloned before running so no parser borrow is held across
//! rule (or Python re-entry through [`Host`](crate::Host)) calls.

pub mod blockquote;
pub mod code;
pub mod fence;
pub mod heading;
pub mod hr;
pub mod html_block;
pub mod html_data;
pub mod lheading;
pub mod list;
pub mod paragraph;
pub mod reference;
pub mod table;

use std::rc::Rc;

use crate::options_env::EnvData;
use crate::ruler::Ruler;
use crate::token::SharedToken;
use crate::{Host, SharedOptions};

/// Link back to the parser instance (options + host callbacks).
#[derive(Clone)]
pub struct MdRef {
    pub options: SharedOptions,
    pub host: Rc<dyn Host>,
}

impl MdRef {
    pub fn max_nesting(&self) -> i64 {
        self.options.borrow().max_nesting
    }
}

/// Block rule chains: builtin discriminant or Python registry id.
#[derive(Debug, Clone)]
pub enum BlockRule {
    Builtin(BlockKind),
    Python(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Table,
    Code,
    Fence,
    Blockquote,
    Hr,
    List,
    Reference,
    HtmlBlock,
    Heading,
    Lheading,
    Paragraph,
}

pub struct StateBlock {
    pub src: Vec<char>,
    pub src_text: String,
    pub src_byte_of: Vec<usize>,
    pub md: MdRef,
    pub env: EnvData,
    pub tokens: Vec<SharedToken>,
    pub b_marks: Vec<usize>,
    pub e_marks: Vec<usize>,
    pub t_shift: Vec<usize>,
    pub s_count: Vec<usize>,
    pub bs_count: Vec<usize>,
    pub blk_indent: usize,
    pub line: usize,
    pub line_max: usize,
    pub tight: bool,
    pub dd_indent: i64,
    pub list_indent: i64,
    pub parent_type: String,
    pub level: usize,
    pub code_enabled: bool,
}

impl StateBlock {
    pub fn new(
        src: &str,
        options: SharedOptions,
        host: Rc<dyn Host>,
        env: EnvData,
        tokens: Vec<SharedToken>,
        code_enabled: bool,
    ) -> Self {
        let chars: Vec<char> = src.chars().collect();
        let mut byte_of: Vec<usize> = Vec::with_capacity(chars.len() + 1);
        for (byte_idx, _) in src.char_indices() {
            byte_of.push(byte_idx);
        }
        byte_of.push(src.len());
        let length = chars.len();
        let mut st = Self {
            src: chars,
            src_text: src.to_string(),
            src_byte_of: byte_of,
            md: MdRef { options, host },
            env,
            tokens,
            b_marks: Vec::new(),
            e_marks: Vec::new(),
            t_shift: Vec::new(),
            s_count: Vec::new(),
            bs_count: Vec::new(),
            blk_indent: 0,
            line: 0,
            line_max: 0,
            tight: false,
            dd_indent: -1,
            list_indent: -1,
            parent_type: "root".to_string(),
            level: 0,
            code_enabled,
        };
        // Line caches (verbatim from `StateBlock.__init__`).
        let mut indent_found = false;
        let (mut start, mut indent, mut offset) = (0usize, 0usize, 0usize);
        let mut pos = 0usize;
        while pos < length {
            let character = st.src[pos];
            if !indent_found {
                if character == '\t' || character == ' ' {
                    indent += 1;
                    if character == '\t' {
                        offset += 4 - offset % 4;
                    } else {
                        offset += 1;
                    }
                    pos += 1;
                    continue;
                } else {
                    indent_found = true;
                }
            }
            if character == '\n' || pos == length - 1 {
                let end = if character != '\n' { pos + 1 } else { pos };
                st.b_marks.push(start);
                st.e_marks.push(end);
                st.t_shift.push(indent);
                st.s_count.push(offset);
                st.bs_count.push(0);
                indent_found = false;
                indent = 0;
                offset = 0;
                start = pos + 1;
            }
            pos += 1;
        }
        st.b_marks.push(length);
        st.e_marks.push(length);
        st.t_shift.push(0);
        st.s_count.push(0);
        st.bs_count.push(0);
        st.line_max = st.b_marks.len() - 1;
        st
    }

    /// `&str` window over a char range (O(1) borrow, no copy).
    pub fn text_range(&self, start: usize, end: usize) -> &str {
        let last = self.src_byte_of.len() - 1;
        let s = self.src_byte_of[start.min(last)];
        let e = self.src_byte_of[end.min(last)];
        &self.src_text[s..e]
    }

    pub fn push(&mut self, ttype: &str, tag: &str, nesting: i8) -> SharedToken {
        let mut token = crate::Token::new(ttype, tag, nesting);
        token.block = true;
        if nesting < 0 {
            self.level = self.level.saturating_sub(1);
        }
        token.level = self.level;
        if nesting > 0 {
            self.level += 1;
        }
        let shared = Rc::new(std::cell::RefCell::new(token));
        self.tokens.push(shared.clone());
        shared
    }

    pub fn is_empty(&self, line: usize) -> bool {
        (self.b_marks[line] + self.t_shift[line]) >= self.e_marks[line]
    }

    pub fn skip_empty_lines(&self, mut from_pos: usize) -> usize {
        while from_pos < self.line_max {
            if (self.b_marks[from_pos] + self.t_shift[from_pos]) < self.e_marks[from_pos] {
                break;
            }
            from_pos += 1;
        }
        from_pos
    }

    pub fn skip_spaces(&self, mut pos: usize) -> usize {
        while let Some(&c) = self.src.get(pos) {
            if c != '\t' && c != ' ' {
                break;
            }
            pos += 1;
        }
        pos
    }

    pub fn skip_spaces_back(&self, mut pos: usize, minimum: usize) -> usize {
        if pos <= minimum {
            return pos;
        }
        while pos > minimum {
            pos -= 1;
            if self.src[pos] != '\t' && self.src[pos] != ' ' {
                return pos + 1;
            }
        }
        pos
    }

    pub fn skip_chars(&self, mut pos: usize, code: u32) -> usize {
        while let Some(&c) = self.src.get(pos) {
            if c as u32 != code {
                break;
            }
            pos += 1;
        }
        pos
    }

    pub fn skip_chars_str(&self, mut pos: usize, ch: char) -> usize {
        while let Some(&c) = self.src.get(pos) {
            if c != ch {
                break;
            }
            pos += 1;
        }
        pos
    }

    pub fn skip_chars_back(&self, mut pos: usize, code: u32, minimum: usize) -> usize {
        if pos <= minimum {
            return pos;
        }
        while pos > minimum {
            pos -= 1;
            if self.src[pos] as u32 != code {
                return pos + 1;
            }
        }
        pos
    }

    pub fn skip_chars_str_back(&self, mut pos: usize, ch: char, minimum: usize) -> usize {
        if pos <= minimum {
            return pos;
        }
        while pos > minimum {
            pos -= 1;
            if self.src[pos] != ch {
                return pos + 1;
            }
        }
        pos
    }

    pub fn get_lines(&self, begin: usize, end: usize, indent: usize, keep_last_lf: bool) -> String {
        if begin >= end {
            return String::new();
        }
        let mut queue: Vec<String> = vec![String::new(); end - begin];
        let mut line = begin;
        let mut i = 1usize;
        while line < end {
            let mut line_indent = 0usize;
            let line_start = self.b_marks[line];
            let mut first = line_start;
            let last = if line + 1 < end || keep_last_lf {
                self.e_marks[line] + 1
            } else {
                self.e_marks[line]
            };
            let last = last.min(self.src.len());
            while first < last && line_indent < indent {
                let ch = self.src[first];
                if ch == '\t' || ch == ' ' {
                    if ch == '\t' {
                        line_indent += 4 - (line_indent + self.bs_count[line]) % 4;
                    } else {
                        line_indent += 1;
                    }
                } else if first - line_start < self.t_shift[line] {
                    line_indent += 1;
                } else {
                    break;
                }
                first += 1;
            }
            let rest: String = self.src[first..last].iter().collect();
            if line_indent > indent {
                queue[i - 1] = " ".repeat(line_indent - indent) + &rest;
            } else {
                queue[i - 1] = rest;
            }
            line += 1;
            i += 1;
        }
        queue.concat()
    }

    pub fn is_code_block(&self, line: usize) -> bool {
        self.code_enabled && (self.s_count[line] as i64 - self.blk_indent as i64) >= 4
    }
}

pub struct ParserBlock {
    pub ruler: Ruler<BlockRule>,
}

impl ParserBlock {
    pub fn new() -> Self {
        let mut ruler = Ruler::new();
        ruler.push("table", BlockRule::Builtin(BlockKind::Table), &["paragraph", "reference"]);
        ruler.push("code", BlockRule::Builtin(BlockKind::Code), &[]);
        ruler.push(
            "fence",
            BlockRule::Builtin(BlockKind::Fence),
            &["paragraph", "reference", "blockquote", "list"],
        );
        ruler.push(
            "blockquote",
            BlockRule::Builtin(BlockKind::Blockquote),
            &["paragraph", "reference", "blockquote", "list"],
        );
        ruler.push(
            "hr",
            BlockRule::Builtin(BlockKind::Hr),
            &["paragraph", "reference", "blockquote", "list"],
        );
        ruler.push(
            "list",
            BlockRule::Builtin(BlockKind::List),
            &["paragraph", "reference", "blockquote"],
        );
        ruler.push("reference", BlockRule::Builtin(BlockKind::Reference), &[]);
        ruler.push(
            "html_block",
            BlockRule::Builtin(BlockKind::HtmlBlock),
            &["paragraph", "reference", "blockquote"],
        );
        ruler.push(
            "heading",
            BlockRule::Builtin(BlockKind::Heading),
            &["paragraph", "reference", "blockquote"],
        );
        ruler.push("lheading", BlockRule::Builtin(BlockKind::Lheading), &[]);
        ruler.push("paragraph", BlockRule::Builtin(BlockKind::Paragraph), &[]);
        Self { ruler }
    }

    /// Silent terminator check over a named chain (owning parser borrow;
    /// prefer [`terminates_with`] on a snapshot when re-entry is possible).
    pub fn terminates(
        &mut self,
        chain: &str,
        state: &crate::SharedBlockState,
        line: usize,
        end: usize,
        host: &Rc<dyn Host>,
    ) -> bool {
        let rules = self.ruler.get_rules(chain);
        terminates_with(&rules, state, line, end, host)
    }

    pub fn default_chain(&mut self) -> Vec<BlockRule> {
        self.ruler.get_rules("")
    }

    /// Snapshot any named chain (brief borrow; run with [`terminates_with`]
    /// so no parser borrow crosses rule calls).
    pub fn chain(&mut self, name: &str) -> Vec<BlockRule> {
        self.ruler.get_rules(name)
    }

    pub fn tokenize(
        &mut self,
        state: &crate::SharedBlockState,
        start_line: usize,
        end_line: usize,
        host: &Rc<dyn Host>,
    ) {
        let rules = self.ruler.get_rules("");
        tokenize_with(&rules, state, start_line, end_line, host);
    }
}

impl Default for ParserBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// Tokenize loop over a pre-cloned rule snapshot: no parser borrow is held
/// while rules (or Python re-entry through the host) run.
pub fn tokenize_with(
    rules: &[BlockRule],
    state: &crate::SharedBlockState,
    start_line: usize,
    end_line: usize,
    host: &Rc<dyn Host>,
) {
    let mut line = start_line;
    let mut has_empty_lines = false;
    while line < end_line {
        let next = state.borrow().skip_empty_lines(line);
        state.borrow_mut().line = next;
        line = next;
        if line >= end_line {
            break;
        }
        {
            let st = state.borrow();
            if (st.s_count[line] as i64) < (st.blk_indent as i64) {
                break;
            }
            if (st.level as i64) >= st.md.max_nesting() {
                // Nesting limit: skip tail (verbatim from `tokenize`).
                drop(st);
                state.borrow_mut().line = end_line;
                break;
            }
        }
        for rule in rules {
            if run_block_rule(rule, state, line, end_line, false, host) {
                break;
            }
        }
        {
            let mut st = state.borrow_mut();
            st.tight = !has_empty_lines;
            line = st.line;
            if line > 0 && (line - 1) < end_line && st.is_empty(line - 1) {
                has_empty_lines = true;
            }
            if line < end_line && st.is_empty(line) {
                has_empty_lines = true;
                line += 1;
                st.line = line;
            }
        }
    }
}

/// Silent terminator check over a pre-cloned snapshot: no parser borrow
/// is held while rules (or Python re-entry through the host) run.
pub fn terminates_with(
    rules: &[BlockRule],
    state: &crate::SharedBlockState,
    line: usize,
    end: usize,
    host: &Rc<dyn Host>,
) -> bool {
    for rule in rules {
        if run_block_rule(rule, state, line, end, true, host) {
            return true;
        }
    }
    false
}

pub fn run_block_rule(
    rule: &BlockRule,
    state: &crate::SharedBlockState,
    start: usize,
    end: usize,
    silent: bool,
    host: &Rc<dyn Host>,
) -> bool {
    match rule {
        BlockRule::Builtin(kind) => call_builtin(*kind, state, start, end, silent, host),
        BlockRule::Python(id) => host.call_block_rule(*id, state, start, end, silent),
    }
}

pub fn call_builtin(
    kind: BlockKind,
    state: &crate::SharedBlockState,
    start: usize,
    end: usize,
    silent: bool,
    host: &Rc<dyn Host>,
) -> bool {
    match kind {
        BlockKind::Table => table::table(state, start, end, silent, host),
        BlockKind::Code => code::code(state, start, end, silent),
        BlockKind::Fence => fence::fence(state, start, end, silent),
        BlockKind::Blockquote => blockquote::blockquote(state, start, end, silent, host),
        BlockKind::Hr => hr::hr(state, start, end, silent),
        BlockKind::List => list::list_block(state, start, end, silent, host),
        BlockKind::Reference => reference::reference(state, start, end, silent, host),
        BlockKind::HtmlBlock => html_block::html_block(state, start, end, silent),
        BlockKind::Heading => heading::heading(state, start, end, silent),
        BlockKind::Lheading => lheading::lheading(state, start, end, silent, host),
        BlockKind::Paragraph => paragraph::paragraph(state, start, end, silent, host),
    }
}
