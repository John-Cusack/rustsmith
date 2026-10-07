// SPDX-License-Identifier: Apache-2.0
//! `rrule` recurrence engine on wall ordinals, mirroring `dateutil.rrule`
//! (`rrule._iter` + `_iterinfo`, `__mod_distance`).
//! Candidate filtering against `until`/`dtstart` and `tzinfo` attachment
//! happen at the binding boundary via Python comparisons, exactly like the
//! original's naive-wall `combine` + rich comparison.

use crate::civil::{days_in_month, is_leap, to_ordinal, weekday_of_ordinal};
use crate::easter::easter;

pub const YEARLY: u8 = 0;
pub const MONTHLY: u8 = 1;
pub const WEEKLY: u8 = 2;
pub const DAILY: u8 = 3;
pub const HOURLY: u8 = 4;
pub const MINUTELY: u8 = 5;
pub const SECONDLY: u8 = 6;

pub const MAXYEAR: i64 = 9999;

/// Mask tables built exactly like the module-level expressions.
pub struct Masks {
    pub m366mask: Vec<i64>,
    pub m365mask: Vec<i64>,
    pub mday366mask: Vec<i64>,
    pub mday365mask: Vec<i64>,
    pub nmday366mask: Vec<i64>,
    pub nmday365mask: Vec<i64>,
    pub m366range: Vec<i64>,
    pub m365range: Vec<i64>,
    pub wdaymask_full: Vec<i64>,
}

impl Masks {
    pub fn build() -> Masks {
        let rep = |v: i64, n: usize| vec![v; n];
        let mut m366: Vec<i64> = Vec::new();
        for (m, n) in [(1, 31), (2, 29), (3, 31), (4, 30), (5, 31), (6, 30), (7, 31), (8, 31), (9, 30), (10, 31), (11, 30), (12, 31), (1, 7)] {
            m366.extend(rep(m, n));
        }
        let mut m365 = m366.clone();
        m365.remove(59);
        let r = |a: i64, b: i64| (a..b).collect::<Vec<i64>>();
        let m31 = r(1, 32);
        let m29 = r(1, 30);
        let m30 = r(1, 31);
        let cat = |parts: &[&[i64]]| parts.concat();
        let mday366 = cat(&[&m31, &m29, &m31, &m30, &m31, &m30, &m31, &m31, &m30, &m31, &m30, &m31, &m31[..7]]);
        let mut mday365 = mday366.clone();
        mday365.remove(59);
        // NMDAY: reassigned M31=range(-31,0), M29=range(-29,0), M30=range(-30,0).
        let nm29 = r(-29, 0);
        let nm30 = r(-30, 0);
        let nm31b = r(-31, 0);
        let nmday366 = cat(&[&nm31b, &nm29, &nm31b, &nm30, &nm31b, &nm30, &nm31b, &nm31b, &nm30, &nm31b, &nm30, &nm31b, &nm31b[..7]]);
        let mut nmday365 = nmday366.clone();
        nmday365.remove(31);
        let wday: Vec<i64> = (0..55).flat_map(|_| 0..7).map(|x| x as i64).collect();
        Masks {
            m366mask: m366,
            m365mask: m365,
            mday366mask: mday366,
            mday365mask: mday365,
            nmday366mask: nmday366,
            nmday365mask: nmday365,
            m366range: vec![0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366],
            m365range: vec![0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365],
            wdaymask_full: wday,
        }
    }
}

/// All iteration inputs, wall-based. `None` = unset (mirrors the `_byxxx`
/// attribute shapes: `()` for empty monthday sets, `None` elsewhere).
#[derive(Clone, Debug)]
pub struct Params {
    pub freq: u8,
    pub interval: i64,
    pub wkst: u8,
    pub bymonth: Option<Vec<i64>>,
    pub byweekno: Option<Vec<i64>>,
    pub byyearday: Option<Vec<i64>>,
    pub byeaster: Option<Vec<i64>>,
    pub bymonthday: Vec<i64>,
    pub bynmonthday: Vec<i64>,
    pub byweekday: Option<Vec<i64>>,
    pub bynweekday: Option<Vec<(i64, i64)>>,
    pub byhour: Option<Vec<i64>>,
    pub byminute: Option<Vec<i64>>,
    pub bysecond: Option<Vec<i64>>,
    pub bysetpos: Option<Vec<i64>>,
    /// Precomputed (hour, minute, second) sets for freq < HOURLY.
    pub timeset: Option<Vec<(u8, u8, u8)>>,
}

