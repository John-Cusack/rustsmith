//! `paragraph` rule (mirror of `markdown_it/rules_block/paragraph.py`).

use std::rc::Rc;

use crate::common_utils::md_trim;
use crate::{Host, SharedBlockState};

pub fn paragraph(
    state: &SharedBlockState,
    start_line: usize,
    _end_line: usize,
    _silent: bool,
    host: &Rc<dyn Host>,
) -> bool {
    // Verbatim: `endLine` is shadowed by `state.lineMax`.
    let end_line = state.borrow().line_max;
    let old_parent_type = state.borrow().parent_type.clone();
    state.borrow_mut().parent_type = "paragraph".to_string();
    let mut next_line = start_line + 1;
    loop {
        if next_line >= end_line {
            break;
        }
        if state.borrow().is_empty(next_line) {
            break;
        }
        // Lazy continuation regardless of what is there.
        {
            let st = state.borrow();
            if (st.s_count[next_line] as i64) - (st.blk_indent as i64) > 3 {
                next_line += 1;
                continue;
            }
            // Quirk for blockquotes.
            if (st.s_count[next_line] as i64) < 0 {
                next_line += 1;
                continue;
            }
        }
        // Some tags can terminate paragraph without empty line.
        if host.block_terminates("paragraph", state, next_line, end_line) {
            break;
        }
        next_line += 1;
    }
    let content = {
        let st = state.borrow();
        md_trim(&st.get_lines(start_line, next_line, st.blk_indent, false))
    };
    let mut st = state.borrow_mut();
    st.line = next_line;
    let token = st.push("paragraph_open", "p", 1);
    token.borrow_mut().map = Some((start_line, st.line));
    let token = st.push("inline", "", 0);
    {
        let line = st.line;
        let mut t = token.borrow_mut();
        t.content = content;
        t.map = Some((start_line, line));
        t.children = Some(Vec::new());
    }
    st.push("paragraph_close", "p", -1);
    st.parent_type = old_parent_type;
    true
}
