//! `reference` rule (mirror of `markdown_it/rules_block/reference.py`).

use std::rc::Rc;

use crate::common_utils::{is_space, normalize_reference};
use crate::helpers::{parse_link_destination, parse_link_title};
use crate::{DuplicateRef, Host, MetaVal, Reference, SharedBlockState};

pub fn reference(
    state: &SharedBlockState,
    start_line: usize,
    _end_line: usize,
    silent: bool,
    host: &Rc<dyn Host>,
) -> bool {
    let scan = match scan_reference(state, start_line, host) {
        Some(s) => s,
        None => return false,
    };
    if scan.label.is_empty() {
        // CommonMark 0.20 disallows empty labels.
        return false;
    }
    // Reference can not terminate anything. This check is for safety only.
    if silent {
        return true;
    }
    let mut st = state.borrow_mut();
    st.line = scan.next_line;
    let line = st.line;
    if st.md.options.borrow().extra_bool("inline_definitions", false) {
        let token = st.push("definition", "", 0);
        {
            let mut t = token.borrow_mut();
            t.meta = vec![
                ("id".to_string(), MetaVal::Str(scan.label.clone())),
                ("title".to_string(), MetaVal::Str(scan.title.clone())),
                ("url".to_string(), MetaVal::Str(scan.href.clone())),
                ("label".to_string(), MetaVal::Str(scan.raw_label.clone())),
            ];
            t.map = Some((start_line, st.line));
        }
    }
    if !st.env.references.contains_key(&scan.label) {
        st.env.references.insert(
            scan.label.clone(),
            Reference {
                title: scan.title,
                href: scan.href,
                map: Some((start_line, line)),
            },
        );
    } else {
        st.env.duplicate_refs.push(DuplicateRef {
            href: scan.href,
            label: scan.label,
            map: Some((start_line, line)),
            title: scan.title,
        });
    }
    true
}

fn get_next_line(state: &SharedBlockState, next_line: usize, host: &Rc<dyn Host>) -> Option<String> {
    if next_line >= state.borrow().line_max || state.borrow().is_empty(next_line) {
        return None;
    }
    let mut is_continuation = false;
    if state.borrow().is_code_block(next_line) {
        is_continuation = true;
    }
    if (state.borrow().s_count[next_line] as i64) < 0 {
        is_continuation = true;
    }
    if !is_continuation {
        // Verbatim: parentType is "reference" while terminators run.
        let old = state.borrow().parent_type.clone();
        state.borrow_mut().parent_type = "reference".to_string();
        let terminate = host.block_terminates("reference", state, next_line, state.borrow().line_max);
        state.borrow_mut().parent_type = old;
        if terminate {
            return None;
        }
    }
    let st = state.borrow();
    let pos = st.b_marks[next_line] + st.t_shift[next_line];
    let maximum = st.e_marks[next_line];
    // max + 1 explicitly includes the newline.
    Some(st.src[pos..(maximum + 1).min(st.src.len())].iter().collect())
}

fn code_at(s: &[char], pos: usize) -> Option<u32> {
    s.get(pos).map(|&c| c as u32)
}

struct ScanOut {
    label: String,
    raw_label: String,
    href: String,
    title: String,
    next_line: usize,
}

fn scan_reference(
    state: &SharedBlockState,
    start_line: usize,
    host: &Rc<dyn Host>,
) -> Option<ScanOut> {
    let (mut string, mut next_line) = {
        let st = state.borrow();
        if st.is_code_block(start_line) {
            return None;
        }
        let pos = st.b_marks[start_line] + st.t_shift[start_line];
        let maximum = st.e_marks[start_line];
        if st.src.get(pos) != Some(&'[') {
            return None;
        }
        let s: Vec<char> = st.src[pos..(maximum + 1).min(st.src.len())].to_vec();
        (s, start_line + 1)
    };

    let mut maximum = string.len();
    let mut label_end: Option<usize> = None;
    let mut pos = 1usize;
    while pos < maximum {
        let ch = code_at(&string, pos);
        if ch == Some(0x5B) {
            return None;
        } else if ch == Some(0x5D) {
            label_end = Some(pos);
            break;
        } else if ch == Some(0x0A) {
            if let Some(line) = get_next_line(state, next_line, host) {
                string.extend(line.chars());
                maximum = string.len();
                next_line += 1;
            }
        } else if ch == Some(0x5C) {
            pos += 1;
            if pos < maximum && code_at(&string, pos) == Some(0x0A) {
                if let Some(line) = get_next_line(state, next_line, host) {
                    string.extend(line.chars());
                    maximum = string.len();
                    next_line += 1;
                }
            }
        }
        pos += 1;
    }
    let label_end = label_end?;
    if code_at(&string, label_end + 1) != Some(0x3A) {
        return None;
    }
    // Skip optional whitespace after the colon.
    pos = label_end + 2;
    while pos < maximum {
        let ch = code_at(&string, pos);
        if ch == Some(0x0A) {
            if let Some(line) = get_next_line(state, next_line, host) {
                string.extend(line.chars());
                maximum = string.len();
                next_line += 1;
            }
        } else if is_space(ch) {
            // skip
        } else {
            break;
        }
        pos += 1;
    }
    let dest = parse_link_destination(&string, pos, maximum);
    if !dest.ok {
        return None;
    }
    let href = host.normalize_link(&dest.text);
    if !host.validate_link(&href, None) {
        return None;
    }
    pos = dest.pos;
    let dest_end_pos = pos;
    let dest_end_line_no = next_line;
    // Skip spaces before the title.
    let start = pos;
    while pos < maximum {
        let ch = code_at(&string, pos);
        if ch == Some(0x0A) {
            if let Some(line) = get_next_line(state, next_line, host) {
                string.extend(line.chars());
                maximum = string.len();
                next_line += 1;
            }
        } else if is_space(ch) {
            // skip
        } else {
            break;
        }
        pos += 1;
    }
    let mut title_res = parse_link_title(&string, pos, maximum, None);
    while title_res.can_continue {
        let Some(line) = get_next_line(state, next_line, host) else {
            break;
        };
        string.extend(line.chars());
        pos = maximum;
        maximum = string.len();
        next_line += 1;
        title_res = parse_link_title(&string, pos, maximum, Some(&title_res));
    }
    let (mut title, mut pos, mut next_line) = if pos < maximum && start != pos && title_res.ok {
        (title_res.text.clone(), title_res.pos, next_line)
    } else {
        (String::new(), dest_end_pos, dest_end_line_no)
    };
    pos = skip_spaces_only(&string, pos, maximum);
    if pos < maximum && code_at(&string, pos) != Some(0x0A) && !title.is_empty() {
        // Garbage after title: roll back to the destination end.
        title = String::new();
        pos = dest_end_pos;
        next_line = dest_end_line_no;
        pos = skip_spaces_only(&string, pos, maximum);
    }
    if pos < maximum && code_at(&string, pos) != Some(0x0A) {
        return None;
    }
    let raw_label: String = string[1..label_end].iter().collect();
    let label = normalize_reference(&raw_label);
    Some(ScanOut { label, raw_label, href, title, next_line })
}

fn skip_spaces_only(string: &[char], mut pos: usize, maximum: usize) -> usize {
    while pos < maximum {
        if !is_space(code_at(string, pos)) {
            break;
        }
        pos += 1;
    }
    pos
}