/// Rebuilt mask state (`_iterinfo`).
pub struct IterInfo {
    pub yearlen: i64,
    pub nextyearlen: i64,
    pub yearordinal: i64,
    pub mmask: Vec<i64>,
    pub mrange: Vec<i64>,
    pub mdaymask: Vec<i64>,
    pub nmdaymask: Vec<i64>,
    pub wdaymask: Vec<i64>,
    pub wnomask: Option<Vec<i64>>,
    pub nwdaymask: Option<Vec<i64>>,
    pub eastermask: Option<Vec<i64>>,
    pub lastyear: Option<i64>,
    pub lastmonth: Option<i64>,
}

impl IterInfo {
    pub fn fresh() -> IterInfo {
        IterInfo {
            yearlen: 0,
            nextyearlen: 0,
            yearordinal: 0,
            mmask: Vec::new(),
            mrange: Vec::new(),
            mdaymask: Vec::new(),
            nmdaymask: Vec::new(),
            wdaymask: Vec::new(),
            wnomask: None,
            nwdaymask: None,
            eastermask: None,
            lastyear: None,
            lastmonth: None,
        }
    }

    /// `rebuild(year, month)` verbatim. Fails with [`EngineError::Index`]
    /// when a user-supplied `byeaster` offset leaves the mask (the original
    /// raises `IndexError` there).
    pub fn rebuild(&mut self, masks: &Masks, p: &Params, year: i64, month: i64) -> Result<(), EngineError> {
        if Some(year) != self.lastyear {
            self.yearlen = 365 + is_leap(year) as i64;
            self.nextyearlen = 365 + is_leap(year + 1) as i64;
            let first = to_ordinal(year, 1, 1);
            self.yearordinal = first;
            let yearweekday = weekday_of_ordinal(first) as i64;
            let wday = yearweekday as usize;
            if self.yearlen == 365 {
                self.mmask = masks.m365mask.clone();
                self.mdaymask = masks.mday365mask.clone();
                self.nmdaymask = masks.nmday365mask.clone();
                self.wdaymask = masks.wdaymask_full[wday..].to_vec();
                self.mrange = masks.m365range.clone();
            } else {
                self.mmask = masks.m366mask.clone();
                self.mdaymask = masks.mday366mask.clone();
                self.nmdaymask = masks.nmday366mask.clone();
                self.wdaymask = masks.wdaymask_full[wday..].to_vec();
                self.mrange = masks.m366range.clone();
            }
            match &p.byweekno {
                None => self.wnomask = None,
                Some(byweekno) => {
                    let mut wnomask = vec![0i64; (self.yearlen + 7) as usize];
                    let no1wkst0 = (7 - yearweekday + p.wkst as i64) % 7;
                    let firstwkst = no1wkst0;
                    let (no1wkst, wyearlen) = if no1wkst0 >= 4 {
                        (0, self.yearlen + (yearweekday - p.wkst as i64).rem_euclid(7))
                    } else {
                        (no1wkst0, self.yearlen - no1wkst0)
                    };
                    let _ = no1wkst;
                    let (div, modu) = (wyearlen / 7, wyearlen % 7);
                    let numweeks = div + modu / 4;
                    for n0 in byweekno {
                        let mut n = *n0;
                        if n < 0 {
                            n += numweeks + 1;
                        }
                        if !(0 < n && n <= numweeks) {
                            continue;
                        }
                        let mut i: i64;
                        if n > 1 {
                            i = no1wkst + (n - 1) * 7;
                            if no1wkst != firstwkst {
                                i -= 7 - firstwkst;
                            }
                        } else {
                            i = no1wkst;
                        }
                        for _ in 0..7 {
                            wnomask[i as usize] = 1;
                            i += 1;
                            if self.wdaymask[i as usize] == p.wkst as i64 {
                                break;
                            }
                        }
                    }
                    if byweekno.contains(&1) {
                        let mut i = no1wkst + numweeks * 7;
                        if no1wkst != firstwkst {
                            i -= 7 - firstwkst;
                        }
                        if i < self.yearlen {
                            for _ in 0..7 {
                                wnomask[i as usize] = 1;
                                i += 1;
                                if self.wdaymask[i as usize] == p.wkst as i64 {
                                    break;
                                }
                            }
                        }
                    }
                    if no1wkst != 0 {
                        if !byweekno.contains(&-1) {
                            let lyearweekday =
                                weekday_of_ordinal(to_ordinal(year - 1, 1, 1)) as i64;
                            let mut lno1wkst = (7 - lyearweekday + p.wkst as i64) % 7;
                            let lnumweeks: i64;
                            if lno1wkst >= 4 {
                                lno1wkst = 0;
                                let lyearlen = 365 + is_leap(year - 1) as i64;
                                lnumweeks = 52 + (lyearlen + (lyearweekday - p.wkst as i64).rem_euclid(7)) % 7 / 4;
                            } else {
                                lnumweeks = 52 + (self.yearlen - no1wkst) % 7 / 4;
                            }
                            let _ = lno1wkst;
                            if byweekno.contains(&lnumweeks) {
                                for i in 0..no1wkst {
                                    wnomask[i as usize] = 1;
                                }
                            }
                        } else {
                            // `lnumweeks = -1` branch: -1 in byweekno.
                            for i in 0..no1wkst {
                                wnomask[i as usize] = 1;
                            }
                        }
                    }
                    self.wnomask = Some(wnomask);
                }
            }
        }

        if p.bynweekday.is_some() && (Some(month) != self.lastmonth || Some(year) != self.lastyear) {
            let mut ranges: Vec<(i64, i64)> = Vec::new();
            if p.freq == YEARLY {
                match &p.bymonth {
                    Some(bym) => {
                        for m in bym {
                            ranges.push((self.mrange[(m - 1) as usize], self.mrange[*m as usize]));
                        }
                    }
                    None => ranges.push((0, self.yearlen)),
                }
            } else if p.freq == MONTHLY {
                ranges.push((self.mrange[(month - 1) as usize], self.mrange[month as usize]));
            }
            if !ranges.is_empty() {
                let mut nwdaymask = vec![0i64; self.yearlen as usize];
                for (first, last0) in ranges {
                    let last = last0 - 1;
                    for (wday, n) in p.bynweekday.as_ref().unwrap() {
                        let i: i64;
                        if *n < 0 {
                            i = last + (n + 1) * 7;
                            let k = i - (self.wdaymask[i as usize] - wday).rem_euclid(7);
                            let i = k;
                            if first <= i && i <= last {
                                nwdaymask[i as usize] = 1;
                            }
                        } else {
                            i = first + (n - 1) * 7;
                            let k = i + (7 - self.wdaymask[i as usize] + wday) % 7;
                            let i = k;
                            if first <= i && i <= last {
                                nwdaymask[i as usize] = 1;
                            }
                        }
                    }
                }
                self.nwdaymask = Some(nwdaymask);
            }
        }

        if p.byeaster.is_some() {
            let mut eastermask = vec![0i64; (self.yearlen + 7) as usize];
            let (ey, em, ed) = easter(year, crate::easter::EASTER_WESTERN).unwrap();
            let _ = (ey, em);
            let eyday = to_ordinal(year, em, ed) - self.yearordinal;
            for offset in p.byeaster.as_ref().unwrap() {
                let idx = eyday + offset;
                if idx < 0 || idx as usize >= eastermask.len() {
                    return Err(EngineError::Index);
                }
                eastermask[idx as usize] = 1;
            }
            self.eastermask = Some(eastermask);
        }

        self.lastyear = Some(year);
        self.lastmonth = Some(month);
        Ok(())
    }
}

