//! `text` rule (mirror of `markdown_it/rules_inline/text.py`).
//!
//! The default terminator set mirrors `_DEFAULT_TERMINATORS`
//! (plugin-reserved `{}$%@~+=:` excluded). Matching scans the char source;
//! the compiled-regex cache of the original is an optimization with no
//! observable behavior (except `terminator_re` object identity, which the
//! binding preserves separately).

use std::collections::HashSet;

use crate::SharedInlineState;

pub fn is_default_terminator(ch: char) -> bool {
    matches!(
        ch,
        '\n' | '!' | '#' | '$' | '%' | '&' | '*' | '+' | '-' | ':' | '<' | '=' | '>'
            | '@' | '[' | '\\' | ']' | '^' | '_' | '`' | '{' | '}' | '~'
    )
}

pub fn default_terminators() -> HashSet<char> {
    [
        '\n', '!', '#', '$', '%', '&', '*', '+', '-', ':', '<', '=', '>', '@', '[', '\\',
        ']', '^', '_', '`', '{', '}', '~',
    ]
    .into_iter()
    .collect()
}

pub fn text(state: &SharedInlineState, silent: bool, terms: &HashSet<char>) -> bool {
    let (from, to) = {
        let st = state.borrow();
        let mut pos = st.pos;
        while pos < st.pos_max && !terms.contains(&st.src[pos]) {
            pos += 1;
        }
        (st.pos, pos)
    };
    if to == from {
        return false;
    }
    if !silent {
        let chunk: String = state.borrow().src[from..to].iter().collect();
        state.borrow_mut().append_pending(&chunk);
    }
    state.borrow_mut().pos = to;
    true
}
