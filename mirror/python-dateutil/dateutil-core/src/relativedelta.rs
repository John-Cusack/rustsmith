// SPDX-License-Identifier: Apache-2.0
//! `relativedelta` calendar math on wall ordinals, mirroring
//! `dateutil.relativedelta` (`__add__` year/month/day staging and the
//! weekday jump; time-of-day arithmetic stays on Python `timedelta`).

use crate::civil::{days_in_month, from_ordinal, to_ordinal, weekday_of_ordinal};

/// Apply the year/month/day stage of `relativedelta.__add__`:
/// `year = (abs_year or other.year) + rel_years`, month likewise with the
/// single-carry rule, day clamped to the month length. Mirrors the
/// `__add__` construction exactly. `None` when the year leaves 1..9999
/// (the binding raises `ValueError` like `datetime.replace`).
pub fn apply_ymd(
    ord: i64,
    rel_years: i64,
    rel_months: i64,
    abs_year: Option<i64>,
    abs_month: Option<i64>,
    abs_day: Option<i64>,
) -> Option<i64> {
    let (oy, om, od) = from_ordinal(ord);
    let mut year = abs_year.unwrap_or(oy) + rel_years;
    let mut month = abs_month.unwrap_or(om as i64);
    if rel_months != 0 {
        month += rel_months;
        if month > 12 {
            year += 1;
            month -= 12;
        } else if month < 1 {
            year -= 1;
            month += 12;
        }
    }
    if !(1 <= year && year <= 9999) || !(1 <= month && month <= 12) {
        return None;
    }
    let day = days_in_month(year, month as u8).min(abs_day.unwrap_or(od as i64) as u8);
    Some(to_ordinal(year, month as u8, day))
}

/// Weekday jump of `relativedelta.__add__`: from `ord`, move to the
/// `nth` weekday `wday` (0=Monday). Mirrors the jump block exactly.
pub fn weekday_jump(ord: i64, wday: u8, nth: Option<i64>) -> i64 {
    let nth = nth.unwrap_or(1);
    let wd = weekday_of_ordinal(ord) as i64;
    let w = wday as i64;
    let mut jump = (nth.abs() - 1) * 7;
    if nth > 0 {
        jump += (7 - wd + w) % 7;
    } else {
        jump += (wd - w).rem_euclid(7);
        jump *= -1;
    }
    ord + jump
}

/// Month difference between two (year, month) pairs (dt1 - dt2 direction).
pub fn diff_months(y1: i64, m1: u8, y2: i64, m2: u8) -> i64 {
    (y1 - y2) * 12 + (m1 as i64 - m2 as i64)
}

/// `_set_months`: normalize a month count into (years, months) with the
/// `abs(months) > 11` carry rule.
pub fn set_months(months: i64) -> (i64, i64) {
    if months.abs() > 11 {
        let s = if months < 0 { -1 } else { 1 };
        let (div, modu) = crate::civil::py_divmod(months * s, 12);
        (div * s, modu * s)
    } else {
        (0, months)
    }
}

/// `_fix` cascade over the relative time fields (floats, Python `divmod`
/// semantics). Returns (days, hours, minutes, seconds, microseconds).
pub fn fix_time(
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    microseconds: f64,
) -> (f64, f64, f64, f64, f64) {
    use crate::civil::py_fdivmod;
    let sign = |x: f64| if x < 0.0 { -1.0 } else { 1.0 };
    let (mut days, mut hours, mut minutes, mut seconds, mut microseconds) =
        (days, hours, minutes, seconds, microseconds);
    if microseconds.abs() > 999999.0 {
        let s = sign(microseconds);
        let (div, modu) = py_fdivmod(microseconds * s, 1_000_000.0);
        microseconds = modu * s;
        seconds += div * s;
    }
    if seconds.abs() > 59.0 {
        let s = sign(seconds);
        let (div, modu) = py_fdivmod(seconds * s, 60.0);
        seconds = modu * s;
        minutes += div * s;
    }
    if minutes.abs() > 59.0 {
        let s = sign(minutes);
        let (div, modu) = py_fdivmod(minutes * s, 60.0);
        minutes = modu * s;
        hours += div * s;
    }
    if hours.abs() > 23.0 {
        let s = sign(hours);
        let (div, modu) = py_fdivmod(hours * s, 24.0);
        hours = modu * s;
        days += div * s;
    }
    (days, hours, minutes, seconds, microseconds)
}

