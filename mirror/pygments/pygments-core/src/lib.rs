//! Pure-Rust text kernels for the pygments Stage-1 mirror.
//!
//! This crate has no Python dependency and contains **no `unsafe` code**.
//! It owns four hot pure-text transforms from the highlight path, each a
//! verbatim port of the upstream Python (referenced per function):
//!
//! - [`escape_html_text`]: `formatters/html.py::escape_html` (per-token
//!   HTML escaping in `HtmlFormatter._format_lines`, via the cached
//!   `_translate_parts`) and `formatters/svg.py::escape_html` (same
//!   mapping; the replace-chain order — `&` first — yields output
//!   identical to the `translate` table).
//! - [`normalize_newlines`]: the `\r\n` → `\n`, `\r` → `\n` head of
//!   `lexer.py::Lexer._preprocess_lexer_input` (runs on every highlight).
//! - [`expand_tabs_and_ensure`]: the `expandtabs` + `ensurenl` tail of
//!   the same method (whitespace stripping stays in Python: `str.strip`
//!   follows CPython's `Py_UNICODE_ISSPACE` set, which this crate does
//!   not re-derive; see ADR-038).
//! - [`regex_opt`]: `regexopt.py::regex_opt` (keyword-list regex builder
//!   used at lexer-definition import time).
//!
//! Dynamic user plugin surfaces — custom `RegexLexer` subclasses with
//! Python `re` patterns and callbacks (`bygroups`, `using`), custom
//! formatters, entry-point lexers — stay in Python behind the documented
//! seam; they cannot be ported (see ADR-038).

