// SPDX-License-Identifier: Apache-2.0
//! Lexer (`_timelex`): byte-table classes, single-pass tokenizer, split helpers.
//!
//! The lexer walks the original buffer exactly once as bytes. ASCII bytes
//! classify through a `[u8; 256]` table (no data-dependent branches beyond
//! the table lookup); bytes `>= 0x80` fall back to `char` decode + the
//! `is_word`/`is_num`/`is_space` classes. Tokens borrow the input
//! ([`Token::Borrowed`]) except for two rare normalizations the original
//! performs eagerly: `","` -> `"."` folding (`"12,5"` -> `"12.5"`) and
//! `\x00` filtering (`"a\x00b"` -> `"ab"`), which materialize one `String`.
//! The caller keeps the single owned `String` alive: no `Vec<char>` copy
//! (4x expansion) and no per-token `String` growth on the hot path.

use std::borrow::Cow;
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

// Byte classes for the ASCII fast loop: word chars, digit chars and
// whitespace classify through this table; `.` / `,` / NUL / other bytes are
// handled explicitly per state, and bytes `>= 0x80` fall back to `char`
// decode + the `is_word` / `is_num` / `is_space` classes.
const C_OTHER: u8 = 0;
const C_WORD: u8 = 1;
const C_NUM: u8 = 2;
const C_SPACE: u8 = 3;

const CLASS: [u8; 256] = build_class();

const fn build_class() -> [u8; 256] {
    let mut t = [C_OTHER; 256];
    let mut b = 0u32;
    while b < 256 {
        // Matches `is_space` on ASCII exactly: `is_ascii_whitespace`
        // (`0x09..=0x0D`, `0x20`) plus `\x1c..=\x1f`.
        let v = if (b >= 65 && b <= 90) || (b >= 97 && b <= 122) {
            C_WORD
        } else if b >= 48 && b <= 57 {
            C_NUM
        } else if b == 32 || (b >= 9 && b <= 13) || (b >= 28 && b <= 31) {
            C_SPACE
        } else {
            C_OTHER
        };
        t[b as usize] = v;
        b += 1;
    }
    t
}

/// A lexer token: a borrow of the input buffer, except for two rare
/// normalizations the original performs eagerly (`","` -> `"."` folding and
/// `\x00` filtering), which materialize one `String`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token<'a> {
    text: Cow<'a, str>,
    nchars: u32,
    ascii: bool,
}

impl<'a> Token<'a> {
    /// Counters are filled by the lexer during its single pass; this
    /// constructor (cold paths only) recounts from the slice.
    pub fn from_slice(s: &'a str) -> Self {
        let (nchars, ascii) = count_span(s.as_bytes());
        Token {
            text: Cow::Borrowed(s),
            nchars,
            ascii,
        }
    }