/// Engine errors (the binding maps these to Python errors).
#[derive(Clone, Debug, PartialEq)]
pub enum EngineError {
    /// Exhausted MINUTELY/SECONDLY search: the "Invalid combination" `ValueError`.
    EmptyRule,
    /// `__mod_distance` exhaustion: unpacking `None` (`TypeError`).
    UnpackNone,
    /// Division by zero in the filtered jumps (`ZeroDivisionError`).
    ZeroDiv,
    /// Mask index out of range (mirrors an `IndexError`).
    Index,
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// `__mod_distance(value, byxxx, base)`: next reachable value and the day
/// carry, or `None` when exhausted (the original falls off the end).
pub fn mod_distance(value: i64, byxxx: &[i64], interval: i64, base: i64) -> Option<(i64, i64)> {
    let mut accumulator = 0i64;
    let mut value = value;
    for _ in 1..=base {
        let (div, v) = crate::civil::py_divmod(value + interval, base);
        value = v;
        accumulator += div;
        if byxxx.contains(&value) {
            return Some((accumulator, value));
        }
    }
    None
}

/// One wall candidate: absolute ordinal + wall time.
pub type Candidate = (i64, u8, u8, u8);

/// Process-wide mask tables (deterministic; built once).
static MASKS: std::sync::LazyLock<Masks> = std::sync::LazyLock::new(Masks::build);

/// Shared mask tables.
pub fn masks() -> &'static Masks {
    &MASKS
}

