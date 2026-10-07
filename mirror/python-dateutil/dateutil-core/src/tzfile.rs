// SPDX-License-Identifier: Apache-2.0
//! TZif file parser, mirroring `dateutil.tz.tzfile._read_tzfile` (v1
//! 32-bit section only, like the original) plus the transition lookup
//! helpers (`_find_last_transition`, `_get_ttinfo`, `_find_ttinfo`,
//! `_resolve_ambiguous_time`, `is_ambiguous`, `fromutc`).

/// One parsed ttinfo record. `isdst` stays an integer (0/1) like the
/// original; `isstd`/`isgmt` are booleans like the original.
#[derive(Clone, Debug, PartialEq)]
pub struct TtInfo {
    pub offset: i64,
    pub isdst: i64,
    pub abbr: String,
    pub isstd: bool,
    pub isgmt: bool,
    pub dstoffset: i64,
}

/// Parsed tzfile data (the `_tzfile` attributes, wall-adjusted).
#[derive(Clone, Debug, PartialEq)]
pub struct TzFileData {
    pub trans_list_utc: Vec<i64>,
    pub trans_list: Vec<i64>,
    pub trans_idx: Vec<usize>,
    pub ttinfo_list: Vec<TtInfo>,
    pub ttinfo_std: Option<usize>,
    pub ttinfo_dst: Option<usize>,
    pub ttinfo_before: Option<usize>,
    pub ttinfo_first: Option<usize>,
}