    fn borrowed(text: &'a str, nchars: u32, ascii: bool) -> Self {
        Token {
            text: Cow::Borrowed(text),
            nchars,
            ascii,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// `chars().count()` without recounting: the lexer counted every char it
    /// collected, so token length checks are O(1).
    pub fn nchars(&self) -> usize {
        self.nchars as usize
    }

    /// True iff the token text is ASCII (then byte offsets are char offsets).
    pub fn is_ascii(&self) -> bool {
        self.ascii
    }

    pub fn into_owned(self) -> String {
        self.text.into_owned()
    }
}

impl<'a> Default for Token<'a> {
    fn default() -> Self {
        Token {
            text: Cow::Borrowed(""),
            nchars: 0,
            ascii: true,
        }
    }
}

impl<'a> std::ops::Deref for Token<'a> {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

impl<'a> AsRef<str> for Token<'a> {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl<'a, 'b> PartialEq<&'b str> for Token<'a> {
    fn eq(&self, other: &&'b str) -> bool {
        self.as_str() == *other
    }
}

impl<'a> PartialEq<String> for Token<'a> {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

/// Char count + ASCII flag over raw bytes (UTF-8 lead-byte count; exact for
/// `&str` slices, no `char` decode).
fn count_span(bytes: &[u8]) -> (u32, bool) {
    let mut n = 0u32;
    let mut ascii = true;
    for &byte in bytes {
        if byte < 0x80 {
            n += 1;
        } else {
            ascii = false;
            if byte >= 0xC0 {
                n += 1;
            }
        }
    }
    (n, ascii)
}

/// Byte span of one token inside the input (`dots` counts `.` chars, which
/// the split-overflow check needs).
#[derive(Clone, Copy)]
struct RawSpan {
    start: u32,
    end: u32,
    nchars: u32,
    dots: u32,
    ascii: bool,
}

/// Owned lexer cursor: byte offset + pending single-char pushback (as byte
/// length) + queued token spans + EOF. Lifetime-free, so the borrowed
/// [`Timelex`] and the owning binding object share one machine.
#[derive(Default)]
pub struct LexCursor {
    pos: usize,
    rewind: u8,
    spans: VecDeque<RawSpan>,
    eof: bool,
}

impl LexCursor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_eof(&self) -> bool {
        self.eof
    }

    /// Pending pushback char (mirrors the `charstack` introspection).
    pub fn charstack(&self, s: &str) -> Vec<String> {
        if self.rewind == 0 {
            return Vec::new();
        }
        vec![s[self.pos..self.pos + self.rewind as usize].to_string()]
    }

    pub fn tokenstack(&self, s: &str) -> Vec<String> {
        self.spans
            .iter()
            .map(|sp| materialize(s, *sp, false).into_owned())
            .collect()
    }
}

/// Next char + byte length, skipping `\x00` exactly like the
/// `StringIO.read(1)` loop. `pos` always rests on a char boundary.
fn fetch(s: &str, b: &[u8], pos: &mut usize) -> Option<(char, u8)> {
    loop {
        if *pos >= b.len() {
            return None;
        }
        let byte = b[*pos];
        if byte == 0 {
            *pos += 1;
            continue;
        }
        if byte < 0x80 {
            *pos += 1;
            return Some((byte as char, 1));
        }
        let c = s[*pos..].chars().next().unwrap();
        let len = c.len_utf8() as u8;
        *pos += len as usize;
        return Some((c, len));
    }
}

/// Resolve a span to its token text: borrow, unless the span needs the
/// original's eager normalizations (`","` -> `"."` when `fold`, NUL
/// filtering), which materialize one `String`.
fn materialize(s: &str, sp: RawSpan, fold: bool) -> Token<'_> {
    let raw = &s[sp.start as usize..sp.end as usize];
    let do_fold = fold && sp.dots == 0;
    if !raw.as_bytes().contains(&0) {
        if !do_fold {
            return Token::borrowed(raw, sp.nchars, sp.ascii);
        }
        let mut out = String::with_capacity(raw.len());
        for c in raw.chars() {
            out.push(if c == ',' { '.' } else { c });
        }
        return Token {
            text: Cow::Owned(out),
            nchars: sp.nchars,
            ascii: sp.ascii,
        };
    }
    let mut out = String::with_capacity(raw.len());
    let mut nchars = 0u32;
    let mut ascii = true;
    for c in raw.chars() {
        if c == '\x00' {
            continue;
        }
        nchars += 1;
        if !c.is_ascii() {
            ascii = false;
        }
        if do_fold && c == ',' {
            out.push('.');
        } else {
            out.push(c);
        }
    }
    Token {
        text: Cow::Owned(out),
        nchars,
        ascii,
    }
}

/// Split a dotted token at `.` / `,` keeping separators (mirrors
/// `re.split("([.,])", token)`); the head is returned, the overflow queued.
/// Every part is boundary-aligned (separators are single ASCII bytes).
fn split_token<'a>(
    s: &'a str,
    b: &[u8],
    cur: &mut LexCursor,
    sp: RawSpan,
    fold: bool,
) -> Option<Token<'a>> {
    let start = sp.start as usize;
    let end = sp.end as usize;
    let mut parts: Vec<(usize, usize)> = Vec::new();
    let mut pstart = start;
    for (i, &byte) in b[start..end].iter().enumerate() {
        if byte == b'.' || byte == b',' {
            parts.push((pstart, start + i));
            parts.push((start + i, start + i + 1));
            pstart = start + i + 1;
        }
    }
    parts.push((pstart, end));
    let mut it = parts.into_iter();
    // The head is never empty (no token starts with a separator), but match
    // the original's empty-head behavior instead of panicking.
    let (hs, he) = it.next().unwrap();
    for (ps, pe) in it {
        // Skips raw empties like the original; all-NUL parts materialize to
        // `""` and are skipped the same way.
        if b[ps..pe].iter().all(|&x| x == 0) {
            continue;
        }
        let (n, a) = if sp.ascii {
            ((pe - ps) as u32, true)
        } else {
            count_span(&b[ps..pe])
        };
        cur.spans.push_back(RawSpan {
            start: ps as u32,
            end: pe as u32,
            nchars: n,
            dots: 0,
            ascii: a,
        });
    }
    if hs == he {
        return None;
    }
    let (n, a) = if sp.ascii {
        ((he - hs) as u32, true)
    } else {
        count_span(&b[hs..he])
    };
    Some(materialize(
        s,
        RawSpan {
            start: hs as u32,
            end: he as u32,
            nchars: n,
            dots: 0,
            ascii: a,
        },
        fold,
    ))
}

/// One token from `s` at `cur` (mirrors `get_token`, including the
/// decimal-split overflow queue).
pub fn next_token<'a>(s: &'a str, cur: &mut LexCursor) -> Option<Token<'a>> {
    if let Some(sp) = cur.spans.pop_front() {
        return Some(materialize(s, sp, false));
    }
    let b = s.as_bytes();
    // `nchars` / `dots` / `ascii` / `last_byte` are the consumed state the
    // old `token.chars().count()` / `ends_with` calls re-derived per step;
    // each arm below extends them inline (a closure would borrow-conflict
    // with the counter reads in the `Num` / `WordDot` / `NumDot` arms).
    let mut state = LexState::Start;
    let mut seenletters = false;
    let mut started = false;
    let mut start = 0usize;
    let mut nchars = 0u32;
    let mut dots = 0u32;
    let mut ascii = true;
    let mut last_byte = 0u8;
    loop {
        if cur.eof {
            break;
        }
        let (c, clen) = match fetch(s, b, &mut cur.pos) {
            Some(v) => v,
            None => {
                cur.eof = true;
                break;
            }
        };
        if !started {
            started = true;
            start = cur.pos - clen as usize;
        }
        // ASCII fast loop through the class table; non-ASCII decodes through
        // the char classes in the original's check order.
        let cls = if clen == 1 {
            CLASS[c as usize]
        } else if is_word(c) {
            C_WORD
        } else if is_num(c) {
            C_NUM
        } else if is_space(c) {
            C_SPACE
        } else {
            C_OTHER
        };
        match state {
            LexState::Start => {
                if cls == C_WORD {
                    state = LexState::Word;
                    nchars += 1;
                    ascii &= clen == 1;
                    last_byte = if clen == 1 { c as u8 } else { 0 };
                } else if cls == C_NUM {
                    state = LexState::Num;
                    nchars += 1;
                    ascii &= clen == 1;
                    last_byte = if clen == 1 { c as u8 } else { 0 };
                } else if cls == C_SPACE {
                    // Whitespace canonicalizes to one `" "` per char.
                    return Some(Token::borrowed(" ", 1, true));
                } else {
                    // Single punctuation token (never NUL: fetch skips it).
                    return Some(materialize(
                        s,
                        RawSpan {
                            start: start as u32,
                            end: cur.pos as u32,
                            nchars: 1,
                            dots: 0,
                            ascii: clen == 1,
                        },
                        false,
                    ));
                }
            }
            LexState::Word => {
                seenletters = true;
                if cls == C_WORD {
                    nchars += 1;
                    ascii &= clen == 1;
                    last_byte = if clen == 1 { c as u8 } else { 0 };
                } else if c == '.' {
                    nchars += 1;
                    dots += 1;
                    last_byte = b'.';
                    state = LexState::WordDot;
                } else {
                    cur.pos -= clen as usize;
                    cur.rewind = clen;
                    break;
                }
            }
            LexState::Num => {
                if cls == C_NUM {
                    nchars += 1;
                    ascii &= clen == 1;
                    last_byte = if clen == 1 { c as u8 } else { 0 };
                } else if c == '.' || (c == ',' && nchars >= 2) {
                    nchars += 1;
                    if c == '.' {
                        dots += 1;
                    }
                    last_byte = c as u8;
                    state = LexState::NumDot;
                } else {
                    cur.pos -= clen as usize;
                    cur.rewind = clen;
                    break;
                }
            }
            LexState::WordDot => {
                seenletters = true;
                if c == '.' || cls == C_WORD {
                    nchars += 1;
                    ascii &= clen == 1;
                    if c == '.' {
                        dots += 1;
                    }
                    last_byte = if clen == 1 { c as u8 } else { 0 };
                } else if cls == C_NUM && last_byte == b'.' {
                    nchars += 1;
                    ascii &= clen == 1;
                    last_byte = if clen == 1 { c as u8 } else { 0 };
                    state = LexState::NumDot;
                } else {
                    cur.pos -= clen as usize;
                    cur.rewind = clen;
                    break;
                }
            }
            LexState::NumDot => {
                if c == '.' || cls == C_NUM {
                    nchars += 1;
                    ascii &= clen == 1;
                    if c == '.' {
                        dots += 1;
                    }
                    last_byte = if clen == 1 { c as u8 } else { 0 };
                } else if cls == C_WORD && last_byte == b'.' {
                    nchars += 1;
                    ascii &= clen == 1;
                    last_byte = if clen == 1 { c as u8 } else { 0 };
                    state = LexState::WordDot;
                } else {
                    cur.pos -= clen as usize;
                    cur.rewind = clen;
                    break;
                }
            }
        }
    }
    if !started {
        return None;
    }
    let sp = RawSpan {
        start: start as u32,
        end: cur.pos as u32,
        nchars,
        dots,
        ascii,
    };
    // `last_byte` is the last collected char (NULs are never collected), so
    // these match `token.chars().filter(== '.').count() > 1` and
    // `token.chars().last()` on the original token exactly.
    if matches!(state, LexState::WordDot | LexState::NumDot)
        && (seenletters || dots > 1 || last_byte == b'.' || last_byte == b',')
    {
        return split_token(s, b, cur, sp, state == LexState::NumDot);
    }
    Some(materialize(s, sp, state == LexState::NumDot))
}

