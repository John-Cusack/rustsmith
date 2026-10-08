//! `emphasis` rule (mirror of `markdown_it/rules_inline/emphasis.py`).

use crate::rules_inline::Delimiter;
use crate::SharedInlineState;

pub fn tokenize(state: &SharedInlineState, silent: bool) -> bool {
    let start = state.borrow().pos;
    let marker = state.borrow().src[start];
    if silent {
        return false;
    }
    if marker != '_' && marker != '*' {
        return false;
    }
    let scanned = state.borrow().scan_delims(start, marker == '*');
    for _ in 0..scanned.length {
        let token = state.borrow_mut().push("text", "", 0);
        token.borrow_mut().content = marker.to_string();
        let idx = state.borrow().tokens.len() - 1;
        state.borrow_mut().delimiters.borrow_mut().push(Delimiter {
            marker: marker as u32,
            length: scanned.length as i64,
            token: idx,
            end: -1,
            open: scanned.can_open,
            close: scanned.can_close,
            level: None,
        });
    }
    state.borrow_mut().pos += scanned.length;
    true
}

fn post_process_list(state: &SharedInlineState, delimiters: &crate::rules_inline::SharedDelims) {
    let delims = delimiters.borrow();
    let mut i = delims.len() as i64 - 1;
    while i >= 0 {
        let start_delim = delims[i as usize].clone();
        if start_delim.marker != 0x5F && start_delim.marker != 0x2A {
            i -= 1;
            continue;
        }
        if start_delim.end == -1 {
            i -= 1;
            continue;
        }
        let end_delim = delims[start_delim.end as usize].clone();
        let is_strong = i > 0
            && delims[i as usize - 1].end == start_delim.end + 1
            && delims[i as usize - 1].marker == start_delim.marker
            && delims[i as usize - 1].token == start_delim.token.wrapping_sub(1)
            && delims[start_delim.end as usize + 1].token == end_delim.token + 1;
        let ch = char::from_u32(start_delim.marker).unwrap_or('?');
        {
            let token = state.borrow().tokens[start_delim.token].clone();
            let mut t = token.borrow_mut();
            if is_strong {
                t.typ = "strong_open".to_string();
                t.tag = "strong".to_string();
            } else {
                t.typ = "em_open".to_string();
                t.tag = "em".to_string();
            }
            t.nesting = 1;
            t.markup = if is_strong { ch.to_string().repeat(2) } else { ch.to_string() };
            t.content = String::new();
        }
        {
            let token = state.borrow().tokens[end_delim.token].clone();
            let mut t = token.borrow_mut();
            if is_strong {
                t.typ = "strong_close".to_string();
                t.tag = "strong".to_string();
            } else {
                t.typ = "em_close".to_string();
                t.tag = "em".to_string();
            }
            t.nesting = -1;
            t.markup = if is_strong { ch.to_string().repeat(2) } else { ch.to_string() };
            t.content = String::new();
        }
        if is_strong {
            state.borrow().tokens[delims[i as usize - 1].token].borrow_mut().content = String::new();
            state.borrow().tokens[delims[start_delim.end as usize + 1].token].borrow_mut().content =
                String::new();
            i -= 1;
        }
        i -= 1;
    }
}

pub fn post_process(state: &SharedInlineState) {
    let delims = state.borrow().delimiters.clone();
    post_process_list(state, &delims);
    for d in state.borrow().tokens_meta.clone().into_iter().flatten() {
        post_process_list(state, &d);
    }
}
