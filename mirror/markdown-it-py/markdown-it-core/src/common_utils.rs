//! Text utilities (mirror of `markdown_it/common/utils.py`).
//!
//! All cursors address Unicode scalar values (`&[char]`), matching Python
//! `str` indexing. Regexes use the `regex` crate; the three lookahead
//! patterns live with their rules (`fancy-regex`, see `rules_*`).

use std::sync::LazyLock;

use regex::Regex;

use crate::entities_data::ENTITIES;

pub fn char_code_at(src: &[char], pos: usize) -> Option<u32> {
    src.get(pos).map(|&c| c as u32)
}

pub fn char_str_at(src: &[char], pos: usize) -> Option<char> {
    src.get(pos).copied()
}

pub fn is_valid_entity_code(c: u32) -> bool {
    if (0xD800..=0xDFFF).contains(&c) {
        return false;
    }
    if (0xFDD0..=0xFDEF).contains(&c) {
        return false;
    }
    if (c & 0xFFFF) == 0xFFFF || (c & 0xFFFF) == 0xFFFE {
        return false;
    }
    if c <= 0x08 {
        return false;
    }
    if c == 0x0B {
        return false;
    }
    if (0x0E..=0x1F).contains(&c) {
        return false;
    }
    if (0x7F..=0x9F).contains(&c) {
        return false;
    }
    c <= 0x10FFFF
}

pub fn from_code_point(c: u32) -> Option<char> {
    char::from_u32(c)
}

pub fn lookup_entity(name: &str) -> Option<&'static str> {
    ENTITIES.binary_search_by(|(k, _)| k.cmp(&name)).ok().map(|i| ENTITIES[i].1)
}

static DIGITAL_BASE10_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#([0-9]{1,8})$").unwrap());
static DIGITAL_BASE16_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^#x([a-f0-9]{1,8})$").unwrap());

pub fn replace_entity_pattern(matched: &str, name: &str) -> String {
    if let Some(chars) = lookup_entity(name) {
        return chars.to_string();
    }
    let mut code: Option<u32> = None;
    if let Some(cap) = DIGITAL_BASE10_RE.captures(name) {
        code = cap[1].parse::<u32>().ok();
    } else if let Some(cap) = DIGITAL_BASE16_RE.captures(name) {
        code = u32::from_str_radix(&cap[1], 16).ok();
    }
    if let Some(c) = code {
        if is_valid_entity_code(c) {
            if let Some(ch) = from_code_point(c) {
                return ch.to_string();
            }
        }
    }
    matched.to_string()
}

/// Case-insensitive entity/escape scanner (Python `re.IGNORECASE`).
static UNESCAPE_ALL_RE_I: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r##"(?i)\\([!"#$%&'()*+,\-./:;<=>?@\[\\\]^_`{|}~])|&([a-z#][a-z0-9]{1,31});"##,
    )
    .unwrap()
});

pub fn unescape_all(s: &str) -> String {
    if !s.contains('\\') && !s.contains('&') {
        return s.to_string();
    }
    UNESCAPE_ALL_RE_I
        .replace_all(s, |caps: &regex::Captures| {
            if let Some(m) = caps.get(1) {
                m.as_str().to_string()
            } else {
                let entity = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                replace_entity_pattern(caps.get(0).map(|m| m.as_str()).unwrap_or(""), entity)
            }
        })
        .into_owned()
}

pub fn strip_escape(s: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r##"\\([\\!"#$%&'()*+,./:;<=>?@\[\]^`{}|_~-])"##).unwrap()
    });
    RE.replace_all(s, "$1").into_owned()
}

pub fn escape_html(raw: &str) -> String {
    // like html.escape, but without escaping single quotes; `&` first.
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

static REGEXP_ESCAPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[.?*+^$\[\]\\(){}|-]").unwrap());

pub fn escape_re(s: &str) -> String {
    REGEXP_ESCAPE_RE.replace_all(s, "\\$0").into_owned()
}

/// `isSpace`: code is TAB or SPACE.
pub fn is_space(code: Option<u32>) -> bool {
    matches!(code, Some(0x09) | Some(0x20))
}

pub fn is_str_space(ch: Option<char>) -> bool {
    matches!(ch, Some('\t') | Some(' '))
}

/// `isWhiteSpace`: Zs-ish CommonMark set (`\t\f\v\r\n` + listed spaces +
/// U+2000..U+200A).
pub fn is_white_space(code: u32) -> bool {
    if (0x2000..=0x200A).contains(&code) {
        return true;
    }
    matches!(
        code,
        0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x20 | 0xA0 | 0x1680 | 0x202F | 0x205F | 0x3000
    )
}

fn is_md_trim_char(c: char) -> bool {
    let code = c as u32;
    if (0x2000..=0x200A).contains(&code) {
        return true;
    }
    if code == 0x2028 || code == 0x2029 {
        return true;
    }
    is_white_space(code)
}

/// Strip the CommonMark whitespace set (NOT Python `str.strip`).
pub fn md_trim(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut start = 0;
    let mut end = chars.len();
    while start < end && is_md_trim_char(chars[start]) {
        start += 1;
    }
    while end > start && is_md_trim_char(chars[end - 1]) {
        end -= 1;
    }
    chars[start..end].iter().collect()
}

static MD_TRIM_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Runs of MD_TRIM_CHARS: explicit set + U+2000..U+200A + U+2028/9.
    Regex::new("[\t\n\x0B\x0C\r \u{a0}\u{1680}\u{202f}\u{205f}\u{3000}\u{2000}-\u{200a}\u{2028}\u{2029}]+").unwrap()
});

