//! `normalize` core rule (mirror of `markdown_it/rules_core/normalize.py`).

use crate::rules_core::{NEWLINES_RE, NULL_RE};

pub fn normalize(state: &crate::SharedCoreState) {
    let src = state.borrow().src.clone();
    let s = NEWLINES_RE.replace_all(&src, "\n");
    let s = NULL_RE.replace_all(&s, "\u{FFFD}");
    state.borrow_mut().src = s.into_owned();
}