/// Stateful lexer over one input string (mirrors `_timelex`).
///
/// The input is borrowed: the machine walks `&[u8]` once and hands out
/// `&str` slices of the original buffer (kept alive by the caller — the
/// binding holds the single owned `String`).
pub struct Timelex<'a> {
    s: &'a str,
    cursor: LexCursor,
}

impl<'a> Timelex<'a> {
    pub fn new(s: &'a str) -> Self {
        Timelex {
            s,
            cursor: LexCursor::new(),
        }
    }

    /// Next token, or `None` at end of input (mirrors `get_token`).
    pub fn get_token(&mut self) -> Option<Token<'a>> {
        next_token(self.s, &mut self.cursor)
    }

    /// Whole-input token list (mirrors `_timelex.split`).
    pub fn split(s: &'a str) -> Vec<Token<'a>> {
        let mut cur = LexCursor::new();
        let mut out = Vec::new();
        while let Some(t) = next_token(s, &mut cur) {
            out.push(t);
        }
        out
    }

    pub fn is_eof(&self) -> bool {
        self.cursor.is_eof()
    }

    pub fn charstack(&self) -> Vec<String> {
        self.cursor.charstack(self.s)
    }

    pub fn tokenstack(&self) -> Vec<String> {
        self.cursor.tokenstack(self.s)
    }
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
/// the table below stands in). Borrows ASCII input (folding is the identity
/// there); only non-ASCII digits materialize a `String`.
pub fn fold_digits(s: &str) -> Option<Cow<'_, str>> {
    if s.is_ascii() {
        return Some(Cow::Borrowed(s));
    }
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
    Some(Cow::Owned(out))
}

/// `float(value_repr)` probe: true iff Python would parse it (digit runs
/// with an optional dot, plus `inf`/`infinity`/`nan` in any case).
/// The value itself is discarded; `_to_decimal` re-parses. The ASCII fast
/// path scans bytes with no allocation; non-ASCII falls back to folding.
pub fn probe_float(tok: &str) -> bool {
    if tok.is_ascii() {
        if tok.eq_ignore_ascii_case("inf")
            || tok.eq_ignore_ascii_case("infinity")
            || tok.eq_ignore_ascii_case("nan")
        {
            return true;
        }
        let mut seen_dot = false;
        let mut seen_digit = false;
        for &b in tok.as_bytes() {
            if b.is_ascii_digit() {
                seen_digit = true;
            } else if b == b'.' && !seen_dot {
                seen_dot = true;
            } else {
                return false;
            }
        }
        return seen_digit;
    }
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
