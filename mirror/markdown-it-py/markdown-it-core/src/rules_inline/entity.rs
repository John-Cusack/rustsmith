//! `entity` rule (mirror of `markdown_it/rules_inline/entity.py`).

use std::sync::LazyLock;

use regex::Regex;

use crate::common_utils::{from_code_point, is_valid_entity_code, lookup_entity};
use crate::SharedInlineState;

static DIGITAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^&#((?:x[a-f0-9]{1,6}|[0-9]{1,7}));").unwrap()
});
static NAMED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^&([a-z][a-z0-9]{1,31});").unwrap()
});

pub fn entity(state: &SharedInlineState, silent: bool) -> bool {
    let pos = state.borrow().pos;
    let pos_max = state.borrow().pos_max;
    if state.borrow().src[pos] != '&' {
        return false;
    }
    if pos + 1 >= pos_max {
        return false;
    }
    enum Hit {
        Digital { code: u32, markup: String, advance: usize },
        Named { name: String, markup: String, advance: usize },
    }
    // Immutable phase: match on the O(1) `&str` window (no copy).
    let hit: Option<Hit> = {
        let st = state.borrow();
        let window = st.text_range(pos, pos_max);
        if st.src[pos + 1] == '#' {
            DIGITAL_RE.find(window).map(|m| {
                let group1 = &window[m.start() + 2..m.end() - 1];
                let code = if group1[..1].eq_ignore_ascii_case("x") {
                    u32::from_str_radix(&group1[1..], 16).unwrap_or(0)
                } else {
                    group1.parse::<u32>().unwrap_or(0)
                };
                Hit::Digital {
                    code,
                    markup: window[m.start()..m.end()].to_string(),
                    advance: window[m.start()..m.end()].chars().count(),
                }
            })
        } else {
            NAMED_RE.find(window).and_then(|m| {
                let name = &window[m.start() + 1..m.end() - 1];
                if lookup_entity(name).is_some() {
                    Some(Hit::Named {
                        name: name.to_string(),
                        markup: window[m.start()..m.end()].to_string(),
                        advance: window[m.start()..m.end()].chars().count(),
                    })
                } else {
                    None
                }
            })
        }
    };
    let hit = match hit {
        Some(h) => h,
        None => return false,
    };
    if !silent {
        let (content, markup, advance) = match hit {
            Hit::Digital { code, markup, advance } => {
                let content = if is_valid_entity_code(code) {
                    from_code_point(code)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "\u{FFFD}".to_string())
                } else {
                    "\u{FFFD}".to_string()
                };
                (content, markup, advance)
            }
            Hit::Named { name, markup, advance } => {
                (lookup_entity(&name).unwrap_or("").to_string(), markup, advance)
            }
        };
        let token = state.borrow_mut().push("text_special", "", 0);
        {
            let mut t = token.borrow_mut();
            t.content = content;
            t.markup = markup;
            t.info = "entity".to_string();
        }
        state.borrow_mut().pos += advance;
    } else {
        let advance = match hit {
            Hit::Digital { advance, .. } | Hit::Named { advance, .. } => advance,
        };
        state.borrow_mut().pos += advance;
    }
    true
}
