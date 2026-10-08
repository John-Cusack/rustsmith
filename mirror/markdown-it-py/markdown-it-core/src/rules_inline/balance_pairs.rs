//! `link_pairs` (delimiter pairing, mirror of
//! `markdown_it/rules_inline/balance_pairs.py`).
//!
//! Verbatim port of `processDelimiters`, including the `openersBottom`
//! lower-bound cache that keeps pathological runs linear.

use std::collections::HashMap;

use crate::rules_inline::SharedDelims;
use crate::SharedInlineState;

pub fn process_delimiters(_state: &SharedInlineState, delimiters: &SharedDelims) {
    let mut delims = delimiters.borrow_mut();
    if delims.is_empty() {
        return;
    }
    let mut openers_bottom: HashMap<u32, [i64; 6]> = HashMap::new();
    let maximum = delims.len();
    let mut header_idx = 0usize;
    let mut last_token_idx: i64 = -2;
    let mut jumps: Vec<i64> = Vec::new();
    let mut closer_idx = 0usize;
    while closer_idx < maximum {
        jumps.push(0);
        if delims[header_idx].marker != delims[closer_idx].marker
            || last_token_idx != delims[closer_idx].token as i64 - 1
        {
            header_idx = closer_idx;
        }
        last_token_idx = delims[closer_idx].token as i64;
        if !delims[closer_idx].close {
            closer_idx += 1;
            continue;
        }
        let closer_marker = delims[closer_idx].marker;
        let closer_open = delims[closer_idx].open;
        let closer_len = delims[closer_idx].length;
        let bottom = openers_bottom.entry(closer_marker).or_insert([-1, -1, -1, -1, -1, -1]);
        let min_opener_idx = bottom[(if closer_open { 3 } else { 0 }) + (closer_len % 3) as usize];
        let mut opener_idx = header_idx as i64 - jumps[header_idx] - 1;
        let mut new_min_opener_idx = opener_idx;
        while opener_idx > min_opener_idx {
            let oi = opener_idx as usize;
            if delims[oi].marker != closer_marker {
                opener_idx -= jumps[oi] + 1;
                continue;
            }
            if delims[oi].open && delims[oi].end < 0 {
                let opener = delims[oi].clone();
                let closer = delims[closer_idx].clone();
                let mut is_odd_match = false;
                if (opener.close || closer.open)
                    && (opener.length + closer.length) % 3 == 0
                    && (opener.length % 3 != 0 || closer.length % 3 != 0)
                {
                    is_odd_match = true;
                }
                if !is_odd_match {
                    let last_jump = if opener_idx > 0 && !delims[opener_idx as usize - 1].open {
                        jumps[opener_idx as usize - 1] + 1
                    } else {
                        0
                    };
                    jumps[closer_idx] = closer_idx as i64 - opener_idx + last_jump;
                    jumps[oi] = last_jump;
                    delims[closer_idx].close = false;
                    delims[closer_idx].open = false;
                    delims[oi].end = closer_idx as i64;
                    delims[oi].close = false;
                    new_min_opener_idx = -1;
                    last_token_idx = -2;
                    break;
                }
            }
            opener_idx -= jumps[oi] + 1;
        }
        if new_min_opener_idx != -1 {
            let bottom = openers_bottom.entry(closer_marker).or_insert([-1, -1, -1, -1, -1, -1]);
            bottom[(if closer_open { 3 } else { 0 }) + (closer_len % 3) as usize] =
                new_min_opener_idx;
        }
        closer_idx += 1;
    }
}

pub fn link_pairs(state: &SharedInlineState) {
    let delims = state.borrow().delimiters.clone();
    process_delimiters(state, &delims);
    let maximum = state.borrow().tokens_meta.len();
    let mut curr = 0;
    while curr < maximum {
        let meta = state.borrow().tokens_meta[curr].clone();
        if let Some(d) = meta {
            process_delimiters(state, &d);
        }
        curr += 1;
    }
}