/// Stepping state for `_iter`. `day`/`weekday`/`hour`/`minute`/`second`
/// mirror the loop variables; the binding filters candidates against
/// `until`/`dtstart` and attaches `tzinfo`. Params are owned so the
/// engine moves freely across the binding boundary.
pub struct Engine {
    pub params: Params,
    pub info: IterInfo,
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub weekday: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    pub done: bool,
}

impl Engine {
    /// Initial state from the dtstart wall triple.
    pub fn new(
        params: Params,
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        minute: i64,
        second: i64,
    ) -> Engine {
        let weekday = weekday_of_ordinal(to_ordinal(year, month as u8, day as u8)) as i64;
        Engine {
            params,
            info: IterInfo::fresh(),
            year,
            month,
            day,
            weekday,
            hour,
            minute,
            second,
            done: false,
        }
    }

    fn getdayset(&self, freq: u8, year: i64, month: i64, day: i64) -> (Vec<Option<i64>>, i64, i64) {
        let ii = &self.info;
        match freq {
            YEARLY => ((0..ii.yearlen).map(Some).collect(), 0, ii.yearlen),
            MONTHLY => {
                let mut dset = vec![None; ii.yearlen as usize];
                let (start, end) = (ii.mrange[(month - 1) as usize], ii.mrange[month as usize]);
                for i in start..end {
                    dset[i as usize] = Some(i);
                }
                (dset, start, end)
            }
            WEEKLY => {
                let mut dset = vec![None; (ii.yearlen + 7) as usize];
                let mut i = to_ordinal(year, month as u8, day as u8) - ii.yearordinal;
                let start = i;
                for _ in 0..7 {
                    dset[i as usize] = Some(i);
                    i += 1;
                    if ii.wdaymask[i as usize] == self.params.wkst as i64 {
                        break;
                    }
                }
                (dset, start, i)
            }
            _ => {
                let mut dset = vec![None; ii.yearlen as usize];
                let i = to_ordinal(year, month as u8, day as u8) - ii.yearordinal;
                dset[i as usize] = Some(i);
                (dset, i, i + 1)
            }
        }
    }

    fn gettimeset(&self, freq: u8, hour: i64, minute: i64, second: i64) -> Vec<(u8, u8, u8)> {
        let p = &self.params;
        let mut tset: Vec<(u8, u8, u8)> = Vec::new();
        match freq {
            HOURLY => {
                for m in p.byminute.as_ref().unwrap() {
                    for s in p.bysecond.as_ref().unwrap() {
                        tset.push((hour as u8, *m as u8, *s as u8));
                    }
                }
            }
            MINUTELY => {
                for s in p.bysecond.as_ref().unwrap() {
                    tset.push((hour as u8, minute as u8, *s as u8));
                }
            }
            _ => {
                tset.push((hour as u8, minute as u8, second as u8));
            }
        }
        tset.sort();
        tset
    }

