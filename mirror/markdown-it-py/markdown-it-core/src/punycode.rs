//! Punycode codec (exact mirror of CPython `encodings/punycode.py`, as used
//! by `markdown_it/_punycode.py`).
//!
//! Deliberately NOT RFC-3492-case-preserving: like the codec, non-basic
//! code points carry no case flags, and decoding uppercases the extended
//! part. Errors surface as [`DecodeError`] (message-verbatim); the binding
//! raises `UnicodeError`.

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeError(pub String);

fn t(j: i64, bias: i64) -> i64 {
    (36 * (j + 1) - bias).clamp(1, 26)
}

fn adapt(mut delta: i64, first: bool, numchars: i64) -> i64 {
    if first {
        delta /= 700;
    } else {
        delta /= 2;
    }
    delta += delta / numchars;
    let mut divisions = 0;
    while delta > 455 {
        delta /= 35;
        divisions += 36;
    }
    divisions + (36 * delta / (delta + 38))
}

const DIGITS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

fn generate_generalized_integer(mut n: i64, bias: i64) -> Vec<u8> {
    let mut result = Vec::new();
    let mut j = 0;
    loop {
        let t = t(j, bias);
        if n < t {
            result.push(DIGITS[n as usize]);
            return result;
        }
        result.push(DIGITS[(t + ((n - t) % (36 - t))) as usize]);
        n = (n - t) / (36 - t);
        j += 1;
    }
}

fn selective_len(s: &[char], max: u32) -> i64 {
    s.iter().filter(|&&c| (c as u32) < max).count() as i64
}

fn selective_find(s: &[char], ch: char, mut index: i64, mut pos: i64) -> (i64, i64) {
    let l = s.len() as i64;
    loop {
        pos += 1;
        if pos == l {
            return (-1, -1);
        }
        let c = s[pos as usize];
        if c == ch {
            return (index + 1, pos);
        } else if c < ch {
            index += 1;
        }
    }
}

pub fn punycode_encode(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut base: Vec<u8> = Vec::new();
    let mut extended_set = BTreeSet::new();
    for &c in &chars {
        if (c as u32) < 128 {
            base.push(c as u8);
        } else {
            extended_set.insert(c);
        }
    }
    let extended: Vec<char> = extended_set.into_iter().collect();
    // insertion_unsort
    let mut oldchar: u32 = 0x80;
    let mut result: Vec<i64> = Vec::new();
    let mut oldindex: i64 = -1;
    for &c in &extended {
        let (mut index, mut pos) = (-1i64, -1i64);
        let char_ord = c as u32;
        let curlen = selective_len(&chars, char_ord);
        let mut delta = (curlen + 1) * (char_ord as i64 - oldchar as i64);
        loop {
            let (ni, np) = selective_find(&chars, c, index, pos);
            index = ni;
            pos = np;
            if index == -1 {
                break;
            }
            delta += index - oldindex;
            result.push(delta - 1);
            oldindex = index;
            delta = 0;
        }
        oldchar = char_ord;
    }
    // generate_integers
    let mut out: Vec<u8> = Vec::new();
    let mut bias = 72i64;
    for (points, delta) in result.into_iter().enumerate() {
        out.extend(generate_generalized_integer(delta, bias));
        bias = adapt(delta, points == 0, base.len() as i64 + points as i64 + 1);
    }
    if base.is_empty() {
        String::from_utf8(out).unwrap_or_default()
    } else {
        base.push(b'-');
        base.extend(out);
        String::from_utf8(base).unwrap_or_default()
    }
}

fn decode_generalized_number(
    extended: &[char],
    mut extpos: usize,
    bias: i64,
) -> Result<(usize, Option<i64>), DecodeError> {
    let mut result: i64 = 0;
    let mut w: i64 = 1;
    let mut j = 0;
    loop {
        let ch = extended.get(extpos).copied().ok_or_else(|| DecodeError("incomplete punicode string".to_string()))?;
        extpos += 1;
        let cu = ch as u32;
        let digit = if (0x41..=0x5A).contains(&cu) {
            cu as i64 - 0x41
        } else if (0x30..=0x39).contains(&cu) {
            cu as i64 - 22
        } else {
            return Err(DecodeError(format!("Invalid extended code point '{ch}'")));
        };
        let t = t(j, bias);
        result += digit * w;
        if digit < t {
            return Ok((extpos, Some(result)));
        }
        w *= 36 - t;
        j += 1;
    }
}

fn insertion_sort(mut base: Vec<char>, extended: &[char]) -> Result<Vec<char>, DecodeError> {
    let mut char_ord: u32 = 0x80;
    let mut pos: i64 = -1;
    let mut bias = 72i64;
    let mut extpos = 0;
    while extpos < extended.len() {
        let (newpos, delta) = decode_generalized_number(extended, extpos, bias)?;
        let Some(delta) = delta else {
            return Ok(base);
        };
        pos += delta + 1;
        char_ord = char_ord.wrapping_add((pos / (base.len() as i64 + 1)) as u32);
        if char_ord > 0x10FFFF {
            return Err(DecodeError(format!("Invalid character U+{char_ord:x}")));
        }
        pos %= base.len() as i64 + 1;
        let ch = char::from_u32(char_ord).unwrap_or('?');
        base.insert(pos as usize, ch);
        bias = adapt(delta, extpos == 0, base.len() as i64);
        extpos = newpos;
    }
    Ok(base)
}

pub fn punycode_decode(text: &str) -> Result<String, DecodeError> {
    // Verbatim: split at the LAST "-"; no dash means empty base.
    let (base, extended) = match text.rfind('-') {
        None => (String::new(), text.to_uppercase()),
        Some(pos) => (text[..pos].to_string(), text[pos + 1..].to_uppercase()),
    };
    let base_chars: Vec<char> = base.chars().collect();
    let ext_chars: Vec<char> = extended.chars().collect();
    Ok(insertion_sort(base_chars, &ext_chars)?.into_iter().collect())
}

fn is_separator(c: char) -> bool {
    matches!(c, '.' | '。' | '．' | '｡')
}

fn map_domain(s: &str, f: impl Fn(&str) -> String) -> String {
    // Verbatim: `split("@")`, only parts[0]/parts[1] survive.
    let parts: Vec<&str> = s.split('@').collect();
    let (mut result, mut string) = (String::new(), s);
    if parts.len() > 1 {
        result = format!("{}@", parts[0]);
        string = parts[1];
    }
    let mut labels: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in string.chars() {
        if is_separator(c) {
            labels.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    labels.push(current);
    result + &labels.iter().map(|l| f(l)).collect::<Vec<_>>().join(".")
}

pub fn to_unicode(obj: &str) -> Result<String, DecodeError> {
    // Verbatim: decode errors propagate (no `suppress` on this path).
    let parts: Vec<&str> = obj.split('@').collect();
    let (mut result, string) = if parts.len() > 1 {
        (format!("{}@", parts[0]), parts[1])
    } else {
        (String::new(), obj)
    };
    let mut labels: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in string.chars() {
        if is_separator(c) {
            labels.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    labels.push(current);
    let mut out = Vec::new();
    for label in &labels {
        if let Some(rest) = label.strip_prefix("xn--") {
            out.push(punycode_decode(&rest.to_lowercase())?);
        } else {
            out.push(label.clone());
        }
    }
    result.push_str(&out.join("."));
    Ok(result)
}

pub fn to_ascii(obj: &str) -> String {
    map_domain(obj, |label| {
        if label.chars().any(|c| (c as u32) > 0x7E) {
            format!("xn--{}", punycode_encode(label))
        } else {
            label.to_string()
        }
    })
}
