//! `heading` rule (mirror of `markdown_it/rules_block/heading.py`).

use crate::common_utils::{is_str_space, md_trim};
use crate::SharedBlockState;

pub fn heading(state: &SharedBlockState, start_line: usize, _end_line: usize, silent: bool) -> bool {
    let (level, pos, maximum) = {
        let st = state.borrow();
        if st.is_code_block(start_line) {
            return false;
        }
        let pos = st.b_marks[start_line] + st.t_shift[start_line];
        let maximum = st.e_marks[start_line];
        let ch = st.src.get(pos).copied();
        if ch != Some('#') || pos >= maximum {
            return false;
        }
        let mut level = 1usize;
        let mut pos = pos + 1;
        let mut ch = st.src.get(pos).copied();
        while ch == Some('#') && pos < maximum && level <= 6 {
            level += 1;
            pos += 1;
            ch = st.src.get(pos).copied();
        }
        if level > 6 || (pos < maximum && !is_str_space(ch)) {
            return false;
        }
        if silent {
            return true;
        }
        (level, pos, maximum)
    };
    let mut st = state.borrow_mut();
    let mut maximum = st.skip_spaces_back(maximum, pos);
    let tmp = st.skip_chars_str_back(maximum, '#', pos);
    if tmp > pos {
        if let Some(&c) = st.src.get(tmp - 1) {
            if c == '\t' || c == ' ' {
                maximum = tmp;
            }
        }
    }
    st.line = start_line + 1;
    let content: String = md_trim(&st.src[pos..maximum].iter().collect::<String>());
    let tag = format!("h{level}");
    let markup = "########"[..level].to_string();
    let token = st.push("heading_open", &tag, 1);
    {
        let mut t = token.borrow_mut();
        t.markup = markup.clone();
        t.map = Some((start_line, st.line));
    }
    let token = st.push("inline", "", 0);
    {
        let mut t = token.borrow_mut();
        t.content = content;
        t.map = Some((start_line, st.line));
        t.children = Some(Vec::new());
    }
    let token = st.push("heading_close", &tag, -1);
    token.borrow_mut().markup = markup;
    true
}
