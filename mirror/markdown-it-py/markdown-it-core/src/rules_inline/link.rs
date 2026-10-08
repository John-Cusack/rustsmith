//! `link` rule (mirror of `markdown_it/rules_inline/link.py`).

use std::rc::Rc;

use crate::common_utils::{is_str_space, normalize_reference};
use crate::helpers::{parse_link_destination, parse_link_label, parse_link_title};
use crate::{Host, SharedInlineState};

pub fn link(state: &SharedInlineState, silent: bool, host: &Rc<dyn Host>) -> bool {
    let start = state.borrow().pos;
    let pos_max = state.borrow().pos_max;
    let old_pos = start;
    if state.borrow().src[start] != '[' {
        return false;
    }
    let label_start = start + 1;
    let label_end = parse_link_label(state, start, true, host);
    if label_end < 0 {
        return false;
    }
    let label_end = label_end as usize;
    let mut pos = label_end + 1;
    let mut href = String::new();
    let mut title = String::new();
    let mut parse_reference = true;

    if pos < pos_max && state.borrow().src[pos] == '(' {
        // Inline link.
        parse_reference = false;
        pos += 1;
        while pos < pos_max {
            let ch = state.borrow().src[pos];
            if !is_str_space(Some(ch)) && ch != '\n' {
                break;
            }
            pos += 1;
        }
        if pos >= pos_max {
            return false;
        }
        let res = {
            let st = state.borrow();
            parse_link_destination(&st.src, pos, st.pos_max)
        };
        if res.ok {
            href = host.normalize_link(&res.text);
            if host.validate_link(&href, None) {
                pos = res.pos;
            } else {
                href = String::new();
            }
            let start = pos;
            while pos < pos_max {
                let ch = state.borrow().src[pos];
                if !is_str_space(Some(ch)) && ch != '\n' {
                    break;
                }
                pos += 1;
            }
            let res = {
                let st = state.borrow();
                parse_link_title(&st.src, pos, st.pos_max, None)
            };
            if pos < pos_max && start != pos && res.ok {
                title = res.text.clone();
                pos = res.pos;
                while pos < pos_max {
                    let ch = state.borrow().src[pos];
                    if !is_str_space(Some(ch)) && ch != '\n' {
                        break;
                    }
                    pos += 1;
                }
            }
        }
        if pos >= pos_max || state.borrow().src[pos] != ')' {
            // Valid shortcut link parse failed; fall back to reference.
            parse_reference = true;
        }
        pos += 1;
    }

    if parse_reference {
        if state.borrow().env.references.is_empty() {
            return false;
        }
        let label: Option<String>;
        if pos < pos_max && state.borrow().src[pos] == '[' {
            let start = pos + 1;
            let end = parse_link_label(state, pos, false, host);
            if end >= 0 {
                label = Some(state.borrow().src[start..end as usize].iter().collect());
                pos = end as usize + 1;
            } else {
                pos = label_end + 1;
                label = None;
            }
        } else {
            pos = label_end + 1;
            label = None;
        }
        let label = match label {
            Some(l) if !l.is_empty() => l,
            _ => state.borrow().src[label_start..label_end].iter().collect(),
        };
        let label = normalize_reference(&label);
        let found = state.borrow().env.references.get(&label).cloned();
        match found {
            Some(r) => {
                href = r.href;
                title = r.title;
                if !silent {
                    emit_reference_link(state, host, label_start, label_end, &label, &href, &title);
                }
            }
            None => {
                state.borrow_mut().pos = old_pos;
                return false;
            }
        }
    } else if !silent {
        emit_inline_link(state, host, label_start, label_end, &href, &title);
    }
    {
        let mut st = state.borrow_mut();
        st.pos = pos;
        st.pos_max = pos_max;
    }
    true
}

fn emit_inline_link(
    state: &SharedInlineState,
    host: &Rc<dyn Host>,
    label_start: usize,
    label_end: usize,
    href: &str,
    title: &str,
) {
    state.borrow_mut().pos = label_start;
    state.borrow_mut().pos_max = label_end;
    let token = state.borrow_mut().push("link_open", "a", 1);
    {
        let mut t = token.borrow_mut();
        t.attrs = vec![("href".to_string(), crate::token::AttrVal::Str(href.to_string()))];
        if !title.is_empty() {
            t.attr_set("title", crate::token::AttrVal::Str(title.to_string()));
        }
    }
    state.borrow_mut().link_level += 1;
    host.inline_tokenize(state);
    state.borrow_mut().link_level -= 1;
    state.borrow_mut().push("link_close", "a", -1);
}

fn emit_reference_link(
    state: &SharedInlineState,
    host: &Rc<dyn Host>,
    label_start: usize,
    label_end: usize,
    label: &str,
    href: &str,
    title: &str,
) {
    let store =
        !label.is_empty() && state.borrow().md.options.borrow().extra_bool("store_labels", false);
    state.borrow_mut().pos = label_start;
    state.borrow_mut().pos_max = label_end;
    let token = state.borrow_mut().push("link_open", "a", 1);
    {
        let mut t = token.borrow_mut();
        t.attrs = vec![("href".to_string(), crate::token::AttrVal::Str(href.to_string()))];
        if !title.is_empty() {
            t.attr_set("title", crate::token::AttrVal::Str(title.to_string()));
        }
        if store {
            t.meta = vec![("label".to_string(), crate::MetaVal::Str(label.to_string()))];
        }
    }
    state.borrow_mut().link_level += 1;
    host.inline_tokenize(state);
    state.borrow_mut().link_level -= 1;
    state.borrow_mut().push("link_close", "a", -1);
}
