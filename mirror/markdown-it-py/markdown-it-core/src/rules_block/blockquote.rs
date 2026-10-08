//! `blockquote` rule (mirror of `markdown_it/rules_block/blockquote.py`),
//! including GitHub-style alerts (`> [!NOTE]`).
//!
//! `s_count` stays `Vec<usize>`; the `-1` paragraph-continuation marker and
//! the blkIndent rebase use `wrapping_sub`, preserving two's-complement
//! semantics for the `as i64` comparisons (verbatim Python ints).

use std::rc::Rc;

use crate::token::AttrVal;
use crate::{Host, MetaVal, SharedBlockState, SharedToken};

const ALERT_TYPES: &[&str] = &["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];

pub fn blockquote(
    state: &SharedBlockState,
    start_line: usize,
    end_line: usize,
    silent: bool,
    host: &Rc<dyn Host>,
) -> bool {
    struct Saved {
        b: usize,
        bs: usize,
        t: usize,
        s: usize,
    }
    let mut saved: Vec<Saved> = Vec::new();
    let old_line_max = state.borrow().line_max;

    // First-line marker check + adjustment (immutable phase).
    struct First {
        /// Post-`>` position (pre-space-loop): the new `bMarks`.
        bmark: usize,
        /// Post-space-loop position (content start).
        pos: usize,
        max: usize,
        initial: usize,
        offset: usize,
        adjust_tab: bool,
        space_after: bool,
    }
    let first = {
        let st = state.borrow();
        if st.is_code_block(start_line) {
            return false;
        }
        let pos = st.b_marks[start_line] + st.t_shift[start_line];
        let max = st.e_marks[start_line];
        if st.src.get(pos) != Some(&'>') {
            return false;
        }
        if silent {
            return true;
        }
        let mut p = pos + 1;
        let mut initial = st.s_count[start_line] + 1;
        let mut offset = initial;
        let mut adjust_tab = false;
        let mut space_after = false;
        match st.src.get(p).copied() {
            Some(' ') => {
                p += 1;
                initial += 1;
                offset += 1;
                space_after = true;
            }
            Some('\t') => {
                space_after = true;
                if (st.bs_count[start_line] + offset) % 4 == 3 {
                    p += 1;
                    initial += 1;
                    offset += 1;
                } else {
                    adjust_tab = true;
                }
            }
            _ => {}
        }
        let mut q = p;
        while q < max {
            let ch = st.src[q];
            if ch == '\t' || ch == ' ' {
                if ch == '\t' {
                    offset += 4 - (offset + st.bs_count[start_line] + usize::from(adjust_tab)) % 4;
                } else {
                    offset += 1;
                }
            } else {
                break;
            }
            q += 1;
        }
        First { bmark: p, pos: q, max, initial, offset, adjust_tab, space_after }
    };
    {
        let mut st = state.borrow_mut();
        saved.push(Saved {
            b: st.b_marks[start_line],
            bs: st.bs_count[start_line],
            t: st.t_shift[start_line],
            s: st.s_count[start_line],
        });
        let base = st.s_count[start_line];
        st.b_marks[start_line] = first.bmark;
        st.bs_count[start_line] = base + 1 + usize::from(first.space_after);
        st.s_count[start_line] = first.offset - first.initial;
        st.t_shift[start_line] = first.pos - st.b_marks[start_line];
    }
    let old_parent_type = state.borrow().parent_type.clone();
    state.borrow_mut().parent_type = "blockquote".to_string();

    let mut next_line = start_line + 1;
    let mut last_line_empty = first.pos >= first.max;
    let _ = first.adjust_tab;
    loop {
        if next_line >= end_line {
            break;
        }
        let is_outdented =
            (state.borrow().s_count[next_line] as i64) < (state.borrow().blk_indent as i64);
        let (pos, max) = {
            let st = state.borrow();
            (st.b_marks[next_line] + st.t_shift[next_line], st.e_marks[next_line])
        };
        if pos >= max {
            break;
        }

        if state.borrow().src[pos] == '>' && !is_outdented {
            let mut p = pos + 1;
            let base = state.borrow().s_count[next_line] + 1;
            let (mut initial, mut offset) = (base, base);
            let (mut adjust_tab, mut space_after) = (false, false);
            match state.borrow().src.get(p).copied() {
                Some(' ') => {
                    p += 1;
                    initial += 1;
                    offset += 1;
                    space_after = true;
                }
                Some('\t') => {
                    space_after = true;
                    if (state.borrow().bs_count[next_line] + offset) % 4 == 3 {
                        p += 1;
                        initial += 1;
                        offset += 1;
                    } else {
                        adjust_tab = true;
                    }
                }
                _ => {}
            }
            let bmark = p;
            while p < max {
                let (ch, bs) = {
                    let st = state.borrow();
                    (st.src[p], st.bs_count[next_line])
                };
                if ch == '\t' || ch == ' ' {
                    if ch == '\t' {
                        offset += 4 - (offset + bs + usize::from(adjust_tab)) % 4;
                    } else {
                        offset += 1;
                    }
                } else {
                    break;
                }
                p += 1;
            }
            last_line_empty = p >= max;
            {
                let mut st = state.borrow_mut();
                saved.push(Saved {
                    b: st.b_marks[next_line],
                    bs: st.bs_count[next_line],
                    t: st.t_shift[next_line],
                    s: st.s_count[next_line],
                });
                let base = st.s_count[next_line];
                st.b_marks[next_line] = bmark;
                st.bs_count[next_line] = base + 1 + usize::from(space_after);
                st.s_count[next_line] = offset - initial;
                st.t_shift[next_line] = p - st.b_marks[next_line];
            }
            next_line += 1;
            continue;
        }
        if last_line_empty {
            break;
        }
        if host.block_terminates("blockquote", state, next_line, end_line) {
            let mut st = state.borrow_mut();
            st.line_max = next_line;
            if st.blk_indent != 0 {
                saved.push(Saved {
                    b: st.b_marks[next_line],
                    bs: st.bs_count[next_line],
                    t: st.t_shift[next_line],
                    s: st.s_count[next_line],
                });
                let bi = st.blk_indent;
                st.s_count[next_line] = st.s_count[next_line].wrapping_sub(bi);
            }
            break;
        }
        {
            let mut st = state.borrow_mut();
            saved.push(Saved {
                b: st.b_marks[next_line],
                bs: st.bs_count[next_line],
                t: st.t_shift[next_line],
                s: st.s_count[next_line],
            });
            // A negative indentation means that this is a paragraph continuation.
            st.s_count[next_line] = 0usize.wrapping_sub(1);
        }
        next_line += 1;
    }
    let old_indent = state.borrow().blk_indent;
    state.borrow_mut().blk_indent = 0;

    let alerts_on = state.borrow().md.options.borrow().extra_bool("alerts", false);
    let alert_kind = if alerts_on && next_line > start_line {
        detect_alert(state, start_line)
    } else {
        None
    };
    // The opening token handle is held so its `map` end updates verbatim
    // (the original aliases the `lines` list into the token).
    let open_token: SharedToken;
    if let Some(kind) = alert_kind {
        let lower = kind.to_lowercase();
        let title = capitalize(&kind);
        {
            let mut st = state.borrow_mut();
            let token = st.push("alert_open", "div", 1);
            {
                let mut t = token.borrow_mut();
                t.markup = ">".to_string();
                t.attr_set("class", AttrVal::Str(format!("markdown-alert markdown-alert-{lower}")));
                t.map = Some((start_line, 0));
                t.info = kind.clone();
                t.meta = vec![("kind".to_string(), MetaVal::Str(kind))];
            }
            open_token = token;
            let token = st.push("alert_title_open", "p", 1);
            token.borrow_mut().attr_set("class", AttrVal::Str("markdown-alert-title".to_string()));
            let title_token = st.push("inline", "", 0);
            {
                let mut t = title_token.borrow_mut();
                t.content = title;
                t.children = Some(Vec::new());
            }
            st.push("alert_title_close", "p", -1);
        }
        let content_start = start_line + 1;
        if content_start < next_line {
            host.tokenize_block(state, content_start, next_line);
        } else {
            state.borrow_mut().line = next_line;
        }
        state.borrow_mut().push("alert_close", "div", -1).borrow_mut().markup = ">".to_string();
    } else {
        {
            let mut st = state.borrow_mut();
            let token = st.push("blockquote_open", "blockquote", 1);
            {
                let mut t = token.borrow_mut();
                t.markup = ">".to_string();
                t.map = Some((start_line, 0));
            }
            open_token = token;
        }
        host.tokenize_block(state, start_line, next_line);
        state.borrow_mut().push("blockquote_close", "blockquote", -1).borrow_mut().markup =
            ">".to_string();
    }
    {
        let mut st = state.borrow_mut();
        st.line_max = old_line_max;
        st.parent_type = old_parent_type;
        let end = st.line;
        for (i, s) in saved.iter().enumerate() {
            st.b_marks[i + start_line] = s.b;
            st.t_shift[i + start_line] = s.t;
            st.s_count[i + start_line] = s.s;
            st.bs_count[i + start_line] = s.bs;
        }
        st.blk_indent = old_indent;
        open_token.borrow_mut().map = Some((start_line, end));
    }
    true
}

/// Python `str.capitalize`: first char upper, the rest lower.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut out = String::new();
            for c in first.to_uppercase() {
                out.push(c);
            }
            out.push_str(&chars.collect::<String>().to_lowercase());
            out
        }
    }
}

fn detect_alert(state: &SharedBlockState, start_line: usize) -> Option<String> {
    let st = state.borrow();
    let pos = st.b_marks[start_line] + st.t_shift[start_line];
    let mut maximum = st.e_marks[start_line];
    while maximum > pos && matches!(st.src.get(maximum - 1), Some(' ') | Some('\t')) {
        maximum -= 1;
    }
    if maximum.saturating_sub(pos) < 4 {
        return None;
    }
    if st.src.get(pos) != Some(&'[') || st.src.get(pos + 1) != Some(&'!') {
        return None;
    }
    if st.src.get(maximum - 1) != Some(&']') {
        return None;
    }
    let type_str: String = st.src[pos + 2..maximum - 1].iter().collect();
    let upper = type_str.to_uppercase();
    if ALERT_TYPES.contains(&upper.as_str()) {
        Some(upper)
    } else {
        None
    }
}
