//! `list` rule (mirror of `markdown_it/rules_block/list.py`), including
//! GFM task checkboxes (`- [x]`, gated by the `tasklists` option).

use std::rc::Rc;

use crate::{Host, SharedBlockState};

pub fn skip_bullet_list_marker(state: &SharedBlockState, start_line: usize) -> i64 {
    let st = state.borrow();
    let pos = st.b_marks[start_line] + st.t_shift[start_line];
    let maximum = st.e_marks[start_line];
    let marker = match st.src.get(pos) {
        Some(&c) => c,
        None => return -1,
    };
    let pos = pos + 1;
    if !matches!(marker, '*' | '-' | '+') {
        return -1;
    }
    if pos < maximum && !matches!(st.src[pos], '\t' | ' ') {
        return -1;
    }
    pos as i64
}

pub fn skip_ordered_list_marker(state: &SharedBlockState, start_line: usize) -> i64 {
    let st = state.borrow();
    let start = st.b_marks[start_line] + st.t_shift[start_line];
    let mut pos = start;
    let maximum = st.e_marks[start_line];
    if pos + 1 >= maximum {
        return -1;
    }
    let ch = st.src[pos] as u32;
    pos += 1;
    if !(0x30..=0x39).contains(&ch) {
        return -1;
    }
    loop {
        if pos >= maximum {
            return -1;
        }
        let ch = st.src[pos] as u32;
        pos += 1;
        if (0x30..=0x39).contains(&ch) {
            // List marker should have no more than 9 digits.
            if pos - start >= 10 {
                return -1;
            }
            continue;
        }
        if ch == 0x29 || ch == 0x2E {
            break;
        }
        return -1;
    }
    if pos < maximum && !matches!(st.src[pos], '\t' | ' ') {
        return -1;
    }
    pos as i64
}

fn mark_tight_paragraphs(state: &SharedBlockState, idx: usize) {
    let level = state.borrow().level + 2;
    let mut i = idx + 2;
    let length = state.borrow().tokens.len().saturating_sub(2);
    while i < length {
        let is_target = {
            let st = state.borrow();
            st.tokens[i].borrow().level == level && st.tokens[i].borrow().typ == "paragraph_open"
        };
        if is_target {
            state.borrow().tokens[i + 2].borrow_mut().hidden = true;
            state.borrow().tokens[i].borrow_mut().hidden = true;
            i += 2;
        }
        i += 1;
    }
}

