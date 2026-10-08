//! `hr` rule (mirror of `markdown_it/rules_block/hr.py`).

use crate::common_utils::is_str_space;
use crate::SharedBlockState;

pub fn hr(state: &SharedBlockState, start_line: usize, _end_line: usize, silent: bool) -> bool {
    let (marker, cnt) = {
        let st = state.borrow();
        if st.is_code_block(start_line) {
            return false;
        }
        let mut pos = st.b_marks[start_line] + st.t_shift[start_line];
        let maximum = st.e_marks[start_line];
        let marker = match st.src.get(pos) {
            Some(&c) => c,
            None => return false,
        };
        pos += 1;
        if !matches!(marker, '*' | '-' | '_') {
            return false;
        }
        let mut cnt = 1usize;
        while pos < maximum {
            let ch = st.src[pos];
            pos += 1;
            if ch != marker && !is_str_space(Some(ch)) {
                return false;
            }
            if ch == marker {
                cnt += 1;
            }
        }
        if cnt < 3 {
            return false;
        }
        if silent {
            return true;
        }
        (marker, cnt)
    };
    let mut st = state.borrow_mut();
    st.line = start_line + 1;
    let token = st.push("hr", "hr", 0);
    {
        let mut t = token.borrow_mut();
        t.map = Some((start_line, st.line));
        // Verbatim quirk: `marker * (cnt + 1)`.
        t.markup = std::iter::repeat_n(marker, cnt + 1).collect();
    }
    true
}
