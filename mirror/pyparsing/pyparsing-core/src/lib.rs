//! Pure-Rust character-scan engine for the pyparsing Stage-1 mirror.
//!
//! This crate has no Python dependency and contains **no `unsafe` code**.
//! It owns the two remainder loops that dominate text-heavy pyparsing
//! parses (`CharsNotIn` consumed 96% of a 20KB-block parse in the recon
//! profile):
//!
//! - [`scan_while_in`]: first index in `[start, end)` whose unit is NOT in
//!   the table (mirrors the `White` remainder loop).
//! - [`scan_while_not_in`]: first index whose unit IS in the table
//!   (mirrors the `CharsNotIn` remainder loop).
//!
//! Text arrives as [`RawUnits`]: raw CPython unicode bytes (`PyUnicode`
//! kinds 1/2/4) borrowed as a plain `&[u8]`. The single `unsafe` block of
//! the whole port — slice construction at the FFI boundary — lives in the
//! `pyparsing._pyparsing` binding crate, where the gate's FFI-boundary +
//! `SAFETY:` rule audits it. Everything here is safe, bounds-checked, and
//! Miri-clean. Units compare by value, so every code point — including
//! lone surrogates, which have no UTF-8 form — matches exactly as Python
//! `in` does.

/// Threshold above which a table is kept sorted for binary search.
/// Small charsets (the common case: whitespace, delimiters) scan faster
/// linearly with no allocation beyond the build.
pub const SORT_THRESHOLD: usize = 32;

/// Width of one CPython unicode unit in bytes (`PyUnicode_KIND` 1/2/4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    One,
    Two,
    Four,
}

impl UnitKind {
    /// Width in bytes. `None` for impossible kinds (never produced by
    /// CPython, rejected at the boundary rather than here).
    pub fn from_kind(kind: u32) -> Option<Self> {
        match kind {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            4 => Some(Self::Four),
            _ => None,
        }
    }

    #[inline]
    pub fn width(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
        }
    }
}

/// Borrowed unicode units: `len` code points of `kind` width over `bytes`.
///
/// The binding guarantees `bytes.len() >= len * kind.width()` at
/// construction (the only `unsafe` of the port); decoding below is total
/// for in-bounds indices, and scans never read out of bounds.
#[derive(Debug, Clone, Copy)]
pub struct RawUnits<'a> {
    bytes: &'a [u8],
    kind: UnitKind,
    len: usize,
}

impl<'a> RawUnits<'a> {
    pub fn new(bytes: &'a [u8], kind: UnitKind, len: usize) -> Option<Self> {
        if bytes.len() >= len.saturating_mul(kind.width()) {
            Some(Self { bytes, kind, len })
        } else {
            None
        }
    }

    /// Number of code points.
    pub fn len(&self) -> usize {
        self.len
    }

    /// No code points (empty text).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Value of unit `idx` as a `u32` code point. Total for `idx < len`
    /// (callers uphold this; scans clamp by construction).
    #[inline]
    pub fn unit_at(&self, idx: usize) -> u32 {
        let o = idx * self.kind.width();
        match self.kind {
            UnitKind::One => self.bytes[o] as u32,
            UnitKind::Two => u16::from_ne_bytes([self.bytes[o], self.bytes[o + 1]]) as u32,
            UnitKind::Four => u32::from_ne_bytes([
                self.bytes[o],
                self.bytes[o + 1],
                self.bytes[o + 2],
                self.bytes[o + 3],
            ]),
        }
    }
}

/// Membership table of single-char values (`u32` code points).
///
/// Multi-char or empty charset members are dropped at construction: under
/// Python `in`, a one-character string can never equal them, so they match
/// nothing. Construction sorts + dedups tables larger than
/// [`SORT_THRESHOLD`].
#[derive(Debug, Clone, Default)]
pub struct ScanTable {
    values: Vec<u32>,
    sorted: bool,
}

impl ScanTable {
    /// Build from an iterator of candidate code points (`None` = skipped
    /// member, e.g. a multi-char string).
    pub fn from_members(members: impl IntoIterator<Item = Option<u32>>) -> Self {
        let mut values: Vec<u32> = members.into_iter().flatten().collect();
        let sorted = values.len() > SORT_THRESHOLD;
        if sorted {
            values.sort_unstable();
            values.dedup();
        }
        Self { values, sorted }
    }

    /// Build directly from code points (same normalization as
    /// [`from_members`](Self::from_members)).
    pub fn from_values(values: &[u32]) -> Self {
        Self::from_members(values.iter().copied().map(Some))
    }

    #[inline]
    pub fn contains(&self, v: u32) -> bool {
        if self.sorted {
            self.values.binary_search(&v).is_ok()
        } else {
            self.values.contains(&v)
        }
    }

