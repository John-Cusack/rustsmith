//! `table` rule (mirror of `markdown_it/rules_block/table.py`).

use std::rc::Rc;
use std::sync::LazyLock;

use crate::common_utils::{is_str_space, md_trim};
use crate::{Host, SharedBlockState, SharedToken};

static HEADER_LINE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^:?-+:?$").unwrap());

const MAX_AUTOCOMPLETED_CELLS: i64 = 0x10000;

fn get_line(state: &SharedBlockState, line: usize) -> String {
    let st = state.borrow();
    let pos = st.b_marks[line] + st.t_shift[line];
    let maximum = st.e_marks[line];
    st.src[pos..maximum].iter().collect()
}

fn char_str_at(s: &[char], pos: usize) -> Option<char> {
    s.get(pos).copied()
}

fn escaped_split(string: &[char]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut pos = 0usize;
    let max = string.len();
    let mut is_escaped = false;
    let mut last_pos = 0usize;
    let mut current = String::new();
    let mut ch = char_str_at(string, pos);
    while pos < max {
        if ch == Some('|') {
            if !is_escaped {
                current.push_str(&string[last_pos..pos].iter().collect::<String>());
                result.push(std::mem::take(&mut current));
                last_pos = pos + 1;
            } else {
                current.push_str(&string[last_pos..pos - 1].iter().collect::<String>());
                last_pos = pos;
            }
        }
        is_escaped = ch == Some('\\');
        pos += 1;
        ch = char_str_at(string, pos);
    }
    current.push_str(&string[last_pos..].iter().collect::<String>());
    result.push(current);
    result
}

pub fn table(
    state: &SharedBlockState,
    start_line: usize,
    end_line: usize,
    silent: bool,
    host: &Rc<dyn Host>,
) -> bool {
    // Phase 1: header/delimiter scan (immutable).
    let header: Option<ScanHeader> = scan_header(state, start_line, end_line);
    let header = match header {
        Some(h) => h,
        None => return false,
    };
    if silent {
        return true;
    }
    let old_parent_type = state.borrow().parent_type.clone();
    state.borrow_mut().parent_type = "table".to_string();

    let table_open = {
        let mut st = state.borrow_mut();
        let token = st.push("table_open", "table", 1);
        token.borrow_mut().map = Some((start_line, 0));
        token
    };
    {
        let mut st = state.borrow_mut();
        let token = st.push("thead_open", "thead", 1);
        token.borrow_mut().map = Some((start_line, start_line + 1));
        let token = st.push("tr_open", "tr", 1);
        token.borrow_mut().map = Some((start_line, start_line + 1));
        for (i, col) in header.columns.iter().enumerate() {
            let token = st.push("th_open", "th", 1);
            if !header.aligns[i].is_empty() {
                token.borrow_mut().attrs =
                    vec![("style".to_string(), crate::token::AttrVal::Str(format!("text-align:{}", header.aligns[i])))];
            }
            let token = st.push("inline", "", 0);
            {
                let mut t = token.borrow_mut();
                t.map = Some((start_line, start_line + 1));
                t.content = md_trim(col);
                t.children = Some(Vec::new());
            }
            st.push("th_close", "th", -1);
        }
        st.push("tr_close", "tr", -1);
        st.push("thead_close", "thead", -1);
    }
    let column_count = header.columns.len();
    let mut autocompleted: i64 = 0;
    let mut next_line = start_line + 2;
    let mut tbody_open: Option<SharedToken> = None;
    let mut tbody_start = 0usize;
    loop {
        if next_line >= end_line {
            break;
        }
        if (state.borrow().s_count[next_line] as i64) < (state.borrow().blk_indent as i64) {
            break;
        }
        if host.block_terminates("blockquote", state, next_line, end_line) {
            break;
        }
        let line_text = md_trim(&get_line(state, next_line));
        if line_text.is_empty() {
            break;
        }
        if state.borrow().is_code_block(next_line) {
            break;
        }
        let mut columns = escaped_split(&line_text.chars().collect::<Vec<char>>());
        if columns.first().map(|c| c.is_empty()).unwrap_or(false) {
            columns.remove(0);
        }
        if columns.last().map(|c| c.is_empty()).unwrap_or(false) {
            columns.pop();
        }
        autocompleted += column_count as i64 - columns.len() as i64;
        if autocompleted > MAX_AUTOCOMPLETED_CELLS {
            break;
        }
        if next_line == start_line + 2 {
            let mut st = state.borrow_mut();
            let token = st.push("tbody_open", "tbody", 1);
            token.borrow_mut().map = Some((start_line + 2, 0));
            tbody_open = Some(token);
            tbody_start = start_line + 2;
        }
        {
            let mut st = state.borrow_mut();
            let token = st.push("tr_open", "tr", 1);
            token.borrow_mut().map = Some((next_line, next_line + 1));
            for i in 0..column_count {
                let token = st.push("td_open", "td", 1);
                if !header.aligns[i].is_empty() {
                    token.borrow_mut().attrs = vec![(
                        "style".to_string(),
                        crate::token::AttrVal::Str(format!("text-align:{}", header.aligns[i])),
                    )];
                }
                let token = st.push("inline", "", 0);
                {
                    let mut t = token.borrow_mut();
                    t.map = Some((next_line, next_line + 1));
                    t.content = columns.get(i).map(|c| md_trim(c)).unwrap_or_default();
                    t.children = Some(Vec::new());
                }
                st.push("td_close", "td", -1);
            }
            st.push("tr_close", "tr", -1);
        }
        next_line += 1;
    }
    {
        let mut st = state.borrow_mut();
        if let Some(tbody) = tbody_open {
            st.push("tbody_close", "tbody", -1);
            tbody.borrow_mut().map = Some((tbody_start, next_line));
        }
        st.push("table_close", "table", -1);
        table_open.borrow_mut().map = Some((start_line, next_line));
        st.parent_type = old_parent_type;
        st.line = next_line;
    }
    true
}

