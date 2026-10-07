//! Pure-Rust core: PEP 440 version parsing, ordering, validation.
//!
//! Stage-1 mirror of `packaging/version.py`. No Python dependency: parsing,
//! comparison keys, display, and `from_parts`/`__replace__` validation live
//! here; the PyO3 binding in `src/lib.rs` only translates values and builds
//! the exact Python key tuples for hashing.
//!
//! License: Apache-2.0 OR BSD-2-Clause (preserved from the original).

use std::cmp::Ordering;

/// Letter normalization (`_LETTER_NORMALIZATION` in the original).
pub fn normalize_pre(letter: &str) -> String {
    let lower = letter.to_lowercase();
    match lower.as_str() {
        "alpha" => "a".to_string(),
        "beta" => "b".to_string(),
        "c" | "pre" | "preview" => "rc".to_string(),
        "rev" | "r" => "post".to_string(),
        _ => lower,
    }
}

/// Normalize a pre-release letter to its canonical short form (`a`/`b`/`rc`),
/// or `None` when it is not a pre-release spelling.
pub fn normalize_pre_letter(letter: &str) -> Option<&'static str> {
    let lower = letter.to_lowercase();
    match lower.as_str() {
        "alpha" | "a" => Some("a"),
        "beta" | "b" => Some("b"),
        "c" | "pre" | "preview" | "rc" => Some("rc"),
        _ => None,
    }
}

/// Normalize a post-release letter to `"post"`, or `None` when not one.
pub fn normalize_post_letter(letter: &str) -> Option<&'static str> {
    let lower = letter.to_lowercase();
    match lower.as_str() {
        "post" | "rev" | "r" => Some("post"),
        _ => None,
    }
}

/// A decimal integer stored without leading zeros (`"0"` for zero).
/// Release/epoch/dev/post numbers can exceed `u64` (up to the
/// `sys.get_int_max_str_digits()` limit), so they are kept as digit strings
/// and compared numerically.
pub type NumString = String;

/// Strip leading zeros; empty becomes `"0"`.
pub fn normalize_num(digits: &str) -> NumString {
    let stripped = digits.trim_start_matches('0');
    if stripped.is_empty() {
        "0".to_string()
    } else {
        stripped.to_string()
    }
}

/// Numeric comparison of normalized digit strings.
pub fn cmp_num(a: &str, b: &str) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => a.cmp(b),
        other => other,
    }
}

/// Add one to a normalized digit string (arbitrary precision).
pub fn add_one_num(digits: &str) -> NumString {
    let mut out: Vec<u8> = digits.bytes().collect();
    let mut i = out.len();
    while i > 0 {
        i -= 1;
        if out[i] == b'9' {
            out[i] = b'0';
        } else {
            out[i] += 1;
            return String::from_utf8(out).unwrap();
        }
    }
    let mut s = String::with_capacity(digits.len() + 1);
    s.push('1');
    s.push_str(&"0".repeat(digits.len()));
    s
}

/// Subtract one from a normalized positive digit string (no underflow check;
/// callers guarantee a value `>= 1`).
pub fn sub_one_num(digits: &str) -> NumString {
    let mut out: Vec<u8> = digits.bytes().collect();
    let mut i = out.len();
    while i > 0 {
        i -= 1;
        if out[i] == b'0' {
            out[i] = b'9';
        } else {
            out[i] -= 1;
            break;
        }
    }
    normalize_num(std::str::from_utf8(&out).unwrap())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalSeg {
    Num(NumString),
    Str(String),
}

/// A parsed PEP 440 version (normalized parts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVersion {
    pub epoch: NumString,
    pub release: Vec<NumString>,
    pub pre: Option<(String, NumString)>,
    pub post: Option<NumString>,
    pub dev: Option<NumString>,
    pub local: Option<Vec<LocalSeg>>,
}

impl ParsedVersion {
    pub fn blank() -> Self {
        ParsedVersion {
            epoch: "0".to_string(),
            release: vec!["0".to_string()],
            pre: None,
            post: None,
            dev: None,
            local: None,
        }
    }
}

/// Parse failure modes. `Invalid` renders as
/// `Invalid version: {input!r}` (the binding applies Python `repr`);
/// `DigitLimit` renders as the CPython `int()` limit `ValueError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Invalid,
    DigitLimit { max: usize, got: usize },
}

