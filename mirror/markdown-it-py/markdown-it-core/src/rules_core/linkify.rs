//! `linkify` core rule (mirror of `markdown_it/rules_core/linkify.py`).
//!
//! `md.linkify` calls (`pretest`/`test`/`match`) cross the [`Host`]; the
//! missing-install `ModuleNotFoundError` surfaces through the same
//! [`LinkifyMissing`](crate::rules_inline::linkify::LinkifyMissing) panic as
//! the inline rule.

use std::rc::Rc;
use std::sync::LazyLock;

use regex::Regex;

use crate::common_utils::{char_slice, char_suffix, is_link_close, is_link_open};
use crate::token::shared_token;
use crate::{Host, Token};

static HTTP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^http://").unwrap());
static MAILTO_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^mailto:").unwrap());
static TEST_MAILTO_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^mailto:").unwrap());

pub fn linkify(state: &crate::SharedCoreState, host: &Rc<dyn Host>) {
    if !state.borrow().md.options.borrow().linkify {
        return;
    }
    if !host.has_linkify() {
        std::panic::panic_any(crate::rules_inline::linkify::LinkifyMissing);
    }
    let tokens = state.borrow().tokens.clone();
    for inline_token in &tokens {
        if inline_token.borrow().typ != "inline" {
            continue;
        }
        let content = inline_token.borrow().content.clone();
        if host.linkify_pretest(&content) != Some(true) {
            continue;
        }
        let children = match inline_token.borrow().children.clone() {
            Some(c) => c,
            // Verbatim `assert tokens is not None`; unreachable after the
            // inline rule (it installs `[]`), skipped defensively.
            None => continue,
        };
        let mut tokens_vec = children;
        let mut html_link_level: i64 = 0;
        let mut i = tokens_vec.len() as i64;
        while i >= 1 {
            i -= 1;
            let ui = i as usize;
            let ctype = tokens_vec[ui].borrow().typ.clone();
            if ctype == "link_close" {
                let level = tokens_vec[ui].borrow().level;
                i -= 1;
                while tokens_vec[i as usize].borrow().level != level
                    && tokens_vec[i as usize].borrow().typ != "link_open"
                {
                    i -= 1;
                }
                continue;
            }
            if ctype == "html_inline" {
                let c = tokens_vec[ui].borrow().content.clone();
                if is_link_open(&c) && html_link_level > 0 {
                    html_link_level -= 1;
                }
                if is_link_close(&c) {
                    html_link_level += 1;
                }
            }
            if html_link_level > 0 {
                continue;
            }
            if ctype == "text" && host.linkify_test(&tokens_vec[ui].borrow().content) == Some(true) {
                let text = tokens_vec[ui].borrow().content.clone();
                let mut links = host.linkify_match(&text);
                let level = tokens_vec[ui].borrow().level;
                let mut last_pos = 0usize;
                if !links.is_empty()
                    && links[0].index == 0
                    && i > 0
                    && tokens_vec[ui - 1].borrow().typ == "text_special"
                {
                    links.remove(0);
                }
                let mut nodes: Vec<crate::SharedToken> = Vec::new();
                for link in &links {
                    let url = link.url.clone();
                    let full_url = host.normalize_link(&url);
                    if !host.validate_link(&full_url, None) {
                        continue;
                    }
                    let url_text = if link.schema.is_empty() {
                        HTTP_RE.replace(&host.normalize_link_text(&format!("http://{}", link.text)), "").into_owned()
                    } else if link.schema == "mailto:" && TEST_MAILTO_RE.is_match(&link.text) {
                        MAILTO_RE.replace(&host.normalize_link_text(&format!("mailto:{}", link.text)), "").into_owned()
                    } else {
                        host.normalize_link_text(&link.text)
                    };
                    let pos = link.index.max(0) as usize;
                    if pos > last_pos {
                        let mut token = Token::new("text", "", 0);
                        token.content = char_slice(&text, last_pos, pos);
                        token.level = level;
                        nodes.push(shared_token(token));
                    }
                    let mut token = Token::new("link_open", "a", 1);
                    token.attrs = vec![("href".to_string(), crate::token::AttrVal::Str(full_url))];
                    token.level = level;
                    token.markup = "linkify".to_string();
                    token.info = "auto".to_string();
                    nodes.push(shared_token(token));
                    let mut token = Token::new("text", "", 0);
                    token.content = url_text;
                    token.level = level + 1;
                    nodes.push(shared_token(token));
                    let mut token = Token::new("link_close", "a", -1);
                    token.level = level;
                    token.markup = "linkify".to_string();
                    token.info = "auto".to_string();
                    nodes.push(shared_token(token));
                    last_pos = link.last_index.max(0) as usize;
                }
                if last_pos < text.chars().count() {
                    let mut token = Token::new("text", "", 0);
                    token.content = char_suffix(&text, last_pos);
                    token.level = level;
                    nodes.push(shared_token(token));
                }
                // `arrayReplaceAt(tokens, i, nodes)` verbatim.
                let mut replaced = tokens_vec[..ui].to_vec();
                replaced.extend(nodes);
                replaced.extend_from_slice(&tokens_vec[ui + 1..]);
                tokens_vec = replaced;
            }
        }
        inline_token.borrow_mut().children = Some(tokens_vec);
    }
}
