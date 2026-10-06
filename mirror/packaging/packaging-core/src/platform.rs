//! Pure-Rust core: glibc/musl version-string parsing for platform tags.
//!
//! Stage-1 mirror of the pure helpers in `packaging/_manylinux.py` and
//! `packaging/_musllinux.py`. Environment reads (confstr, ctypes, subprocess)
//! stay on the binding side so test doubles keep working.
//!
//! License: Apache-2.0 OR BSD-2-Clause (preserved from the original).

/// Parsed `major.minor` from a glibc version string, mirroring
/// `_parse_glibc_version`: leading ASCII `[0-9]+\.[0-9]+` match; `None` when
/// absent (the caller emits the `RuntimeWarning` and returns `(-1, -1)`).
pub fn parse_glibc_version(s: &str) -> Option<(i64, i64)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i >= b.len() || b[i] != b'.' {
        return None;
    }
    let major = &s[..i];
    let mut j = i + 1;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    if j == i + 1 {
        return None;
    }
    let minor = &s[i + 1..j];
    Some((major.parse().unwrap_or(-1), minor.parse().unwrap_or(-1)))
}

/// Parse musl loader output, mirroring `_parse_musl_version`: at least two
/// non-blank lines, first starting with `musl`, second matching
/// `Version (\d+)\.(\d+)` at its start.
pub fn parse_musl_version(output: &str) -> Option<(i64, i64)> {
    let mut lines = output
        .lines()
        .map(|l: &str| l.trim())
        .filter(|l: &&str| !l.is_empty());
    let first = lines.next()?;
    if first.get(..4) != Some("musl") {
        return None;
    }
    let second = lines.next()?;
    // `re.match(r"Version (\d+)\.(\d+)", ...)`: prefix match, trailing junk
    // after the minor digits is ignored.
    let rest = second.strip_prefix("Version ")?;
    let dot = rest.find('.')?;
    let (major, minor) = rest.split_at(dot);
    let minor: String = minor[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
    if major.is_empty() || !major.bytes().all(|b: u8| b.is_ascii_digit()) {
        return None;
    }
    if minor.is_empty() {
        return None;
    }
    Some((
        major.parse().unwrap_or(-1),
        minor.parse().unwrap_or(-1),
    ))
}