    /// Run one `_iter` loop pass: the filtered, ordered wall candidates
    /// for the current step, then advance the stepping state. An empty
    /// vector means "no candidates this step, call again"; `done` ends
    /// iteration (the binding still applies its own until/count filters).
    pub fn next_step(&mut self) -> Result<Vec<Candidate>, EngineError> {
        if self.done {
            return Ok(Vec::new());
        }
        let p = &self.params;
        let freq = p.freq;
        let (year, month, day) = (self.year, self.month, self.day);
        self.info.rebuild(masks(), p, year, month)?;
        let (mut dayset, start, end) = self.getdayset(freq, year, month, day);
        let timeset: Vec<(u8, u8, u8)>;
        if freq < HOURLY {
            timeset = p.timeset.clone().unwrap();
        } else {
            let (hour, minute, second) = (self.hour, self.minute, self.second);
            let empty = (freq >= HOURLY && p.byhour.as_ref().map(|h| !h.contains(&hour)).unwrap_or(false))
                || (freq >= MINUTELY && p.byminute.as_ref().map(|m| !m.contains(&minute)).unwrap_or(false))
                || (freq >= SECONDLY && p.bysecond.as_ref().map(|s| !s.contains(&second)).unwrap_or(false));
            if empty {
                timeset = Vec::new();
            } else {
                timeset = self.gettimeset(freq, hour, minute, second);
            }
        }
        // Dayset filter pass.
        let ii = &self.info;
        let mut filtered = false;
        let snapshot: Vec<Option<i64>> = dayset[start as usize..end as usize].to_vec();
        for slot in snapshot {
            let i = match slot {
                None => continue,
                Some(v) => v,
            };
            let iu = i as usize;
            let skip = (p.bymonth.as_ref().map(|m| !m.contains(&ii.mmask[iu])).unwrap_or(false))
                || (p.byweekno.is_some() && ii.wnomask.as_ref().unwrap()[iu] == 0)
                || (p.byweekday.is_some() && !p.byweekday.as_ref().unwrap().contains(&ii.wdaymask[iu]))
                || (ii.nwdaymask.is_some() && ii.nwdaymask.as_ref().unwrap()[iu] == 0)
                || (p.byeaster.is_some() && ii.eastermask.as_ref().unwrap()[iu] == 0)
                || ((!p.bymonthday.is_empty() || !p.bynmonthday.is_empty())
                    && !p.bymonthday.contains(&ii.mdaymask[iu])
                    && !p.bynmonthday.contains(&ii.nmdaymask[iu]))
                || (p.byyearday.is_some()
                    && byyearday_skip(i, ii.yearlen, self.info.nextyearlen, p.byyearday.as_ref().unwrap()));
            if skip {
                dayset[iu] = None;
                filtered = true;
            }
        }
        // Expand to wall candidates.
        let mut out: Vec<Candidate> = Vec::new();
        match &p.bysetpos {
            Some(bysetpos) if !timeset.is_empty() => {
                let mut poslist: Vec<Candidate> = Vec::new();
                for pos in bysetpos {
                    let (daypos, timepos) = if *pos < 0 {
                        crate::civil::py_divmod(*pos, timeset.len() as i64)
                    } else {
                        crate::civil::py_divmod(pos - 1, timeset.len() as i64)
                    };
                    let valid: Vec<i64> = dayset[start as usize..end as usize]
                        .iter()
                        .filter_map(|x| *x)
                        .collect();
                    let di = if daypos < 0 {
                        valid.len() as i64 + daypos
                    } else {
                        daypos
                    };
                    if di < 0 || di as usize >= valid.len() {
                        continue;
                    }
                    if timepos < 0 || timepos as usize >= timeset.len() {
                        continue;
                    }
                    let cand = (ii.yearordinal + valid[di as usize], timeset[timepos as usize].0, timeset[timepos as usize].1, timeset[timepos as usize].2);
                    if !poslist.contains(&cand) {
                        poslist.push(cand);
                    }
                }
                poslist.sort();
                out = poslist;
            }
            _ => {
                for slot in dayset[start as usize..end as usize].iter() {
                    if let Some(i) = slot {
                        let base = ii.yearordinal + i;
                        for t in &timeset {
                            out.push((base, t.0, t.1, t.2));
                        }
                    }
                }
            }
        }
        self.advance(filtered)?;
        Ok(out)
    }

