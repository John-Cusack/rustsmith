//! HTML renderer (mirror of `markdown_it/renderer.py`).
//!
//! The render loop consults `renderer.rules` through [`Host`] for every
//! token type present there (the binding populates it with bound methods,
//! exactly like `RendererHTML.__init__`, so subclass overrides and
//! `add_render_rule` entries flow through the same path); types absent from
//! the dict fall back to [`render_token`]. `image` mutates the token's `alt`
//! attribute verbatim.

use std::rc::Rc;

use crate::common_utils::{escape_html, md_trim, md_trim_split_max, unescape_all};
use crate::token::AttrVal;
use crate::{Host, SharedToken};

/// Scalar render options snapshotted per render call.
#[derive(Debug, Clone)]
pub struct RenderOpts {
    pub xhtml_out: bool,
    pub breaks: bool,
    pub lang_prefix: String,
    pub tasklists_editable: bool,
}

pub fn render(
    tokens: &[SharedToken],
    opts: &RenderOpts,
    env: &crate::options_env::EnvData,
    host: &Rc<dyn Host>,
    options: &crate::SharedOptions,
) -> String {
    let mut result = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if token.borrow().typ == "inline" {
            if let Some(children) = token.borrow().children.clone() {
                result.push_str(&render_inline(&children, opts, env, host, options));
            }
        } else if host.render_override(&token.borrow().typ).is_some() {
            let id = host.render_override(&token.borrow().typ).unwrap();
            result.push_str(&host.call_render_rule(id, tokens, i, options, env));
        } else {
            result.push_str(&render_token(tokens, i, opts));
        }
    }
    result
}

pub fn render_inline(
    tokens: &[SharedToken],
    opts: &RenderOpts,
    env: &crate::options_env::EnvData,
    host: &Rc<dyn Host>,
    options: &crate::SharedOptions,
) -> String {
    let mut result = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if host.render_override(&token.borrow().typ).is_some() {
            let id = host.render_override(&token.borrow().typ).unwrap();
            result.push_str(&host.call_render_rule(id, tokens, i, options, env));
        } else {
            result.push_str(&render_token(tokens, i, opts));
        }
    }
    result
}

pub fn render_token(tokens: &[SharedToken], idx: usize, opts: &RenderOpts) -> String {
    let token = tokens[idx].borrow();
    if token.hidden {
        return String::new();
    }
    let mut result = String::new();
    if token.block && token.nesting != -1 && idx > 0 && tokens[idx - 1].borrow().hidden {
        result.push('\n');
    }
    result.push_str(if token.nesting == -1 { "</" } else { "<" });
    result.push_str(&token.tag);
    result.push_str(&render_attrs(&token));
    if token.nesting == 0 && opts.xhtml_out {
        result.push_str(" /");
    }
    let mut need_lf = false;
    if token.block {
        need_lf = true;
        if token.nesting == 1 && idx + 1 < tokens.len() {
            let next = tokens[idx + 1].borrow();
            if next.typ == "inline"
                || next.hidden
                || (next.nesting == -1 && next.tag == token.tag)
            {
                need_lf = false;
            }
        }
    }
    if need_lf {
        result.push_str(">\n");
    } else {
        result.push('>');
    }
    result
}

pub fn render_attrs(token: &crate::Token) -> String {
    let mut result = String::new();
    for (key, value) in token.attr_items() {
        result.push(' ');
        result.push_str(&escape_html(key));
        result.push_str("=\"");
        result.push_str(&escape_html(&attr_value_str(value)));
        result.push('"');
    }
    result
}

fn attr_value_str(v: &AttrVal) -> String {
    match v {
        AttrVal::Str(s) => s.clone(),
        AttrVal::Int(i) => i.to_string(),
        AttrVal::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{f:.1}")
            } else {
                format!("{f}")
            }
        }
        AttrVal::Bool(b) => {
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
    }
}

pub fn render_inline_as_text(tokens: &[SharedToken], _opts: &RenderOpts) -> String {
    let mut result = String::new();
    for token in tokens {
        let t = token.borrow();
        match t.typ.as_str() {
            "text" => result.push_str(&t.content),
            "image" => {
                if let Some(children) = t.children.clone() {
                    drop(t);
                    result.push_str(&render_inline_as_text(&children, _opts));
                }
            }
            "html_inline" | "html_block" => result.push_str(&t.content),
            "softbreak" | "hardbreak" => result.push('\n'),
            _ => {}
        }
    }
    result
}

// Default render rules (bound-method bodies).