    /// Number of tabled values (post-normalization).
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// No values tabled: `scan_while_in` stops immediately,
    /// `scan_while_not_in` runs to `end` (matches Python `in` on an
    /// all-unmatchable charset).
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[inline]
fn scan(
    units: &RawUnits,
    start: usize,
    end: usize,
    table: &ScanTable,
    want_in: bool,
) -> usize {
    let start = start.min(units.len());
    let end = end.min(units.len());
    let mut i = start.min(end);
    while i < end {
        if table.contains(units.unit_at(i)) != want_in {
            break;
        }
        i += 1;
    }
    i
}

/// First index in `[start, end)` whose unit is NOT tabled, or `end`.
/// Mirrors the `White` remainder loop
/// (`while loc < maxloc and instring[loc] in matchWhite`).
pub fn scan_while_in(
    units: &RawUnits,
    start: usize,
    end: usize,
    table: &ScanTable,
) -> usize {
    scan(units, start, end, table, true)
}

/// First index in `[start, end)` whose unit IS tabled, or `end`.
/// Mirrors the `CharsNotIn` remainder loop
/// (`while loc < maxlen and instring[loc] not in notchars`).
pub fn scan_while_not_in(
    units: &RawUnits,
    start: usize,
    end: usize,
    table: &ScanTable,
) -> usize {
    scan(units, start, end, table, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(chars: &str) -> ScanTable {
        ScanTable::from_members(chars.chars().map(|c| Some(c as u32)))
    }

    /// Units of `text` laid out at `width` bytes (1 = ASCII/Latin-1,
    /// 2 = BMP/UCS-2 incl. surrogates, 4 = astral/UCS-4).
    fn layout(text: &str, width: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for c in text.chars() {
            let u = c as u32;
            match width {
                1 => out.push(u as u8),
                2 => out.extend_from_slice(&(u as u16).to_ne_bytes()),
                4 => out.extend_from_slice(&u.to_ne_bytes()),
                _ => panic!("width"),
            }
        }
        out
    }

    fn run(text: &str, width: usize, start: usize, end: usize, t: &ScanTable, want_in: bool) -> usize {
        let buf = layout(text, width);
        let kind = UnitKind::from_kind(width as u32).unwrap();
        let units = RawUnits::new(&buf, kind, text.chars().count()).unwrap();
        if want_in {
            scan_while_in(&units, start, end, t)
        } else {
            scan_while_not_in(&units, start, end, t)
        }
    }

    #[test]
    fn units_reject_short_buffers() {
        assert!(RawUnits::new(&[0u8; 3], UnitKind::One, 4).is_none());
        assert!(RawUnits::new(&[0u8; 8], UnitKind::Four, 2).is_some());
        assert!(UnitKind::from_kind(8).is_none());
    }

    #[test]
    fn in_scan_stops_at_first_outsider() {
        let t = table(" \t\n");
        for width in [1, 2, 4] {
            assert_eq!(run("   \tx", width, 0, 5, &t, true), 4);
            assert_eq!(run("   \tx", width, 4, 5, &t, false), 5);
        }
    }

    #[test]
    fn not_in_scan_stops_at_first_member() {
        let t = table(";");
        for width in [1, 2, 4] {
            assert_eq!(run("ab;cd", width, 1, 5, &t, false), 2);
            assert_eq!(run("ab;cd", width, 0, 5, &t, true), 0);
        }
    }

    #[test]
    fn empty_table_matches_nothing() {
        let t = ScanTable::from_values(&[]);
        assert!(t.is_empty());
        assert_eq!(run("abc", 1, 0, 3, &t, true), 0);
        assert_eq!(run("abc", 1, 0, 3, &t, false), 3);
    }

    #[test]
    fn bounds_clamp_defensively() {
        let t = table("ab");
        assert_eq!(run("ab", 1, 0, 99, &t, true), 2);
        assert_eq!(run("ab", 1, 99, 99, &t, true), 2);
        assert_eq!(run("ab", 1, 5, 1, &t, true), 1);
    }

    #[test]
    fn big_table_sorts_and_matches() {
        let vals: Vec<u32> = (0..100u32).collect();
        let t = ScanTable::from_values(&vals);
        assert_eq!(t.len(), 100);
        let buf: Vec<u8> = [7u32, 50, 150].iter().flat_map(|u| u.to_ne_bytes()).collect();
        let units = RawUnits::new(&buf, UnitKind::Four, 3).unwrap();
        assert_eq!(scan_while_in(&units, 0, 3, &t), 2);
        assert_eq!(scan_while_not_in(&units, 0, 3, &t), 0);
    }

    #[test]
    fn wide_units_compare_by_value() {
        // Surrogate + astral values survive the u32 round trip.
        let t = ScanTable::from_values(&[0xD800, 0x1F600]);
        let buf: Vec<u8> = [0x61u32, 0xD800, 0x62].iter().flat_map(|u| (*u as u16).to_ne_bytes()).collect();
        let units = RawUnits::new(&buf, UnitKind::Two, 3).unwrap();
        assert_eq!(scan_while_not_in(&units, 0, 3, &t), 1);
        let buf2: Vec<u8> = [0x1F600u32, 0x61].iter().flat_map(|u| u.to_ne_bytes()).collect();
        let units2 = RawUnits::new(&buf2, UnitKind::Four, 2).unwrap();
        assert_eq!(scan_while_in(&units2, 0, 2, &t), 1);
    }
}