    /// Frequency/interval stepping verbatim (`_iter` tail): a local
    /// `fixday` flag with one month walk at the end, like the original.
    fn advance(&mut self, filtered: bool) -> Result<(), EngineError> {
        if self.params.interval == 0 && self.params.freq >= HOURLY && filtered {
            return Err(EngineError::ZeroDiv);
        }
        let p = &self.params;
        let freq = p.freq;
        let interval = p.interval;
        let mut fixday = false;
        match freq {
            YEARLY => {
                self.year += interval;
                if self.year > MAXYEAR {
                    self.done = true;
                    return Ok(());
                }
                self.info.rebuild(masks(), p, self.year, self.month)?;
            }
            MONTHLY => {
                self.month += interval;
                if self.month > 12 {
                    let (div, modu) = crate::civil::py_divmod(self.month, 12);
                    self.month = modu;
                    self.year += div;
                    if self.month == 0 {
                        self.month = 12;
                        self.year -= 1;
                    }
                    if self.year > MAXYEAR {
                        self.done = true;
                        return Ok(());
                    }
                }
                let (y, m) = (self.year, self.month);
                self.info.rebuild(masks(), p, y, m)?;
            }
            WEEKLY => {
                if p.wkst as i64 > self.weekday {
                    self.day += -(self.weekday + 1 + (6 - p.wkst as i64)) + interval * 7;
                } else {
                    self.day += -(self.weekday - p.wkst as i64) + interval * 7;
                }
                self.weekday = p.wkst as i64;
                fixday = true;
            }
            DAILY => {
                self.day += interval;
                fixday = true;
            }
            HOURLY => {
                if filtered {
                    self.hour += ((23 - self.hour) / interval) * interval;
                }
                if let Some(byhour) = &p.byhour {
                    match mod_distance(self.hour, byhour, interval, 24) {
                        Some((ndays, h)) => {
                            if ndays != 0 {
                                self.day += ndays;
                                fixday = true;
                            }
                            self.hour = h;
                        }
                        None => return Err(EngineError::UnpackNone),
                    }
                } else {
                    let (ndays, h) = crate::civil::py_divmod(self.hour + interval, 24);
                    if ndays != 0 {
                        self.day += ndays;
                        fixday = true;
                    }
                    self.hour = h;
                }
            }
            MINUTELY => {
                if filtered {
                    self.minute += ((1439 - (self.hour * 60 + self.minute)) / interval) * interval;
                }
                let rep_rate = 24 * 60;
                let mut valid = false;
                for _ in 0..rep_rate / gcd(interval, rep_rate) {
                    if let Some(byminute) = &p.byminute {
                        match mod_distance(self.minute, byminute, interval, 60) {
                            Some((nhours, m)) => {
                                let (div, h) = crate::civil::py_divmod(self.hour + nhours, 24);
                                self.minute = m;
                                self.hour = h;
                                if div != 0 {
                                    self.day += div;
                                    fixday = true;
                                }
                            }
                            None => return Err(EngineError::UnpackNone),
                        }
                    } else {
                        let (nhours, m) = crate::civil::py_divmod(self.minute + interval, 60);
                        let (div, h) = crate::civil::py_divmod(self.hour + nhours, 24);
                        self.minute = m;
                        self.hour = h;
                        if div != 0 {
                            self.day += div;
                            fixday = true;
                        }
                    }
                    if p.byhour.is_none() || p.byhour.as_ref().unwrap().contains(&self.hour) {
                        valid = true;
                        break;
                    }
                }
                if !valid {
                    return Err(EngineError::EmptyRule);
                }
            }
            SECONDLY => {
                if filtered {
                    self.second += (86399 - (self.hour * 3600 + self.minute * 60 + self.second)) / interval * interval;
                }
                let rep_rate = 24 * 3600;
                let mut valid = false;
                for _ in 0..rep_rate / gcd(interval, rep_rate) {
                    if let Some(bysecond) = &p.bysecond {
                        match mod_distance(self.second, bysecond, interval, 60) {
                            Some((nminutes, s)) => {
                                let (div, m) = crate::civil::py_divmod(self.minute + nminutes, 60);
                                self.second = s;
                                self.minute = m;
                                if div != 0 {
                                    self.hour += div;
                                    let (div2, h) = crate::civil::py_divmod(self.hour, 24);
                                    self.hour = h;
                                    if div2 != 0 {
                                        self.day += div2;
                                        fixday = true;
                                    }
                                }
                            }
                            None => return Err(EngineError::UnpackNone),
                        }
                    } else {
                        let (nminutes, s) = crate::civil::py_divmod(self.second + interval, 60);
                        let (div, m) = crate::civil::py_divmod(self.minute + nminutes, 60);
                        self.second = s;
                        self.minute = m;
                        if div != 0 {
                            self.hour += div;
                            let (div2, h) = crate::civil::py_divmod(self.hour, 24);
                            self.hour = h;
                            if div2 != 0 {
                                self.day += div2;
                                fixday = true;
                            }
                        }
                    }
                    if (p.byhour.is_none() || p.byhour.as_ref().unwrap().contains(&self.hour))
                        && (p.byminute.is_none() || p.byminute.as_ref().unwrap().contains(&self.minute))
                        && (p.bysecond.is_none() || p.bysecond.as_ref().unwrap().contains(&self.second))
                    {
                        valid = true;
                        break;
                    }
                }
                if !valid {
                    return Err(EngineError::EmptyRule);
                }
            }
            _ => {}
        }
        if fixday {
            self.fixday_walk()?;
        }
        Ok(())
    }