impl ParseError {
    /// Exact CPython `int()` digit-limit message.
    pub fn digit_limit_message(max: usize, got: usize) -> String {
        format!(
            "Exceeds the limit ({max} digits) for integer string conversion: \
             value has {got} digits; use sys.set_int_max_str_digits() to increase the limit"
        )
    }
}

/// `sys.get_int_max_str_digits()`: `None` disables the limit
/// (`set_int_max_str_digits(0)`), matching CPython's `int()`.
pub type IntLimit = Option<usize>;

pub(crate) fn check_digits(digits: &str, limit: IntLimit) -> Result<(), ParseError> {
    if let Some(max) = limit {
        if digits.len() > max {
            return Err(ParseError::DigitLimit {
                max,
                got: digits.len(),
            });
        }
    }
    Ok(())
}

/// Python `str` whitespace (`\s` without `re.ASCII`): ASCII whitespace plus
/// the Unicode `White_Space` property set.
pub(crate) fn is_py_space(c: char) -> bool {
    matches!(c,
        '\u{09}'..='\u{0d}' | '\u{20}' | '\u{1c}'..='\u{1f}' | '\u{85}' | '\u{a0}' |
        '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' |
        '\u{202f}' | '\u{205f}' | '\u{3000}')
}

/// Case-insensitive ASCII-letter match with `re.IGNORECASE` (non-ASCII)
/// semantics: a non-ASCII char matches when it lowercases to exactly the
/// target ASCII letter (covers U+017F `ſ`, U+212A `K`); multi-char
/// lowercases (U+0130 `İ`) never match.
pub(crate) fn ci_eq(c: char, target_ascii_lower: char) -> bool {
    if c.is_ascii() {
        c.to_ascii_lowercase() == target_ascii_lower
    } else {
        let mut it = c.to_lowercase();
        matches!((it.next(), it.next()), (Some(l), None) if l == target_ascii_lower)
    }
}

/// Case-insensitive word match at `pos`; returns the end position.
pub(crate) fn match_word_ci(chars: &[char], pos: usize, word: &str) -> Option<usize> {
    let mut p = pos;
    for wc in word.chars() {
        if p >= chars.len() || !ci_eq(chars[p], wc) {
            return None;
        }
        p += 1;
    }
    Some(p)
}

pub(crate) fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// Longest-match word table for pre-release letters, in regex-alternation
/// order (`alpha|a|beta|b|preview|pre|c|rc`): longer spellings precede their
/// prefixes, so first match is the longest match.
pub(crate) const PRE_WORDS: &[&str] = &["alpha", "beta", "preview", "pre", "a", "b", "c", "rc"];
/// Post-release letters in alternation order (`post|rev|r`).
pub(crate) const POST_WORDS: &[&str] = &["post", "rev", "r"];

pub(crate) fn is_sep(c: char) -> bool {
    matches!(c, '.' | '_' | '-')
}

