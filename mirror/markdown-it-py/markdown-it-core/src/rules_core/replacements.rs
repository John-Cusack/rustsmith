//! `replacements` core rule (mirror of
//! `markdown_it/rules_core/replacements.py`).
//!
//! The em/en-dash patterns carry `(?=...)` lookahead plus multiline `^` and
//! run on `fancy-regex`; the rest use `regex`.

use std::sync::LazyLock;

use crate::SharedCoreState;

static RARE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\+-|\.\.|\?\?\?\?|!!!!|,,|--").unwrap());
static SCOPED_ABBR_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)\((c|tm|r)\)").unwrap());
static PLUS_MINUS_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\+-").unwrap());
static ELLIPSIS_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\.{2,}").unwrap());
static ELLIPSIS_QE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"([?!])…").unwrap());
static QE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"([?!]){4,}").unwrap());
static COMMA_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r",{2,}").unwrap());
/// Hand-rolled dash rules (the originals use `(?=...)` lookahead).
/// `(?m)^` needs no special case: a preceding newline is itself `[^-]`.
fn em_dash(s: &str) -> String {
    apply_dash(s, 3, "\u{2014}", &|prev| prev != Some('-'), &|next| next != Some('-'))
}

fn en_dash(s: &str) -> String {
    apply_dash(
        s,
        2,
        "\u{2013}",
        &|prev| prev.is_none() || prev.map(crate::common_utils::py_is_space).unwrap_or(false),
        &|next| next.is_none() || next.map(crate::common_utils::py_is_space).unwrap_or(false),
    )
}

fn en_dash_indent(s: &str) -> String {
    apply_dash(
        s,
        2,
        "\u{2013}",
        &|prev| match prev {
            None => true,
            Some(c) => c != '-' && !crate::common_utils::py_is_space(c),
        },
        &|next| match next {
            None => true,
            Some(c) => c != '-' && !crate::common_utils::py_is_space(c),
        },
    )
}

fn apply_dash(
    s: &str,
    run: usize,
    replacement: &str,
    prev_ok: &dyn Fn(Option<char>) -> bool,
    next_ok: &dyn Fn(Option<char>) -> bool,
) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '-' && (i..i + run).all(|j| chars.get(j) == Some(&'-')) {
            let prev = if i == 0 { None } else { Some(chars[i - 1]) };
            let next = chars.get(i + run).copied();
            // `---` vs `--`: a 3-run also contains a 2-run; the original
            // patterns are applied in EM, EN, EN-INDENT order, each scanning
            // independently, so mirror that exactly (no cross-pattern state).
            if prev_ok(prev) && next_ok(next) {
                out.push_str(replacement);
                i += run;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn scoped_abbr_lower(name: &str) -> &str {
    match name.to_lowercase().as_str() {
        "c" => "©",
        "r" => "®",
        _ => "™",
    }
}

fn replace_scoped(children: &[crate::SharedToken]) {
    let mut inside_autolink = 0i64;
    for token in children {
        let typ = token.borrow().typ.clone();
        if typ == "text" && inside_autolink == 0 {
            let content = token.borrow().content.clone();
            let replaced = SCOPED_ABBR_RE
                .replace_all(&content, |caps: &regex::Captures| scoped_abbr_lower(&caps[1]).to_string())
                .into_owned();
            token.borrow_mut().content = replaced;
        }
        if typ == "link_open" && token.borrow().info == "auto" {
            inside_autolink -= 1;
        }
        if typ == "link_close" && token.borrow().info == "auto" {
            inside_autolink += 1;
        }
    }
}

fn replace_rare(children: &[crate::SharedToken]) {
    let mut inside_autolink = 0i64;
    for token in children {
        let typ = token.borrow().typ.clone();
        if typ == "text" && inside_autolink == 0 && RARE_RE.is_match(&token.borrow().content) {
            let mut content = token.borrow().content.clone();
            content = PLUS_MINUS_RE.replace_all(&content, "±").into_owned();
            content = ELLIPSIS_RE.replace_all(&content, "…").into_owned();
            content = ELLIPSIS_QE_RE.replace_all(&content, "$1..").into_owned();
            content = QE_RE.replace_all(&content, "$1$1$1").into_owned();
            content = COMMA_RE.replace_all(&content, ",").into_owned();
            content = em_dash(&content);
            content = en_dash(&content);
            content = en_dash_indent(&content);
            token.borrow_mut().content = content;
        }
        if typ == "link_open" && token.borrow().info == "auto" {
            inside_autolink -= 1;
        }
        if typ == "link_close" && token.borrow().info == "auto" {
            inside_autolink += 1;
        }
    }
}

pub fn replace(state: &SharedCoreState) {
    if !state.borrow().md.options.borrow().typographer {
        return;
    }
    let tokens = state.borrow().tokens.clone();
    for token in &tokens {
        if token.borrow().typ != "inline" {
            continue;
        }
        let children = match token.borrow().children.clone() {
            Some(c) => c,
            None => continue,
        };
        // Verbatim: the gates search the parent inline content.
        let content = token.borrow().content.clone();
        if SCOPED_ABBR_RE.is_match(&content) {
            replace_scoped(&children);
        }
        if RARE_RE.is_match(&content) {
            replace_rare(&children);
        }
    }
}
