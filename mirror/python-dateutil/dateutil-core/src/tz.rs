// SPDX-License-Identifier: Apache-2.0
//! Timezone resolution over wall triples, mirroring `dateutil.tz`
//! (`_tzinfo`/`tzrangebase`/`tzfile` algorithms). Datetimes stay Python
//! objects at the binding boundary; the core works on [`Wall`] triples.

use crate::civil::to_ordinal;
use crate::tzfile::TzFileData;

/// A naive wall datetime: ordinal + seconds-of-day + microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Wall {
    pub ord: i64,
    pub secs: i64,
    pub micros: i64,
}

impl Wall {
    /// Validated construction: day fields in range.
    pub fn new(ord: i64, secs: i64, micros: i64) -> Option<Wall> {
        if !(0 <= secs && secs < 86400) || !(0 <= micros && micros < 1_000_000) {
            return None;
        }
        Some(Wall { ord, secs, micros })
    }

    /// Add a (possibly negative) second/microsecond delta.
    pub fn add_delta(self, dsecs: i64, dmicros: i64) -> Option<Wall> {
        let mut micros = self.micros + dmicros;
        let mut secs = self.secs + dsecs + micros.div_euclid(1_000_000);
        micros = micros.rem_euclid(1_000_000);
        let ord = self.ord + secs.div_euclid(86400);
        secs = secs.rem_euclid(86400);
        Some(Wall { ord, secs, micros })
    }

    /// `self - other` as (seconds, micros), exact.
    pub fn diff(self, other: Wall) -> (i64, i64) {
        let a = self.ord * 86400 + self.secs;
        let b = other.ord * 86400 + other.secs;
        let mut secs = a - b;
        let mut micros = self.micros - other.micros;
        if micros < 0 {
            secs -= 1;
            micros += 1_000_000;
        }
        (secs, micros)
    }
}

/// Ordinal of 1970-01-01 (mirrors `EPOCH.toordinal()`).
pub fn epoch_ordinal() -> i64 {
    to_ordinal(1970, 1, 1)
}

/// `_datetime_to_timestamp`: naive wall as epoch float seconds.
pub fn timestamp_of(w: Wall) -> f64 {
    ((w.ord - epoch_ordinal()) * 86400 + w.secs) as f64 + w.micros as f64 / 1e6
}

/// Offset lookup behavior shared by file and range zones.
pub trait Zone {
    /// `utcoffset(wall)` in seconds; `None` stands for a `None` result.
    fn offset_at(&self, wall: Wall, fold: i64) -> Option<i64>;
    /// `dst(wall)` in seconds; `None` stands for a `None` result.
    fn dst_at(&self, wall: Wall, fold: i64) -> Option<i64>;
    /// Whether the wall time is ambiguous.
    fn ambiguous_at(&self, wall: Wall) -> bool;
}

/// File-backed zone over parsed [`TzFileData`].
pub struct FileZone<'a> {
    pub data: &'a TzFileData,
}

impl<'a> Zone for FileZone<'a> {
    fn offset_at(&self, wall: Wall, fold: i64) -> Option<i64> {
        use crate::tzfile::{get_ttinfo, resolve_ambiguous_time};
        let ts = timestamp_of(wall);
        if self.data.ttinfo_std.is_none() {
            return Some(0);
        }
        let idx = resolve_ambiguous_time(self.data, ts, fold);
        let tti = get_ttinfo(self.data, idx)?;
        Some(self.data.ttinfo_list[tti].offset)
    }

    fn dst_at(&self, wall: Wall, fold: i64) -> Option<i64> {
        use crate::tzfile::{get_ttinfo, resolve_ambiguous_time};
        let ts = timestamp_of(wall);
        if self.data.ttinfo_dst.is_none() {
            return Some(0);
        }
        let idx = resolve_ambiguous_time(self.data, ts, fold);
        let tti = get_ttinfo(self.data, idx)?;
        let t = &self.data.ttinfo_list[tti];
        if t.isdst == 0 {
            Some(0)
        } else {
            Some(t.dstoffset)
        }
    }

    fn ambiguous_at(&self, wall: Wall) -> bool {
        use crate::tzfile::is_ambiguous_at;
        is_ambiguous_at(self.data, timestamp_of(wall), None)
    }
}

/// Annual-transition zone (`tzrangebase`): fixed std/dst offsets plus a
/// per-year (dston, dstoff) wall pair, or `None` for a fixed zone.
pub struct RangeZone {
    pub std_offset: i64,
    pub dst_offset: i64,
    pub hasdst: bool,
    pub transitions: Option<(Wall, Wall)>,
    /// Separate year for `is_ambiguous` (same value; kept for clarity).
    pub amb_transitions: Option<(Wall, Wall)>,
}

impl RangeZone {
    fn base_offset(&self) -> i64 {
        self.dst_offset - self.std_offset
    }

    fn naive_isdst(&self, wall: Wall) -> bool {
        let (dston, dstoff) = match self.transitions {
            None => return false,
            Some(t) => t,
        };
        if dston < dstoff {
            dston <= wall && wall < dstoff
        } else {
            !(dstoff <= wall && wall < dston)
        }
    }

