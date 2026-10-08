//! `strikethrough` rule (mirror of `markdown_it/rules_inline/strikethrough.py`).

use crate::{SharedInlineState, SharedToken};

pub fn tokenize(state: &SharedInlineState, silent: bool) -> bool {
    let start = state.borrow().pos;
    let ch = state.borrow().src[start];
    if silent {
        return false;
    }
    if ch != '~' {
        return false;
    }
    let scanned = state.borrow().scan_delims(start, true);
    let mut length = scanned.length;
    let single_tilde = state.borrow().md.options.borrow().extra_bool("strikethrough_single_tilde", false);
    if single_tilde {
        if length < 1 {
            return false;
        }
        if length > 2 {
            // Consume 3+ tildes as plain text (GitHub behavior).
            let token = state.borrow_mut().push("text", "", 0);
            token.borrow_mut().content = ch.to_string().repeat(length);
            state.borrow_mut().pos += scanned.length;
            return true;
        }
        let token = state.borrow_mut().push("text", "", 0);
        token.borrow_mut().content = ch.to_string().repeat(length);
        let idx = state.borrow().tokens.len() - 1;
        state.borrow_mut().delimiters.borrow_mut().push(crate::rules_inline::Delimiter {
            marker: ch as u32,
            length: 0,
            token: idx,
            end: -1,
            open: scanned.can_open,
            close: scanned.can_close,
            level: None,
        });
    } else {
        if length < 2 {
            return false;
        }
        if length % 2 == 1 {
            let token = state.borrow_mut().push("text", "", 0);
            token.borrow_mut().content = ch.to_string();
            length -= 1;
        }
        let mut i = 0;
        while i < length {
            let token = state.borrow_mut().push("text", "", 0);
            token.borrow_mut().content = ch.to_string().repeat(2);
            let idx = state.borrow().tokens.len() - 1;
            state.borrow_mut().delimiters.borrow_mut().push(crate::rules_inline::Delimiter {
                marker: ch as u32,
                length: 0,
                token: idx,
                end: -1,
                open: scanned.can_open,
                close: scanned.can_close,
                level: None,
            });
            i += 2;
        }
    }
    state.borrow_mut().pos += scanned.length;
    true
}

fn post_process_list(state: &SharedInlineState, delimiters: &crate::rules_inline::SharedDelims) {
    let single_tilde = state.borrow().md.options.borrow().extra_bool("strikethrough_single_tilde", false);
    let mut lone_markers: Vec<usize> = Vec::new();
    let delims = delimiters.borrow();
    let maximum = delims.len();
    let mut i = 0;
    while i < maximum {
        let start_delim = delims[i].clone();
        if start_delim.marker != 0x7E {
            i += 1;
            continue;
        }
        if start_delim.end == -1 {
            i += 1;
            continue;
        }
        let end_delim = delims[start_delim.end as usize].clone();
        if single_tilde {
            let opener_content = state.borrow().tokens[start_delim.token].borrow().content.clone();
            let closer_content = state.borrow().tokens[end_delim.token].borrow().content.clone();
            if opener_content != closer_content {
                i += 1;
                continue;
            }
        }
        let markup = state.borrow().tokens[start_delim.token].borrow().content.clone();
        {
            let token = state.borrow().tokens[start_delim.token].clone();
            let mut t = token.borrow_mut();
            t.typ = "s_open".to_string();
            t.tag = "s".to_string();
            t.nesting = 1;
            t.markup = markup.clone();
            t.content = String::new();
        }
        {
            let token = state.borrow().tokens[end_delim.token].clone();
            let mut t = token.borrow_mut();
            t.typ = "s_close".to_string();
            t.tag = "s".to_string();
            t.nesting = -1;
            t.markup = markup;
            t.content = String::new();
        }
        if end_delim.token > 0 {
            let prev: SharedToken = state.borrow().tokens[end_delim.token - 1].clone();
            let is_lone = {
                let t = prev.borrow();
                t.typ == "text" && t.content == "~"
            };
            if is_lone {
                lone_markers.push(end_delim.token - 1);
            }
        }
        i += 1;
    }
    drop(delims);
    while let Some(i) = lone_markers.pop() {
        let mut j = i + 1;
        while j < state.borrow().tokens.len() && state.borrow().tokens[j].borrow().typ == "s_close" {
            j += 1;
        }
        j -= 1;
        if i != j {
            let a = state.borrow().tokens[j].clone();
            let b = state.borrow().tokens[i].clone();
            state.borrow_mut().tokens[j] = b;
            state.borrow_mut().tokens[i] = a;
        }
    }
}

pub fn post_process(state: &SharedInlineState) {
    let delims = state.borrow().delimiters.clone();
    post_process_list(state, &delims);
    let maximum = state.borrow().tokens_meta.len();
    let mut curr = 0;
    while curr < maximum {
        let meta = state.borrow().tokens_meta[curr].clone();
        if let Some(d) = meta {
            post_process_list(state, &d);
        }
        curr += 1;
    }
}