/// Parse failure modes (the binding maps these to Python errors).
#[derive(Clone, Debug, PartialEq)]
pub enum TzFileError {
    MagicNotFound,
    DecodeError,
    Truncated,
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn read(&mut self, n: usize) -> Result<&'a [u8], TzFileError> {
        if self.pos + n > self.data.len() {
            return Err(TzFileError::Truncated);
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn i32_be(&mut self) -> Result<i64, TzFileError> {
        let b = self.read(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64)
    }

    fn u8v(&mut self) -> Result<u8, TzFileError> {
        Ok(self.read(1)?[0])
    }

    fn i8v(&mut self) -> Result<i64, TzFileError> {
        Ok(self.read(1)?[0] as i8 as i64)
    }

    fn skip(&mut self, n: usize) -> Result<(), TzFileError> {
        self.read(n).map(|_| ())
    }
}

/// Parse TZif bytes exactly like `_read_tzfile` (v1 section).
pub fn parse_tzfile(data: &[u8]) -> Result<TzFileData, TzFileError> {
    let mut c = Cursor { data, pos: 0 };
    // `fileobj.read(4).decode() != "TZif"`; short reads compare unequal.
    let magic = c.read(4).unwrap_or(&[]);
    let magic_ok = std::str::from_utf8(magic).map(|s| s == "TZif");
    match magic_ok {
        Ok(true) => {}
        Ok(false) => return Err(TzFileError::MagicNotFound),
        Err(_) => return Err(TzFileError::DecodeError),
    }
    c.skip(16)?;
    let ttisgmtcnt = c.i32_be()? as usize;
    let ttisstdcnt = c.i32_be()? as usize;
    let leapcnt = c.i32_be()? as usize;
    let timecnt = c.i32_be()? as usize;
    let typecnt = c.i32_be()? as usize;
    let charcnt = c.i32_be()? as usize;

    let mut trans_list_utc = Vec::with_capacity(timecnt);
    for _ in 0..timecnt {
        trans_list_utc.push(c.i32_be()?);
    }
    let mut trans_idx_raw = Vec::with_capacity(timecnt);
    for _ in 0..timecnt {
        trans_idx_raw.push(c.u8v()? as usize);
    }
    let mut ttinfo_raw = Vec::with_capacity(typecnt);
    for _ in 0..typecnt {
        let gmtoff = c.i32_be()?;
        let isdst = c.i8v()?;
        let abbrind = c.u8v()? as usize;
        ttinfo_raw.push((gmtoff, isdst, abbrind));
    }
    let abbr_bytes = c.read(charcnt)?;
    let abbr = std::str::from_utf8(abbr_bytes).map_err(|_| TzFileError::DecodeError)?;
    if leapcnt > 0 {
        c.skip(leapcnt * 8)?;
    }
    let mut isstd = Vec::new();
    if ttisstdcnt > 0 {
        for _ in 0..ttisstdcnt {
            isstd.push(c.i8v()?);
        }
    }
    let mut isgmt = Vec::new();
    if ttisgmtcnt > 0 {
        for _ in 0..ttisgmtcnt {
            isgmt.push(c.i8v()?);
        }
    }

    let mut ttinfo_list = Vec::with_capacity(typecnt);
    for (gmtoff, isdst, abbrind) in ttinfo_raw {
        let end = abbr[abbrind..].find('\x00').map(|i| abbrind + i).unwrap_or(abbr.len());
        ttinfo_list.push(TtInfo {
            offset: gmtoff,
            isdst,
            abbr: abbr[abbrind.min(abbr.len())..end].to_string(),
            isstd: ttisstdcnt > ttinfo_list.len() && isstd[ttinfo_list.len()] != 0,
            isgmt: ttisgmtcnt > ttinfo_list.len() && isgmt[ttinfo_list.len()] != 0,
            dstoffset: 0,
        });
    }

    let trans_idx: Vec<usize> = trans_idx_raw;
    let mut out = TzFileData {
        trans_list_utc,
        trans_list: Vec::new(),
        trans_idx,
        ttinfo_list,
        ttinfo_std: None,
        ttinfo_dst: None,
        ttinfo_before: None,
        ttinfo_first: None,
    };

    if !out.ttinfo_list.is_empty() {
        if out.trans_list_utc.is_empty() {
            out.ttinfo_std = Some(0);
            out.ttinfo_first = Some(0);
        } else {
            for i in (0..timecnt).rev() {
                let tti = out.trans_idx[i];
                if out.ttinfo_std.is_none() && out.ttinfo_list[tti].isdst == 0 {
                    out.ttinfo_std = Some(tti);
                } else if out.ttinfo_dst.is_none() && out.ttinfo_list[tti].isdst != 0 {
                    out.ttinfo_dst = Some(tti);
                }
                if out.ttinfo_std.is_some() && out.ttinfo_dst.is_some() {
                    break;
                }
            }
            if out.ttinfo_dst.is_some() && out.ttinfo_std.is_none() {
                out.ttinfo_std = out.ttinfo_dst;
            }
            let mut before = 0;
            for (n, tti) in out.ttinfo_list.iter().enumerate() {
                if tti.isdst == 0 {
                    before = n;
                    break;
                }
            }
            out.ttinfo_before = Some(before);
        }
    }

    // Wall-adjust the transition list (mutates shared ttinfo dstoffsets).
    let mut lastdst: Option<i64> = None;
    let mut lastoffset = 0i64;
    let mut lastdstoffset = 0i64;
    let mut lastbaseoffset: Option<i64> = None;
    let mut trans_list = Vec::with_capacity(timecnt);
    for i in 0..timecnt {
        let ti = out.trans_idx[i];
        let (offset, isdst) = (out.ttinfo_list[ti].offset, out.ttinfo_list[ti].isdst);
        let mut dstoffset = 0i64;
        if let Some(ld) = lastdst {
            if isdst != 0 {
                if ld == 0 {
                    dstoffset = offset - lastoffset;
                }
                if dstoffset == 0 && lastdstoffset != 0 {
                    dstoffset = lastdstoffset;
                }
                out.ttinfo_list[ti].dstoffset = dstoffset;
                lastdstoffset = dstoffset;
            }
        }
        let baseoffset = offset - dstoffset;
        let mut adjustment = baseoffset;
        if let Some(lbo) = lastbaseoffset {
            if baseoffset != lbo && isdst != lastdst.unwrap_or(isdst) {
                adjustment = lbo;
            }
        }
        lastdst = Some(isdst);
        lastoffset = offset;
        lastbaseoffset = Some(baseoffset);
        trans_list.push(out.trans_list_utc[i] + adjustment);
    }
    out.trans_list = trans_list;
    Ok(out)
}

/// `_find_last_transition`: bisect_right index minus one; `None` when empty.
pub fn find_last_transition(data: &TzFileData, timestamp: f64, in_utc: bool) -> Option<i64> {
    if data.trans_list.is_empty() {
        return None;
    }
    let list = if in_utc { &data.trans_list_utc } else { &data.trans_list };
    let mut lo = 0usize;
    let mut hi = list.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if (list[mid] as f64) <= timestamp {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    Some(lo as i64 - 1)
}

/// `_get_ttinfo`: resolve an index to a ttinfo slot.
pub fn get_ttinfo(data: &TzFileData, idx: Option<i64>) -> Option<usize> {
    match idx {
        None => data.ttinfo_std,
        Some(i) => {
            if (i + 1) as usize >= data.trans_list.len() {
                data.ttinfo_std
            } else if i < 0 {
                data.ttinfo_before
            } else {
                Some(data.trans_idx[i as usize])
            }
        }
    }
}

/// `is_ambiguous(dt, idx)`: wall-repeat test from the transition table.
pub fn is_ambiguous_at(data: &TzFileData, timestamp: f64, idx: Option<i64>) -> bool {
    let idx = match idx {
        None => find_last_transition(data, timestamp, false),
        Some(i) => Some(i),
    };
    let tti = match get_ttinfo(data, idx) {
        None => return false,
        Some(t) => t,
    };
    let idx = match idx {
        None => return false,
        Some(i) => i,
    };
    if idx <= 0 {
        return false;
    }
    let prev = match get_ttinfo(data, Some(idx - 1)) {
        None => return false,
        Some(t) => t,
    };
    let od = data.ttinfo_list[prev].offset - data.ttinfo_list[tti].offset;
    let tt = data.trans_list[idx as usize];
    timestamp < (tt + od) as f64
}

/// `_resolve_ambiguous_time`: fold-aware index shift.
pub fn resolve_ambiguous_time(data: &TzFileData, timestamp: f64, fold: i64) -> Option<i64> {
    let idx = find_last_transition(data, timestamp, false);
    match idx {
        None => None,
        Some(0) => Some(0),
        Some(i) => {
            let shift = i64::from(fold == 0 && is_ambiguous_at(data, timestamp, Some(i)));
            Some(i - shift)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures decoded from the dateutil test suite's own vectors.
    const NEW_YORK_BYTES: &[u8] = include_bytes!("../testdata/new_york.tz");
    const EST5EDT_BYTES: &[u8] = include_bytes!("../testdata/est5edt.tz");

    #[test]
    fn magic_rejected() {
        assert_eq!(parse_tzfile(b"NOPE"), Err(TzFileError::MagicNotFound));
        assert_eq!(parse_tzfile(b""), Err(TzFileError::MagicNotFound));
    }

    #[test]
    fn new_york_parse_matches() {
        let d = parse_tzfile(NEW_YORK_BYTES).unwrap();
        assert_eq!(d.trans_list.len(), 235);
        assert_eq!(d.ttinfo_list.len(), 4);
        let std = d.ttinfo_std.unwrap();
        assert_eq!(d.ttinfo_list[std].offset, -18000);
        assert_eq!(d.ttinfo_list[std].abbr, "EST");
        let dst = d.ttinfo_dst.unwrap();
        assert_eq!(d.ttinfo_list[dst].offset, -14400);
        assert_eq!(d.ttinfo_list[dst].abbr, "EDT");
        let before = d.ttinfo_before.unwrap();
        assert_eq!(d.ttinfo_list[before].offset, -18000);
        // First transitions, wall-adjusted (from the original's parse).
        assert_eq!(&d.trans_list[..3], &[-1633294800, -1615154400, -1601848800]);
        assert_eq!(&d.trans_list_utc[..3], &[-1633280400, -1615140000, -1601830800]);
        assert_eq!(
            d.ttinfo_list.iter().map(|t| t.isstd).collect::<Vec<_>>(),
            vec![false, false, false, true]
        );
    }

    #[test]
    fn est5edt_parse_matches() {
        let d = parse_tzfile(EST5EDT_BYTES).unwrap();
        assert!(!d.trans_list.is_empty());
        assert!(!d.ttinfo_list.is_empty());
    }
}
