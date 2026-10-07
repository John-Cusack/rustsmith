// SPDX-License-Identifier: Apache-2.0
//! Differential vectors for the parser core: every expectation was read off
//! the pure-Python 2.9.0.post0 implementation (`_timelex.split` and
//! `parser()._parse`), so these fail if the port drifts.

use dateutil_core::parser::{
    convertyear_str, fold_digits, parse::parse_tokens, recombine_skipped, validate_default, Dec,
    DefaultInfo, Fail, Info, ParseOptions, Timelex, Ymd,
};

fn opts(fuzzy: bool) -> ParseOptions {
    ParseOptions {
        dayfirst: false,
        yearfirst: false,
        fuzzy,
        cur_year: 2024,
        century: 2000,
    }
}

fn parse(s: &str, fuzzy: bool) -> Result<dateutil_core::parser::ParseOk, Fail> {
    let info = DefaultInfo;
    let mut toks = Timelex::split(s);
    parse_tokens(&mut toks, &info, &opts(fuzzy))
}

#[test]
fn lexer_splits_report_line() {
    assert_eq!(
        Timelex::split("On 2024-01-15 at 10:30:00 the server started"),
        vec![
            "On", " ", "2024", "-", "01", "-", "15", " ", "at", " ", "10", ":", "30", ":",
            "00", " ", "the", " ", "server", " ", "started"
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>()
    );
}

#[test]
fn lexer_dot_and_comma_runs() {
    assert_eq!(Timelex::split("Sep.20.2009"), vec!["Sep", ".", "20", ".", "2009"]);
    assert_eq!(Timelex::split("12,5"), vec!["12.5"]);
    assert_eq!(Timelex::split("5.6h"), vec!["5.6", "h"]);
}

#[test]
fn lexer_skips_nuls_and_keeps_spaces() {
    assert_eq!(Timelex::split("a\x00b"), vec!["ab"]);
    assert_eq!(
        Timelex::split(" On  2024 "),
        vec![" ", "On", " ", " ", "2024", " "]
    );
}

#[test]
fn lexer_stateful_iteration_matches_split() {
    let s = "Thu Sep 25 10:36:28 BRST 2003";
    let mut lx = Timelex::new(s);
    let mut toks = Vec::new();
    while let Some(t) = lx.get_token() {
        toks.push(t);
    }
    assert_eq!(toks, Timelex::split(s));
    assert!(lx.is_eof());
    assert!(lx.get_token().is_none());
}

#[test]
fn strict_iso_like() {
    let r = parse("2024-01-15T10:30:00", false).unwrap();
    assert_eq!(r.year.as_deref(), Some("2024"));
    assert_eq!(r.month.as_deref(), Some("1"));
    assert_eq!(r.day.as_deref(), Some("15"));
    assert_eq!(r.hour.as_deref(), Some("10"));
    assert_eq!(r.minute.as_deref(), Some("30"));
    assert_eq!(r.second.as_deref(), Some("0"));
    assert_eq!(r.micro, Some(0));
    assert!(r.tzname.is_none() && r.tzoffset.is_none());
    assert!(r.century_specified);
}

#[test]
fn fractional_hour_gh427() {
    let r = parse("5.6h", false).unwrap();
    assert_eq!(r.hour.as_deref(), Some("5"));
    assert_eq!(r.minute.as_deref(), Some("36"));
}

#[test]
fn named_zone_without_offset_stays_unresolved() {
    // `info.tzoffset("BRST")` is None (default TZOFFSET empty); the binding
    // resolves it against `tzinfos`.
    let r = parse("Thu Sep 25 10:36:28 BRST 2003", false).unwrap();
    assert_eq!(r.year.as_deref(), Some("2003"));
    assert_eq!(r.weekday, Some(3));
    assert_eq!(r.tzname.as_deref(), Some("BRST"));
    assert!(r.tzoffset.is_none());
}

#[test]
fn fuzzy_absorbs_words_and_keeps_skips() {
    let r = parse("On 2024-01-15 at 10:30:00 the server started", true).unwrap();
    assert_eq!(r.year.as_deref(), Some("2024"));
    assert_eq!(r.hour.as_deref(), Some("10"));
    assert!(!r.skipped.is_empty());
    assert!(r.skipped.iter().any(|t| t.contains("server")));
}

#[test]
fn strict_rejects_noise() {
    assert_eq!(
        parse("On 2024-01-15 at 10:30:00 the server started", false),
        Err(Fail::NoResult)
    );
}

#[test]
fn inf_nan_fail_even_fuzzy() {
    // The `float()` probe accepts them, `_to_decimal` rejects them.
    assert_eq!(parse("inf", true), Err(Fail::NoResult));
    assert_eq!(parse("Nan", true), Err(Fail::NoResult));
    assert_eq!(parse("1: test", true), Err(Fail::NoResult));
}

#[test]
fn empty_means_no_date() {
    let r = parse("", true).unwrap();
    assert!(r.is_empty());
    let r = parse("foo", true).unwrap();
    assert!(r.is_empty());
}

#[test]
fn stray_digits_fold_into_ymd() {
    // Fuzzy tolerates stray numbers by folding them into y/m/d.
    let r = parse("retry 3", true).unwrap();
    assert_eq!(r.day.as_deref(), Some("3"));
}

#[test]
fn ymd_disambiguation() {
    let mut y = Ymd::new();
    y.append_str("01", None).unwrap();
    y.append_str("05", None).unwrap();
    y.append_str("2024", None).unwrap();
    assert!(y.century_specified);
    let (yy, mm, dd) = y.resolve_ymd(false, false).unwrap();
    assert_eq!((yy.as_deref(), mm.as_deref(), dd.as_deref()), (Some("2024"), Some("1"), Some("5")));
    let (yy, mm, dd) = y.resolve_ymd(false, true).unwrap();
    assert_eq!((yy.as_deref(), mm.as_deref(), dd.as_deref()), (Some("2024"), Some("5"), Some("1")));
}

#[test]
fn convertyear_window() {
    assert_eq!(convertyear_str("07", false, 2024, 2000), "2007");
    assert_eq!(convertyear_str("68", false, 2024, 2000), "2068");
    assert_eq!(convertyear_str("80", false, 2024, 2000), "1980");
    assert_eq!(convertyear_str("2024", false, 2024, 2000), "2024");
    assert_eq!(convertyear_str("07", true, 2024, 2000), "07");
}

#[test]
fn validate_utc_fixups() {
    let mut r = parse("2024-01-15 10:30:00 Z", false).unwrap();
    assert_eq!(r.tzname.as_deref(), Some("Z"));
    validate_default(&mut r, 2024, 2000);
    assert_eq!(r.tzname.as_deref(), Some("UTC"));
    assert!(r.tzoffset.as_ref().unwrap().is_zero());
}

#[test]
fn validate_gmt_name_normalizes_offset() {
    // `GMT` with no numeric offset: `tzoffset` stays `None` out of
    // `_parse` and becomes 0 in `validate` (`None != 0` in the guard).
    let mut r = parse("2024-01-15 10:30:00 GMT", false).unwrap();
    assert_eq!(r.tzname.as_deref(), Some("GMT"));
    assert!(r.tzoffset.is_none());
    validate_default(&mut r, 2024, 2000);
    assert!(r.tzoffset.as_ref().unwrap().is_zero());
}

#[test]
fn recombine_matches_doc_example() {
    let toks: Vec<String> = ["foo", " ", "bar", " ", "19June2000", "baz"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(
        recombine_skipped(&toks, &[0, 1, 2, 5]),
        vec!["foo bar".to_string(), "baz".to_string()]
    );
}

#[test]
fn dec_frac60_gh427() {
    let d = Dec::parse("5.6").unwrap();
    assert_eq!(d.frac60(), 36);
    assert!(d.frac_nonzero);
    let d = Dec::parse("5.0").unwrap();
    assert!(!d.frac_nonzero);
}

#[test]
fn default_tables_spot_check() {
    let info = DefaultInfo;
    assert_eq!(info.weekday("sunday"), Some(6));
    assert_eq!(info.month("sept"), Some(9));
    assert_eq!(info.hms("minutes"), Some(1));
    assert_eq!(info.ampm("p"), Some(1));
    assert!(info.jump("st"));
    assert!(info.utczone("GMT"));
    assert!(!info.utczone("BRST"));
    assert!(info.tzoffset("BRST").is_none());
}

#[test]
fn resolve_month_name_forms() {
    // "Jan 01": mstridx=0 wraparound (self[-1]).
    let mut y = Ymd::new();
    y.append_small(1, 'M').unwrap();
    y.append_str("01", None).unwrap();
    assert_eq!(
        y.resolve_ymd(false, false).unwrap(),
        (None, Some("1".to_string()), Some("1".to_string()))
    );
    // "01-99-Jan": mstridx=2, day/year/month.
    let mut y = Ymd::new();
    y.append_str("01", None).unwrap();
    y.append_str("99", None).unwrap();
    y.append_small(1, 'M').unwrap();
    assert_eq!(
        y.resolve_ymd(false, false).unwrap(),
        (
            Some("99".to_string()),
            Some("1".to_string()),
            Some("1".to_string())
        )
    );
    // "99-01-Jan": year/day/month.
    let mut y = Ymd::new();
    y.append_str("99", None).unwrap();
    y.append_str("01", None).unwrap();
    y.append_small(1, 'M').unwrap();
    assert_eq!(
        y.resolve_ymd(false, false).unwrap(),
        (
            Some("99".to_string()),
            Some("1".to_string()),
            Some("1".to_string())
        )
    );
    // "090107": yearfirst/dayfirst variants.
    let mut y = Ymd::new();
    y.append_str("09", None).unwrap();
    y.append_str("01", None).unwrap();
    y.append_str("07", None).unwrap();
    assert!(!y.century_specified);
    assert_eq!(
        y.resolve_ymd(false, false).unwrap(),
        (
            Some("7".to_string()),
            Some("9".to_string()),
            Some("1".to_string())
        )
    );
    assert_eq!(
        y.resolve_ymd(false, true).unwrap(),
        (
            Some("7".to_string()),
            Some("1".to_string()),
            Some("9".to_string())
        )
    );
    assert_eq!(
        y.resolve_ymd(true, false).unwrap(),
        (
            Some("9".to_string()),
            Some("1".to_string()),
            Some("7".to_string())
        )
    );
}

#[test]
fn fold_unicode_digits() {
    // `int()`/`float()` accept exactly `Nd`; `to_digit` is ASCII-only.
    assert_eq!(fold_digits("2024"), Some("2024".to_string()));
    assert_eq!(fold_digits("٢٣"), Some("23".to_string()));
    assert_eq!(fold_digits("２０２４"), Some("2024".to_string()));
    assert_eq!(fold_digits("²"), None);
    assert_eq!(fold_digits("Ⅻ"), None);
    // ASCII passes through untouched (callers validate shape).
    assert_eq!(fold_digits("12a"), Some("12a".to_string()));
}
