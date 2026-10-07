// SPDX-License-Identifier: Apache-2.0
//! Default `parserinfo` tables (static English tables).
//!
//! Each probe lowercases internally, exactly like the `name.lower()`
//! calls in the original methods; the dispatch loop therefore passes raw
//! tokens.

use super::lex::lower_token;
use super::{convertyear_str, days_in_month_str, Fail, Info, TzOff};

/// Default `parserinfo` tables.
pub struct DefaultInfo;

impl Info for DefaultInfo {
    fn jump(&self, tok: &str) -> bool {
        matches!(
            lower_token(tok).as_str(),
            " " | "."
                | ","
                | ";"
                | "-"
                | "/"
                | "'"
                | "at"
                | "on"
                | "and"
                | "ad"
                | "m"
                | "t"
                | "of"
                | "st"
                | "nd"
                | "rd"
                | "th"
        )
    }

    fn weekday(&self, tok: &str) -> Option<i64> {
        Some(match lower_token(tok).as_str() {
            "mon" | "monday" => 0,
            "tue" | "tuesday" => 1,
            "wed" | "wednesday" => 2,
            "thu" | "thursday" => 3,
            "fri" | "friday" => 4,
            "sat" | "saturday" => 5,
            "sun" | "sunday" => 6,
            _ => return None,
        })
    }

    fn month(&self, tok: &str) -> Option<i64> {
        Some(match lower_token(tok).as_str() {
            "jan" | "january" => 1,
            "feb" | "february" => 2,
            "mar" | "march" => 3,
            "apr" | "april" => 4,
            "may" => 5,
            "jun" | "june" => 6,
            "jul" | "july" => 7,
            "aug" | "august" => 8,
            "sep" | "sept" | "september" => 9,
            "oct" | "october" => 10,
            "nov" | "november" => 11,
            "dec" | "december" => 12,
            _ => return None,
        })
    }

    fn hms(&self, tok: &str) -> Option<i64> {
        Some(match lower_token(tok).as_str() {
            "h" | "hour" | "hours" => 0,
            "m" | "minute" | "minutes" => 1,
            "s" | "second" | "seconds" => 2,
            _ => return None,
        })
    }

    fn ampm(&self, tok: &str) -> Option<i64> {
        Some(match lower_token(tok).as_str() {
            "am" | "a" => 0,
            "pm" | "p" => 1,
            _ => return None,
        })
    }

    fn pertain(&self, tok: &str) -> bool {
        lower_token(tok).as_str() == "of"
    }

    fn utczone(&self, tok: &str) -> bool {
        matches!(lower_token(tok).as_str(), "utc" | "gmt" | "z")
    }

    fn tzoffset(&self, name: &str) -> Option<TzOff> {
        // Mirrors `tzoffset()` literally: the `in self._utczone` test uses
        // the ORIGINAL case, so only lowercase hits return 0 here; the
        // default `TZOFFSET` is empty.
        match name {
            "utc" | "gmt" | "z" => Some(TzOff::Secs(0)),
            _ => None,
        }
    }

    fn is_utc_abbr(&self, token: &str) -> bool {
        matches!(token, "UTC" | "GMT" | "Z" | "z")
    }

    fn convertyear(
        &self,
        year: &str,
        century_specified: bool,
        cur_year: i64,
        century: i64,
    ) -> String {
        convertyear_str(year, century_specified, cur_year, century)
    }

    fn days_in_month(&self, year: &str, month: &str) -> Result<u8, Fail> {
        let m: i64 = month.parse().map_err(|_| Fail::NoResult)?;
        if !(1..=12).contains(&m) {
            return Err(Fail::NoResult);
        }
        Ok(days_in_month_str(year, m))
    }
}
