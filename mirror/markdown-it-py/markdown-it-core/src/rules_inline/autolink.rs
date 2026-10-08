//! `autolink` rule (mirror of `markdown_it/rules_inline/autolink.py`).

use std::rc::Rc;
use std::sync::LazyLock;

use regex::Regex;

use crate::{Host, SharedInlineState};

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*)$").unwrap()
});
static AUTOLINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z][a-zA-Z0-9+.\-]{1,31}):([^<>\x00-\x20]*)$").unwrap()
});

pub fn autolink(state: &SharedInlineState, silent: bool, host: &Rc<dyn Host>) -> bool {
    let start = state.borrow().pos;
    let pos_max = state.borrow().pos_max;
    if state.borrow().src[start] != '<' {
        return false;
    }
    let mut pos = start;
    loop {
        pos += 1;
        if pos >= pos_max {
            return false;
        }
        let ch = state.borrow().src[pos];
        if ch == '<' {
            return false;
        }
        if ch == '>' {
            break;
        }
    }
    let url: String = state.borrow().src[start + 1..pos].iter().collect();
    if AUTOLINK_RE.is_match(&url) {
        let full = host.normalize_link(&url);
        if !host.validate_link(&full, None) {
            return false;
        }
        if !silent {
            let text = host.normalize_link_text(&url);
            let mut st = state.borrow_mut();
            let token = st.push("link_open", "a", 1);
            {
                let mut t = token.borrow_mut();
                t.attrs = vec![("href".to_string(), crate::token::AttrVal::Str(full))];
                t.markup = "autolink".to_string();
                t.info = "auto".to_string();
            }
            let token = st.push("text", "", 0);
            token.borrow_mut().content = text;
            let token = st.push("link_close", "a", -1);
            {
                let mut t = token.borrow_mut();
                t.markup = "autolink".to_string();
                t.info = "auto".to_string();
            }
        }
        state.borrow_mut().pos += url.chars().count() + 2;
        return true;
    }
    if EMAIL_RE.is_match(&url) {
        let full = host.normalize_link(&format!("mailto:{url}"));
        if !host.validate_link(&full, None) {
            return false;
        }
        if !silent {
            let text = host.normalize_link_text(&url);
            let mut st = state.borrow_mut();
            let token = st.push("link_open", "a", 1);
            {
                let mut t = token.borrow_mut();
                t.attrs = vec![("href".to_string(), crate::token::AttrVal::Str(full))];
                t.markup = "autolink".to_string();
                t.info = "auto".to_string();
            }
            let token = st.push("text", "", 0);
            token.borrow_mut().content = text;
            let token = st.push("link_close", "a", -1);
            {
                let mut t = token.borrow_mut();
                t.markup = "autolink".to_string();
                t.info = "auto".to_string();
            }
        }
        state.borrow_mut().pos += url.chars().count() + 2;
        return true;
    }
    false
}