pub fn md_trim_split(s: &str) -> Vec<String> {
    MD_TRIM_RE.split(s).map(|p| p.to_string()).collect()
}

/// `MD_TRIM_RE.split(s, maxsplit=n)` (Python `re.split` semantics: at most
/// `n` splits, trailing remainder kept whole).
pub fn md_trim_split_max(s: &str, maxsplit: usize) -> Vec<String> {
    MD_TRIM_RE.splitn(s, maxsplit + 1).map(|p| p.to_string()).collect()
}

/// `isPunctChar`: Unicode general categories P* or S*.
pub fn is_punct_char(ch: char) -> bool {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\p{P}\p{S}]$").unwrap());
    let mut buf = [0u8; 4];
    RE.is_match(ch.encode_utf8(&mut buf))
}

/// Markdown ASCII punctuation set.
pub fn is_md_ascii_punct(ch: u32) -> bool {
    matches!(
        ch,
        0x21 | 0x22 | 0x23 | 0x24 | 0x25 | 0x26 | 0x27 | 0x28 | 0x29 | 0x2A | 0x2B
            | 0x2C | 0x2D | 0x2E | 0x2F | 0x3A | 0x3B | 0x3C | 0x3D | 0x3E | 0x3F
            | 0x40 | 0x5B | 0x5C | 0x5D | 0x5E | 0x5F | 0x60 | 0x7B | 0x7C | 0x7D
            | 0x7E
    )
}

/// Unify a reference label: trim/collapse CommonMark whitespace, then
/// `lower().upper()` (NOT case folding).
pub fn normalize_reference(s: &str) -> String {
    let trimmed = md_trim(s);
    let collapsed = MD_TRIM_RE.replace_all(&trimmed, " ");
    collapsed.to_lowercase().to_uppercase()
}

static LINK_OPEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^<a[>\s]").unwrap());
static LINK_CLOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^</a\s*>").unwrap());

pub fn is_link_open(s: &str) -> bool {
    LINK_OPEN_RE.is_match(s)
}

pub fn is_link_close(s: &str) -> bool {
    LINK_CLOSE_RE.is_match(s)
}

/// Python `str.isspace` (superset of Rust `char::is_whitespace`: also
/// U+001C..U+001F and U+0085). Used where the original calls `.strip()`.
pub fn py_is_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}' | '\u{85}')
}

/// Whether Python `s.strip()` empties `s`.
pub fn py_strip_is_empty(s: &str) -> bool {
    s.chars().all(py_is_space)
}

/// Char-index slicing of a `str` (Python `s[a:b]` semantics; out-of-range
/// indices clamp like Python slices).
pub fn char_slice(s: &str, from: usize, to: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let a = from.min(len);
    let b = to.min(len);
    if a >= b {
        return String::new();
    }
    chars[a..b].iter().collect()
}

/// Char-index suffix (Python `s[a:]`).
pub fn char_suffix(s: &str, from: usize) -> String {
    char_slice(s, from, usize::MAX)
}

static BAD_PROTO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(vbscript|javascript|file|data):").unwrap());
static GOOD_DATA_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^data:image\/(gif|png|jpeg|webp);").unwrap()
});

/// Default `validateLink` (no custom validator): verbatim
/// `url.strip().lower()` with Python whitespace, then the scheme checks.
pub fn validate_link_default(url: &str) -> bool {
    let trimmed: String = url
        .trim_matches(|c: char| py_is_space(c))
        .to_lowercase();
    if BAD_PROTO_RE.is_match(&trimmed) {
        GOOD_DATA_RE.is_match(&trimmed)
    } else {
        true
    }
}