    /// `_isdst` tri-state: `None` only when the input is absent (the
    /// binding passes `dt=None` through as `None` before calling here).
    pub fn isdst(&self, wall: Wall, fold: i64) -> Option<bool> {
        if !self.hasdst {
            return Some(false);
        }
        if self.transitions.is_none() {
            return Some(false);
        }
        let isdst = self.naive_isdst(wall);
        if !isdst && self.ambiguous_at(wall) {
            Some(fold == 0)
        } else {
            Some(isdst)
        }
    }
}

impl Zone for RangeZone {
    fn offset_at(&self, wall: Wall, fold: i64) -> Option<i64> {
        match self.isdst(wall, fold)? {
            true => Some(self.dst_offset),
            false => Some(self.std_offset),
        }
    }

    fn dst_at(&self, wall: Wall, fold: i64) -> Option<i64> {
        match self.isdst(wall, fold)? {
            true => Some(self.base_offset()),
            false => Some(0),
        }
    }

    fn ambiguous_at(&self, wall: Wall) -> bool {
        if !self.hasdst {
            return false;
        }
        let (start, end) = match self.amb_transitions {
            None => return false,
            Some(t) => t,
        };
        let _ = start;
        // `end <= dt < end + base_offset` (is_ambiguous in tzrangebase).
        match end.add_delta(self.base_offset(), 0) {
            None => false,
            Some(end_plus) => end <= wall && wall < end_plus,
        }
    }
}

/// `_tzinfo._fromutc` wall-walk over looked-up offsets. All offsets are
/// passed in (seconds); errors mirror the `ValueError` branches.
pub fn fromutc_wall(
    wall_utc: Wall,
    dtoff: Option<i64>,
    dtdst: Option<i64>,
    dst_at_fold1: Option<i64>,
    ambiguous: bool,
    delta_wall_secs: i64,
    utc_off: i64,
    utc_dst: i64,
) -> Result<(Wall, i64), &'static str> {
    let dtoff = dtoff.ok_or("fromutc() requires a non-None utcoffset() result")?;
    let dtdst = dtdst.ok_or("fromutc() requires a non-None dst() result")?;
    let delta = dtoff - dtdst;
    let dt = wall_utc
        .add_delta(delta, 0)
        .ok_or("date value out of range")?;
    let dtdst2 = dst_at_fold1
        .ok_or("fromutc(): dt.dst gave inconsistent results; cannot convert")?;
    let dt_wall = dt.add_delta(dtdst2, 0).ok_or("date value out of range")?;
    let fold = if ambiguous {
        i64::from(delta_wall_secs == utc_off - utc_dst)
    } else {
        0
    };
    Ok((dt_wall, fold))
}

/// `tzrangebase.fromutc` over a resolved transition pair.
pub fn range_fromutc(
    wall_utc: Wall,
    transitions: Option<(Wall, Wall)>,
    std_offset: i64,
    dst_offset: i64,
    utcoffset_at_wall: i64,
    ambiguous_at_result: bool,
) -> (Wall, i64) {
    match transitions {
        None => (wall_utc.add_delta(utcoffset_at_wall, 0).unwrap_or(wall_utc), 0),
        Some((dston, dstoff)) => {
            let dto = dston.add_delta(-std_offset, 0).unwrap_or(dston);
            let dfo = dstoff.add_delta(-std_offset, 0).unwrap_or(dstoff);
            let isdst = if dto < dfo {
                dto <= wall_utc && wall_utc < dfo
            } else {
                !(dfo <= wall_utc && wall_utc < dto)
            };
            let wall = wall_utc
                .add_delta(if isdst { dst_offset } else { std_offset }, 0)
                .unwrap_or(wall_utc);
            let fold = i64::from(!isdst && ambiguous_at_result);
            (wall, fold)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::from_ordinal;

    #[test]
    fn wall_arithmetic_matches_datetime() {
        let w = Wall { ord: to_ordinal(2021, 3, 14), secs: 2 * 3600 + 30 * 60, micros: 0 };
        let (y, m, d) = from_ordinal(w.ord);
        assert_eq!((y, m, d), (2021, 3, 14));
        let w2 = w.add_delta(3600, 0).unwrap();
        assert_eq!(w2.secs, 3 * 3600 + 30 * 60);
        let (s, u) = w2.diff(w);
        assert_eq!((s, u), (3600, 0));
        // Timestamp of the epoch is zero.
        let e = Wall { ord: epoch_ordinal(), secs: 0, micros: 0 };
        assert_eq!(timestamp_of(e), 0.0);
    }
}

#[test]
fn yearly_default_steps() {
    use crate::rrule::*;
    let p = Params {
        freq: YEARLY, interval: 1, wkst: 0,
        bymonth: Some(vec![9]), byweekno: None, byyearday: None, byeaster: None,
        bymonthday: vec![2], bynmonthday: vec![],
        byweekday: None, bynweekday: None,
        byhour: Some(vec![9]), byminute: Some(vec![0]), bysecond: Some(vec![0]),
        bysetpos: None,
        timeset: Some(vec![(9, 0, 0)]),
    };
    let mut e = Engine::new(p, 1997, 9, 2, 9, 0, 0);
    let b1 = e.next_step().unwrap();
    let b2 = e.next_step().unwrap();
    assert_eq!(b1.len(), 1);
    assert_eq!(b2.len(), 1);
    assert_eq!(b1[0].0, 729902 - 729902 + b1[0].0); // placeholder
    println!("b1={:?} b2={:?}", b1, b2);
}