/// HTML-escape `&`, `<`, `>`, `"`, `'`.
///
/// Mirrors `"".translate({38: '&amp;', 60: '&lt;', 62: '&gt;',
/// 34: '&quot;', 39: '&#39;'})` exactly: every other char (including all
/// non-ASCII) passes through unchanged.
pub fn escape_html_text(text: &str) -> String {
    if !text
        .bytes()
        .any(|b| matches!(b, b'&' | b'<' | b'>' | b'"' | b'\''))
    {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Replace `\r\n` → `\n`, then lone `\r` → `\n`.
///
/// Mirrors the two `str.replace` calls at the head of
/// `Lexer._preprocess_lexer_input`. A single pass: `\r\n` collapses to
/// one `\n`, a lone `\r` becomes `\n`, everything else is copied.
pub fn normalize_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            out.push('\n');
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Expand tabs and ensure a trailing newline.
///
/// Mirrors the tail of `Lexer._preprocess_lexer_input` for text that is
/// already newline-normalized and stripped: `text.expandtabs(tabsize)`
/// when `tabsize > 0`, then append `\n` when `ensurenl` and the text does
/// not already end with one. Column counting matches CPython's
/// `str.expandtabs` on `\r`-free text: `\n` resets the column, every
/// other char (including `\v`, `\f`, non-BMP) advances it by one, and a
/// tab advances to the next multiple of `tabsize`.
pub fn expand_tabs_and_ensure(text: &str, tabsize: usize, ensurenl: bool) -> String {
    let expanded = if tabsize > 0 && text.contains('\t') {
        let mut out = String::with_capacity(text.len() + 8);
        let mut col: usize = 0;
        for c in text.chars() {
            match c {
                '\n' => {
                    out.push('\n');
                    col = 0;
                }
                '\t' => {
                    let pad = tabsize - (col % tabsize);
                    out.extend(std::iter::repeat_n(' ', pad));
                    col += pad;
                }
                _ => {
                    out.push(c);
                    col += 1;
                }
            }
        }
        out
    } else {
        text.to_string()
    };
    if ensurenl && !expanded.ends_with('\n') {
        let mut out = expanded;
        out.push('\n');
        out
    } else {
        expanded
    }
}

/// Characters escaped by CPython ≥3.7 `re.escape` (`str.translate` with
/// `_special_chars_map`, built from `b'()[]{}?*+-|^$\\.&~# \t\n\r\v\f'`):
/// only these ASCII specials gain a backslash. Everything else —
/// including all non-ASCII, which `re.escape` leaves raw — passes
/// through. Per-character, so it commutes with common-prefix/suffix
/// splitting.
const RE_ESCAPE_SPECIALS: &[char] = &[
    '(', ')', '[', ']', '{', '}', '?', '*', '+', '-', '|', '^', '$', '\\', '.', '&', '~', '#', ' ',
    '\t', '\n', '\r', '\u{000B}', '\u{000C}',
];

fn re_escape_char(c: char) -> bool {
    RE_ESCAPE_SPECIALS.contains(&c)
}

fn escape_pattern(s: &[char]) -> String {
    let mut out = String::new();
    for &c in s {
        if re_escape_char(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Characters escaped by `regexopt.CS_ESCAPE` (`[\[\^\\\-\]]`).
fn make_charset(letters: &[char]) -> String {
    let mut out = String::with_capacity(letters.len() + 2);
    out.push('[');
    for &c in letters {
        if matches!(c, '[' | '^' | '\\' | '-' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push(']');
    out
}

fn common_prefix_len(strs: &[Vec<char>]) -> usize {
    let first = &strs[0];
    let mut len = first.len();
    for s in &strs[1..] {
        len = len.min(s.len());
        for i in 0..len {
            if s[i] != first[i] {
                len = i;
                break;
            }
        }
    }
    len
}

fn regex_opt_inner(strings: &[Vec<char>], open_paren: &str) -> String {
    let close_paren = if open_paren.is_empty() { "" } else { ")" };
    if strings.is_empty() {
        return String::new();
    }
    let first = &strings[0];
    if strings.len() == 1 {
        return format!("{open_paren}{}{close_paren}", escape_pattern(first));
    }
    if first.is_empty() {
        // Mirrors `open_paren + regex_opt_inner(strings[1:], '(?:') + '?'
        // + close_paren`.
        return format!(
            "{open_paren}{}?{close_paren}",
            regex_opt_inner(&strings[1..], "(?:")
        );
    }
    if first.len() == 1 {
        let mut oneletter: Vec<char> = Vec::new();
        let mut rest: Vec<Vec<char>> = Vec::new();
        for s in strings {
            if s.len() == 1 {
                oneletter.push(s[0]);
            } else {
                rest.push(s.clone());
            }
        }
        if oneletter.len() > 1 {
            let charset = make_charset(&oneletter);
            if !rest.is_empty() {
                return format!(
                    "{open_paren}{}|{charset}{close_paren}",
                    regex_opt_inner(&rest, "")
                );
            }
            return format!("{open_paren}{charset}{close_paren}");
        }
    }
    let plen = common_prefix_len(strings);
    if plen > 0 {
        let prefix: Vec<char> = strings[0][..plen].to_vec();
        let stripped: Vec<Vec<char>> = strings.iter().map(|s| s[plen..].to_vec()).collect();
        return format!(
            "{open_paren}{}{}{close_paren}",
            escape_pattern(&prefix),
            regex_opt_inner(&stripped, "(?:")
        );
    }
    let revs: Vec<Vec<char>> = strings
        .iter()
        .map(|s| s.iter().rev().copied().collect())
        .collect();
    let slen = common_prefix_len(&revs);
    if slen > 0 {
        let suffix_rev: Vec<char> = revs[0][..slen].to_vec();
        let suffix: Vec<char> = suffix_rev.iter().rev().copied().collect();
        let mut stripped: Vec<Vec<char>> = strings
            .iter()
            .map(|s| s[..s.len() - slen].to_vec())
            .collect();
        stripped.sort();
        return format!(
            "{open_paren}{}{}{close_paren}",
            regex_opt_inner(&stripped, "(?:"),
            escape_pattern(&suffix)
        );
    }
    // Last resort: group consecutive strings by first-char equality.
    let mut out = String::from(open_paren);
    let mut first_group = true;
    let mut i = 0;
    while i < strings.len() {
        let key = strings[i][0] == first[0];
        let mut j = i + 1;
        while j < strings.len() && (strings[j][0] == first[0]) == key {
            j += 1;
        }
        if !first_group {
            out.push('|');
        }
        first_group = false;
        out.push_str(&regex_opt_inner(&strings[i..j], ""));
        i = j;
    }
    out.push_str(close_paren);
    out
}

/// Build a regex matching any of the given literal strings.
///
/// Mirrors `regexopt.regex_opt(strings, prefix='', suffix='')`: sorts the
/// strings (code-point order, same as Python's `sorted` on `str`), runs
/// the structural recursion, wraps with `prefix`/`suffix`.
pub fn regex_opt(strings: &[String], prefix: &str, suffix: &str) -> String {
    let mut sorted: Vec<Vec<char>> = strings.iter().map(|s| s.chars().collect()).collect();
    sorted.sort();
    format!("{prefix}{}{suffix}", regex_opt_inner(&sorted, "("))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_passthrough_without_specials() {
        assert_eq!(escape_html_text("hello world 123"), "hello world 123");
        assert_eq!(escape_html_text("héllo wörld ✓"), "héllo wörld ✓");
        assert_eq!(escape_html_text(""), "");
    }

    #[test]
    fn escape_all_five() {
        assert_eq!(escape_html_text("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;");
        assert_eq!(
            escape_html_text("a & b <c> \"d\" 'e'"),
            "a &amp; b &lt;c&gt; &quot;d&quot; &#39;e&#39;"
        );
    }

    #[test]
    fn escape_does_not_double_escape() {
        // translate() semantics: input `&amp;` escapes the `&` once.
        assert_eq!(escape_html_text("&amp;"), "&amp;amp;");
    }

    #[test]
    fn newlines_no_cr_passthrough() {
        assert_eq!(normalize_newlines("a\nb\n"), "a\nb\n");
        assert_eq!(normalize_newlines(""), "");
    }

    #[test]
    fn newlines_crlf_and_lone_cr() {
        assert_eq!(normalize_newlines("a\r\nb"), "a\nb");
        assert_eq!(normalize_newlines("a\rb"), "a\nb");
        assert_eq!(normalize_newlines("a\r\r\nb\r"), "a\n\nb\n");
    }

    #[test]
    fn expand_no_tabs_no_ensure() {
        assert_eq!(expand_tabs_and_ensure("a\nb\n", 0, false), "a\nb\n");
        assert_eq!(expand_tabs_and_ensure("ab", 8, false), "ab");
    }

    #[test]
    fn expand_tabs_columns() {
        assert_eq!(expand_tabs_and_ensure("\t", 8, false), "        ");
        assert_eq!(expand_tabs_and_ensure("ab\t", 8, false), "ab      ");
        assert_eq!(expand_tabs_and_ensure("a\n\tb", 4, false), "a\n    b");
    }

    #[test]
    fn ensure_newline() {
        assert_eq!(expand_tabs_and_ensure("ab", 0, true), "ab\n");
        assert_eq!(expand_tabs_and_ensure("ab\n", 0, true), "ab\n");
        assert_eq!(expand_tabs_and_ensure("", 0, true), "\n");
        assert_eq!(expand_tabs_and_ensure("", 0, false), "");
    }

    #[test]
    fn regex_opt_single_and_empty() {
        assert_eq!(regex_opt(&[], "", ""), "");
        assert_eq!(regex_opt(&["if".to_string()], "", ""), "(if)");
        assert_eq!(regex_opt(&["a+b".to_string()], "", ""), "(a\\+b)");
        // `re.escape` leaves non-ASCII raw (only ASCII specials escape).
        assert_eq!(regex_opt(&["café✓".to_string()], "", ""), "(café✓)");
    }

    #[test]
    fn regex_opt_prefix_suffix_charset() {
        assert_eq!(
            regex_opt(&["a".to_string(), "b".to_string()], "", ""),
            "([ab])"
        );
        assert_eq!(
            regex_opt(&["if".to_string(), "else".to_string()], "\\b", "\\b"),
            "\\b(else|if)\\b"
        );
    }

    #[test]
    fn regex_opt_empty_string_member() {
        assert_eq!(
            regex_opt(&["".to_string(), "a".to_string()], "", ""),
            "((?:a)?)"
        );
    }

    #[test]
    fn regex_opt_common_prefix_and_suffix() {
        assert_eq!(
            regex_opt(&["foobar".to_string(), "foobaz".to_string()], "", ""),
            "(fooba(?:[rz]))"
        );
        assert_eq!(
            regex_opt(&["aa".to_string(), "ba".to_string()], "", ""),
            "((?:[ab])a)"
        );
    }
}
