//! HTML block data (mirror of `markdown_it/common/html_blocks.py` and
//! `markdown_it/common/html_re.py`).
//!
//! The two lookahead start patterns (`script`-family and block-tag names)
//! are hand-rolled char scanners: the `regex` crate cannot express
//! `(?=...)`, and a backtracking engine is unnecessary for these anchored
//! ASCII checks. Case-insensitivity is ASCII-only, matching the originals
//! (their literals are ASCII). The `\s` inside the lookaheads is Python
//! `re` whitespace ([`py_is_space`](crate::common_utils::py_is_space)).

use std::sync::LazyLock;

use crate::common_utils::py_is_space;

pub const BLOCK_NAMES: &[&str] = &[
    "address", "article", "aside", "base", "basefont", "blockquote", "body",
    "caption", "center", "col", "colgroup", "dd", "details", "dialog", "dir",
    "div", "dl", "dt", "fieldset", "figcaption", "figure", "footer", "form",
    "frame", "frameset", "h1", "h2", "h3", "h4", "h5", "h6", "head", "header",
    "hr", "html", "iframe", "legend", "li", "link", "main", "menu", "menuitem",
    "nav", "noframes", "ol", "optgroup", "option", "p", "param", "search",
    "section", "summary", "table", "tbody", "td", "tfoot", "th", "thead",
    "title", "tr", "track", "ul",
];

pub const ATTR_NAME: &str = "[a-zA-Z_:][a-zA-Z0-9:._-]*";
pub const UNQUOTED: &str = "[^\"'=<>`\\x00-\\x20]+";
pub const SINGLE_QUOTED: &str = "'[^']*'";
pub const DOUBLE_QUOTED: &str = "\"[^\"]*\"";

pub fn attr_value() -> String {
    format!("(?:{UNQUOTED}|{SINGLE_QUOTED}|{DOUBLE_QUOTED})")
}

pub fn attribute() -> String {
    format!("(?:\\s+{ATTR_NAME}(?:\\s*=\\s*{})?)", attr_value())
}

pub fn open_tag() -> String {
    format!("<[A-Za-z][A-Za-z0-9\\-]*{}*\\s*\\/?>", attribute())
}

pub fn close_tag() -> String {
    "<\\/[A-Za-z][A-Za-z0-9\\-]*\\s*>".to_string()
}

pub const COMMENT: &str = "<!---?>|<!--(?:[^-]|-[^-]|--[^>])*-->";
pub const PROCESSING: &str = "<[?][\\s\\S]*?[?]>";
pub const DECLARATION: &str = "<![A-Za-z][^>]*>";
pub const CDATA: &str = "<!\\[CDATA\\[[\\s\\S]*?\\]\\]>";

/// `HTML_TAG_RE`, anchored at the match position by the caller through an
/// O(1) `&str` slice (verbatim `.match(src, pos)`).
pub static HTML_TAG_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(&format!(
        "^(?:{}|{}|{}|{}|{}|{})",
        open_tag(),
        close_tag(),
        COMMENT,
        PROCESSING,
        DECLARATION,
        CDATA
    ))
    .unwrap()
});

pub fn html_open_close_tag_str() -> String {
    format!("^(?:{}|{})", open_tag(), close_tag())
}

/// `^<(script|pre|style|textarea)(?=(\s|>|$))` + IGNORECASE, hand-rolled.
fn script_start(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    for name in ["<script", "<pre", "<style", "<textarea"] {
        if lower.starts_with(name) {
            match line[name.len()..].chars().next() {
                None => return true,
                Some('>') => return true,
                Some(c) if py_is_space(c) => return true,
                _ => {}
            }
        }
    }
    false
}

/// `^</?(names)(?=(\s|/?>|$))` + IGNORECASE, hand-rolled.
fn block_tag_start(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut pos = 0;
    if bytes.first() != Some(&b'<') {
        return false;
    }
    pos += 1;
    if bytes.get(pos) == Some(&b'/') {
        pos += 1;
    }
    let lower = line.to_ascii_lowercase();
    let rest = &lower[pos..];
    for name in BLOCK_NAMES {
        if let Some(after) = rest.strip_prefix(name) {
            let mut chars = after.chars();
            match chars.next() {
                None => return true,
                Some('>') => return true,
                Some(c) if py_is_space(c) => return true,
                Some('/') => {
                    // `/?>`: slash must be followed by `>` or end.
                    match chars.next() {
                        Some('>') | None => return true,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    let _ = pos;
    false
}

/// Start-pattern matcher for the seven HTML sequences, in order.
pub fn html_sequence_start(line: &str) -> Option<usize> {
    static COMMENT_START: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^<!--").unwrap());
    static PI_START: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^<\?").unwrap());
    static DECL_START: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^<![A-Z]").unwrap());
    static CDATA_START: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^<!\[CDATA\[").unwrap());
    static OPEN_CLOSE_START: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(&format!("{}\\s*$", html_open_close_tag_str())).unwrap()
    });
    if script_start(line) {
        return Some(0);
    }
    if COMMENT_START.is_match(line) {
        return Some(1);
    }
    if PI_START.is_match(line) {
        return Some(2);
    }
    if DECL_START.is_match(line) {
        return Some(3);
    }
    if CDATA_START.is_match(line) {
        return Some(4);
    }
    if block_tag_start(line) {
        return Some(5);
    }
    if OPEN_CLOSE_START.is_match(line) {
        return Some(6);
    }
    None
}

/// End-pattern matcher for sequence `idx` on `line` (`.search` semantics).
pub fn html_sequence_end(idx: usize, line: &str) -> bool {
    static SCRIPT_END: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)<\/(script|pre|style|textarea)>").unwrap()
    });
    match idx {
        0 => SCRIPT_END.is_match(line),
        1 => line.contains("-->"),
        2 => line.contains("?>"),
        3 => line.contains('>'),
        4 => line.contains("]]>"),
        5 | 6 => line.is_empty(),
        _ => false,
    }
}

/// Whether sequence `idx` can terminate a paragraph.
pub fn html_sequence_terminates(idx: usize) -> bool {
    idx != 6
}
