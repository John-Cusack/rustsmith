//! `newline` rule (mirror of `markdown_it/rules_inline/newline.py`).

use crate::common_utils::is_str_space;
use crate::SharedInlineState;

pub fn newline(state: &SharedInlineState, silent: bool) -> bool {
    let pos = state.borrow().pos;
    let pos_max = state.borrow().pos_max;
    if state.borrow().src[pos] != '\n' {
        return false;
    }
    if !silent {
        // Verbatim: pending lookups materialize the buffer.
        let pending = state.borrow_mut().pending();
        let chars: Vec<char> = pending.chars().collect();
        let pmax = chars.len() as i64 - 1;
        if pmax >= 0 && chars[pmax as usize] == ' ' {
            if pmax >= 1 && chars[(pmax - 1) as usize] == ' ' {
                let mut ws = pmax - 1;
                while ws >= 1 && chars[(ws - 1) as usize] == ' ' {
                    ws -= 1;
                }
                let kept: String = chars[..ws as usize].iter().collect();
                state.borrow_mut().set_pending(kept);
                state.borrow_mut().push("hardbreak", "br", 0);
            } else {
                let kept: String = chars[..pmax as usize].iter().collect();
                state.borrow_mut().set_pending(kept);
                state.borrow_mut().push("softbreak", "br", 0);
            }
        } else {
            state.borrow_mut().push("softbreak", "br", 0);
        }
    }
    let mut pos = pos + 1;
    while pos < pos_max && is_str_space(Some(state.borrow().src[pos])) {
        pos += 1;
    }
    state.borrow_mut().pos = pos;
    true
}
