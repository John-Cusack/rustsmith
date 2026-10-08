//! `text_join` core rule (mirror of `markdown_it/rules_core/text_join.py`).

use crate::SharedToken;

pub fn text_join(state: &crate::SharedCoreState) {
    let tokens = state.borrow().tokens.clone();
    for inline_token in &tokens {
        if inline_token.borrow().typ != "inline" {
            continue;
        }
        let children = inline_token.borrow().children.clone().unwrap_or_default();
        let mut new_tokens: Vec<SharedToken> = Vec::new();
        let mut i = 0;
        while i < children.len() {
            let child = children[i].clone();
            if child.borrow().typ == "text_special" {
                child.borrow_mut().typ = "text".to_string();
            }
            let is_text = child.borrow().typ == "text";
            let last_is_text = new_tokens.last().map(|t: &SharedToken| t.borrow().typ == "text").unwrap_or(false);
            if is_text && last_is_text {
                let mut parts = vec![
                    new_tokens.last().unwrap().borrow().content.clone(),
                    child.borrow().content.clone(),
                ];
                i += 1;
                while i < children.len() {
                    let next = children[i].clone();
                    if next.borrow().typ == "text_special" {
                        next.borrow_mut().typ = "text".to_string();
                    }
                    if next.borrow().typ != "text" {
                        break;
                    }
                    parts.push(next.borrow().content.clone());
                    i += 1;
                }
                new_tokens.last().unwrap().borrow_mut().content = parts.concat();
            } else {
                new_tokens.push(child);
                i += 1;
            }
        }
        inline_token.borrow_mut().children = Some(new_tokens);
    }
}