    /// `if fixday and day > 28` month walk.
    fn fixday_walk(&mut self) -> Result<(), EngineError> {
        if self.day > 28 {
            let mut daysinmonth = days_in_month(self.year, self.month as u8) as i64;
            if self.day > daysinmonth {
                while self.day > daysinmonth {
                    self.day -= daysinmonth;
                    self.month += 1;
                    if self.month == 13 {
                        self.month = 1;
                        self.year += 1;
                        if self.year > MAXYEAR {
                            self.done = true;
                            return Ok(());
                        }
                    }
                    daysinmonth = days_in_month(self.year, self.month as u8) as i64;
                }
                let (y, m) = (self.year, self.month);
                self.info.rebuild(masks(), &self.params, y, m)?;
            }
        }
        Ok(())
    }
}

/// `byyearday` skip predicate verbatim.
fn byyearday_skip(i: i64, yearlen: i64, nextyearlen: i64, byyearday: &[i64]) -> bool {
    if i < yearlen {
        !byyearday.contains(&(i + 1)) && !byyearday.contains(&(-yearlen + i))
    } else {
        !byyearday.contains(&(i + 1 - yearlen)) && !byyearday.contains(&(-nextyearlen + i - yearlen))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::to_ordinal;

    fn yearly_params() -> Params {
        Params {
            freq: YEARLY,
            interval: 1,
            wkst: 0,
            bymonth: Some(vec![9]),
            byweekno: None,
            byyearday: None,
            byeaster: None,
            bymonthday: vec![2],
            bynmonthday: vec![],
            byweekday: None,
            bynweekday: None,
            byhour: Some(vec![9]),
            byminute: Some(vec![0]),
            bysecond: Some(vec![0]),
            bysetpos: None,
            timeset: Some(vec![(9, 0, 0)]),
        }
    }

    #[test]
    fn yearly_default_steps() {
        let mut e = Engine::new(yearly_params(), 1997, 9, 2, 9, 0, 0);
        let b1 = e.next_step().unwrap();
        assert_eq!(b1.len(), 1);
        assert_eq!(b1[0].0, to_ordinal(1997, 9, 2));
        let b2 = e.next_step().unwrap();
        assert_eq!(b2.len(), 1);
        assert_eq!(b2[0].0, to_ordinal(1998, 9, 2));
    }
}
