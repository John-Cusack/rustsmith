//! `lheading` rule (mirror of `markdown_it/rules_block/lheading.py`).

use std::rc::Rc;

use crate::common_utils::md_trim;
use crate::{Host, SharedBlockState};

pub fn lheading(
    state: &SharedBlockState,
    start_line: usize,
    end_line: usize,
    _silent: bool,
    host: &Rc<dyn Host>,
) -> bool {
    // Verbatim control flow with the `parentType` mutation visible to
    // terminator rules (no state borrow is held across the host call).
    if state.borrow().is_code_block(start_line) {
        return false;
    }
    let old_parent_type = state.borrow().parent_type.clone();
    state.borrow_mut().parent_type = "paragraph".to_string();
    let mut next_line = start_line + 1;
    let mut level: Option<u32> = None;
    let mut marker = '\0';
    loop {
        let cont = {
            let st = state.borrow();
            next_line < end_line && !st.is_empty(next_line)
        };
        if !cont {
            break;
        }
        let (s_count, blk_indent) = {
            let st = state.borrow();
            (st.s_count[next_line] as i64, st.blk_indent)
        };
        // This would be a code block normally, but after paragraph it is a
        // lazy continuation regardless of what is there.
        if s_count - blk_indent as i64 > 3 {
            next_line += 1;
            continue;
        }
        // Check for underline in setext header.
        if s_count >= blk_indent as i64 {
            let (pos, maximum, m) = {
                let st = state.borrow();
                let pos = st.b_marks[next_line] + st.t_shift[next_line];
                let maximum = st.e_marks[next_line];
                let m = if pos < maximum { Some(st.src[pos]) } else { None };
                (pos, maximum, m)
            };
            if let Some(m) = m {
                if m == '-' || m == '=' {
                    let done = {
                        let st = state.borrow();
                        let mut p = st.skip_chars_str(pos, m);
                        p = st.skip_spaces(p);
                        p >= maximum
                    };
                    if done {
                        level = Some(if m == '=' { 1 } else { 2 });
                        marker = m;
                        break;
                    }
                }
            }
        }
        // Quirk for blockquotes, this line should already be checked by that rule.
        if s_count < 0 {
            next_line += 1;
            continue;
        }
        // Some tags can terminate paragraph without empty line.
        if host.block_terminates("paragraph", state, next_line, end_line) {
            break;
        }
        next_line += 1;
    }
    let level = match level {
        Some(l) => l,
        // Verbatim: `parentType` is NOT restored on this path.
        None => return false,
    };
    let content = {
        let st = state.borrow();
        md_trim(&st.get_lines(start_line, next_line, st.blk_indent, false))
    };
    let mut st = state.borrow_mut();
    st.line = next_line + 1;
    let tag = format!("h{level}");
    let token = st.push("heading_open", &tag, 1);
    {
        let mut t = token.borrow_mut();
        t.markup = marker.to_string();
        t.map = Some((start_line, st.line));
    }
    let token = st.push("inline", "", 0);
    {
        let line = st.line;
        let mut t = token.borrow_mut();
        t.content = content;
        t.map = Some((start_line, line - 1));
        t.children = Some(Vec::new());
    }
    let token = st.push("heading_close", &tag, -1);
    token.borrow_mut().markup = marker.to_string();
    st.parent_type = old_parent_type;
    true
}
