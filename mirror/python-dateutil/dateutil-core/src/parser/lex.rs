// SPDX-License-Identifier: Apache-2.0
//! Lexer (`_timelex`): char classes, stateful tokenizer, split helpers.

use std::collections::VecDeque;

/// `str.isalpha` equivalent: ASCII-exact; non-ASCII best-effort
/// (`Alphabetic` minus numeric runes such as Roman numerals, which Python
/// classifies as neither word nor number).
pub fn is_word(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphabetic()
    } else {
        c.is_alphabetic() && !c.is_numeric()
    }
}

/// `str.isdigit` equivalent: ASCII-exact; non-ASCII best-effort
/// (`No`/`Nd` runes such as `²` count, `Nl` runes do not).
pub fn is_num(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_digit()
    } else {
        c.is_numeric() && !c.is_alphabetic()
    }
}

/// `str.isspace` equivalent: ASCII-exact incl. `\x1c..\x1f`, which Python
/// counts as space although they lack the `White_Space` property.
pub fn is_space(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_whitespace() || ('\x1c'..='\x1f').contains(&c)
    } else {
        c.is_whitespace()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LexState {
    Start,
    Word,
    Num,
    WordDot,
    NumDot,
}

/// Stateful lexer over one input string (mirrors `_timelex`).
pub struct Timelex {
    input: Vec<char>,
    pos: usize,
    charstack: VecDeque<char>,
    tokenstack: VecDeque<String>,
    eof: bool,
}

impl Timelex {
    pub fn new(s: &str) -> Self {
        Timelex {
            input: s.chars().collect(),
            pos: 0,
            charstack: VecDeque::new(),
            tokenstack: VecDeque::new(),
            eof: false,
        }
    }

    fn fetch(&mut self) -> Option<char> {
        if let Some(c) = self.charstack.pop_front() {
            return Some(c);
        }
        while self.pos < self.input.len() {
            let c = self.input[self.pos];
            self.pos += 1;
            if c != '\x00' {
                return Some(c);
            }
        }
        None
    }

    /// Next token, or `None` at end of input (mirrors `get_token`).
    pub fn get_token(&mut self) -> Option<String> {
        if let Some(t) = self.tokenstack.pop_front() {
            return Some(t);
        }
        let mut token = String::new();
        let mut seenletters = false;
        let mut state = LexState::Start;
        loop {
            if self.eof {
                break;
            }
            let nextchar = match self.fetch() {
                Some(c) => c,
                None => {
                    self.eof = true;
                    break;
                }
            };
            match state {
                LexState::Start => {
                    token.push(nextchar);
                    if is_word(nextchar) {
                        state = LexState::Word;
                    } else if is_num(nextchar) {
                        state = LexState::Num;
                    } else if is_space(nextchar) {
                        token.clear();
                        token.push(' ');
                        break;
                    } else {
                        break;
                    }
                }
                LexState::Word => {
                    seenletters = true;
                    if is_word(nextchar) {
                        token.push(nextchar);
                    } else if nextchar == '.' {
                        token.push(nextchar);
                        state = LexState::WordDot;
                    } else {
                        self.charstack.push_back(nextchar);
                        break;
                    }
                }
                LexState::Num => {
                    if is_num(nextchar) {
                        token.push(nextchar);
                    } else if nextchar == '.'
                        || (nextchar == ',' && token.chars().count() >= 2)
                    {
                        token.push(nextchar);
                        state = LexState::NumDot;
                    } else {
                        self.charstack.push_back(nextchar);
                        break;
                    }
                }
                LexState::WordDot => {
                    seenletters = true;
                    if nextchar == '.' || is_word(nextchar) {
                        token.push(nextchar);
                    } else if is_num(nextchar) && token.ends_with('.') {
                        token.push(nextchar);
                        state = LexState::NumDot;
                    } else {
                        self.charstack.push_back(nextchar);
                        break;
                    }
                }
                LexState::NumDot => {
                    if nextchar == '.' || is_num(nextchar) {
                        token.push(nextchar);
                    } else if is_word(nextchar) && token.ends_with('.') {
                        token.push(nextchar);
                        state = LexState::WordDot;
                    } else {
                        self.charstack.push_back(nextchar);
                        break;
                    }
                }
            }
        }
        if token.is_empty() {
            return None;
        }
        if matches!(state, LexState::WordDot | LexState::NumDot)
            && (seenletters
                || token.chars().filter(|&c| c == '.').count() > 1
                || matches!(token.chars().last(), Some('.') | Some(',')))
        {
            let parts = split_decimal(&token);
            token = parts[0].clone();
            for tok in parts.into_iter().skip(1) {
                if !tok.is_empty() {
                    self.tokenstack.push_back(tok);
                }
            }
        }
        if state == LexState::NumDot && !token.contains('.') {
            token = token.replace(',', ".");
        }
        if token.is_empty() {
            return None;
        }
        Some(token)
    }

    /// Whole-input token list (mirrors `_timelex.split`).
    pub fn split(s: &str) -> Vec<String> {
        let mut lx = Timelex::new(s);
        let mut out = Vec::new();
        while let Some(t) = lx.get_token() {
            out.push(t);
        }
        out
    }

    pub fn is_eof(&self) -> bool {
        self.eof
    }

    pub fn charstack(&self) -> Vec<String> {
        self.charstack.iter().map(|c| c.to_string()).collect()
    }

    pub fn tokenstack(&self) -> Vec<String> {
        self.tokenstack.iter().cloned().collect()
    }
}

/// `re.split("([.,])", token)` semantics: split, keeping separators.
fn split_decimal(token: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    for c in token.chars() {
        if c == '.' || c == ',' {
            parts.push(c.to_string());
            parts.push(String::new());
        } else {
            parts.last_mut().unwrap().push(c);
        }
    }
    parts
}

/// Lowercase a token the way the `parserinfo` probes do (`name.lower()`).
/// ASCII-exact; non-ASCII `to_lowercase` (context-free, so a trailing
/// capital sigma folds to `σ` rather than `ς`; date tables never contain
/// sigma, documented here rather than worked around).
pub fn lower_token(tok: &str) -> String {
    if tok.is_ascii() {
        tok.to_ascii_lowercase()
    } else {
        tok.to_lowercase()
    }
}

/// Fold Unicode decimal (`Nd`) digits to ASCII; `None` if any char is not
/// foldable (mirrors what `float()`/`int()`/`Decimal()` accept, which is
/// exactly the `Nd` general category — `char::to_digit` is ASCII-only, so
/// the table below stands in).
pub fn fold_digits(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii() {
            out.push(c);
        } else if let Some(d) = nd_value(c) {
            out.push((b'0' + d) as char);
        } else {
            return None;
        }
    }
    Some(out)
}

