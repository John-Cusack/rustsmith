// SPDX-License-Identifier: Apache-2.0
//! Exact decimal arithmetic for the numeric-token path (`Decimal` minus
//! everything the parser never uses: only truncation, `% 1` truthiness
//! and `int(60 * frac)`).

use std::cmp::Ordering;

use super::lex::fold_digits;

/// Non-negative decimal: normalized int digits + raw fraction digits.
#[derive(Clone, Debug)]
pub struct Dec {
    int: String,
    frac: String,
    pub frac_nonzero: bool,
}

impl Dec {
    /// Parse what `_to_decimal` accepts (digit runs with an optional dot;
    /// `inf`/`nan` yield `None`, mirroring the finite check).
    pub fn parse(s: &str) -> Option<Dec> {
        let folded = fold_digits(s)?;
        if folded.is_empty()
            || !folded
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'.')
            || folded.bytes().filter(|&b| b == b'.').count() > 1
        {
            return None;
        }
        let mut it = folded.split('.');
        let int_raw = it.next().unwrap_or("");
        let frac_raw = it.next().unwrap_or("");
        let int = {
            let t = int_raw.trim_start_matches('0');
            if t.is_empty() {
                "0".to_string()
            } else {
                t.to_string()
            }
        };
        let frac_nonzero = frac_raw.bytes().any(|b| b != b'0');
        Some(Dec {
            int,
            frac: frac_raw.to_string(),
            frac_nonzero,
        })
    }

    pub fn int_part(&self) -> &str {
        &self.int
    }

    /// Exact comparison against a small int (non-negative values only).
    pub fn cmp_u32(&self, n: u32) -> Ordering {
        let ns = n.to_string();
        match self.int.len().cmp(&ns.len()) {
            Ordering::Equal => match self.int.cmp(&ns) {
                Ordering::Equal => {
                    if self.frac_nonzero {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                }
                o => o,
            },
            o => o,
        }
    }

    /// `int(60 * (value % 1))`, exact: only the first 38 fraction digits
    /// can influence `int()` (an integer sits strictly between the
    /// truncated and true product only for fractions terminating within
    /// two digits, which truncation preserves exactly).
    pub fn frac60(&self) -> u32 {
        let mut d: u128 = 0;
        let mut n: u32 = 0;
        for b in self.frac.bytes().take(38) {
            d = d * 10 + (b - b'0') as u128;
            n += 1;
        }
        if n == 0 {
            return 0;
        }
        ((60 * d) / 10u128.pow(n)) as u32
    }
}

/// `int(val)` on a raw token: fold Unicode digits, require all digits,
/// strip leading zeros. `None` mirrors the `ValueError`.
pub fn norm_int_str(val: &str) -> Option<String> {
    let folded = fold_digits(val)?;
    if folded.is_empty() || !folded.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let t = folded.trim_start_matches('0');
    if t.is_empty() {
        Some("0".to_string())
    } else {
        Some(t.to_string())
    }
}
