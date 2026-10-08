//! `html_block` rule (mirror of `markdown_it/rules_block/html_block.py`).

use crate::rules_block::html_data::{
    html_sequence_end, html_sequence_start, html_sequence_terminates,
};
use crate::SharedBlockState;

pub fn html_block(
    state: &SharedBlockState,
    start_line: usize,
    end_line: usize,
    silent: bool,
) -> bool {
    let seq = {
        let st = state.borrow();
        if st.is_code_block(start_line) {
            return false;
        }
        if !st.md.options.borrow().html {
            return false;
        }
        let pos = st.b_marks[start_line] + st.t_shift[start_line];
        let maximum = st.e_marks[start_line];
        if st.src.get(pos) != Some(&'<') {
            return false;
        }
        let line_text = st.text_range(pos, maximum).to_string();
        match html_sequence_start(&line_text) {
            Some(idx) => idx,
            None => return false,
        }
    };
    if silent {
        return html_sequence_terminates(seq);
    }
    let mut next_line = start_line + 1;
    {
        let st = state.borrow();
        let pos = st.b_marks[start_line] + st.t_shift[start_line];
        let maximum = st.e_marks[start_line];
        let line_text = st.text_range(pos, maximum).to_string();
        if !html_sequence_end(seq, &line_text) {
            drop(st);
            loop {
                if next_line >= end_line {
                    break;
                }
                let st = state.borrow();
                if (st.s_count[next_line] as i64) < (st.blk_indent as i64) {
                    break;
                }
                let pos = st.b_marks[next_line] + st.t_shift[next_line];
                let maximum = st.e_marks[next_line];
                let line_text = st.text_range(pos, maximum).to_string();
                if html_sequence_end(seq, &line_text) {
                    if !line_text.is_empty() {
                        next_line += 1;
                    }
                    break;
                }
                drop(st);
                next_line += 1;
            }
        }
    }
    let mut st = state.borrow_mut();
    st.line = next_line;
    let content = st.get_lines(start_line, next_line, st.blk_indent, true);
    let token = st.push("html_block", "", 0);
    {
        let mut t = token.borrow_mut();
        t.map = Some((start_line, next_line));
        t.content = content;
    }
    true
}