struct ScanHeader {
    aligns: Vec<String>,
    columns: Vec<String>,
}

fn scan_header(
    state: &SharedBlockState,
    start_line: usize,
    end_line: usize,
) -> Option<ScanHeader> {
    if start_line + 2 > end_line {
        return None;
    }
    let next_line = start_line + 1;
    {
        let st = state.borrow();
        if (st.s_count[next_line] as i64) < (st.blk_indent as i64) {
            return None;
        }
        if st.is_code_block(next_line) {
            return None;
        }
        let pos = st.b_marks[next_line] + st.t_shift[next_line];
        if pos >= st.e_marks[next_line] {
            return None;
        }
        let first = st.src[pos];
        if !matches!(first, '|' | '-' | ':') {
            return None;
        }
        let pos = pos + 1;
        if pos >= st.e_marks[next_line] {
            return None;
        }
        let second = st.src[pos];
        if !matches!(second, '|' | '-' | ':') && !is_str_space(Some(second)) {
            return None;
        }
        if first == '-' && is_str_space(Some(second)) {
            return None;
        }
        let mut p = pos + 1;
        while p < st.e_marks[next_line] {
            let ch = st.src[p];
            if !matches!(ch, '|' | '-' | ':') && !is_str_space(Some(ch)) {
                return None;
            }
            p += 1;
        }
    }
    let line_text = get_line(state, start_line + 1);
    let columns: Vec<String> = line_text.split('|').map(|s| s.to_string()).collect();
    let mut aligns: Vec<String> = Vec::new();
    for (i, col) in columns.iter().enumerate() {
        let t = md_trim(col);
        if t.is_empty() {
            if i == 0 || i == columns.len() - 1 {
                continue;
            } else {
                return None;
            }
        }
        if !HEADER_LINE_RE.is_match(&t) {
            return None;
        }
        let chars: Vec<char> = t.chars().collect();
        if chars.last() == Some(&':') {
            aligns.push(if chars.first() == Some(&':') { "center".to_string() } else { "right".to_string() });
        } else if chars.first() == Some(&':') {
            aligns.push("left".to_string());
        } else {
            aligns.push(String::new());
        }
    }
    let first_text = md_trim(&get_line(state, start_line));
    if !first_text.contains('|') {
        return None;
    }
    if state.borrow().is_code_block(start_line) {
        return None;
    }
    let mut columns = escaped_split(&first_text.chars().collect::<Vec<char>>());
    if columns.first().map(|c| c.is_empty()).unwrap_or(false) {
        columns.remove(0);
    }
    if columns.last().map(|c| c.is_empty()).unwrap_or(false) {
        columns.pop();
    }
    let column_count = columns.len();
    if column_count == 0 || column_count != aligns.len() {
        return None;
    }
    Some(ScanHeader { aligns, columns })
}
