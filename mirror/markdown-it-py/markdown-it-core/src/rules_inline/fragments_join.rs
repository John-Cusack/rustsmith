//! `fragments_join` rule (mirror of
//! `markdown_it/rules_inline/fragments_join.py`).

use crate::SharedInlineState;

pub fn fragments_join(state: &SharedInlineState) {
    let mut level: i64 = 0;
    let maximum = state.borrow().tokens.len();
    let mut curr = 0usize;
    let mut last = 0usize;
    while curr < maximum {
        {
            let tokens = state.borrow().tokens.clone();
            let t = tokens[curr].borrow();
            if t.nesting < 0 {
                level -= 1;
            }
        }
        state.borrow().tokens[curr].borrow_mut().level = level.max(0) as usize;
        {
            let tokens = state.borrow().tokens.clone();
            if tokens[curr].borrow().nesting > 0 {
                level += 1;
            }
        }
        let merge = {
            let tokens = state.borrow().tokens.clone();
            tokens[curr].borrow().typ == "text"
                && curr + 1 < maximum
                && tokens[curr + 1].borrow().typ == "text"
        };
        if merge {
            let mut parts: Vec<String> = Vec::new();
            {
                let tokens = state.borrow().tokens.clone();
                parts.push(tokens[curr].borrow().content.clone());
            }
            curr += 1;
            while curr < maximum && state.borrow().tokens[curr].borrow().typ == "text" {
                parts.push(state.borrow().tokens[curr].borrow().content.clone());
                curr += 1;
            }
            let merged = state.borrow().tokens[curr - 1].clone();
            {
                let mut t = merged.borrow_mut();
                t.content = parts.concat();
                t.level = level.max(0) as usize;
            }
            state.borrow_mut().tokens[last] = merged;
            last += 1;
            continue;
        }
        if curr != last {
            let tok = state.borrow().tokens[curr].clone();
            state.borrow_mut().tokens[last] = tok;
        }
        last += 1;
        curr += 1;
    }
    if curr != last {
        state.borrow_mut().tokens.truncate(last);
    }
}
