//! `image` rule (mirror of `markdown_it/rules_inline/image.py`).

use std::rc::Rc;

use crate::common_utils::{is_str_space, normalize_reference};
use crate::helpers::{parse_link_destination, parse_link_label, parse_link_title};
use crate::{Host, SharedInlineState, SharedToken};

pub fn image(state: &SharedInlineState, silent: bool, host: &Rc<dyn Host>) -> bool {
    let old_pos = state.borrow().pos;
    let pos_max = state.borrow().pos_max;
    if state.borrow().src[old_pos] != '!' {
        return false;
    }
    if !(old_pos + 1 < pos_max && state.borrow().src[old_pos + 1] == '[') {
        return false;
    }
    let label_start = old_pos + 2;
    let label_end = parse_link_label(state, old_pos + 1, false, host);
    if label_end < 0 {
        return false;
    }
    let label_end = label_end as usize;
    let mut pos = label_end + 1;
    let mut href = String::new();
    let title: String;
    let mut label: Option<String> = None;
    let mut is_inline = false;

    if pos < pos_max && state.borrow().src[pos] == '(' {
        is_inline = true;
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
        } else {
            title = String::new();
        }
        if pos >= pos_max || state.borrow().src[pos] != ')' {
            state.borrow_mut().pos = old_pos;
            return false;
        }
        pos += 1;
    } else {
        if state.borrow().env.references.is_empty() {
            return false;
        }
        if pos < pos_max && state.borrow().src[pos] == '[' {
            let start = pos + 1;
            let end = parse_link_label(state, pos, false, host);
            if end >= 0 {
                label = Some(state.borrow().src[start..end as usize].iter().collect());
                pos = end as usize + 1;
            } else {
                pos = label_end + 1;
            }
        } else {
            pos = label_end + 1;
        }
        let lab = match label {
            Some(l) if !l.is_empty() => l,
            _ => state.borrow().src[label_start..label_end].iter().collect(),
        };
        // Verbatim: `label` is rebound to the normalized form before use.
        label = Some(normalize_reference(&lab));
        let norm = label.clone().unwrap();
        let found = state.borrow().env.references.get(&norm).cloned();
        match found {
            Some(r) => {
                href = r.href;
                title = r.title;
            }
            None => {
                state.borrow_mut().pos = old_pos;
                return false;
            }
        }
    }

    if !silent {
        let content: String = state.borrow().src[label_start..label_end].iter().collect();
        // Full inline parse of the alt content (verbatim `md.inline.parse`).
        let alt_tokens: Vec<SharedToken> = host.inline_parse_content(state, &content);
        let token = state.borrow_mut().push("image", "img", 0);
        {
            let mut t = token.borrow_mut();
            t.attrs = vec![
                ("src".to_string(), crate::token::AttrVal::Str(href)),
                ("alt".to_string(), crate::token::AttrVal::Str(String::new())),
            ];
            t.children = if alt_tokens.is_empty() { None } else { Some(alt_tokens) };
            t.content = content;
            if !title.is_empty() {
                t.attr_set("title", crate::token::AttrVal::Str(title));
            }
            let store = label.as_ref().map(|l| !l.is_empty()).unwrap_or(false)
                && state.borrow().md.options.borrow().extra_bool("store_labels", false);
            if store {
                t.meta = vec![("label".to_string(), crate::MetaVal::Str(label.unwrap()))];
            }
        }
    }
    {
        let mut st = state.borrow_mut();
        st.pos = pos;
        st.pos_max = pos_max;
    }
    let _ = is_inline;
    true
}