pub(crate) struct Parser<'a> {
    pub(crate) chars: &'a [char],
    pub(crate) pos: usize,
    pub(crate) limit: IntLimit,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(chars: &'a [char], limit: IntLimit) -> Self {
        Parser { chars, pos: 0, limit }
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    pub(crate) fn eat_sep(&mut self) -> bool {
        if matches!(self.peek(), Some(c) if is_sep(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub(crate) fn eat_digits(&mut self) -> Option<String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_digit(c)) {
            self.pos += 1;
        }
        if self.pos > start {
            Some(self.chars[start..self.pos].iter().collect())
        } else {
            None
        }
    }

    /// Optional pre-release segment. Rolls back a consumed separator when no
    /// letter follows (the regex group fails as a whole).
    pub(crate) fn eat_pre(&mut self) -> Result<Option<(String, NumString)>, ParseError> {
        let save = self.pos;
        self.eat_sep();
        let mut matched: Option<(&str, usize)> = None;
        for w in PRE_WORDS {
            if let Some(end) = match_word_ci(self.chars, self.pos, w) {
                matched = Some((w, end));
                break;
            }
        }
        let Some((word, end)) = matched else {
            self.pos = save;
            return Ok(None);
        };
        self.pos = end;
        self.eat_sep();
        let num = match self.eat_digits() {
            Some(d) => {
                check_digits(&d, self.limit)?;
                normalize_num(&d)
            }
            None => "0".to_string(),
        };
        let letter = normalize_pre_letter(word).expect("pre word table is canonical");
        Ok(Some((letter.to_string(), num)))
    }

    /// Optional post-release segment: `-N` or `[sep](post|rev|r)[sep][N]`.
    pub(crate) fn eat_post(&mut self) -> Result<Option<NumString>, ParseError> {
        // Implicit `-N` form first (regex alternation order).
        if self.peek() == Some('-') {
            let save = self.pos;
            self.pos += 1;
            if let Some(d) = self.eat_digits() {
                check_digits(&d, self.limit)?;
                return Ok(Some(normalize_num(&d)));
            }
            self.pos = save;
        }
        let save = self.pos;
        self.eat_sep();
        let mut matched: Option<usize> = None;
        for w in POST_WORDS {
            if let Some(end) = match_word_ci(self.chars, self.pos, w) {
                matched = Some(end);
                break;
            }
        }
        let Some(end) = matched else {
            self.pos = save;
            return Ok(None);
        };
        let _ = normalize_post_letter(&self.chars[self.pos..end].iter().collect::<String>())
            .expect("post word table is canonical");
        self.pos = end;
        self.eat_sep();
        let num = match self.eat_digits() {
            Some(d) => {
                check_digits(&d, self.limit)?;
                normalize_num(&d)
            }
            None => "0".to_string(),
        };
        Ok(Some(num))
    }

    /// Optional dev-release segment: `[sep]dev[sep][N]`.
    pub(crate) fn eat_dev(&mut self) -> Result<Option<NumString>, ParseError> {
        let save = self.pos;
        self.eat_sep();
        let Some(end) = match_word_ci(self.chars, self.pos, "dev") else {
            self.pos = save;
            return Ok(None);
        };
        self.pos = end;
        self.eat_sep();
        let num = match self.eat_digits() {
            Some(d) => {
                check_digits(&d, self.limit)?;
                normalize_num(&d)
            }
            None => "0".to_string(),
        };
        Ok(Some(num))
    }

    /// Optional local segment: `+[alnum]+([sep][alnum]+)*`.
    pub(crate) fn eat_local(&mut self) -> Result<Option<Vec<LocalSeg>>, ParseError> {
        if self.peek() != Some('+') {
            return Ok(None);
        }
        self.pos += 1;
        let mut segs = Vec::new();
        loop {
            let start = self.pos;
            while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || (!c.is_ascii() && ci_letter_or_digit(c))) {
                self.pos += 1;
            }
            if self.pos == start {
                return Err(ParseError::Invalid);
            }
            let part: String = self.chars[start..self.pos].iter().collect();
            if part.bytes().all(|b| b.is_ascii_digit()) {
                check_digits(&part, self.limit)?;
                segs.push(LocalSeg::Num(normalize_num(&part)));
            } else {
                segs.push(LocalSeg::Str(part.to_lowercase()));
            }
            if matches!(self.peek(), Some(c) if is_sep(c)) {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(Some(segs))
    }
}

/// Non-ASCII char matching `[a-z0-9]` under `re.IGNORECASE`: letters whose
/// lowercase is a single ASCII alphanumeric.
pub(crate) fn ci_letter_or_digit(c: char) -> bool {
    if c.is_ascii() {
        return false;
    }
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l.is_ascii_alphanumeric(),
        _ => false,
    }
}

/// Parse a full version string (simple digit-path fast path included).
pub fn parse(s: &str, limit: IntLimit) -> Result<ParsedVersion, ParseError> {
    if s.is_empty() {
        return Err(ParseError::Invalid);
    }
    // Fast path: only digits and dots (mirrors `_SIMPLE_VERSION_INDICATORS`).
    if s.bytes().all(|b| b == b'.' || b.is_ascii_digit()) {
        let mut release = Vec::new();
        for part in s.split('.') {
            if part.is_empty() {
                return Err(ParseError::Invalid);
            }
            check_digits(part, limit)?;
            release.push(normalize_num(part));
        }
        return Ok(ParsedVersion {
            epoch: "0".to_string(),
            release,
            pre: None,
            post: None,
            dev: None,
            local: None,
        });
    }

    let chars: Vec<char> = s.chars().collect();
    let mut start = 0;
    let mut end = chars.len();
    while start < end && is_py_space(chars[start]) {
        start += 1;
    }
    while end > start && is_py_space(chars[end - 1]) {
        end -= 1;
    }
    if start == end {
        return Err(ParseError::Invalid);
    }
    let slice = &chars[start..end];
    let mut p = Parser {
        chars: slice,
        pos: 0,
        limit,
    };

    // Optional leading `v`.
    if matches!(p.peek(), Some('v') | Some('V')) {
        p.pos += 1;
    }

    // Optional epoch: digits followed by `!`.
    let mut epoch = "0".to_string();
    {
        let save = p.pos;
        if let Some(d) = p.eat_digits() {
            if p.peek() == Some('!') {
                p.pos += 1;
                check_digits(&d, limit)?;
                epoch = normalize_num(&d);
            } else {
                p.pos = save;
            }
        }
    }

    // Release segment (required).
    let Some(first) = p.eat_digits() else {
        return Err(ParseError::Invalid);
    };
    check_digits(&first, limit)?;
    let mut release = vec![normalize_num(&first)];
    while p.peek() == Some('.') {
        // A `.` only continues the release when followed by a digit;
        // otherwise it starts the pre/post/dev/local segment handling below.
        if p.chars.get(p.pos + 1).copied().is_some_and(is_digit) {
            p.pos += 1;
            let d = p.eat_digits().expect("digit follows dot");
            check_digits(&d, limit)?;
            release.push(normalize_num(&d));
        } else {
            break;
        }
    }

    let pre = p.eat_pre()?;
    let post = p.eat_post()?;
    let dev = p.eat_dev()?;
    let local = p.eat_local()?;

    if p.pos != p.chars.len() {
        return Err(ParseError::Invalid);
    }
    Ok(ParsedVersion {
        epoch,
        release,
        pre,
        post,
        dev,
        local,
    })
}

/// Parse a local segment string (for `from_parts`/`__replace__` validation).
/// Mirrors `_LOCAL_PATTERN.fullmatch` + `_parse_local_version`.
pub fn parse_local(s: &str, limit: IntLimit) -> Result<Vec<LocalSeg>, ParseError> {
    let chars: Vec<char> = s.chars().collect();
    let mut p = Parser {
        chars: &chars,
        pos: 0,
        limit,
    };
    let mut segs = Vec::new();
    loop {
        let start = p.pos;
        while matches!(p.peek(), Some(c) if c.is_ascii_alphanumeric() || (!c.is_ascii() && ci_letter_or_digit(c))) {
            p.pos += 1;
        }
        if p.pos == start {
            return Err(ParseError::Invalid);
        }
        let part: String = p.chars[start..p.pos].iter().collect();
        if part.bytes().all(|b| b.is_ascii_digit()) {
            check_digits(&part, limit)?;
            segs.push(LocalSeg::Num(normalize_num(&part)));
        } else {
            segs.push(LocalSeg::Str(part.to_lowercase()));
        }
        if matches!(p.peek(), Some(c) if is_sep(c)) {
            p.pos += 1;
        } else {
            break;
        }
    }
    if p.pos != p.chars.len() {
        return Err(ParseError::Invalid);
    }
    Ok(segs)
}

/// Render the normalized string form (mirrors `Version.__str__`).
pub fn display(v: &ParsedVersion) -> String {
    display_with_release(v, &v.release)
}

/// Render with an overridden release (for `_TrimmedRelease.__str__`, which
/// uses the trimmed `release` property).
pub fn display_with_release(v: &ParsedVersion, release: &[NumString]) -> String {
    let mut out = release.join(".");
    if v.epoch != "0" {
        out = format!("{}!{out}", v.epoch);
    }
    if let Some((letter, n)) = &v.pre {
        out.push_str(letter);
        out.push_str(n);
    }
    if let Some(n) = &v.post {
        out.push_str(".post");
        out.push_str(n);
    }
    if let Some(n) = &v.dev {
        out.push_str(".dev");
        out.push_str(n);
    }
    if let Some(local) = &v.local {
        out.push('+');
        let parts: Vec<String> = local
            .iter()
            .map(|s| match s {
                LocalSeg::Num(n) => n.clone(),
                LocalSeg::Str(t) => t.clone(),
            })
            .collect();
        out.push_str(&parts.join("."));
    }
    out
}

/// Strip trailing zero components, leaving at least nothing (`1.0.0` -> `1`).
/// Mirrors the `_cmpkey` trim (all-zero release trims to empty).
pub fn trim_release(release: &[NumString]) -> Vec<NumString> {
    let mut i = release.len();
    while i > 0 && release[i - 1] == "0" {
        i -= 1;
    }
    release[..i].to_vec()
}

/// Strip trailing zeros but leave one (`1.0.0` -> `1`): mirrors
/// `_TrimmedRelease.release` ("This leaves one 0" — `(0,)` stays `(0,)`).
pub fn trim_release_leave_one(release: &[NumString]) -> Vec<NumString> {
    let mut i = release.len();
    while i > 1 && release[i - 1] == "0" {
        i -= 1;
    }
    release[..i].to_vec()
}

pub fn pre_rank(pre: &Option<(String, NumString)>, post: &Option<NumString>, dev: &Option<NumString>) -> (i64, NumString) {
    if pre.is_none() && post.is_none() && dev.is_some() {
        return (-1, "0".to_string());
    }
    if pre.is_none() {
        return (3, "0".to_string());
    }
    let (letter, n) = pre.as_ref().unwrap();
    let rank = match letter.as_str() {
        "a" => 0,
        "b" => 1,
        _ => 2,
    };
    (rank, n.clone())
}

/// Compare two parsed versions (mirrors `_cmpkey` tuple ordering).
pub fn cmp(a: &ParsedVersion, b: &ParsedVersion) -> Ordering {
    match cmp_num(&a.epoch, &b.epoch) {
        Ordering::Equal => {}
        other => return other,
    }
    let ta = trim_release(&a.release);
    let tb = trim_release(&b.release);
    for (x, y) in ta.iter().zip(tb.iter()) {
        match cmp_num(x, y) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    match ta.len().cmp(&tb.len()) {
        Ordering::Equal => {}
        other => return other,
    }
    let (arank, an) = pre_rank(&a.pre, &a.post, &a.dev);
    let (brank, bn) = pre_rank(&b.pre, &b.post, &b.dev);
    match arank.cmp(&brank).then_with(|| cmp_num(&an, &bn)) {
        Ordering::Equal => {}
        other => return other,
    }
    let apost = a.post.is_some();
    let bpost = b.post.is_some();
    match apost.cmp(&bpost) {
        Ordering::Equal => {}
        other => return other,
    }
    if let (Some(an), Some(bn)) = (&a.post, &b.post) {
        match cmp_num(an, bn) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    // dev_rank: dev=0 sorts before no-dev=1.
    match a.dev.is_none().cmp(&b.dev.is_none()) {
        Ordering::Equal => {}
        other => return other,
    }
    if let (Some(an), Some(bn)) = (&a.dev, &b.dev) {
        match cmp_num(an, bn) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    match (&a.local, &b.local) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => cmp_local(x, y),
    }
}

pub(crate) fn cmp_local(a: &[LocalSeg], b: &[LocalSeg]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let ord = match (x, y) {
            (LocalSeg::Num(m), LocalSeg::Num(n)) => cmp_num(m, n),
            (LocalSeg::Str(s), LocalSeg::Str(t)) => s.cmp(t),
            // Strings sort before ints per PEP 440.
            (LocalSeg::Str(_), LocalSeg::Num(_)) => Ordering::Less,
            (LocalSeg::Num(_), LocalSeg::Str(_)) => Ordering::Greater,
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}
/// ASCII-only local parser for `from_parts`/`__replace__` validation, where
/// the original `_LOCAL_PATTERN` carries `re.ASCII` (unlike the constructor
/// path, which allows non-ASCII letters under `re.IGNORECASE`).
pub fn parse_local_ascii(s: &str, limit: IntLimit) -> Result<Vec<LocalSeg>, ParseError> {
    if s.is_empty()
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(ParseError::Invalid);
    }
    let mut segs = Vec::new();
    for part in s.split(['.', '_', '-']) {
        if part.is_empty() {
            return Err(ParseError::Invalid);
        }
        if part.bytes().all(|b| b.is_ascii_digit()) {
            check_digits(part, limit)?;
            segs.push(LocalSeg::Num(normalize_num(part)));
        } else {
            segs.push(LocalSeg::Str(part.to_ascii_lowercase()));
        }
    }
    Ok(segs)
}