pub fn rule_list_item_open(
    tokens: &[SharedToken],
    idx: usize,
    opts: &RenderOpts,
    host: &Rc<dyn Host>,
) -> String {
    let mut result = render_token(tokens, idx, opts);
    let token = tokens[idx].borrow();
    if token.meta.iter().any(|(k, _)| k == "checked") {
        let checked = token
            .meta
            .iter()
            .find(|(k, _)| k == "checked")
            .map(|(_, v)| v.truthy())
            .unwrap_or(false);
        let checked_attr = if checked { " checked=\"\"" } else { "" };
        let disabled_attr = if opts.tasklists_editable { "" } else { " disabled=\"\"" };
        result.push_str(&format!(
            "<input class=\"task-list-item-checkbox\"{disabled_attr} type=\"checkbox\"{checked_attr}> "
        ));
    }
    let _ = host;
    result
}

pub fn rule_code_inline(tokens: &[SharedToken], idx: usize, _opts: &RenderOpts) -> String {
    let token = tokens[idx].borrow();
    format!(
        "<code{}>{}</code>",
        render_attrs(&token),
        escape_html(&token.content)
    )
}

pub fn rule_code_block(tokens: &[SharedToken], idx: usize, _opts: &RenderOpts) -> String {
    let token = tokens[idx].borrow();
    format!(
        "<pre{}><code>{}</code></pre>\n",
        render_attrs(&token),
        escape_html(&token.content)
    )
}

pub fn rule_fence(
    tokens: &[SharedToken],
    idx: usize,
    opts: &RenderOpts,
    host: &Rc<dyn Host>,
) -> String {
    let (info, content, attrs) = {
        let t = tokens[idx].borrow();
        (t.info.clone(), t.content.clone(), t.attrs.clone())
    };
    let info = if info.is_empty() { String::new() } else { md_trim(&unescape_all(&info)) };
    let mut lang_name = String::new();
    let mut lang_attrs = String::new();
    if !info.is_empty() {
        // Not `str.split()`: CommonMark whitespace set, maxsplit 1.
        let arr = md_trim_split_max(&info, 1);
        lang_name = arr[0].clone();
        if arr.len() == 2 {
            lang_attrs = arr[1].clone();
        }
    }
    let highlighted = match host.call_highlight(&content, &lang_name, &lang_attrs) {
        Some(h) if !h.is_empty() => h,
        _ => escape_html(&content),
    };
    if highlighted.starts_with("<pre") {
        return highlighted + "\n";
    }
    if !info.is_empty() {
        let mut tmp = crate::Token::new("", "", 0);
        tmp.attrs = attrs;
        if let Err(e) = tmp.attr_join("class", &(opts.lang_prefix.clone() + &lang_name)) {
            crate::panic_type_error(e);
        }
        return format!(
            "<pre><code{}>{}</code></pre>\n",
            render_attrs(&tmp),
            highlighted
        );
    }
    let t = tokens[idx].borrow();
    format!(
        "<pre><code{}>{}</code></pre>\n",
        render_attrs(&t),
        highlighted
    )
}

pub fn rule_image(
    tokens: &[SharedToken],
    idx: usize,
    opts: &RenderOpts,
    env: &crate::options_env::EnvData,
    host: &Rc<dyn Host>,
) -> String {
    {
        let token = tokens[idx].clone();
        let children = token.borrow().children.clone();
        let alt = match children {
            Some(c) => render_inline_as_text(&c, opts),
            None => String::new(),
        };
        token.borrow_mut().attr_set("alt", AttrVal::Str(alt));
    }
    // Verbatim: `return self.renderToken(tokens, idx, options, env)`.
    let _ = (env, host);
    render_token(tokens, idx, opts)
}

pub fn rule_hardbreak(_tokens: &[SharedToken], _idx: usize, opts: &RenderOpts) -> String {
    if opts.xhtml_out {
        "<br />\n".to_string()
    } else {
        "<br>\n".to_string()
    }
}

pub fn rule_softbreak(_tokens: &[SharedToken], _idx: usize, opts: &RenderOpts) -> String {
    if opts.breaks {
        if opts.xhtml_out {
            "<br />\n".to_string()
        } else {
            "<br>\n".to_string()
        }
    } else {
        "\n".to_string()
    }
}

pub fn rule_text(tokens: &[SharedToken], idx: usize, _opts: &RenderOpts) -> String {
    escape_html(&tokens[idx].borrow().content)
}

pub fn rule_html_block(tokens: &[SharedToken], idx: usize, _opts: &RenderOpts) -> String {
    tokens[idx].borrow().content.clone()
}

pub fn rule_html_inline(tokens: &[SharedToken], idx: usize, _opts: &RenderOpts) -> String {
    tokens[idx].borrow().content.clone()
}