pub fn list_block(
    state: &SharedBlockState,
    start_line: usize,
    end_line: usize,
    silent: bool,
    host: &Rc<dyn Host>,
) -> bool {
    let mut tight = true;
    // Phase 1: detect marker (immutable).
    struct Detected {
        is_ordered: bool,
        pos_after_marker: usize,
        marker_value: i64,
        marker_char: char,
        is_terminating_paragraph: bool,
    }
    let detected = {
        let st = state.borrow();
        if st.is_code_block(start_line) {
            return false;
        }
        if st.list_indent >= 0
            && (st.s_count[start_line] as i64) - st.list_indent >= 4
            && (st.s_count[start_line] as i64) < (st.blk_indent as i64)
        {
            return false;
        }
        let is_terminating = st.parent_type == "paragraph"
            && (st.s_count[start_line] as i64) >= (st.blk_indent as i64);
        drop(st);
        let ordered = skip_ordered_list_marker(state, start_line);
        if ordered >= 0 {
            let (start, marker_value) = {
                let st = state.borrow();
                let start = st.b_marks[start_line] + st.t_shift[start_line];
                let text: String = st.src[start..ordered as usize - 1].iter().collect();
                (start, text.parse::<i64>().unwrap_or(0))
            };
            let _ = start;
            if silent && is_terminating && marker_value != 1 {
                return false;
            }
            let marker_char = state.borrow().src[ordered as usize - 1];
            Detected {
                is_ordered: true,
                pos_after_marker: ordered as usize,
                marker_value,
                marker_char,
                is_terminating_paragraph: silent && is_terminating,
            }
        } else {
            let bullet = skip_bullet_list_marker(state, start_line);
            if bullet < 0 {
                return false;
            }
            let marker_char = state.borrow().src[bullet as usize - 1];
            Detected {
                is_ordered: false,
                pos_after_marker: bullet as usize,
                marker_value: 0,
                marker_char,
                is_terminating_paragraph: silent && is_terminating,
            }
        }
    };
    // Verbatim: right after a paragraph, the first line must not be
    // empty (applies to ordered too despite the comment naming unordered;
    // the ordered-`1` check ran during detection).
    if detected.is_terminating_paragraph {
        let st = state.borrow();
        if st.skip_spaces(detected.pos_after_marker) >= st.e_marks[start_line] {
            return false;
        }
    }
    // Verbatim: the unordered-empty check above runs for silent too; the
    // ordered-1 check ran during detection. Validation mode exits here.
    if silent {
        // Re-check the exact verbatim order: silent + parentType paragraph +
        // sCount >= blkIndent sets the flag; ordered!=1 fails; unordered
        // empty fails; otherwise True.
        return true;
    }
    let _ = detected.is_terminating_paragraph;

    let list_tok_idx = state.borrow().tokens.len();
    let list_open = {
        let mut st = state.borrow_mut();
        let token = if detected.is_ordered {
            let t = st.push("ordered_list_open", "ol", 1);
            if detected.marker_value != 1 {
                t.borrow_mut().attrs = vec![(
                    "start".to_string(),
                    crate::token::AttrVal::Int(detected.marker_value),
                )];
            }
            t
        } else {
            st.push("bullet_list_open", "ul", 1)
        };
        {
            let mut t = token.borrow_mut();
            t.map = Some((start_line, 0));
            t.markup = detected.marker_char.to_string();
        }
        token
    };

    let mut next_line = start_line;
    let mut start_line = start_line;
    let mut pos_after_marker = detected.pos_after_marker;
    let mut prev_empty_end = false;
    let old_parent_type = state.borrow().parent_type.clone();
    state.borrow_mut().parent_type = "list".to_string();

    loop {
        if next_line >= end_line {
            break;
        }
        let maximum = state.borrow().e_marks[next_line];
        let (mut pos, initial, offset) = {
            let st = state.borrow();
            let pos = pos_after_marker;
            let initial = st.s_count[next_line] + pos_after_marker
                - (st.b_marks[start_line] + st.t_shift[start_line]);
            (pos, initial, initial)
        };
        let mut offset = offset;
        while pos < maximum {
            let ch = state.borrow().src[pos];
            if ch == '\t' {
                let bs = state.borrow().bs_count[next_line];
                offset += 4 - (offset + bs) % 4;
            } else if ch == ' ' {
                offset += 1;
            } else {
                break;
            }
            pos += 1;
        }
        let content_start = pos;
        let mut indent_after_marker = if content_start >= maximum { 1 } else { offset - initial };
        if indent_after_marker > 4 {
            indent_after_marker = 1;
        }
        let indent = initial + indent_after_marker;

        let item_open = {
            let mut st = state.borrow_mut();
            let token = st.push("list_item_open", "li", 1);
            {
                let mut t = token.borrow_mut();
                t.markup = detected.marker_char.to_string();
                t.map = Some((start_line, 0));
                if detected.is_ordered {
                    let info: String = st.src[st.b_marks[start_line] + st.t_shift[start_line]
                        ..pos_after_marker - 1]
                        .iter()
                        .collect();
                    t.info = info;
                }
            }
            token
        };
        let item_lines_start = start_line;
        // Task checkbox detection.
        let mut checkbox_len = 0usize;
        if state.borrow().md.options.borrow().extra_bool("tasklists", false) && content_start < maximum {
            let (src_vec,): (Vec<char>,) = {
                let st = state.borrow();
                (st.src.clone(),)
            };
            if let Some(checked) = detect_task_checkbox(&src_vec, content_start, maximum) {
                item_open.borrow_mut().meta =
                    vec![("checked".to_string(), crate::MetaVal::Bool(checked))];
                checkbox_len = 4;
            }
        }
        // Change current state, then restore after the subcall.
        let (old_tight, old_bmark, old_tshift, old_scount, old_list_indent, old_blk_indent) = {
            let mut st = state.borrow_mut();
            let saved = (
                st.tight,
                st.b_marks[start_line],
                st.t_shift[start_line],
                st.s_count[start_line],
                st.list_indent,
                st.blk_indent,
            );
            st.list_indent = st.blk_indent as i64;
            st.blk_indent = indent;
            st.tight = true;
            st.t_shift[start_line] = content_start - st.b_marks[start_line];
            st.s_count[start_line] = offset;
            if checkbox_len > 0 {
                st.b_marks[start_line] = content_start + checkbox_len;
                st.t_shift[start_line] = 0;
            }
            saved
        };
        let empty_next = {
            let st = state.borrow();
            content_start >= maximum && st.is_empty(start_line + 1)
        };
        if empty_next {
            let line = (state.borrow().line + 2).min(end_line);
            state.borrow_mut().line = line;
        } else {
            host.tokenize_block(state, start_line, end_line);
        }
        if !state.borrow().tight || prev_empty_end {
            tight = false;
        }
        prev_empty_end = (state.borrow().line as i64 - start_line as i64) > 1
            && state.borrow().is_empty(state.borrow().line - 1);
        {
            let mut st = state.borrow_mut();
            // Verbatim: `blkIndent = listIndent; listIndent = oldListIndent`.
            st.blk_indent = old_blk_indent;
            st.list_indent = old_list_indent;
            if checkbox_len > 0 {
                st.b_marks[start_line] = old_bmark;
            }
            st.t_shift[start_line] = old_tshift;
            st.s_count[start_line] = old_scount;
            st.tight = old_tight;
        }
        {
            let mut st = state.borrow_mut();
            let token = st.push("list_item_close", "li", -1);
            token.borrow_mut().markup = detected.marker_char.to_string();
        }
        next_line = state.borrow().line;
        start_line = next_line;
        {
            // itemLines[1] = nextLine (verbatim alias into the open token).
            item_open.borrow_mut().map = Some((item_lines_start, next_line));
        }
        if next_line >= end_line {
            break;
        }
        if (state.borrow().s_count[next_line] as i64) < (state.borrow().blk_indent as i64) {
            break;
        }
        if state.borrow().is_code_block(start_line) {
            break;
        }
        if host.block_terminates("list", state, next_line, end_line) {
            break;
        }
        if detected.is_ordered {
            let m = skip_ordered_list_marker(state, next_line);
            if m < 0 {
                break;
            }
            pos_after_marker = m as usize;
        } else {
            let m = skip_bullet_list_marker(state, next_line);
            if m < 0 {
                break;
            }
            pos_after_marker = m as usize;
        }
        if detected.marker_char != state.borrow().src[pos_after_marker - 1] {
            break;
        }
    }

    if state.borrow().md.options.borrow().extra_bool("tasklists", false) {
        let mut contains_task = false;
        let level = list_open.borrow().level;
        let len = state.borrow().tokens.len();
        for j in list_tok_idx + 1..len {
            let is_item = {
                let st = state.borrow();
                let tok = st.tokens[j].borrow();
                tok.level == level + 1
                    && tok.typ == "list_item_open"
                    && tok.meta.iter().any(|(k, _)| k == "checked")
            };
            if is_item {
                state.borrow().tokens[j]
                    .borrow_mut()
                    .attr_join("class", "task-list-item")
                    .unwrap_or(());
                contains_task = true;
            }
        }
        if contains_task {
            list_open
                .borrow_mut()
                .attr_join("class", "contains-task-list")
                .unwrap_or_else(|e| crate::panic_type_error(e));
        }
    }
    {
        let mut st = state.borrow_mut();
        let token = if detected.is_ordered {
            st.push("ordered_list_close", "ol", -1)
        } else {
            st.push("bullet_list_close", "ul", -1)
        };
        token.borrow_mut().markup = detected.marker_char.to_string();
        let list_start = list_open.borrow().map.unwrap().0;
        list_open.borrow_mut().map = Some((list_start, next_line));
        st.line = next_line;
        st.parent_type = old_parent_type;
    }
    if tight {
        mark_tight_paragraphs(state, list_tok_idx);
    }
    true
}

fn detect_task_checkbox(src: &[char], pos: usize, maximum: usize) -> Option<bool> {
    if pos + 4 > maximum {
        return None;
    }
    if src.get(pos) != Some(&'[') {
        return None;
    }
    let inner = *src.get(pos + 1)?;
    if src.get(pos + 2) != Some(&']') {
        return None;
    }
    let checked = if inner == ' ' {
        false
    } else if inner == 'x' || inner == 'X' {
        true
    } else {
        return None;
    };
    if !matches!(src.get(pos + 3), Some(' ') | Some('\t')) {
        return None;
    }
    Some(checked)
}
