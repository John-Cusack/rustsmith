// SPDX-License-Identifier: Apache-2.0
//! ISO-8601 parser: pure-Rust port of `dateutil/parser/isoparser.py`.
//!
//! Byte-level literal port (the original works on ASCII bytes after
//! `_takes_ascii`; the binding performs that normalization). Datetimes
//! never materialize here: the binding constructs them so CPython range
//! messages stay exact. `int()` failures return the offending slice so
//! the binding can raise with CPython's own message.

use crate::civil;

pub const MAX_ORDINAL: i64 = 3652059; // date(9999, 12, 31).toordinal()

/// `int()` on a byte slice with Python semantics (ASCII whitespace trim,
/// one optional sign, `0-9(_?0-9)*`); the slice is returned on failure so
/// the binding raises with the exact CPython message.
pub fn pint(s: &[u8]) -> Result<i32, Vec<u8>> {
    let mut t = s;
    while let Some((&b, rest)) = t.split_first() {
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == 0x0b || b == 0x0c {
            t = rest;
        } else {
            break;
        }
    }
    while let Some((&b, _)) = t.split_last() {
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == 0x0b || b == 0x0c {
            t = &t[..t.len() - 1];
        } else {
            break;
        }
    }
    if let Some((&b, rest)) = t.split_first() {
        if b == b'+' || b == b'-' {
            t = rest;
        }
    }
    if t.is_empty() {
        return Err(s.to_vec());
    }
    let mut val: i32 = 0;
    let mut prev_us = false;
    let mut first = true;
    for &b in t {
        if b.is_ascii_digit() {
            val = val.saturating_mul(10).saturating_add((b - b'0') as i32);
            prev_us = false;
            first = false;
        } else if b == b'_' && !first && !prev_us {
            prev_us = true;
        } else {
            return Err(s.to_vec());
        }
    }
    if prev_us {
        return Err(s.to_vec());
    }
    let _ = first;
    let negative = {
        let mut u = s;
        while let Some((&b, rest)) = u.split_first() {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == 0x0b || b == 0x0c {
                u = rest;
            } else {
                break;
            }
        }
        u.first() == Some(&b'-')
    };
    Ok(if negative { -val } else { val })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IsoTz {
    None,
    Utc,
    Offset(i32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IsoCal {
    pub y: i32,
    pub mo: u8,
    pub d: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IsoTime {
    pub h: u8,
    pub mi: u8,
    pub s: u8,
    pub us: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IsoDateTime {
    pub date: IsoCal,
    pub time: IsoTime,
    pub has_time: bool,
    pub tz: IsoTz,
    /// `hh == 24` with zero rest: the caller emits midnight + 1 day.
    pub midnight24: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IsoFail {
    /// `int()` failed; carries the slice for the exact CPython message.
    Int(Vec<u8>),
    Msg(String),
    /// `date value out of range` (week-date ordinal overflow).
    Overflow,
    /// `year {y} is out of range` (core-side date math guard).
    Year(i32),
}

fn msg(s: &str) -> IsoFail {
    IsoFail::Msg(s.to_string())
}

fn common(b: &[u8]) -> Result<(IsoCal, usize), IsoFail> {
    let len = b.len();
    if len < 4 {
        return Err(msg("ISO string too short"));
    }
    let year = pint(&b[0..4]).map_err(IsoFail::Int)?;
    let mut pos = 4;
    if pos >= len {
        return Ok((IsoCal { y: year, mo: 1, d: 1 }, pos));
    }
    let has_sep = b[pos] == b'-';
    if has_sep {
        pos += 1;
    }
    if len - pos < 2 {
        return Err(msg("Invalid common month"));
    }
    let month = pint(&b[pos..pos + 2]).map_err(IsoFail::Int)?;
    pos += 2;
    if pos >= len {
        if has_sep {
            return Ok((
                IsoCal {
                    y: year,
                    mo: month as u8,
                    d: 1,
                },
                pos,
            ));
        }
        return Err(msg("Invalid ISO format"));
    }
    if has_sep {
        if b[pos] != b'-' {
            return Err(msg("Invalid separator in ISO string"));
        }
        pos += 1;
    }
    if len - pos < 2 {
        return Err(msg("Invalid common day"));
    }
    let day = pint(&b[pos..pos + 2]).map_err(IsoFail::Int)?;
    Ok((
        IsoCal {
            y: year,
            mo: month as u8,
            d: day as u8,
        },
        pos + 2,
    ))
}

fn weekdate(year: i32, week: i32, day: i32) -> Result<IsoCal, IsoFail> {
    if !(0 < week && week < 54) {
        return Err(msg(&format!("Invalid week: {}", week)));
    }
    if !(0 < day && day < 8) {
        return Err(msg(&format!("Invalid weekday: {}", day)));
    }
    if !(1..=9999).contains(&year) {
        return Err(IsoFail::Year(year));
    }
    let jan4 = civil::to_ordinal(year as i64, 1, 4);
    let iso_wd = civil::weekday_of_ordinal(jan4) as i64 + 1;
    let week1 = jan4 - (iso_wd - 1);
    let out = week1 + ((week as i64 - 1) * 7) + (day as i64 - 1);
    if !(1..=MAX_ORDINAL).contains(&out) {
        return Err(IsoFail::Overflow);
    }
    let (y, m, d) = civil::from_ordinal(out);
    Ok(IsoCal {
        y: y as i32,
        mo: m,
        d,
    })
}

/// Saturating sub-slice (Python slices never panic).
fn sl(b: &[u8], from: usize, to: usize) -> &[u8] {
    let len = b.len();
    &b[from.min(len)..to.min(len)]
}

fn uncommon(b: &[u8]) -> Result<(IsoCal, usize), IsoFail> {
    if b.len() < 4 {
        return Err(msg("ISO string too short"));
    }
    let year = pint(&b[0..4]).map_err(IsoFail::Int)?;
    let has_sep = b.get(4) == Some(&b'-');
    let mut pos = 4 + has_sep as usize;
    if b.get(pos) == Some(&b'W') {
        pos += 1;
        let week = pint(sl(b, pos, pos + 2)).map_err(IsoFail::Int)?;
        pos += 2;
        let mut dayno = 1;
        if b.len() > pos {
            if (b[pos] == b'-') != has_sep {
                return Err(msg("Inconsistent use of dash separator"));
            }
            pos += has_sep as usize;
            dayno = pint(sl(b, pos, pos + 1)).map_err(IsoFail::Int)?;
            pos += 1;
        }
        Ok((weekdate(year, week, dayno)?, pos))
    } else {
        if b.len() - pos < 3 {
            return Err(msg("Invalid ordinal day"));
        }
        let ordinal = pint(&b[pos..pos + 3]).map_err(IsoFail::Int)?;
        pos += 3;
        if !(1..=9999).contains(&year) {
            return Err(IsoFail::Year(year));
        }
        let max = 365 + civil::is_leap(year as i64) as i32;
        if ordinal < 1 || ordinal > max {
            return Err(msg(&format!(
                "Invalid ordinal day {} for year {}",
                ordinal, year
            )));
        }
        let out = civil::to_ordinal(year as i64, 1, 1) + ordinal as i64 - 1;
        let (y, m, d) = civil::from_ordinal(out);
        Ok((IsoCal { y: y as i32, mo: m, d }, pos))
    }
}

fn parse_date(b: &[u8]) -> Result<(IsoCal, usize), IsoFail> {
    // Any `ValueError` from the common form falls through to uncommon.
    match common(b) {
        Ok(v) => Ok(v),
        Err(_) => uncommon(b),
    }
}

fn parse_tz(b: &[u8], zero_as_utc: bool) -> Result<IsoTz, IsoFail> {
    if b == b"Z" || b == b"z" {
        return Ok(IsoTz::Utc);
    }
    if ![3, 5, 6].contains(&b.len()) {
        return Err(msg("Time zone offset must be 1, 3, 5 or 6 characters"));
    }
    let mult = match b[0] {
        b'-' => -1,
        b'+' => 1,
        _ => return Err(msg("Time zone offset requires sign")),
    };
    let hours = pint(&b[1..3]).map_err(IsoFail::Int)?;
    let minutes = if b.len() == 3 {
        0
    } else {
        pint(&b[if b[3] == b':' { 4 } else { 3 }..]).map_err(IsoFail::Int)?
    };
    if zero_as_utc && hours == 0 && minutes == 0 {
        return Ok(IsoTz::Utc);
    }
    if minutes > 59 {
        return Err(msg("Invalid minutes in time zone offset"));
    }
    if hours > 23 {
        return Err(msg("Invalid hours in time zone offset"));
    }
    Ok(IsoTz::Offset(mult * (hours * 60 + minutes) * 60))
}

fn parse_time(b: &[u8]) -> Result<(IsoTime, IsoTz), IsoFail> {
    let len = b.len();
    if len < 2 {
        return Err(msg("ISO time too short"));
    }
    let mut comp: i32 = -1;
    let mut pos = 0;
    let mut h = 0;
    let mut mi = 0;
    let mut s = 0;
    let mut us = 0;
    let mut tz = IsoTz::None;
    let mut has_sep = false;
    while pos < len && comp < 5 {
        comp += 1;
        if b[pos] == b'-' || b[pos] == b'+' || b[pos] == b'Z' || b[pos] == b'z' {
            tz = parse_tz(&b[pos..], true)?;
            pos = len;
            break;
        }
        if comp == 1 && b[pos] == b':' {
            has_sep = true;
            pos += 1;
        } else if comp == 2 && has_sep {
            if b[pos] != b':' {
                return Err(msg("Inconsistent use of colon separator"));
            }
            pos += 1;
        }
        if comp < 3 {
            let v = pint(&b[pos..(pos + 2).min(len)]).map_err(IsoFail::Int)?;
            match comp {
                0 => h = v,
                1 => mi = v,
                _ => s = v,
            }
            pos += 2;
        }
        if comp == 3 {
            let rest = &b[pos..];
            let mut di = 1;
            if rest.is_empty() || (rest[0] != b'.' && rest[0] != b',') {
                continue;
            }
            while di < rest.len() && rest[di].is_ascii_digit() {
                di += 1;
            }
            if di < 2 {
                continue;
            }
            let digits = &rest[1..di];
            let take = digits.len().min(6);
            let mut uv: u32 = 0;
            for &c in &digits[..take] {
                uv = uv * 10 + (c - b'0') as u32;
            }
            for _ in take..6 {
                uv *= 10;
            }
            us = uv;
            pos += di;
        }
    }
    if pos < len {
        return Err(msg("Unused components in ISO string"));
    }
    if h == 24 && (mi != 0 || s != 0 || us != 0) {
        return Err(msg("Hour may only be 24 at 24:00.000"));
    }
    Ok((
        IsoTime {
            h: h as u8,
            mi: mi as u8,
            s: s as u8,
            us,
        },
        tz,
    ))
}

/// Full `isoparse` (separator: `None` accepts any single character).
pub fn parse_dt(b: &[u8], sep: Option<u8>) -> Result<IsoDateTime, IsoFail> {
    let (date, pos) = parse_date(b)?;
    let mut out = IsoDateTime {
        date,
        time: IsoTime { h: 0, mi: 0, s: 0, us: 0 },
        has_time: false,
        tz: IsoTz::None,
        midnight24: false,
    };
    if b.len() > pos {
        match sep {
            None => {}
            Some(want) => {
                if b[pos] != want {
                    return Err(msg("String contains unknown ISO components"));
                }
            }
        }
        let (t, tz) = parse_time(&b[pos + 1..])?;
        out.time = t;
        out.has_time = true;
        out.tz = tz;
    }
    if out.has_time && out.time.h == 24 {
        out.time.h = 0;
        out.midnight24 = true;
    }
    Ok(out)
}

/// `parse_isodate` entry (position included: trailing bytes are an error).
pub fn parse_date_only(b: &[u8]) -> Result<(IsoCal, usize), IsoFail> {
    parse_date(b)
}

/// `parse_isotime` entry.
pub fn parse_time_only(b: &[u8]) -> Result<(IsoTime, IsoTz), IsoFail> {
    parse_time(b)
}

/// `parse_tzstr` entry.
pub fn parse_tz_only(b: &[u8], zero_as_utc: bool) -> Result<IsoTz, IsoFail> {
    parse_tz(b, zero_as_utc)
}