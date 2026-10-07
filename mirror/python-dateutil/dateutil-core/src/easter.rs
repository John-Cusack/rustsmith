// SPDX-License-Identifier: Apache-2.0
//! Easter computation, mirroring `dateutil.easter.easter` (GM Arts /
//! Tondering algorithm). Only the Western method is used by the engine
//! (`byeaster`); all three are ported for completeness.

pub const EASTER_JULIAN: i64 = 1;
pub const EASTER_ORTHODOX: i64 = 2;
pub const EASTER_WESTERN: i64 = 3;

/// (year, month, day) of Easter for `year` by `method`. `None` on a bad method.
pub fn easter(year: i64, method: i64) -> Option<(i64, u8, u8)> {
    if !(1 <= method && method <= 3) {
        return None;
    }
    let y = year;
    let g = y % 19;
    let mut e = 0;
    let (i, j) = if method < 3 {
        let i = (19 * g + 15) % 30;
        let j = (y + y.div_euclid(4) + i) % 7;
        if method == 2 {
            e = 10;
            if y > 1600 {
                e += y / 100 - 16 - (y / 100 - 16) / 4;
            }
        }
        (i, j)
    } else {
        let c = y / 100;
        let h = (c - c / 4 - (8 * c + 13) / 25 + 19 * g + 15) % 30;
        let i = h - (h / 28) * (1 - (h / 28) * (29 / (h + 1)) * ((21 - g) / 11));
        let j = (y + y / 4 + i + 2 - c + c / 4) % 7;
        (i, j)
    };
    let p = i - j + e;
    let d = 1 + (p + 27 + (p + 6).div_euclid(40)) % 31;
    let m = 3 + (p + 26) / 30;
    Some((y, m as u8, d as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn western_easter_pins() {
        // Values from dateutil's own test suite.
        assert_eq!(easter(1997, EASTER_WESTERN), Some((1997, 3, 30)));
        assert_eq!(easter(1998, EASTER_WESTERN), Some((1998, 4, 12)));
        assert_eq!(easter(1999, EASTER_WESTERN), Some((1999, 4, 4)));
        assert_eq!(easter(2020, EASTER_WESTERN), Some((2020, 4, 12)));
        assert_eq!(easter(2021, EASTER_WESTERN), Some((2021, 4, 4)));
        assert_eq!(easter(2000, EASTER_JULIAN), Some((2000, 4, 17)));
        assert_eq!(easter(2000, EASTER_ORTHODOX), Some((2000, 4, 30)));
        assert_eq!(easter(2020, 0), None);
    }
}
