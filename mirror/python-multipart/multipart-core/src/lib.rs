//! Pure-Rust core of the `python-multipart` Stage-1 mirror.
//!
//! Byte-faithful port of the two streaming state machines in
//! `python_multipart/multipart.py` (`MultipartParser._internal_write`,
//! `QuerystringParser._internal_write`) plus the `parse_options_header` /
//! `_parseparam` header helpers. No Python dependency: dispatch goes through
//! the [`Emitter`] trait, so Rust consumers drive the parsers directly and
//! the PyO3 binding (`src/lib.rs`) forwards events to Python callbacks.
//!
//! Fidelity contract (see PORTING rules in `repo-content.json`):
//! * `internal_write` parses into locals and commits to `self` only on
//!   `Ok` — any error leaves pre-chunk state, exactly like the original.
//! * Offsets are per-chunk (`i64`; `-1` transient indexing wraps like
//!   Python `data[-1]`).
//! * `find`/`rfind`/`startswith`/`count` use Python slice semantics for
//!   negative bounds.
//!
//! License: Apache-2.0 (preserved from the original).

pub mod headers;
pub mod multipart;
pub mod querystring;
/// Sink for parser events. Buffer identity mirrors the original: data
/// events reference the input chunk (or the framed boundary object) with
/// chunk-absolute indexes; only lookbehind prefixes are fresh buffers.
pub trait Emitter {
    /// Error raised by a callback (propagates with no state commit).
    type Err;
    /// Notification event (`part_begin`, `field_end`, `end`, ...).
    fn event(&mut self, name: &str) -> Result<(), Self::Err>;
    /// Data event on the input chunk (`input[start..end]`).
    fn input(&mut self, name: &str, start: usize, end: usize) -> Result<(), Self::Err>;
    /// Data event on the framed boundary object (`boundary[..end]`).
    fn boundary(&mut self, name: &str, end: usize) -> Result<(), Self::Err>;
    /// Data event on a fresh lookbehind buffer (`buf`, whole slice).
    fn owned(&mut self, name: &str, buf: &[u8]) -> Result<(), Self::Err>;
    /// `logger.warning` equivalent, in encounter order.
    fn warn(&mut self, msg: &str) -> Result<(), Self::Err>;
}

/// A parse failure: message plus the per-chunk byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFail {
    /// `true` for multipart errors, `false` for querystring errors.
    pub multipart: bool,
    /// Byte-for-byte copy of the original message.
    pub message: String,
    /// Offset into the current input chunk.
    pub offset: i64,
}

/// `internal_write` outcome: parse failure or callback failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteErr<E> {
    /// The byte stream was rejected (state uncommitted).
    Parse(ParseFail),
    /// A callback failed (state uncommitted; the error propagates as-is).
    Callback(E),
}

impl<E> From<ParseFail> for WriteErr<E> {
    fn from(f: ParseFail) -> Self {
        WriteErr::Parse(f)
    }
}

/// Python `bytes.find(sub, start, end)` with slice semantics for bounds.
/// `start`/`end` are pre-normalization indexes (may be negative); the
/// haystack end defaults to `len` when `end` is `None`.
pub fn py_find(hay: &[u8], needle: &[u8], start: i64, end: Option<i64>) -> Option<usize> {
    let len = hay.len() as i64;
    let s = norm_bound(len, start);
    let e = end.map(|e| norm_bound(len, e)).unwrap_or(hay.len());
    if needle.is_empty() {
        return Some(s.min(e));
    }
    if s >= e || e > hay.len() {
        return None;
    }
    hay[s..e]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| s + p)
}

/// Python `bytes.rfind(byte, lo, end)`: last occurrence of `byte` in
/// `hay[lo..end]`.
pub fn py_rfind_byte(hay: &[u8], byte: u8, lo: i64, end: usize) -> Option<usize> {
    let s = norm_bound(hay.len() as i64, lo).min(end);
    hay[s..end].iter().rposition(|&b| b == byte).map(|p| s + p)
}

/// Python `bytes.startswith(needle, start, end)`: `hay[s..e]` starts with
/// `needle` (a longer needle never matches a shorter window).
pub fn py_startswith(hay: &[u8], needle: &[u8], start: i64, end: usize) -> bool {
    let s = norm_bound(hay.len() as i64, start).min(end);
    let avail = end - s;
    needle.len() <= avail && hay[s..s + needle.len()] == needle[..]
}

/// Python `str.count(sub, start, end)` for ASCII needles (byte-identical on
/// UTF-8 text since ASCII bytes never occur inside multibyte sequences).
pub fn py_count(hay: &[u8], needle: &[u8], start: i64, end: i64) -> usize {
    debug_assert!(!needle.is_empty());
    let len = hay.len() as i64;
    let mut s = norm_bound(len, start);
    let e = norm_bound(len, end);
    if s >= e {
        return 0;
    }
    let mut n = 0;
    while s + needle.len() <= e {
        match hay[s..e]
            .windows(needle.len())
            .position(|w| w == needle)
        {
            Some(p) => {
                n += 1;
                s += p + needle.len();
            }
            None => break,
        }
    }
    n
}

/// Slice-notation bound normalization: negatives wrap, results clamp.
fn norm_bound(len: i64, b: i64) -> usize {
    (if b < 0 { (len + b).max(0) } else { b.min(len) }) as usize
}

/// Python negative indexing for the transient `i == -1` steps
/// (`data[-1]` reads the last byte of the whole chunk).
pub fn py_get(data: &[u8], i: i64) -> u8 {
    if i >= 0 {
        data[i as usize]
    } else {
        data[(data.len() as i64 + i) as usize]
    }
}

pub const CR: u8 = b'\r';
pub const LF: u8 = b'\n';
pub const COLON: u8 = b':';
pub const SPACE: u8 = b' ';
pub const HYPHEN: u8 = b'-';
pub const AMPERSAND: u8 = b'&';
