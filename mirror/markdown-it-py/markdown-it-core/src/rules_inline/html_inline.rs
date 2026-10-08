//! `html_inline` rule (mirror of `markdown_it/rules_inline/html_inline.py`).

use crate::common_utils::{is_link_close, is_link_open};
use crate::SharedInlineState;

fn is_letter(ch: u32) -> bool {
    let lc = ch | 0x20;
    (0x61..=0x7A).contains(&lc)
}

pub fn html_inline(state: &SharedInlineState, silent: bool, host: &std::rc::Rc<dyn crate::Host>) -> bool {
    if !state.borrow().md.options.borrow().html {
        return false;
    }
    let pos = state.borrow().pos;
    let pos_max = state.borrow().pos_max;
    if state.borrow().src[pos] != '<' || pos + 2 >= pos_max {
        return false;
    }
    let ch = state.borrow().src[pos + 1];
    if ch != '!' && ch != '?' && ch != '/' && !is_letter(ch as u32) {
        return false;
    }
    // Terminator pre-checks (verbatim, O(1) via the rfind cache): reject
    // positions where the required closer cannot occur, without running the
    // tag regex whose lazy sub-patterns would rescan to end of input.
    {
        let mut st = state.borrow_mut();
        let starts_comment = st.src[pos + 2] == '-' && st.src.get(pos + 3) == Some(&'-');
        // NOTE: `src.startswith("<!--", pos)` on chars; `pos+2 < len` holds
        // because `pos + 2 < posMax` was checked above... `pos+2 >= maximum`
        // returns False, so pos+2 < posMax <= len: src[pos+2] in range.
        let _ = starts_comment;
        if st.src[pos + 1] == '!'
            && st.src[pos + 2] == '-'
            && st.src.get(pos + 3) == Some(&'-')
        {
            // `<!-- ...`
            if st.html_terminator_last(">") < pos as i64 + 4 {
                return false;
            }
            let is_bang = st.src.get(pos + 4) == Some(&'>');
            let is_dash = st.src.get(pos + 4) == Some(&'-') && st.src.get(pos + 5) == Some(&'>');
            if !is_bang && !is_dash && st.html_terminator_last("-->") < pos as i64 + 4 {
                return false;
            }
        } else if st.src[pos + 1] == '!'
            && st.src[pos + 2] == '['
            && st.src[pos + 3..].starts_with(&['C', 'D', 'A', 'T', 'A', '['])
        {
            if st.html_terminator_last("]]>") < pos as i64 + 9 {
                return false;
            }
        } else if ch == '?' {
            if st.html_terminator_last("?>") < pos as i64 + 2 {
                return false;
            }
        } else if ch == '!' {
            if st.html_terminator_last(">") < pos as i64 + 3 {
                return false;
            }
        } else if st.html_terminator_last(">") < pos as i64 + 2 {
            return false;
        }
    }
    // Verbatim `.match(src, pos)` through the live module object (the
    // host consults `markdown_it.rules_inline.html_inline.HTML_TAG_RE`,
    // so monkeypatched patterns apply). The full source is cloned for the
    // call (the borrow cannot cross into Python); terminator pre-checks
    // above already bound the scan.
    let (markup, advance) = {
        let st = state.borrow();
        let src = st.src_text.clone();
        drop(st);
        match host.html_tag_match(&src, pos) {
            Some(text) => {
                let advance = text.chars().count();
                (text, advance)
            }
            None => return false,
        }
    };
    if !silent {
        let mut st = state.borrow_mut();
        let token = st.push("html_inline", "", 0);
        {
            let mut t = token.borrow_mut();
            t.content = markup.clone();
        }
        drop(st);
        if is_link_open(&markup) {
            state.borrow_mut().link_level += 1;
        }
        if is_link_close(&markup) {
            state.borrow_mut().link_level -= 1;
        }
    }
    state.borrow_mut().pos += advance;
    true
}