/// Python `round(x, n)` (banker's rounding) for the `normalized` cascade:
/// away from an exact binary tie both rules agree, so only exact ties
/// need the even rule.
fn round_ties_even(x: f64, ndigits: i32) -> f64 {
    let factor = 10f64.powi(ndigits);
    let scaled = x * factor;
    let t = scaled.trunc();
    let frac = (scaled - t).abs();
    let r = if frac == 0.5 {
        if (t as i64) % 2 != 0 {
            if scaled > 0.0 {
                t + 1.0
            } else {
                t - 1.0
            }
        } else {
            t
        }
    } else {
        scaled.round()
    };
    r / factor
}

/// `normalized()` cascade. Mirrors the rounding of the fractional
/// remainders, returning integer parts.
pub fn normalized_parts(
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    microseconds: f64,
) -> (i64, i64, i64, i64, i64) {
    let days_i = days.trunc() as i64;
    let hours_f = round_ties_even(hours + 24.0 * (days - days_i as f64), 11);
    let hours_i = hours_f.trunc() as i64;
    let minutes_f = round_ties_even(minutes + 60.0 * (hours_f - hours_i as f64), 10);
    let minutes_i = minutes_f.trunc() as i64;
    let seconds_f = round_ties_even(seconds + 60.0 * (minutes_f - minutes_i as f64), 8);
    let seconds_i = seconds_f.trunc() as i64;
    let micros = (microseconds + 1e6 * (seconds_f - seconds_i as f64)).round() as i64;
    (days_i, hours_i, minutes_i, seconds_i, micros)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ymd_application_matches() {
        // 2020-01-31 + 1 month -> 2020-02-29 (clamp).
        assert_eq!(
            apply_ymd(to_ordinal(2020, 1, 31), 0, 1, None, None, None),
            Some(to_ordinal(2020, 2, 29))
        );
        // Absolute day + relative month.
        assert_eq!(
            apply_ymd(to_ordinal(2020, 5, 17), 0, 0, None, Some(2), Some(15)),
            Some(to_ordinal(2020, 2, 15))
        );
        // Year overflow -> None (binding raises ValueError).
        assert_eq!(apply_ymd(to_ordinal(9999, 6, 1), 1, 0, None, None, None), None);
    }

    #[test]
    fn weekday_jumps_match() {
        // 2020-01-01 (Wed) + FR(2) -> 2020-01-10.
        assert_eq!(
            weekday_jump(to_ordinal(2020, 1, 1), 4, Some(2)),
            to_ordinal(2020, 1, 10)
        );
        // Already Monday + MO(1) -> unchanged.
        assert_eq!(
            weekday_jump(to_ordinal(2020, 2, 3), 0, Some(1)),
            to_ordinal(2020, 2, 3)
        );
    }

    #[test]
    fn fix_and_normalized_match() {
        assert_eq!(set_months(25), (2, 1));
        assert_eq!(set_months(-25), (-2, -1));
        let (d, h, m, s, u) = fix_time(0.0, 25.0, 0.0, 0.0, 0.0);
        assert_eq!((d, h, m, s, u), (1.0, 1.0, 0.0, 0.0, 0.0));
        // weeks=1.25 -> days=8.75 stored; normalized -> 8d 18h.
        let (d, h, _, _, _) = normalized_parts(8.75, 0.0, 0.0, 0.0, 0.0);
        assert_eq!((d, h), (8, 18));
    }
}
