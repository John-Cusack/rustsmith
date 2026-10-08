//! `code` rule (mirror of `markdown_it/rules_block/code.py`).

use crate::SharedBlockState;

pub fn code(state: &SharedBlockState, start_line: usize, _end_line: usize, _silent: bool) -> bool {
    let (last, content, map_end) = {
        let st = state.borrow();
        if !st.is_code_block(start_line) {
            return false;
        }
        let end_line = st.line_max;
        let mut last = start_line + 1;
        let mut next_line = start_line + 1;
        while next_line < end_line {
            if st.is_empty(next_line) {
                next_line += 1;
                continue;
            }
            if st.is_code_block(next_line) {
                next_line += 1;
                last = next_line;
                continue;
            }
            break;
        }
        let content = st.get_lines(start_line, last, 4 + st.blk_indent, false) + "\n";
        (last, content, last)
    };
    let mut st = state.borrow_mut();
    st.line = last;
    let token = st.push("code_block", "code", 0);
    token.borrow_mut().content = content;
    token.borrow_mut().map = Some((start_line, map_end));
    true
}
