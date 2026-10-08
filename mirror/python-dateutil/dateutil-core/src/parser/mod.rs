// SPDX-License-Identifier: Apache-2.0
//! Fuzzy/strict date-time string parser: pure-Rust port of
//! `dateutil/parser/_parser.py` (`_timelex`, `parserinfo` default tables,
//! `_ymd`, `parser._parse`, `_parse_numeric_token` and helpers).
//!
//! Provenance: rustsmith Stage-1 mirror of dateutil/dateutil (Apache-2.0),
//! mirroring 2.9.0.post0. No Python dependency: datetimes never materialize
//! here. [`parse::parse_tokens`] maps `&str in -> ParseOk out`; the PyO3
//! binding builds the `datetime` (so CPython validation messages,
//! `OverflowError` shapes, subclasses and tz assembly stay exact).
//!
//! Fidelity notes (all pinned by the differential corpus in
//! `bench_parser/` plus core unit tests):
//! - Lexer char classes are ASCII-exact; non-ASCII uses `is_alphabetic` /
//!   `is_numeric` best-effort (see `lex`). `\x00` is skipped at fetch,
//!   exactly like the `StringIO.read(1)` loop.
//! - `float()` probing accepts ASCII digit runs, `inf`/`infinity`/`nan`
//!   (any case) and Unicode decimal digits; `inf`/`nan` then fail
//!   `_to_decimal`, which is why `parse("inf")` raises even under fuzzy.
//! - Integers wider than the platform cross the boundary as decimal
//!   `String`s so the binding constructs the identical Python `int`
//!   (identical `OverflowError` vs `ParserError` routing).
//! - `Decimal` arithmetic is limited to `int()` truncation, `% 1`
//!   truthiness and `int(60 * frac)`; `frac60` keeps 38 digits, which is
//!   exact for `int()` purposes (see GH #427).
//! - Computed tz offsets use checked `i128`; a >38-digit offset run fails
//!   the parse instead of raising `OverflowError` (accepted approximation,
//!   documented in the port ADR; unobservable on realistic input).
//! - Custom `parserinfo` subclasses and `tzinfos` callables are served by
//!   the binding through the [`Info`] trait (Python-callback fallback);
//!   only the default tables run on the pure-Rust fast path.

pub mod dec;
pub mod lex;
pub mod parse;
pub mod tables;
pub mod ymd;
pub mod isoparser;

pub use dec::{norm_int_str, Dec};
pub use lex::{
    fold_digits, is_num, is_space, is_word, lower_token, next_token, probe_float, LexCursor,
    Timelex, Token,
};
pub use tables::DefaultInfo;
pub use ymd::{cmp_norm, Ymd, YmdTriple};

/// Failure modes of `_parse`: `NoResult` (any `IndexError`/`ValueError`
/// inside the loop -> `ParserError("Unknown string format: %s")`),
/// `Empty` (no field set -> `"String does not contain a date: %s"`),
/// `Aborted` (a custom-info Python callback raised a non-`ValueError`;
/// the binding re-raises the stashed error and discards the result).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fail {
    NoResult,
    Empty,
    Aborted,
}

/// Timezone offset: small values inline, out-of-`i64` computations kept as
/// decimal text so the binding builds the identical Python `int`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TzOff {
    Secs(i64),
    Big(String),
}

impl TzOff {
    pub fn is_zero(&self) -> bool {
        match self {
            TzOff::Secs(n) => *n == 0,
            TzOff::Big(s) => s.bytes().all(|b| b == b'0'),
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            TzOff::Secs(n) => n.to_string(),
            TzOff::Big(s) => s.clone(),
        }
    }
}

/// Unvalidated `_parse` output (validation is a separate step so custom
/// `parserinfo.validate` overrides can replace it in the binding).
#[derive(Default, Debug, PartialEq)]
pub struct ParseOk {
    pub year: Option<String>,
    pub month: Option<String>,
    pub day: Option<String>,
    pub weekday: Option<i64>,
    pub hour: Option<String>,
    pub minute: Option<String>,
    pub second: Option<String>,
    pub micro: Option<u32>,
    pub tzname: Option<String>,
    pub tzoffset: Option<TzOff>,
    pub ampm: Option<i64>,
    pub century_specified: bool,
    pub skipped: Vec<String>,
}

impl ParseOk {
    /// `len(res) == 0` over the eleven `_result` slots.
    pub fn is_empty(&self) -> bool {
        self.year.is_none()
            && self.month.is_none()
            && self.day.is_none()
            && self.weekday.is_none()
            && self.hour.is_none()
            && self.minute.is_none()
            && self.second.is_none()
            && self.micro.is_none()
            && self.tzname.is_none()
            && self.tzoffset.is_none()
            && self.ampm.is_none()
    }
}

