//! `smartquotes` core rule (mirror of
//! `markdown_it/rules_core/smartquotes.py`).

use crate::common_utils::{is_md_ascii_punct, is_punct_char, is_white_space};
use crate::{SharedCoreState, SharedToken};

const APOSTROPHE: &str = "\u{2019}";

fn code_at(s: &[char], pos: usize) -> Option<u32> {
    s.get(pos).map(|&c| c as u32)
}

/// Python `string[:index] + ch + string[index+1:]` (char indices).
fn replace_at(s: &str, index: usize, ch: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out: String = chars[..index.min(chars.len())].iter().collect();
    out.push_str(ch);
    if index + 1 < chars.len() {
        out.push_str(&chars[index + 1..].iter().collect::<String>());
    }
    out
}

fn find_quote(text: &[char], from: usize) -> Option<usize> {
    text[from..].iter().position(|&c| c == '\'' || c == '"').map(|i| from + i)
}

#[derive(Debug, Clone)]
struct StackItem {
    token: usize,
    pos: usize,
    single: bool,
    level: i64,
}

fn this_token_level(tokens: &[SharedToken], i: usize) -> i64 {
    tokens[i].borrow().level as i64
}

fn process_inlines(tokens: &[SharedToken], state: &SharedCoreState) {
    let mut stack: Vec<StackItem> = Vec::new();
    let ntokens = tokens.len();
    for i in 0..ntokens {
        let this_level = this_token_level(tokens, i);
        // for j in reversed(range(len(stack))): if stack[j].level <= this: break; else: j -= 1
        let mut j: i64 = 0;
        let mut broken = false;
        for jj in (0..stack.len()).rev() {
            j = jj as i64;
            if stack[jj].level <= this_level {
                broken = true;
                break;
            }
        }
        if !broken {
            j -= 1;
        }
        stack.truncate((j + 1).max(0) as usize);
        if tokens[i].borrow().typ != "text" {
            continue;
        }
        let mut text: Vec<char> = tokens[i].borrow().content.chars().collect();
        let mut pos = 0usize;
        let mut maximum = text.len();
        while pos < maximum {
            let last_index = pos;
            let t = match find_quote(&text, last_index) {
                Some(p) => p,
                None => break,
            };
            let mut can_open = true;
            let mut can_close = true;
            pos = t + 1;
            let is_single = text[t] == '\'';
            // Verbatim: `if t.start(0) + lastIndex - 1 >= 0`, where `t`
            // is already absolute (`t.start(0) + lastIndex`).
            let mut last_char: Option<u32> = Some(0x20);
            if (t as i64) > 0 {
                last_char = code_at(&text, t - 1);
            } else {
                for tok in tokens.iter().take(i).rev() {
                    let jt = tok.borrow().typ.clone();
                    if jt == "softbreak" || jt == "hardbreak" {
                        break;
                    }
                    let content = tok.borrow().content.clone();
                    if content.is_empty() {
                        continue;
                    }
                    last_char = code_at(&content.chars().collect::<Vec<char>>(), content.chars().count() - 1);
                    break;
                }
            }
            let mut next_char: Option<u32> = Some(0x20);
            if pos < maximum {
                next_char = code_at(&text, pos);
            } else {
                for tok in tokens.iter().skip(i + 1) {
                    let jt = tok.borrow().typ.clone();
                    if jt == "softbreak" || jt == "hardbreak" {
                        break;
                    }
                    let content = tok.borrow().content.clone();
                    if content.is_empty() {
                        continue;
                    }
                    next_char = code_at(&content.chars().collect::<Vec<char>>(), 0);
                    break;
                }
            }
            let is_last_punct = last_char.map(|c| {
                is_md_ascii_punct(c) || is_punct_char(char::from_u32(c).unwrap_or('?'))
            }).unwrap_or(false);
            let is_next_punct = next_char.map(|c| {
                is_md_ascii_punct(c) || is_punct_char(char::from_u32(c).unwrap_or('?'))
            }).unwrap_or(false);
            let is_last_ws = last_char.map(is_white_space).unwrap_or(false);
            let is_next_ws = next_char.map(is_white_space).unwrap_or(false);
            if is_next_ws || (is_next_punct && !(is_last_ws || is_last_punct)) {
                can_open = false;
            }
            if is_last_ws || (is_last_punct && !(is_next_ws || is_next_punct)) {
                can_close = false;
            }
            if next_char == Some(0x22)
                && text[t] == '"'
                && matches!(last_char, Some(c) if (0x30..=0x39).contains(&c))
            {
                can_close = false;
                can_open = false;
            }
            if can_open && can_close {
                can_open = is_last_punct;
                can_close = is_next_punct;
            }
            if !can_open && !can_close {
                if is_single {
                    let content = tokens[i].borrow().content.clone();
                    tokens[i].borrow_mut().content = replace_at(&content, t, APOSTROPHE);
                    text = tokens[i].borrow().content.chars().collect();
                }
                continue;
            }
            let mut matched = false;
            if can_close {
                for jj in (0..stack.len()).rev() {
                    let item = stack[jj].clone();
                    if stack[jj].level < this_level {
                        break;
                    }
                    if item.single == is_single && stack[jj].level == this_level {
                        let quotes = state.borrow().md.options.borrow().quotes.clone();
                        let (open_quote, close_quote) = if is_single {
                            (quote_at(&quotes, 2), quote_at(&quotes, 3))
                        } else {
                            (quote_at(&quotes, 0), quote_at(&quotes, 1))
                        };
                        let content = tokens[i].borrow().content.clone();
                        tokens[i].borrow_mut().content = replace_at(&content, t, &close_quote);
                        let other = tokens[item.token].borrow().content.clone();
                        tokens[item.token].borrow_mut().content =
                            replace_at(&other, item.pos, &open_quote);
                        pos += close_quote.chars().count().saturating_sub(1);
                        if item.token == i {
                            pos += open_quote.chars().count().saturating_sub(1);
                        }
                        text = tokens[i].borrow().content.chars().collect();
                        maximum = text.len();
                        stack.truncate(jj);
                        matched = true;
                        break;
                    }
                }
                if matched {
                    continue;
                }
            }
            if can_open {
                stack.push(StackItem { token: i, pos: t, single: is_single, level: this_level });
            } else if can_close && is_single {
                let content = tokens[i].borrow().content.clone();
                tokens[i].borrow_mut().content = replace_at(&content, t, APOSTROPHE);
                text = tokens[i].borrow().content.chars().collect();
            }
        }
    }
}

fn quote_at(quotes: &crate::options_env::QuotesVal, idx: usize) -> String {
    quotes.at(idx)
}

pub fn smartquotes(state: &SharedCoreState) {
    if !state.borrow().md.options.borrow().typographer {
        return;
    }
    let tokens = state.borrow().tokens.clone();
    for token in &tokens {
        if token.borrow().typ != "inline" {
            continue;
        }
        let content = token.borrow().content.clone();
        if !(content.contains('\'') || content.contains('"')) {
            continue;
        }
        if let Some(children) = token.borrow().children.clone() {
            process_inlines(&children, state);
        }
    }
}
