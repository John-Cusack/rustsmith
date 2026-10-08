//! Link helpers (mirror of `markdown_it/helpers/parse_link_*.py`).
//!
//! All cursors are char indices over `&[char]`.

use std::rc::Rc;

use crate::common_utils::unescape_all;
use crate::{Host, SharedInlineState};

pub struct LinkDest {
    pub ok: bool,
    pub pos: usize,
    pub text: String,
}

fn code_at(s: &[char], pos: usize) -> Option<u32> {
    s.get(pos).map(|&c| c as u32)
}

fn collect(s: &[char], from: usize, to: usize) -> String {
    s[from..to].iter().collect()
}

pub fn parse_link_destination(s: &[char], mut pos: usize, maximum: usize) -> LinkDest {
    let start = pos;
    let bad = LinkDest { ok: false, pos: 0, text: String::new() };
    if code_at(s, pos) == Some(0x3C) {
        pos += 1;
        while pos < maximum {
            let code = code_at(s, pos);
            if code == Some(0x0A) {
                return bad;
            }
            if code == Some(0x3C) {
                return bad;
            }
            if code == Some(0x3E) {
                return LinkDest {
                    ok: true,
                    pos: pos + 1,
                    text: unescape_all(&collect(s, start + 1, pos)),
                };
            }
            if code == Some(0x5C) && pos + 1 < maximum {
                pos += 2;
                continue;
            }
            pos += 1;
        }
        return bad;
    }
    let mut level = 0i64;
    while pos < maximum {
        let code = code_at(s, pos);
        if code.is_none() || code == Some(0x20) {
            break;
        }
        let code = code.unwrap();
        if code < 0x20 || code == 0x7F {
            break;
        }
        if code == 0x5C && pos + 1 < maximum {
            if code_at(s, pos + 1) == Some(0x20) {
                break;
            }
            pos += 2;
            continue;
        }
        if code == 0x28 {
            level += 1;
            if level > 32 {
                return bad;
            }
        }
        if code == 0x29 {
            if level == 0 {
                break;
            }
            level -= 1;
        }
        pos += 1;
    }
    if start == pos || level != 0 {
        return bad;
    }
    LinkDest { ok: true, pos, text: unescape_all(&collect(s, start, pos)) }
}

#[derive(Debug, Clone)]
pub struct LinkTitle {
    pub ok: bool,
    pub can_continue: bool,
    pub pos: usize,
    pub text: String,
    pub marker: u32,
}

fn fail_title() -> LinkTitle {
    LinkTitle { ok: false, can_continue: false, pos: 0, text: String::new(), marker: 0 }
}

pub fn parse_link_title(
    s: &[char],
    start: usize,
    maximum: usize,
    prev: Option<&LinkTitle>,
) -> LinkTitle {
    if let Some(p) = prev {
        // Continuation of a previous parse on the next line (references
        // only): scan from `start` with the previous marker; the accumulated
        // prefix stays in `text` (verbatim).
        let mut text = p.text.clone();
        return continue_title_from(s, start, maximum, p.marker, &mut text);
    }
    if start >= maximum {
        return fail_title();
    }
    let m = code_at(s, start).unwrap_or(0);
    // /* " */  /* ' */  /* ( */
    if m != 0x22 && m != 0x27 && m != 0x28 {
        return fail_title();
    }
    // If opening marker is "(", switch it to closing marker ")".
    let marker = if m == 0x28 { 0x29 } else { m };
    let mut text = String::new();
    continue_title_from(s, start + 1, maximum, marker, &mut text)
}

fn continue_title_from(
    s: &[char],
    start: usize,
    maximum: usize,
    marker: u32,
    text: &mut String,
) -> LinkTitle {
    let mut pos = start;
    while pos < maximum {
        let code = code_at(s, pos).unwrap_or(0);
        if code == marker {
            *text += &unescape_all(&collect(s, start, pos));
            return LinkTitle { ok: true, can_continue: false, pos: pos + 1, text: text.clone(), marker };
        } else if code == 0x28 && marker == 0x29 {
            return LinkTitle { ok: false, can_continue: false, pos: 0, text: text.clone(), marker };
        } else if code == 0x5C && pos + 1 < maximum {
            pos += 1;
        }
        pos += 1;
    }
    // No closing marker found, but this link title may continue on the next
    // line (for references).
    *text += &unescape_all(&collect(s, start, pos));
    LinkTitle { ok: false, can_continue: true, pos: 0, text: text.clone(), marker }
}

/// Parse a link label starting at `[` (`start`); returns the index of the
/// closing `]` or -1. Restores `state.pos` verbatim.
pub fn parse_link_label(
    state: &SharedInlineState,
    start: usize,
    disable_nested: bool,
    host: &Rc<dyn Host>,
) -> i64 {
    let old_pos = state.borrow().pos;
    state.borrow_mut().pos = start + 1;
    let mut level = 1i64;
    let mut found = false;
    loop {
        let pos = state.borrow().pos;
        let pos_max = state.borrow().pos_max;
        if pos >= pos_max {
            break;
        }
        let marker = state.borrow().src[pos];
        if marker == ']' {
            level -= 1;
            if level == 0 {
                found = true;
                break;
            }
        }
        let prev_pos = state.borrow().pos;
        host.inline_skip_token(state);
        if marker == '[' {
            if prev_pos == state.borrow().pos - 1 {
                level += 1;
            } else if disable_nested {
                state.borrow_mut().pos = old_pos;
                return -1;
            }
        }
    }
    let label_end = if found { state.borrow().pos as i64 } else { -1 };
    state.borrow_mut().pos = old_pos;
    label_end
}
