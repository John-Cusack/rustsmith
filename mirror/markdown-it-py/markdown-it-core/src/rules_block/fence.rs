//! `fence` rule + `make_fence_rule` config (mirror of
//! `markdown_it/rules_block/fence.py`).
//!
//! The Python `make_fence_rule` factory is re-exposed by the binding as a
//! callable object carrying a [`FenceConfig`]; the default `fence` rule uses
//! a default config (backtick/tilde, CommonMark).

use crate::SharedBlockState;

#[derive(Debug, Clone)]
pub struct FenceConfig {
    pub markers: Vec<char>,
    pub token_type: String,
    pub exact_match: bool,
    pub disallow_marker_in_info: Vec<char>,
    pub min_markers: usize,
}

impl Default for FenceConfig {
    fn default() -> Self {
        Self {
            markers: vec!['~', '`'],
            token_type: "fence".to_string(),
            exact_match: false,
            disallow_marker_in_info: vec!['`'],
            min_markers: 3,
        }
    }
}

pub fn fence(state: &SharedBlockState, start_line: usize, end_line: usize, silent: bool) -> bool {
    fence_with(state, start_line, end_line, silent, &FenceConfig::default())
}

pub fn fence_with(
    state: &SharedBlockState,
    start_line: usize,
    end_line: usize,
    silent: bool,
    cfg: &FenceConfig,
) -> bool {
    let (marker, length, markup, params) = {
        let st = state.borrow();
        if st.is_code_block(start_line) {
            return false;
        }
        let pos0 = st.b_marks[start_line] + st.t_shift[start_line];
        let maximum0 = st.e_marks[start_line];
        if pos0 + cfg.min_markers > maximum0 {
            return false;
        }
        let marker = match st.src.get(pos0) {
            Some(&c) => c,
            None => return false,
        };
        if !cfg.markers.contains(&marker) {
            return false;
        }
        let mem = pos0;
        let pos = st.skip_chars_str(pos0, marker);
        let length = pos - mem;
        if length < cfg.min_markers {
            return false;
        }
        let markup: String = st.src[mem..pos].iter().collect();
        let params: String = st.src[pos..maximum0].iter().collect();
        if cfg.disallow_marker_in_info.contains(&marker) && params.contains(marker) {
            return false;
        }
        if silent {
            return true;
        }
        (marker, length, markup, params)
    };

    let mut next_line = start_line;
    let mut have_end_marker = false;
    loop {
        next_line += 1;
        if next_line >= end_line {
            break;
        }
        let st = state.borrow();
        let mut pos = st.b_marks[next_line] + st.t_shift[next_line];
        let mem = pos;
        let maximum = st.e_marks[next_line];
        if pos < maximum && (st.s_count[next_line] as i64) < (st.blk_indent as i64) {
            break;
        }
        // Verbatim: `try: if src[pos] != marker: continue except IndexError: break`.
        match st.src.get(pos) {
            None => break,
            Some(&c) if c != marker => continue,
            _ => {}
        }
        if st.is_code_block(next_line) {
            continue;
        }
        pos = st.skip_chars_str(pos, marker);
        let closing_len = pos - mem;
        let closes = if cfg.exact_match {
            closing_len == length
        } else {
            closing_len >= length
        };
        if !closes {
            continue;
        }
        pos = st.skip_spaces(pos);
        if pos < maximum {
            continue;
        }
        have_end_marker = true;
        break;
    }

    let mut st = state.borrow_mut();
    let indent = st.s_count[start_line];
    st.line = next_line + usize::from(have_end_marker);
    let content = st.get_lines(start_line + 1, next_line, indent, true);
    let token_type = cfg.token_type.clone();
    let token = st.push(&token_type, "code", 0);
    let line = st.line;
    {
        let mut t = token.borrow_mut();
        t.info = params;
        t.content = content;
        t.markup = markup;
        t.map = Some((start_line, line));
    }
    true
}