/// `float(value_repr)` probe: true iff Python would parse it (digit runs
/// with an optional dot, plus `inf`/`infinity`/`nan` in any case).
/// The value itself is discarded; `_to_decimal` re-parses.
pub fn probe_float(tok: &str) -> bool {
    let folded = match fold_digits(tok) {
        Some(f) => f,
        None => return false,
    };
    let low = folded.to_ascii_lowercase();
    if low == "inf" || low == "infinity" || low == "nan" {
        return true;
    }
    let mut seen_dot = false;
    let mut seen_digit = false;
    for c in folded.chars() {
        if c.is_ascii_digit() {
            seen_digit = true;
        } else if c == '.' && !seen_dot {
            seen_dot = true;
        } else {
            return false;
        }
    }
    seen_digit
}

/// Unicode `Nd` decimal-digit value: `Some(cp - block_start)`.
/// Generated from CPython `unicodedata` (Unicode 15.0.0); every block is a
/// stable 0-9 run, verified at generation time. Refresh by re-running the
/// generator in the port ADR when the toolchain Unicode version changes.
fn nd_value(c: char) -> Option<u8> {
    match c as u32 {
        0x30..=0x39 => Some((c as u32 - 0x30) as u8),
        0x660..=0x669 => Some((c as u32 - 0x660) as u8),
        0x6F0..=0x6F9 => Some((c as u32 - 0x6F0) as u8),
        0x7C0..=0x7C9 => Some((c as u32 - 0x7C0) as u8),
        0x966..=0x96F => Some((c as u32 - 0x966) as u8),
        0x9E6..=0x9EF => Some((c as u32 - 0x9E6) as u8),
        0xA66..=0xA6F => Some((c as u32 - 0xA66) as u8),
        0xAE6..=0xAEF => Some((c as u32 - 0xAE6) as u8),
        0xB66..=0xB6F => Some((c as u32 - 0xB66) as u8),
        0xBE6..=0xBEF => Some((c as u32 - 0xBE6) as u8),
        0xC66..=0xC6F => Some((c as u32 - 0xC66) as u8),
        0xCE6..=0xCEF => Some((c as u32 - 0xCE6) as u8),
        0xD66..=0xD6F => Some((c as u32 - 0xD66) as u8),
        0xDE6..=0xDEF => Some((c as u32 - 0xDE6) as u8),
        0xE50..=0xE59 => Some((c as u32 - 0xE50) as u8),
        0xED0..=0xED9 => Some((c as u32 - 0xED0) as u8),
        0xF20..=0xF29 => Some((c as u32 - 0xF20) as u8),
        0x1040..=0x1049 => Some((c as u32 - 0x1040) as u8),
        0x1090..=0x1099 => Some((c as u32 - 0x1090) as u8),
        0x17E0..=0x17E9 => Some((c as u32 - 0x17E0) as u8),
        0x1810..=0x1819 => Some((c as u32 - 0x1810) as u8),
        0x1946..=0x194F => Some((c as u32 - 0x1946) as u8),
        0x19D0..=0x19D9 => Some((c as u32 - 0x19D0) as u8),
        0x1A80..=0x1A89 => Some((c as u32 - 0x1A80) as u8),
        0x1A90..=0x1A99 => Some((c as u32 - 0x1A90) as u8),
        0x1B50..=0x1B59 => Some((c as u32 - 0x1B50) as u8),
        0x1BB0..=0x1BB9 => Some((c as u32 - 0x1BB0) as u8),
        0x1C40..=0x1C49 => Some((c as u32 - 0x1C40) as u8),
        0x1C50..=0x1C59 => Some((c as u32 - 0x1C50) as u8),
        0xA620..=0xA629 => Some((c as u32 - 0xA620) as u8),
        0xA8D0..=0xA8D9 => Some((c as u32 - 0xA8D0) as u8),
        0xA900..=0xA909 => Some((c as u32 - 0xA900) as u8),
        0xA9D0..=0xA9D9 => Some((c as u32 - 0xA9D0) as u8),
        0xA9F0..=0xA9F9 => Some((c as u32 - 0xA9F0) as u8),
        0xAA50..=0xAA59 => Some((c as u32 - 0xAA50) as u8),
        0xABF0..=0xABF9 => Some((c as u32 - 0xABF0) as u8),
        0xFF10..=0xFF19 => Some((c as u32 - 0xFF10) as u8),
        0x104A0..=0x104A9 => Some((c as u32 - 0x104A0) as u8),
        0x10D30..=0x10D39 => Some((c as u32 - 0x10D30) as u8),
        0x11066..=0x1106F => Some((c as u32 - 0x11066) as u8),
        0x110F0..=0x110F9 => Some((c as u32 - 0x110F0) as u8),
        0x11136..=0x1113F => Some((c as u32 - 0x11136) as u8),
        0x111D0..=0x111D9 => Some((c as u32 - 0x111D0) as u8),
        0x112F0..=0x112F9 => Some((c as u32 - 0x112F0) as u8),
        0x11450..=0x11459 => Some((c as u32 - 0x11450) as u8),
        0x114D0..=0x114D9 => Some((c as u32 - 0x114D0) as u8),
        0x11650..=0x11659 => Some((c as u32 - 0x11650) as u8),
        0x116C0..=0x116C9 => Some((c as u32 - 0x116C0) as u8),
        0x11730..=0x11739 => Some((c as u32 - 0x11730) as u8),
        0x118E0..=0x118E9 => Some((c as u32 - 0x118E0) as u8),
        0x11950..=0x11959 => Some((c as u32 - 0x11950) as u8),
        0x11C50..=0x11C59 => Some((c as u32 - 0x11C50) as u8),
        0x11D50..=0x11D59 => Some((c as u32 - 0x11D50) as u8),
        0x11DA0..=0x11DA9 => Some((c as u32 - 0x11DA0) as u8),
        0x11F50..=0x11F59 => Some((c as u32 - 0x11F50) as u8),
        0x16A60..=0x16A69 => Some((c as u32 - 0x16A60) as u8),
        0x16AC0..=0x16AC9 => Some((c as u32 - 0x16AC0) as u8),
        0x16B50..=0x16B59 => Some((c as u32 - 0x16B50) as u8),
        0x1D7CE..=0x1D7D7 => Some((c as u32 - 0x1D7CE) as u8),
        0x1D7D8..=0x1D7E1 => Some((c as u32 - 0x1D7D8) as u8),
        0x1D7E2..=0x1D7EB => Some((c as u32 - 0x1D7E2) as u8),
        0x1D7EC..=0x1D7F5 => Some((c as u32 - 0x1D7EC) as u8),
        0x1D7F6..=0x1D7FF => Some((c as u32 - 0x1D7F6) as u8),
        0x1E140..=0x1E149 => Some((c as u32 - 0x1E140) as u8),
        0x1E2F0..=0x1E2F9 => Some((c as u32 - 0x1E2F0) as u8),
        0x1E4F0..=0x1E4F9 => Some((c as u32 - 0x1E4F0) as u8),
        0x1E950..=0x1E959 => Some((c as u32 - 0x1E950) as u8),
        0x1FBF0..=0x1FBF9 => Some((c as u32 - 0x1FBF0) as u8),
        _ => None,
    }
}