pub struct ParseOptions {
    pub dayfirst: bool,
    pub yearfirst: bool,
    pub fuzzy: bool,
    pub cur_year: i64,
    pub century: i64,
}

/// Lookup tables + hooks for `_parse` (mirrors `parserinfo`).
///
/// Numeric probes return small `i64`s per the documented int protocol
/// (the default tables map names to small ints; custom `parserinfo`
/// subclasses honor the same protocol and the binding coerces via
/// `int()`, exactly where the original truncates).
/// `days_in_month` takes decimal strings so arbitrarily large years never
/// overflow; a `ValueError`-like failure is `Err(Fail::NoResult)`.
pub trait Info {
    fn jump(&self, tok: &str) -> bool;
    fn weekday(&self, tok: &str) -> Option<i64>;
    fn month(&self, tok: &str) -> Option<i64>;
    fn hms(&self, tok: &str) -> Option<i64>;
    fn ampm(&self, tok: &str) -> Option<i64>;
    fn pertain(&self, tok: &str) -> bool;
    fn utczone(&self, tok: &str) -> bool;
    fn tzoffset(&self, name: &str) -> Option<TzOff>;
    /// `token in self.info.UTCZONE` (exact-case list membership).
    fn is_utc_abbr(&self, token: &str) -> bool;
    fn convertyear(
        &self,
        year: &str,
        century_specified: bool,
        cur_year: i64,
        century: i64,
    ) -> String;
    fn days_in_month(&self, year: &str, month: &str) -> Result<u8, Fail>;
}

/// Default `parserinfo.validate` (pure): century-window years plus the
/// UTC-name fixups. Custom overrides run in the binding instead.
pub fn validate_default(res: &mut ParseOk, cur_year: i64, century: i64) {
    if let Some(y) = res.year.clone() {
        res.year = Some(convertyear_str(&y, res.century_specified, cur_year, century));
    }
    let tzoff_zero = matches!(&res.tzoffset, Some(o) if o.is_zero());
    let tzname_z = matches!(res.tzname.as_deref(), Some("Z") | Some("z"));
    if (tzoff_zero && res.tzname.is_none()) || tzname_z {
        res.tzname = Some("UTC".to_string());
        res.tzoffset = Some(TzOff::Secs(0));
    } else if res.tzname.is_some() {
        // `res.tzoffset != 0` is true for `None` too: an unresolved
        // UTC-zone name (e.g. `GMT` with no numeric offset) normalizes
        // to offset 0 here.
        let off_unset = res.tzoffset.is_none()
            || matches!(&res.tzoffset, Some(o) if !o.is_zero());
        if off_unset {
            if let Some(name) = res.tzname.clone() {
                if DefaultInfo.utczone(&name) {
                    res.tzoffset = Some(TzOff::Secs(0));
                }
            }
        }
    }
}

/// `parserinfo.convertyear` on decimal strings (exact for all magnitudes:
/// only years `< 100` without a century enter the window arithmetic).
pub fn convertyear_str(
    year: &str,
    century_specified: bool,
    cur_year: i64,
    century: i64,
) -> String {
    if cmp_norm(year, 100) == std::cmp::Ordering::Less && !century_specified {
        let y: i64 = year.parse().unwrap_or(0);
        let mut out = y + century;
        if out >= cur_year + 50 {
            out -= 100;
        } else if out < cur_year - 50 {
            out += 100;
        }
        return out.to_string();
    }
    year.to_string()
}

/// Days in month for the permissive `could_be_day` path. Pure string leap
/// test so arbitrarily large years never overflow.
pub fn days_in_month_str(year: &str, month: i64) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_str(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap_str(year: &str) -> bool {
    let div4 = last_n_mod(year, 2, 4) == 0;
    let div100 = last_n_mod(year, 2, 100) == 0;
    let div400 = last_n_mod(year, 3, 400) == 0;
    div4 && (!div100 || div400)
}

fn last_n_mod(s: &str, n: usize, m: u32) -> u32 {
    let t = if s.len() > n { &s[s.len() - n..] } else { s };
    t.parse::<u32>().unwrap_or(1) % m
}

/// Mirrors `_recombine_skipped`. Generic over `AsRef<str>` so both owned
/// token vectors and borrowed lexer [`Token`] slices feed it.
pub fn recombine_skipped(tokens: &[impl AsRef<str>], skipped_idxs: &[usize]) -> Vec<String> {
    let mut idxs = skipped_idxs.to_vec();
    idxs.sort_unstable();
    let mut out: Vec<String> = Vec::new();
    for (n, idx) in idxs.iter().enumerate() {
        if n > 0 && *idx == idxs[n - 1] + 1 {
            let last = out.len() - 1;
            out[last].push_str(tokens[*idx].as_ref());
        } else {
            out.push(tokens[*idx].as_ref().to_string());
        }
    }
    out
}
