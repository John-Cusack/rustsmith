//! `backtick` rule (mirror of `markdown_it/rules_inline/backticks.py`).

use crate::SharedInlineState;

pub fn backtick(state: &SharedInlineState, silent: bool) -> bool {
    let start = state.borrow().pos;
    if state.borrow().src[start] != '`' {
        return false;
    }
    let pos_max = state.borrow().pos_max;
    let mut pos = start + 1;
    while pos < pos_max && state.borrow().src[pos] == '`' {
        pos += 1;
    }
    let marker: String = state.borrow().src[start..pos].iter().collect();
    let opener_length = pos - start;
    // Cached upper bound from a previous failed scan.
    if state.borrow().backticks_scanned
        && state.borrow().backticks.get(&opener_length).copied().unwrap_or(0) <= start
    {
        if !silent {
            state.borrow_mut().append_pending(&marker);
        }
        state.borrow_mut().pos += opener_length;
        return true;
    }
    let mut match_start: usize;
    let mut match_end = pos;
    let found: Option<(usize, usize)> = loop {
        // `str.index("`", matchEnd)` verbatim (ValueError -> break).
        let rel = {
            let st = state.borrow();
            st.src[match_end..].iter().position(|&c| c == '`')
        };
        let ms = match rel {
            Some(r) => match_end + r,
            None => break None,
        };
        match_start = ms;
        let mut me = ms + 1;
        while me < pos_max && state.borrow().src[me] == '`' {
            me += 1;
        }
        match_end = me;
        if match_end - match_start == opener_length {
            break Some((match_start, match_end));
        }
        state.borrow_mut().backticks.insert(match_end - match_start, match_start);
    };
    if let Some((ms, me)) = found {
        if !silent {
            let content: String = state.borrow().src[pos..ms].iter().collect::<String>().replace('\n', " ");
            let token = state.borrow_mut().push("code_inline", "code", 0);
            {
                let mut t = token.borrow_mut();
                t.markup = marker;
                let mut content = content;
                if content.starts_with(' ')
                    && content.ends_with(' ')
                    && !crate::common_utils::py_strip_is_empty(&content)
                {
                    content = content[1..content.len() - 1].to_string();
                }
                t.content = content;
            }
        }
        state.borrow_mut().pos = me;
        return true;
    }
    state.borrow_mut().backticks_scanned = true;
    if !silent {
        state.borrow_mut().append_pending(&marker);
    }
    state.borrow_mut().pos += opener_length;
    true
}
