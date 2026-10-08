//! `escape` rule (mirror of `markdown_it/rules_inline/escape.py`).
//!
//! Lone-surrogate pairing (`\` + lead + trail) is preserved structurally:
//! core strings hold Unicode scalar values, so the pairing branch only
//! fires for values the extractor cannot produce; it stays for shape parity.

use crate::common_utils::is_str_space;
use crate::SharedInlineState;

fn is_escaped(ch: char) -> bool {
    matches!(
        ch,
        '!' | '"' | '#' | '$' | '%' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | '-'
            | '.' | '/' | ':' | ';' | '<' | '=' | '>' | '?' | '@' | '[' | '\\' | ']'
            | '^' | '_' | '`' | '{' | '|' | '}' | '~'
    )
}

pub fn escape(state: &SharedInlineState, silent: bool) -> bool {
    let pos = state.borrow().pos;
    let pos_max = state.borrow().pos_max;
    if state.borrow().src[pos] != '\\' {
        return false;
    }
    let mut pos = pos + 1;
    if pos >= pos_max {
        return false;
    }
    let ch1 = state.borrow().src[pos];
    if ch1 == '\n' {
        if !silent {
            state.borrow_mut().push("hardbreak", "br", 0);
        }
        pos += 1;
        while pos < pos_max && is_str_space(Some(state.borrow().src[pos])) {
            pos += 1;
        }
        state.borrow_mut().pos = pos;
        return true;
    }
    let mut escaped = ch1.to_string();
    let ch1_ord = ch1 as u32;
    // Surrogate-pair branch (verbatim; unreachable for valid Unicode input
    // since `char` holds scalar values, never lone surrogates).
    if (0xD800..=0xDBFF).contains(&ch1_ord) && pos + 1 < pos_max {
        let ch2 = state.borrow().src[pos + 1];
        if (0xDC00..=0xDFFF).contains(&(ch2 as u32)) {
            escaped.push(ch2);
            pos += 1;
        }
    }
    let orig = format!("\\{escaped}");
    if !silent {
        let token = state.borrow_mut().push("text_special", "", 0);
        {
            let mut t = token.borrow_mut();
            t.content = if is_escaped(ch1) { escaped } else { orig.clone() };
            t.markup = orig;
            t.info = "escape".to_string();
        }
    }
    state.borrow_mut().pos = pos + 1;
    true
}
