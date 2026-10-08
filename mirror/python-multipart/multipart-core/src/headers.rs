//! `parse_options_header` / `_parseparam` core.
//!
//! Transcription of the module-level helpers, operating on `&str` (the
//! binding decodes latin-1 `bytes` input first, exactly like the original).
//! Case folding and trimming use Unicode semantics like Python's
//! `str.lower`/`str.strip`; every delimiter is ASCII, so byte indexes stay
//! valid. Latin-1 encoding (with its exact `UnicodeEncodeError` behavior)
//! happens in the binding via CPython itself.
//!
//! License: Apache-2.0 (preserved from the original).

use crate::{py_count, py_find};

/// Split a header value into `;`-separated parts without splitting inside
/// double-quoted strings (vendored `email.message._parseparam` behavior).
pub fn parseparam(s: &str) -> Vec<String> {
    let full = format!(";{s}");
    let b = full.as_bytes();
    let len = b.len() as i64;
    let mut plist = Vec::new();
    let mut start: i64 = 0;
    while py_find(b, b";", start, Some(len)) == Some(start as usize) {
        start += 1;
        let mut end = py_find(b, b";", start, Some(len)).map(|p| p as i64);
        let mut ind = start;
        let mut diff: i64 = 0;
        while end.is_some_and(|e| e > 0) {
            let e = end.unwrap();
            diff += py_count(b, b"\"", ind, e) as i64 - py_count(b, b"\\\"", ind, e) as i64;
            if diff % 2 == 0 {
                break;
            }
            let new_ind = py_find(b, b";", e + 1, Some(len)).map(|p| p as i64);
            end = Some(ind);
            ind = new_ind.unwrap_or(-1);
        }
        let e = end.unwrap_or(len);
        // `end` here is exclusive only when found; when missing it equals
        // `len` (same convention as the original's `-1` → `len(s)` fixup).
        let end_usize = e as usize;
        let start_usize = start as usize;
        let i = py_find(b, b"=", start, Some(e)).map(|p| p as i64);
        let f = match i {
            None => slice_of(b, start_usize, end_usize).trim().to_string(),
            Some(i) => {
                slice_of(b, start_usize, i as usize).trim_end().to_lowercase()
                    + "="
                    + slice_of(b, i as usize + 1, end_usize).trim_start()
            }
        };
        plist.push(f.trim().to_string());
        start = end_usize as i64;
    }
    plist
}

/// Parse a Content-Type header into `(ctype, [(key, value)])` strings.
/// Returns `(String::new(), vec![])` for falsy input (checked by the caller).
pub fn parse_options_header_str(value: &str) -> (String, Vec<(String, String)>) {
    if !value.contains(';') {
        return (value.to_lowercase().trim().to_string(), Vec::new());
    }
    let mut parts = parseparam(value).into_iter();
    let ctype = parts.next().unwrap_or_default();
    let mut options = Vec::new();
    for segment in parts {
        let (key, val) = match segment.find('=') {
            Some(p) => (segment[..p].to_string(), segment[p + 1..].to_string()),
            None => (segment.clone(), String::new()),
        };
        // RFC 7578 §4.2 forbids the RFC 5987/2231 extended syntax in
        // multipart/form-data; those parameters are ignored.
        if key.contains('*') {
            continue;
        }
        let mut val = val;
        if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
            val = val[1..val.len() - 1].replace("\\\\", "\\").replace("\\\"", "\"");
        }
        // Work around an IE6 bug where the full file path is sent instead
        // of just the filename.
        if key == "filename" && (val.get(1..3) == Some(":\\") || val.get(..2) == Some("\\\\")) {
            val = val.split('\\').next_back().unwrap_or(&val).to_string();
        }
        options.push((key, val));
    }
    (ctype, options)
}

/// Byte slice as `&str` (bounds are ASCII-derived, always on boundaries).
fn slice_of(b: &[u8], s: usize, e: usize) -> &str {
    // `s`/`e` derive from ASCII delimiter searches, so they sit on UTF-8
    // boundaries exactly like the original's str slices.
    std::str::from_utf8(&b[s..e.min(b.len())]).expect("ascii-derived bounds")
}
