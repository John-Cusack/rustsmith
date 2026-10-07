// SPDX-License-Identifier: Apache-2.0
// dateutil-core: pure-Rust dateutil engine (no Python dependency).
//
// Calendar math mirrors CPython's proleptic Gregorian calendar;
// recurrence/arithmetic/tz/parse algorithms mirror dateutil 2.9.0.post0.

pub mod civil;
pub mod easter;
pub mod ical;
pub mod parser;
pub mod relativedelta;
pub mod rrule;
pub mod tz;
pub mod tzfile;
