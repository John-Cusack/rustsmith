// SPDX-License-Identifier: Apache-2.0
//! `_ymd`: year/month/day accumulator plus `resolve_ymd` disambiguation.
//! Values stay normalized decimal strings (arbitrary magnitude; the
//! binding builds the identical Python `int`, preserving `OverflowError`
//! vs `ParserError` routing).

use std::cmp::Ordering;

use super::dec::{norm_int_str, Dec};
use super::lex::is_num;
use super::{days_in_month_str, Fail, Info};

/// Year/month/day accumulator (mirrors `_ymd`).
#[derive(Default)]
pub struct Ymd {
    vals: Vec<String>,
    pub century_specified: bool,
    dstridx: Option<usize>,
    mstridx: Option<usize>,
    ystridx: Option<usize>,
}

/// Resolved (year, month, day) as decimal strings.
pub type YmdTriple = (Option<String>, Option<String>, Option<String>);

impl Ymd {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.vals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vals.is_empty()
    }

    fn push(&mut self, ival: String, label: Option<char>) -> Result<(), Fail> {
        self.vals.push(ival);
        match label {
            Some('M') => {
                if self.mstridx.is_some() {
                    return Err(Fail::NoResult); // 'Month is already set'
                }
                self.mstridx = Some(self.vals.len() - 1);
            }
            Some('D') => {
                if self.dstridx.is_some() {
                    return Err(Fail::NoResult); // 'Day is already set'
                }
                self.dstridx = Some(self.vals.len() - 1);
            }
            Some('Y') => {
                if self.ystridx.is_some() {
                    return Err(Fail::NoResult); // 'Year is already set'
                }
                self.ystridx = Some(self.vals.len() - 1);
            }
            _ => {}
        }
        Ok(())
    }

    /// `append` of a raw token string (mirrors the `hasattr(val,
    /// '__len__')` branch: digit runs longer than 2 chars imply century).
    pub fn append_str(&mut self, val: &str, label: Option<char>) -> Result<(), Fail> {
        let mut lbl = label;
        if val.chars().count() > 2 && val.chars().all(is_num) {
            self.century_specified = true;
            match lbl {
                None | Some('Y') => {}
                _ => return Err(Fail::NoResult),
            }
            lbl = Some('Y');
        }
        let ival = norm_int_str(val).ok_or(Fail::NoResult)?;
        self.push(ival, lbl)
    }

    /// `append` of a `Decimal` value (mirrors the `val > 100` branch).
    pub fn append_dec(&mut self, v: &Dec, label: Option<char>) -> Result<(), Fail> {
        let mut lbl = label;
        if v.cmp_u32(100) == Ordering::Greater {
            self.century_specified = true;
            match lbl {
                None | Some('Y') => {}
                _ => return Err(Fail::NoResult),
            }
            lbl = Some('Y');
        }
        self.push(v.int_part().to_string(), lbl)
    }

    /// `append` of a small int with a label (month-name path).
    pub fn append_small(&mut self, v: i64, label: char) -> Result<(), Fail> {
        self.push(v.to_string(), Some(label))
    }

    pub fn could_be_day(&self, v: &Dec, info: &dyn Info) -> Result<bool, Fail> {
        if self.dstridx.is_some() {
            return Ok(false);
        }
        if self.mstridx.is_none() {
            return Ok(v.cmp_u32(1) != Ordering::Less && v.cmp_u32(31) != Ordering::Greater);
        }
        let month = self.vals[self.mstridx.unwrap()].clone();
        let dim = if self.ystridx.is_none() {
            let m: i64 = month.parse().map_err(|_| Fail::NoResult)?;
            days_in_month_str("2000", m)
        } else {
            let year = self.vals[self.ystridx.unwrap()].clone();
            info.days_in_month(&year, &month)?
        };
        Ok(v.cmp_u32(1) != Ordering::Less && v.cmp_u32(dim as u32) != Ordering::Greater)
    }

    fn resolve_from_stridxs(
        &self,
        strids: Vec<(&str, usize)>,
    ) -> YmdTriple {
        let mut full = strids;
        if self.vals.len() == 3 && full.len() == 2 {
            let have: Vec<usize> = full.iter().map(|&(_, v)| v).collect();
            let missing = (0..3).find(|x| !have.contains(x)).unwrap();
            let havek: Vec<&str> = full.iter().map(|&(k, _)| k).collect();
            let missk = ["y", "m", "d"]
                .into_iter()
                .find(|k| !havek.contains(k))
                .unwrap();
            full.push((missk, missing));
        }
        let mut y = None;
        let mut m = None;
        let mut d = None;
        for (k, idx) in full {
            let val = self.vals[idx].clone();
            match k {
                "y" => y = Some(val),
                "m" => m = Some(val),
                _ => d = Some(val),
            }
        }
        (y, m, d)
    }

    /// Mirrors `_ymd.resolve_ymd` exactly (values stay decimal strings).
    pub fn resolve_ymd(
        &self,
        yearfirst: bool,
        dayfirst: bool,
    ) -> Result<YmdTriple, Fail> {
        let len_ymd = self.vals.len();
        let mut strids: Vec<(&str, usize)> = Vec::new();
        if let Some(v) = self.ystridx {
            strids.push(("y", v));
        }
        if let Some(v) = self.mstridx {
            strids.push(("m", v));
        }
        if let Some(v) = self.dstridx {
            strids.push(("d", v));
        }
        if (len_ymd == strids.len() && !strids.is_empty())
            || (len_ymd == 3 && strids.len() == 2)
        {
            return Ok(self.resolve_from_stridxs(strids));
        }
        let mstridx = self.mstridx;
        if len_ymd > 3 {
            return Err(Fail::NoResult); // 'More than three YMD values'
        }
        if len_ymd == 1 || (mstridx.is_some() && len_ymd == 2) {
            // `self[mstridx - 1]` wraps on 0 (Python negative indexing).
            let (other, month): (String, Option<String>);
            if let Some(mi) = mstridx {
                month = Some(self.vals[mi].clone());
                other = self.vals[(mi + self.vals.len() - 1) % self.vals.len()].clone();
            } else {
                month = None;
                other = self.vals[0].clone();
            }
            if len_ymd > 1 || mstridx.is_none() {
                if cmp_norm(&other, 31) == Ordering::Greater {
                    return Ok((Some(other), month, None));
                } else {
                    return Ok((None, month, Some(other)));
                }
            }
            return Ok((None, month, None));
        }
        if len_ymd == 2 {
            let (a, b) = (self.vals[0].clone(), self.vals[1].clone());
            if cmp_norm(&a, 31) == Ordering::Greater {
                return Ok((Some(a), Some(b), None));
            } else if cmp_norm(&b, 31) == Ordering::Greater {
                return Ok((Some(b), Some(a), None));
            } else if dayfirst && cmp_norm(&b, 12) != Ordering::Greater {
                return Ok((None, Some(b), Some(a)));
            } else {
                return Ok((None, Some(a), Some(b)));
            }
        }
        if len_ymd == 3 {
            let (a, b, c) = (
                self.vals[0].clone(),
                self.vals[1].clone(),
                self.vals[2].clone(),
            );
            if mstridx == Some(0) {
                if cmp_norm(&b, 31) == Ordering::Greater {
                    return Ok((Some(b), Some(a), Some(c)));
                } else {
                    return Ok((Some(c), Some(a), Some(b)));
                }
            } else if mstridx == Some(1) {
                if cmp_norm(&a, 31) == Ordering::Greater
                    || (yearfirst && cmp_norm(&c, 31) != Ordering::Greater)
                {
                    return Ok((Some(a), Some(b), Some(c)));
                } else {
                    return Ok((Some(c), Some(b), Some(a)));
                }
            } else if mstridx == Some(2) {
                if cmp_norm(&b, 31) == Ordering::Greater {
                    return Ok((Some(b), Some(c), Some(a)));
                } else {
                    return Ok((Some(a), Some(c), Some(b)));
                }
            } else if cmp_norm(&a, 31) == Ordering::Greater
                || self.ystridx == Some(0)
                || (yearfirst
                    && cmp_norm(&b, 12) != Ordering::Greater
                    && cmp_norm(&c, 31) != Ordering::Greater)
            {
                if dayfirst && cmp_norm(&c, 12) != Ordering::Greater {
                    return Ok((Some(a), Some(c), Some(b)));
                } else {
                    return Ok((Some(a), Some(b), Some(c)));
                }
            } else if cmp_norm(&a, 12) == Ordering::Greater
                || (dayfirst && cmp_norm(&b, 12) != Ordering::Greater)
            {
                return Ok((Some(c), Some(b), Some(a)));
            } else {
                return Ok((Some(c), Some(a), Some(b)));
            }
        }
        Ok((None, None, None))
    }
}

/// Compare a normalized int string against a small int exactly.
pub fn cmp_norm(s: &str, n: u32) -> Ordering {
    let ns = n.to_string();
    match s.len().cmp(&ns.len()) {
        Ordering::Equal => s.cmp(&ns),
        o => o,
    }
}
