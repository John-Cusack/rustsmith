// SPDX-License-Identifier: Apache-2.0
//! Proleptic Gregorian calendar math mirroring CPython (`datetime`).
//!
//! Days are counted as an ordinal: day 1 == 0001-01-01 (Monday), matching
//! `date.toordinal()`. Times are seconds-of-day plus microseconds.

/// True for a proleptic Gregorian leap year.
pub fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in `month` (1..=12) of `year`.
pub fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Days before January 1st of `year` (year >= 1): ordinal of Dec 31 of year-1.
fn days_before_year(year: i64) -> i64 {
    let y = year - 1;
    y * 365 + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)
}

/// Ordinal of (year, month, day); day 1 == 0001-01-01. No range checks.
pub fn to_ordinal(year: i64, month: u8, day: u8) -> i64 {
    let mut ord = days_before_year(year) + 1;
    for m in 1..month {
        ord += days_in_month(year, m) as i64;
    }
    ord + day as i64 - 1
}

/// Inverse of [`to_ordinal`]: ordinal (1-based) to (year, month, day).
pub fn from_ordinal(mut ord: i64) -> (i64, u8, u8) {
    // 400-year cycles, then 100, 4, 1 — same shape as CPython's _ord2ymd.
    let mut year: i64 = 1;
    let n400 = (ord - 1).div_euclid(146097);
    year += n400 * 400;
    ord -= n400 * 146097;
    let n100 = (ord - 1).div_euclid(36524).min(3);
    year += n100 * 100;
    ord -= n100 * 36524;
    let n4 = (ord - 1).div_euclid(1461);
    year += n4 * 4;
    ord -= n4 * 1461;
    let n1 = (ord - 1).div_euclid(365).min(3);
    year += n1;
    ord -= n1 * 365;
    let mut month: u8 = 1;
    loop {
        let dim = days_in_month(year, month) as i64;
        if ord <= dim {
            break;
        }
        ord -= dim;
        month += 1;
    }
    (year, month, ord as u8)
}

/// Monday == 0 .. Sunday == 6 for an ordinal (0001-01-01 was a Monday).
pub fn weekday_of_ordinal(ord: i64) -> u8 {
    (ord - 1).rem_euclid(7) as u8
}

/// Python-style `divmod` for integers (floor division, not truncation).
pub fn py_divmod(a: i64, b: i64) -> (i64, i64) {
    (a.div_euclid(b), a.rem_euclid(b))
}

/// Python `divmod` for f64: `div = floor(a/b)`, `mod = a - div*b`.
pub fn py_fdivmod(a: f64, b: f64) -> (f64, f64) {
    let div = (a / b).floor();
    (div, a - div * b)
}

/// Truncation toward zero, like Python `int(x)` for finite floats.
pub fn py_int(x: f64) -> i64 {
    x.trunc() as i64
}

/// Python `+g`-style float formatting used by `relativedelta.__repr__`
/// (`"{value:+g}"`): 6 significant digits, stripped trailing zeros,
/// exponent form when the decimal exponent is < -4 or >= 6.
pub fn py_g_format(value: f64, sign: bool) -> String {
    if value == 0.0 {
        return if sign { "+0".to_string() } else { "0".to_string() };
    }
    let neg = value.is_sign_negative();
    let v = value.abs();
    let mut exp = v.log10().floor() as i32;
    // Round to 6 significant digits.
    let mut digits = (v / 10f64.powi(exp - 5)).round() as i64;
    if digits >= 1_000_000 {
        digits /= 10;
        exp += 1;
    }
    let body = if exp < -4 || exp >= 6 {
        // Scientific form: d.xxxxxxe±XX with stripped zeros.
        let mut s = format!("{:06}", digits);
        while s.ends_with('0') && s.len() > 1 {
            s.pop();
        }
        let mantissa = if s.len() > 1 {
            format!("{}.{}", &s[..1], &s[1..])
        } else {
            s
        };
        let esign = if exp < 0 { "-" } else { "+" };
        format!("{}e{}{:02}", mantissa, esign, exp.abs())
    } else {
        // Fixed form: value = digits * 10^(exp-5), digits > 0.
        let shift = exp - 5;
        if shift >= 0 {
            format!("{}", digits * 10i64.pow(shift as u32))
        } else {
            let decimals = (-shift) as u32;
            let pow10 = 10i64.pow(decimals);
            let intpart = digits / pow10;
            let fracpart = digits % pow10;
            if fracpart == 0 {
                format!("{}", intpart)
            } else {
                let mut s = format!(
                    "{}.{:0>width$}",
                    intpart,
                    fracpart,
                    width = decimals as usize
                );
                while s.ends_with('0') {
                    s.pop();
                }
                s
            }
        }
    };
    if neg {
        format!("-{}", body)
    } else if sign {
        format!("+{}", body)
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinal_roundtrip_matches_cpython() {
        // Spot values from CPython: date(1,1,1).toordinal()==1,
        // date(2020,2,29).toordinal()==737484, date(9999,12,31)==3652059.
        assert_eq!(to_ordinal(1, 1, 1), 1);
        assert_eq!(to_ordinal(2020, 2, 29), 737484);
        assert_eq!(to_ordinal(9999, 12, 31), 3652059);
        for (y, m, d) in [(1, 1, 1), (2020, 2, 29), (1997, 9, 2), (9999, 12, 31), (1900, 1, 1)] {
            let o = to_ordinal(y, m, d);
            assert_eq!(from_ordinal(o), (y, m, d));
        }
        assert_eq!(weekday_of_ordinal(to_ordinal(1997, 9, 2)), 1);
    }

    #[test]
    fn g_format_matches_python() {
        assert_eq!(py_g_format(3.0, true), "+3");
        assert_eq!(py_g_format(-2.0, true), "-2");
        assert_eq!(py_g_format(1.25, true), "+1.25");
        assert_eq!(py_g_format(0.5, true), "+0.5");
        assert_eq!(py_g_format(9.22, true), "+9.22");
        assert_eq!(py_g_format(157221.93, true), "+157222");
        assert_eq!(py_g_format(0.0000123456789, true), "+1.23457e-05");
    }
}
