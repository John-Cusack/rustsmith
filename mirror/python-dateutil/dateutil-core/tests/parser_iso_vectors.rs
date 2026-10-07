// SPDX-License-Identifier: Apache-2.0
//! Differential vectors for the ISO core: expectations read off the
//! pure-Python 2.9.0.post0 `isoparser`.

use dateutil_core::parser::isoparser::{
    parse_date_only, parse_dt, parse_time_only, parse_tz_only, IsoFail, IsoTz,
};

#[test]
fn full_datetime() {
    let r = parse_dt(b"2024-01-15T10:30:00", None).unwrap();
    assert_eq!((r.date.y, r.date.mo, r.date.d), (2024, 1, 15));
    assert!(r.has_time);
    assert_eq!((r.time.h, r.time.mi, r.time.s, r.time.us), (10, 30, 0, 0));
    assert_eq!(r.tz, IsoTz::None);
    assert!(!r.midnight24);
}

#[test]
fn fraction_truncates_to_micros() {
    let r = parse_dt(b"2014-04-11T15:30:15.123456789", None).unwrap();
    assert_eq!((r.time.s, r.time.us), (15, 123456));
}

#[test]
fn week_dates() {
    let r = parse_dt(b"2024-W03-2", None).unwrap();
    assert_eq!((r.date.y, r.date.mo, r.date.d), (2024, 1, 16));
    let r = parse_dt(b"2024W032", None).unwrap();
    assert_eq!((r.date.y, r.date.mo, r.date.d), (2024, 1, 16));
}

#[test]
fn ordinal_dates() {
    let r = parse_dt(b"2024-060", None).unwrap();
    assert_eq!((r.date.y, r.date.mo, r.date.d), (2024, 2, 29));
    let r = parse_dt(b"2024060", None).unwrap();
    assert_eq!((r.date.y, r.date.mo, r.date.d), (2024, 2, 29));
}

#[test]
fn partial_dates_default_lowest() {
    let r = parse_dt(b"2024", None).unwrap();
    assert_eq!((r.date.y, r.date.mo, r.date.d), (2024, 1, 1));
    assert!(!r.has_time);
    let r = parse_dt(b"2024-01", None).unwrap();
    assert_eq!((r.date.y, r.date.mo, r.date.d), (2024, 1, 1));
}

#[test]
fn int_failures_carry_slices() {
    assert_eq!(parse_dt(b"abcd", None), Err(IsoFail::Int(b"abcd".to_vec())));
    assert_eq!(parse_dt(b"24:00", None), Err(IsoFail::Int(b"24:0".to_vec())));
}

#[test]
fn offsets_and_midnight() {
    let r = parse_dt(b"2024-01-15T10:30:00+05:30", None).unwrap();
    assert_eq!(r.tz, IsoTz::Offset(19800));
    let r = parse_dt(b"2024-01-15T24:00", None).unwrap();
    assert!(r.midnight24);
    assert_eq!((r.time.h, r.time.mi), (0, 0));
}

#[test]
fn ordinal_bounds() {
    assert_eq!(
        parse_dt(b"2019-367", None),
        Err(IsoFail::Msg(
            "Invalid ordinal day 367 for year 2019".to_string()
        ))
    );
    assert_eq!(parse_dt(b"9999-W52-7", None), Err(IsoFail::Overflow));
}

#[test]
fn public_entries() {
    let (d, pos) = parse_date_only(b"2024-01-15").unwrap();
    assert_eq!(pos, 10);
    assert_eq!((d.y, d.mo, d.d), (2024, 1, 15));
    let (t, tz) = parse_time_only(b"10:30:05.5").unwrap();
    assert_eq!((t.h, t.mi, t.s, t.us), (10, 30, 5, 500000));
    assert_eq!(tz, IsoTz::None);
    assert_eq!(parse_tz_only(b"+05:30", true).unwrap(), IsoTz::Offset(19800));
    assert_eq!(parse_tz_only(b"+00:00", true).unwrap(), IsoTz::Utc);
    assert_eq!(
        parse_tz_only(b"+00:00", false).unwrap(),
        IsoTz::Offset(0)
    );
    assert_eq!(parse_tz_only(b"Z", true).unwrap(), IsoTz::Utc);
    assert_eq!(
        parse_tz_only(b"123", true),
        Err(IsoFail::Msg(
            "Time zone offset requires sign".to_string()
        ))
    );
}

#[test]
fn sep_gating() {
    assert!(parse_dt(b"2024-01-15T10:30:00", Some(b'T')).is_ok());
    assert!(parse_dt(b"2024-01-15 10:30:00", Some(b'T')).is_err());
    assert!(parse_dt(b"2024-01-15 10:30:00", None).is_ok());
}

#[test]
fn max_ordinal_pins() {
    assert_eq!(dateutil_core::civil::to_ordinal(9999, 12, 31), 3652059);
}
