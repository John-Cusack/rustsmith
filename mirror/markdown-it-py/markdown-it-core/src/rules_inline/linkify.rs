//! `linkify` inline rule (mirror of `markdown_it/rules_inline/linkify.py`).
//!
//! When the `linkify` option is on but no implementation is installed, the
//! original raises `ModuleNotFoundError` lazily (only when a `://`
//! candidate appears). Core cannot raise Python errors, so it panics with
//! [`LinkifyMissing`]; the binding catches it at the parse/render boundary
//! and raises the exact `ModuleNotFoundError`.

use std::rc::Rc;
use std::sync::LazyLock;

use regex::Regex;

use crate::{Host, SharedInlineState};

/// Panic payload for missing linkify-it (caught by the binding).
#[derive(Debug)]
pub struct LinkifyMissing;

static SCHEME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-z0-9.+-])([a-z][a-z0-9.+-]*)$").unwrap()
});

pub fn linkify(state: &SharedInlineState, silent: bool, host: &Rc<dyn Host>) -> bool {
    if !state.borrow().md.options.borrow().linkify {
        return false;
    }
    if state.borrow().link_level > 0 {
        return false;
    }
    if !host.has_linkify() {
        std::panic::panic_any(LinkifyMissing);
    }
    let pos = state.borrow().pos;
    let pos_max = state.borrow().pos_max;
    if pos + 3 > pos_max
        || state.borrow().src[pos] != ':'
        || state.borrow().src[pos + 1] != '/'
        || state.borrow().src[pos + 2] != '/'
    {
        return false;
    }
    // `SCHEME_RE.search(state.pending)` (materializes the buffer, verbatim).
    let pending = state.borrow_mut().pending();
    let proto: String = match SCHEME_RE.captures(&pending) {
        Some(cap) => cap[1].to_string(),
        None => return false,
    };
    // `state.src[pos - len(proto):]` as an O(1) window.
    let fragment = state.borrow().text_range(pos - proto.chars().count(), pos_max).to_string();
    let link = match host.linkify_match_at_start(&fragment) {
        Some(l) => l,
        None => return false,
    };
    let mut url = link.url;
    // Disallow `*` at the end of the link (conflicts with emphasis).
    while url.ends_with('*') {
        url.pop();
    }
    let full_url = host.normalize_link(&url);
    if !host.validate_link(&full_url, None) {
        return false;
    }
    if !silent {
        let text = host.normalize_link_text(&url);
        {
            let mut st = state.borrow_mut();
            let kept: String = pending.chars().take(pending.chars().count() - proto.chars().count()).collect();
            st.set_pending(kept);
            let token = st.push("link_open", "a", 1);
            {
                let mut t = token.borrow_mut();
                t.attrs = vec![("href".to_string(), crate::token::AttrVal::Str(full_url))];
                t.markup = "linkify".to_string();
                t.info = "auto".to_string();
            }
            let token = st.push("text", "", 0);
            token.borrow_mut().content = text;
            let token = st.push("link_close", "a", -1);
            {
                let mut t = token.borrow_mut();
                t.markup = "linkify".to_string();
                t.info = "auto".to_string();
            }
        }
    }
    state.borrow_mut().pos += url.chars().count() - proto.chars().count();
    true
}
