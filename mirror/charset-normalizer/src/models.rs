//! Pure-Rust port of `charset_normalizer/models.py` (charset-normalizer 3.5.1).
//!
//! Mirrors `CharsetMatch`, `CharsetMatches` and `CliDetectionResult`
//! branch-for-branch. Detection logic lives elsewhere; this module is the
//! data model only (structs + ordering + re-encode + JSON shape).
//!
//! # Wiring (owned by the integrator)
//! The crate root must declare the sibling modules; this file expects them
//! as crate-root modules with these exact paths/signatures:
//! - `super::decoders::decode_strict(name: &str, data: &[u8]) -> Result<String, ()>`
//!   (DecodersPort; `Err(())` = strict-decode or unknown-name failure).
//! - `super::cd::{encoding_languages, mb_encoding_languages}(iana: &str) -> Vec<String>`
//!   (CdPort).
//! - `super::utils::unicode_range(c: char) -> Option<&'static str>` (UtilsPort).
//! - `super::tables_constant::{TOO_BIG_SEQUENCE, RE_ENCODING_INDICATION,
//!   IANA_NO_ALIASES, is_multi_byte_encoding}` (frozen tables).
//! - `super::tables_codecs::decode_1b(name: &str, byte: u8) -> Option<char>`
//!   (frozen tables).
//! - `regex = "1"` dependency (orig uses `re` only for the encoding
//!   declaration hint in `output()`; `constant.py:803-806`).
//!
//! # Fingerprint choice (why cross-implementation equality is unaffected)
//! Orig `models.py:253-258` uses `hash(str(self))` (CPython string hash,
//! randomized per process). Fingerprints are only ever compared *within* one
//! implementation: `CharsetMatch.__eq__` (`models.py:40-49`) and the
//! `CharsetMatches.append` dedup (`models.py:313-317`). They are never
//! persisted, never sent across the Python/Rust boundary, and never compared
//! against the other implementation's values. Any deterministic hash therefore
//! preserves every observable behavior; this port uses 64-bit FNV-1a over the
//! decoded UTF-8 bytes. FNV-1a (not `std::collections::hash_map::DefaultHasher`)
//! is deliberately chosen because `DefaultHasher`/SipHash uses per-process
//! random keys, which would violate this crate's determinism requirement.
//!
//! # Known risks / deliberate deviations
//! 1. `Ord` mirrors `__lt__` (`models.py:51-71`) exactly, including its
//!    threshold branches and the asymmetric `len(self.payload)` check. That
//!    ordering is intransitive across threshold boundaries (same as the
//!    original); Rust's stable sort and Python's Timsort may therefore order
//!    pathological near-tie triplets differently. Decisive orderings agree.
//! 2. Rust `Ord`/`PartialEq`/`fingerprint`/`multi_byte_usage`/`alphabets` are
//!    total (cannot fail), while the original raises `UnicodeDecodeError`
//!    when strict decoding fails. Matches built by detection always decode,
//!    so this path is unreachable in practice; where a failure would raise,
//!    these total contexts fall back to lossy-decoded text (documented at
//!    `text_or_lossy`). Fallible entry points (`decoded_str`, `output`)
//!    propagate `ModelError` like the original raises.
//! 3. `output()` re-encode targets: `utf_8`, `ascii`, `iso8859_1`/`latin_1`,
//!    `utf_16(_be|_le)`, `utf_32(_be|_le)` and every single-byte codec present
//!    in `tables_codecs.rs` (via first-wins reverse lookup, `?` for
//!    unmappable chars, mirroring `errors="replace"`) are exact. Multibyte
//!    targets without Rust-side tables (e.g. `shift_jis`, `gb2312`, `utf_7`)
//!    return `ModelError::CannotEncode`; the original would succeed. The
//!    default `utf_8` path is unaffected.
//! 4. Single-byte re-encode uses first-wins inversion of the frozen *decode*
//!    tables; CPython's *encode* tables differ in rare duplicate-mapping
//!    cases (a different byte for the same char may be chosen).
//! 5. `round(x, 3)` in `percent_*` uses exact-decimal half-even rounding to
//!    match CPython (Rust's `f64::round` is half-away-from-zero and would
//!    diverge on ties). See `round_half_even_3`.
//! 6. `CharsetMatches` sorts eagerly on insert instead of lazily via an
//!    `_is_sorted` flag (`models.py:267-274`); the observable element order
//!    after any sequence of operations is identical, only the sort timing
//!    differs (unobservable).
//! 7. `output()` tags the output encoding before decoding, exactly like the
//!    original (`models.py:229-230` sets `_output_encoding` before `str()`),
//!    so error-path cache state matches too.

use std::cmp::Ordering;
use std::fmt;
use std::sync::LazyLock;

use super::cd::{encoding_languages, mb_encoding_languages};
use super::decoders::decode_strict;
use super::tables_codecs::decode_1b;
use super::tables_constant::{
    IANA_NO_ALIASES, RE_ENCODING_INDICATION, TOO_BIG_SEQUENCE, is_multi_byte_encoding,
};
use super::utils::unicode_range;

/// Frozen alias map `alias -> canonical`, verbatim from CPython 3.12.3 `Lib/encodings/aliases.py` (326 entries).
/// Insertion order == aliases.py order (mirrors `aliases.items()`
/// iteration in `models.py:121`). `models.py:3` imports this table
/// directly from the stdlib, so it lives here rather than in
/// `tables_constant.rs` (which ports `constant.py`, not the stdlib).
const ENCODING_ALIASES: &[(&str, &str)] = &[
    ("646", "ascii"),
    ("ansi_x3.4_1968", "ascii"),
    ("ansi_x3_4_1968", "ascii"),
    ("ansi_x3.4_1986", "ascii"),
    ("cp367", "ascii"),
    ("csascii", "ascii"),
    ("ibm367", "ascii"),
    ("iso646_us", "ascii"),
    ("iso_646.irv_1991", "ascii"),
    ("iso_ir_6", "ascii"),
    ("us", "ascii"),
    ("us_ascii", "ascii"),
    ("base64", "base64_codec"),
    ("base_64", "base64_codec"),
    ("big5_tw", "big5"),
    ("csbig5", "big5"),
    ("big5_hkscs", "big5hkscs"),
    ("hkscs", "big5hkscs"),
    ("bz2", "bz2_codec"),
    ("037", "cp037"),
    ("csibm037", "cp037"),
    ("ebcdic_cp_ca", "cp037"),
    ("ebcdic_cp_nl", "cp037"),
    ("ebcdic_cp_us", "cp037"),
    ("ebcdic_cp_wt", "cp037"),
    ("ibm037", "cp037"),
    ("ibm039", "cp037"),
    ("1026", "cp1026"),
    ("csibm1026", "cp1026"),
    ("ibm1026", "cp1026"),
    ("1125", "cp1125"),
    ("ibm1125", "cp1125"),
    ("cp866u", "cp1125"),
    ("ruscii", "cp1125"),
    ("1140", "cp1140"),
    ("ibm1140", "cp1140"),
    ("1250", "cp1250"),
    ("windows_1250", "cp1250"),
    ("1251", "cp1251"),
    ("windows_1251", "cp1251"),
    ("1252", "cp1252"),
    ("windows_1252", "cp1252"),
    ("1253", "cp1253"),
    ("windows_1253", "cp1253"),
    ("1254", "cp1254"),
    ("windows_1254", "cp1254"),
    ("1255", "cp1255"),
    ("windows_1255", "cp1255"),
    ("1256", "cp1256"),
    ("windows_1256", "cp1256"),
    ("1257", "cp1257"),
    ("windows_1257", "cp1257"),
    ("1258", "cp1258"),
    ("windows_1258", "cp1258"),
    ("273", "cp273"),
    ("ibm273", "cp273"),
    ("csibm273", "cp273"),
    ("424", "cp424"),
    ("csibm424", "cp424"),
    ("ebcdic_cp_he", "cp424"),
    ("ibm424", "cp424"),
    ("437", "cp437"),
    ("cspc8codepage437", "cp437"),
    ("ibm437", "cp437"),
    ("500", "cp500"),
    ("csibm500", "cp500"),
    ("ebcdic_cp_be", "cp500"),
    ("ebcdic_cp_ch", "cp500"),
    ("ibm500", "cp500"),
    ("775", "cp775"),
    ("cspc775baltic", "cp775"),
    ("ibm775", "cp775"),
    ("850", "cp850"),
    ("cspc850multilingual", "cp850"),
    ("ibm850", "cp850"),
    ("852", "cp852"),
    ("cspcp852", "cp852"),
    ("ibm852", "cp852"),
    ("855", "cp855"),
    ("csibm855", "cp855"),
    ("ibm855", "cp855"),
    ("857", "cp857"),
    ("csibm857", "cp857"),
    ("ibm857", "cp857"),
    ("858", "cp858"),
    ("csibm858", "cp858"),
    ("ibm858", "cp858"),
    ("860", "cp860"),
    ("csibm860", "cp860"),
    ("ibm860", "cp860"),
    ("861", "cp861"),
    ("cp_is", "cp861"),
    ("csibm861", "cp861"),
    ("ibm861", "cp861"),
    ("862", "cp862"),
    ("cspc862latinhebrew", "cp862"),
    ("ibm862", "cp862"),
    ("863", "cp863"),
    ("csibm863", "cp863"),
    ("ibm863", "cp863"),
    ("864", "cp864"),
    ("csibm864", "cp864"),
    ("ibm864", "cp864"),
    ("865", "cp865"),
    ("csibm865", "cp865"),
    ("ibm865", "cp865"),
    ("866", "cp866"),
    ("csibm866", "cp866"),
    ("ibm866", "cp866"),
    ("869", "cp869"),
    ("cp_gr", "cp869"),
    ("csibm869", "cp869"),
    ("ibm869", "cp869"),
    ("932", "cp932"),
    ("ms932", "cp932"),
    ("mskanji", "cp932"),
    ("ms_kanji", "cp932"),
    ("949", "cp949"),
    ("ms949", "cp949"),
    ("uhc", "cp949"),
    ("950", "cp950"),
    ("ms950", "cp950"),
    ("jisx0213", "euc_jis_2004"),
    ("eucjis2004", "euc_jis_2004"),
    ("euc_jis2004", "euc_jis_2004"),
    ("eucjisx0213", "euc_jisx0213"),
    ("eucjp", "euc_jp"),
    ("ujis", "euc_jp"),
    ("u_jis", "euc_jp"),
    ("euckr", "euc_kr"),
    ("korean", "euc_kr"),
    ("ksc5601", "euc_kr"),
    ("ks_c_5601", "euc_kr"),
    ("ks_c_5601_1987", "euc_kr"),
    ("ksx1001", "euc_kr"),
    ("ks_x_1001", "euc_kr"),
    ("gb18030_2000", "gb18030"),
    ("chinese", "gb2312"),
    ("csiso58gb231280", "gb2312"),
    ("euc_cn", "gb2312"),
    ("euccn", "gb2312"),
    ("eucgb2312_cn", "gb2312"),
    ("gb2312_1980", "gb2312"),
    ("gb2312_80", "gb2312"),
    ("iso_ir_58", "gb2312"),
    ("936", "gbk"),
    ("cp936", "gbk"),
    ("ms936", "gbk"),
    ("hex", "hex_codec"),
    ("roman8", "hp_roman8"),
    ("r8", "hp_roman8"),
    ("csHPRoman8", "hp_roman8"),
    ("cp1051", "hp_roman8"),
    ("ibm1051", "hp_roman8"),
    ("hzgb", "hz"),
    ("hz_gb", "hz"),
    ("hz_gb_2312", "hz"),
    ("csiso2022jp", "iso2022_jp"),
    ("iso2022jp", "iso2022_jp"),
    ("iso_2022_jp", "iso2022_jp"),
    ("iso2022jp_1", "iso2022_jp_1"),
    ("iso_2022_jp_1", "iso2022_jp_1"),
    ("iso2022jp_2", "iso2022_jp_2"),
    ("iso_2022_jp_2", "iso2022_jp_2"),
    ("iso_2022_jp_2004", "iso2022_jp_2004"),
    ("iso2022jp_2004", "iso2022_jp_2004"),
    ("iso2022jp_3", "iso2022_jp_3"),
    ("iso_2022_jp_3", "iso2022_jp_3"),
    ("iso2022jp_ext", "iso2022_jp_ext"),
    ("iso_2022_jp_ext", "iso2022_jp_ext"),
    ("csiso2022kr", "iso2022_kr"),
    ("iso2022kr", "iso2022_kr"),
    ("iso_2022_kr", "iso2022_kr"),
    ("csisolatin6", "iso8859_10"),
    ("iso_8859_10", "iso8859_10"),
    ("iso_8859_10_1992", "iso8859_10"),
    ("iso_ir_157", "iso8859_10"),
    ("l6", "iso8859_10"),
    ("latin6", "iso8859_10"),
    ("thai", "iso8859_11"),
    ("iso_8859_11", "iso8859_11"),
    ("iso_8859_11_2001", "iso8859_11"),
    ("iso_8859_13", "iso8859_13"),
    ("l7", "iso8859_13"),
    ("latin7", "iso8859_13"),
    ("iso_8859_14", "iso8859_14"),
    ("iso_8859_14_1998", "iso8859_14"),
    ("iso_celtic", "iso8859_14"),
    ("iso_ir_199", "iso8859_14"),
    ("l8", "iso8859_14"),
    ("latin8", "iso8859_14"),
    ("iso_8859_15", "iso8859_15"),
    ("l9", "iso8859_15"),
    ("latin9", "iso8859_15"),
    ("iso_8859_16", "iso8859_16"),
    ("iso_8859_16_2001", "iso8859_16"),
    ("iso_ir_226", "iso8859_16"),
    ("l10", "iso8859_16"),
    ("latin10", "iso8859_16"),
    ("csisolatin2", "iso8859_2"),
    ("iso_8859_2", "iso8859_2"),
    ("iso_8859_2_1987", "iso8859_2"),
    ("iso_ir_101", "iso8859_2"),
    ("l2", "iso8859_2"),
    ("latin2", "iso8859_2"),
    ("csisolatin3", "iso8859_3"),
    ("iso_8859_3", "iso8859_3"),
    ("iso_8859_3_1988", "iso8859_3"),
    ("iso_ir_109", "iso8859_3"),
    ("l3", "iso8859_3"),
    ("latin3", "iso8859_3"),
    ("csisolatin4", "iso8859_4"),
    ("iso_8859_4", "iso8859_4"),
    ("iso_8859_4_1988", "iso8859_4"),
    ("iso_ir_110", "iso8859_4"),
    ("l4", "iso8859_4"),
    ("latin4", "iso8859_4"),
    ("csisolatincyrillic", "iso8859_5"),
    ("cyrillic", "iso8859_5"),
    ("iso_8859_5", "iso8859_5"),
    ("iso_8859_5_1988", "iso8859_5"),
    ("iso_ir_144", "iso8859_5"),
    ("arabic", "iso8859_6"),
    ("asmo_708", "iso8859_6"),
    ("csisolatinarabic", "iso8859_6"),
    ("ecma_114", "iso8859_6"),
    ("iso_8859_6", "iso8859_6"),
    ("iso_8859_6_1987", "iso8859_6"),
    ("iso_ir_127", "iso8859_6"),
    ("csisolatingreek", "iso8859_7"),
    ("ecma_118", "iso8859_7"),
    ("elot_928", "iso8859_7"),
    ("greek", "iso8859_7"),
    ("greek8", "iso8859_7"),
    ("iso_8859_7", "iso8859_7"),
    ("iso_8859_7_1987", "iso8859_7"),
    ("iso_ir_126", "iso8859_7"),
    ("csisolatinhebrew", "iso8859_8"),
    ("hebrew", "iso8859_8"),
    ("iso_8859_8", "iso8859_8"),
    ("iso_8859_8_1988", "iso8859_8"),
    ("iso_ir_138", "iso8859_8"),
    ("csisolatin5", "iso8859_9"),
    ("iso_8859_9", "iso8859_9"),
    ("iso_8859_9_1989", "iso8859_9"),
    ("iso_ir_148", "iso8859_9"),
    ("l5", "iso8859_9"),
    ("latin5", "iso8859_9"),
    ("cp1361", "johab"),
    ("ms1361", "johab"),
    ("cskoi8r", "koi8_r"),
    ("kz_1048", "kz1048"),
    ("rk1048", "kz1048"),
    ("strk1048_2002", "kz1048"),
    ("8859", "latin_1"),
    ("cp819", "latin_1"),
    ("csisolatin1", "latin_1"),
    ("ibm819", "latin_1"),
    ("iso8859", "latin_1"),
    ("iso8859_1", "latin_1"),
    ("iso_8859_1", "latin_1"),
    ("iso_8859_1_1987", "latin_1"),
    ("iso_ir_100", "latin_1"),
    ("l1", "latin_1"),
    ("latin", "latin_1"),
    ("latin1", "latin_1"),
    ("maccyrillic", "mac_cyrillic"),
    ("macgreek", "mac_greek"),
    ("maciceland", "mac_iceland"),
    ("maccentraleurope", "mac_latin2"),
    ("mac_centeuro", "mac_latin2"),
    ("maclatin2", "mac_latin2"),
    ("macintosh", "mac_roman"),
    ("macroman", "mac_roman"),
    ("macturkish", "mac_turkish"),
    ("ansi", "mbcs"),
    ("dbcs", "mbcs"),
    ("csptcp154", "ptcp154"),
    ("pt154", "ptcp154"),
    ("cp154", "ptcp154"),
    ("cyrillic_asian", "ptcp154"),
    ("quopri", "quopri_codec"),
    ("quoted_printable", "quopri_codec"),
    ("quotedprintable", "quopri_codec"),
    ("rot13", "rot_13"),
    ("csshiftjis", "shift_jis"),
    ("shiftjis", "shift_jis"),
    ("sjis", "shift_jis"),
    ("s_jis", "shift_jis"),
    ("shiftjis2004", "shift_jis_2004"),
    ("sjis_2004", "shift_jis_2004"),
    ("s_jis_2004", "shift_jis_2004"),
    ("shiftjisx0213", "shift_jisx0213"),
    ("sjisx0213", "shift_jisx0213"),
    ("s_jisx0213", "shift_jisx0213"),
    ("tis620", "tis_620"),
    ("tis_620_0", "tis_620"),
    ("tis_620_2529_0", "tis_620"),
    ("tis_620_2529_1", "tis_620"),
    ("iso_ir_166", "tis_620"),
    ("u16", "utf_16"),
    ("utf16", "utf_16"),
    ("unicodebigunmarked", "utf_16_be"),
    ("utf_16be", "utf_16_be"),
    ("unicodelittleunmarked", "utf_16_le"),
    ("utf_16le", "utf_16_le"),
    ("u32", "utf_32"),
    ("utf32", "utf_32"),
    ("utf_32be", "utf_32_be"),
    ("utf_32le", "utf_32_le"),
    ("u7", "utf_7"),
    ("utf7", "utf_7"),
    ("unicode_1_1_utf_7", "utf_7"),
    ("u8", "utf_8"),
    ("utf", "utf_8"),
    ("utf8", "utf_8"),
    ("utf8_ucs2", "utf_8"),
    ("utf8_ucs4", "utf_8"),
    ("cp65001", "utf_8"),
    ("uu", "uu_codec"),
    ("zip", "zlib_codec"),
    ("zlib", "zlib_codec"),
    ("x_mac_japanese", "shift_jis"),
    ("x_mac_korean", "euc_kr"),
    ("x_mac_simp_chinese", "gb2312"),
    ("x_mac_trad_chinese", "big5"),
];

/// Errors mirroring the exceptions `models.py` lets escape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModelError {
    /// Mirrors `ValueError` from strict `iana_name` (`utils.py:308-309`) and
    /// `LookupError` from `str.encode` with an unknown target encoding.
    UnknownEncoding(String),
    /// Mirrors `UnicodeDecodeError` from strict `str(payload, encoding)`.
    DecodeFailed(String),
    /// Mirrors `ValueError` from `add_submatch` (`models.py:101-106`).
    SelfSubmatch,
    /// No encoder available for a known encoding (multibyte targets without
    /// Rust-side tables). The original succeeds here; see header risk 3.
    CannotEncode(String),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::UnknownEncoding(name) => {
                write!(f, "Unable to retrieve IANA for '{name}'")
            }
            ModelError::DecodeFailed(name) => {
                write!(f, "Failed to decode bytes as '{name}'")
            }
            ModelError::SelfSubmatch => write!(
                f,
                "Unable to add instance <CharsetMatch> as a submatch of a CharsetMatch"
            ),
            ModelError::CannotEncode(name) => {
                write!(f, "No encoder available for '{name}'")
            }
        }
    }
}

impl std::error::Error for ModelError {}

/// Orig `utils.py:302`: lowercase + dash-to-underscore normalization.
fn normalize_name(name: &str) -> String {
    name.to_lowercase().replace('-', "_")
}

/// Orig `_IANA_NAMES.get` (`constant.py:2492-2496`):
/// `{**aliases, **{v: v for v in aliases.values()}, **{n: n for n in IANA_NO_ALIASES}}`.
/// The local `encodings.aliases` copy has no key/value collisions (verified
/// at generation time), so lookup order is irrelevant.
fn iana_lookup(normalized: &str) -> Option<String> {
    if let Some((_, canonical)) = ENCODING_ALIASES
        .iter()
        .find(|(alias, _)| *alias == normalized)
    {
        return Some((*canonical).to_string());
    }
    if ENCODING_ALIASES
        .iter()
        .any(|(_, canonical)| *canonical == normalized)
        || IANA_NO_ALIASES.contains(&normalized)
    {
        return Some(normalized.to_string());
    }
    None
}

/// Orig `utils.py:300-311` `iana_name(name, strict=False)`: lossy echo of the
/// normalized input when unknown (keeps `__eq__`/`__getitem__` total).
fn iana_name_lossy(name: &str) -> String {
    let normalized = normalize_name(name);
    iana_lookup(&normalized).unwrap_or(normalized)
}

/// Orig `utils.py:300-311` `iana_name(name, strict=True)`.
fn iana_name_strict(name: &str) -> Result<String, ModelError> {
    let normalized = normalize_name(name);
    iana_lookup(&normalized).ok_or_else(|| ModelError::UnknownEncoding(name.to_string()))
}

/// FNV-1a 64 over bytes. See header: any deterministic hash works because
/// fingerprints are only compared intra-implementation.
fn fingerprint_of(text: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

/// Exact port of CPython `round(value, 3)` (round-half-even on the exact
/// decimal expansion of the binary value). Rust `f64::round` is
/// half-away-from-zero and diverges on ties (e.g. `round(0.125, 3)` stays
/// `0.125`, but `round(2.5)`-style ties at the 3rd decimal would differ).
/// Used by `percent_chaos`/`percent_coherence` (`models.py:182-188`).
fn round_half_even_3(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let negative = value.is_sign_negative();
    // Exact decimal expansion ("int.frac"); 1074 fractional digits suffice
    // for every finite f64.
    let expanded = format!("{:.1074}", value.abs());
    let (int_part, frac_part) = match expanded.find('.') {
        Some(i) => (&expanded[..i], &expanded[i + 1..]),
        None => (expanded.as_str(), ""),
    };
    let frac: Vec<u8> = frac_part.bytes().collect();
    let mut kept: [u8; 3] = [b'0', b'0', b'0'];
    for (i, slot) in kept.iter_mut().enumerate() {
        if i < frac.len() {
            *slot = frac[i];
        }
    }
    let rest = if frac.len() > 3 { &frac[3..] } else { &[] };
    let head = rest.first().copied().unwrap_or(b'0');
    let stained = rest.iter().any(|&c| c != b'0');
    let round_up = if head < b'5' {
        false
    } else if head > b'5' || stained {
        true
    } else {
        // Exact tie: half to even.
        (kept[2] - b'0') % 2 == 1
    };
    let mut int_digits: Vec<u8> = int_part.bytes().collect();
    if round_up {
        let mut carry = true;
        for slot in kept.iter_mut().rev() {
            if carry {
                if *slot == b'9' {
                    *slot = b'0';
                } else {
                    *slot += 1;
                    carry = false;
                }
            }
        }
        if carry {
            let mut carry = true;
            for digit in int_digits.iter_mut().rev() {
                if carry {
                    if *digit == b'9' {
                        *digit = b'0';
                    } else {
                        *digit += 1;
                        carry = false;
                    }
                }
            }
            if carry {
                int_digits.insert(0, b'1');
            }
        }
    }
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&int_digits).unwrap_or("0"));
    out.push('.');
    out.push_str(std::str::from_utf8(&kept).unwrap_or("000"));
    out.parse::<f64>().unwrap_or(value)
}

/// CPython `repr(float)` (shortest round-trip, exponent iff the decimal
/// exponent of the leading digit is `< -4` or `>= 16`). Needed so `to_json`
/// prints `chaos`/`coherence` exactly like `json.dumps`.
fn py_float_repr(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let negative = value.is_sign_negative();
    // Rust `{:.e}` without precision already yields shortest significant digits.
    let sci = format!("{:e}", value.abs());
    let epos = sci.find('e').unwrap_or(sci.len());
    let (mantissa, exp_str) = (&sci[..epos], &sci[epos + 1..]);
    let exp: i32 = exp_str.parse().unwrap_or(0);
    let digits: Vec<u8> = mantissa.bytes().filter(|&c| c != b'.').collect();
    // Value = 0.digits * 10^point.
    let point = exp + 1;
    let ndigits = digits.len() as i32;
    let mut body = String::new();
    if point < -3 || point >= 17 {
        body.push(digits[0] as char);
        if ndigits > 1 {
            body.push('.');
            body.push_str(std::str::from_utf8(&digits[1..]).unwrap_or(""));
        }
        body.push('e');
        body.push_str(&format!("{:+03}", point - 1));
    } else if point <= 0 {
        body.push_str("0.");
        for _ in 0..-point {
            body.push('0');
        }
        body.push_str(std::str::from_utf8(&digits).unwrap_or(""));
    } else if point >= ndigits {
        body.push_str(std::str::from_utf8(&digits).unwrap_or(""));
        for _ in 0..(point - ndigits) {
            body.push('0');
        }
        body.push_str(".0");
    } else {
        body.push_str(std::str::from_utf8(&digits[..point as usize]).unwrap_or(""));
        body.push('.');
        body.push_str(std::str::from_utf8(&digits[point as usize..]).unwrap_or(""));
    }
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

/// `json.dumps(..., ensure_ascii=True)` string escaping: keeps only
/// printable ASCII verbatim, short escapes where Python uses them, else
/// lowercase `\uXXXX` (surrogate pairs for astral chars).
fn json_escape_into(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let n = c as u32;
                if n <= 0xffff {
                    out.push_str(&format!("\\u{n:04x}"));
                } else {
                    let v = n - 0x10000;
                    out.push_str(&format!(
                        "\\u{:04x}\\u{:04x}",
                        0xd800 + (v >> 10),
                        0xdc00 + (v & 0x3ff)
                    ));
                }
            }
            c => out.push(c),
        }
    }
}

/// Orig `models.py:11-38`.
#[derive(Debug, Clone)]
pub(crate) struct CharsetMatch {
    pub payload: Vec<u8>,
    pub encoding: String,
    pub chaos: f64,
    /// Legacy mirror of `_mean_coherence_ratio` (`models.py:31`): set from
    /// `languages` at construction; the `coherence()` method (like the
    /// original `coherence` property, `models.py:176-180`) derives from
    /// `languages` and is the source of truth.
    pub coherence: f64,
    pub bom: bool,
    pub languages: Vec<(String, f64)>,
    /// Lazy strict-decoded cache (`_string`, preset via `decoded_payload`).
    pub decoded: Option<String>,
    pub preemptive: Option<String>,
    leaves: Vec<CharsetMatch>,
    unicode_ranges: Option<Vec<String>>,
    out_encoding: Option<String>,
    out_payload: Option<Vec<u8>>,
}

impl CharsetMatch {
    /// Orig `CharsetMatch.__init__` (`models.py:12-38`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        payload: Vec<u8>,
        encoding: String,
        chaos: f64,
        bom: bool,
        languages: Vec<(String, f64)>,
        decoded: Option<String>,
        preemptive: Option<String>,
    ) -> Self {
        // Like the `coherence` property, derive from languages (0.0 when none).
        let coherence = languages.first().map(|(_, ratio)| *ratio).unwrap_or(0.0);
        Self {
            payload,
            encoding,
            chaos,
            coherence,
            bom,
            languages,
            decoded,
            preemptive,
            leaves: Vec::new(),
            unicode_ranges: None,
            out_encoding: None,
            out_payload: None,
        }
    }

    /// Strict decode with the UTF-7 BOM strip (`models.py:81-95` shared core,
    /// without caching). `decode_strict` mirrors `str(payload, encoding,
    /// "strict")`; `Err(())` covers both `UnicodeDecodeError` and
    /// `LookupError`.
    fn strict_text(&self) -> Result<String, ModelError> {
        let mut text = decode_strict(&self.encoding, &self.payload)
            .map_err(|_| ModelError::DecodeFailed(self.encoding.clone()))?;
        // Orig `models.py:88-94`: UTF-7 BOM arrives as a decoded char (raw
        // byte stripping is unreliable across the Base64 boundary).
        if self.bom && self.encoding == "utf_7" && text.starts_with('\u{feff}') {
            text.drain(..'\u{feff}'.len_utf8());
        }
        Ok(text)
    }

    /// Total-context fallback (see header risk 2): preset cache, else strict
    /// decode, else lossy decode. Unreachable for detection-built matches.
    fn text_or_lossy(&self) -> String {
        if let Some(text) = self.decoded.as_ref() {
            return text.clone();
        }
        self.strict_text()
            .unwrap_or_else(|_| String::from_utf8_lossy(&self.payload).into_owned())
    }

    /// Orig `__str__` (`models.py:81-95`): lazy strict decode + cache.
    pub(crate) fn decoded_str(&mut self) -> Result<&str, ModelError> {
        if self.decoded.is_none() {
            let text = self.strict_text()?;
            self.decoded = Some(text);
        }
        Ok(self.decoded.as_deref().unwrap_or(""))
    }

    /// Orig `fingerprint` (`models.py:253-258`).
    pub(crate) fn fingerprint(&self) -> u64 {
        fingerprint_of(&self.text_or_lossy())
    }

    /// Orig `__eq__` str branch (`models.py:42-47`): lossy IANA comparison.
    pub(crate) fn eq_str(&self, other: &str) -> bool {
        iana_name_lossy(other) == self.encoding
    }

    /// Orig `multi_byte_usage` (`models.py:73-79`).
    pub(crate) fn multi_byte_usage(&self) -> f64 {
        let raw_len = self.payload.len();
        if raw_len == 0 {
            return 0.0;
        }
        1.0 - (self.text_or_lossy().chars().count() as f64 / raw_len as f64)
    }

    /// Orig `add_submatch` (`models.py:100-109`), including the RAM-saving
    /// cache unload (`other._string = None`).
    pub(crate) fn add_submatch(&mut self, other: CharsetMatch) -> Result<(), ModelError> {
        if other == *self {
            return Err(ModelError::SelfSubmatch);
        }
        let mut other = other;
        other.decoded = None;
        self.leaves.push(other);
        Ok(())
    }

    /// Orig `encoding` (`models.py:111-113`).
    pub(crate) fn encoding(&self) -> &str {
        &self.encoding
    }

    /// Orig `encoding_aliases` (`models.py:115-126`): both-ways scan of the
    /// alias table in table order.
    pub(crate) fn encoding_aliases(&self) -> Vec<String> {
        let mut also_known_as = Vec::new();
        for (alias, canonical) in ENCODING_ALIASES {
            if self.encoding == *alias {
                also_known_as.push((*canonical).to_string());
            } else if self.encoding == *canonical {
                also_known_as.push((*alias).to_string());
            }
        }
        also_known_as
    }

    /// Orig `bom` (`models.py:128-130`).
    pub(crate) fn bom(&self) -> bool {
        self.bom
    }

    /// Orig `byte_order_mark` (`models.py:132-134`).
    pub(crate) fn byte_order_mark(&self) -> bool {
        self.bom
    }

    /// Orig `languages` (`models.py:136-142`).
    pub(crate) fn languages_names(&self) -> Vec<String> {
        self.languages.iter().map(|(name, _)| name.clone()).collect()
    }

    /// Orig `language` (`models.py:144-170`), fallback chain included.
    pub(crate) fn language(&self) -> String {
        if let Some((name, _)) = self.languages.first() {
            return name.clone();
        }
        if self
            .could_be_from_charset()
            .iter()
            .any(|encoding| encoding == "ascii")
        {
            return "English".to_string();
        }
        let languages = if is_multi_byte_encoding(&self.encoding) {
            mb_encoding_languages(&self.encoding)
        } else {
            encoding_languages(&self.encoding)
        };
        if languages.is_empty() || languages.iter().any(|l| l == "Latin Based") {
            return "Unknown".to_string();
        }
        languages[0].clone()
    }

    /// Orig `chaos` (`models.py:172-174`).
    pub(crate) fn chaos(&self) -> f64 {
        self.chaos
    }

    /// Orig `coherence` (`models.py:176-180`).
    pub(crate) fn coherence(&self) -> f64 {
        self.languages.first().map(|(_, ratio)| *ratio).unwrap_or(0.0)
    }

    /// Orig `percent_chaos` (`models.py:182-184`).
    pub(crate) fn percent_chaos(&self) -> f64 {
        round_half_even_3(self.chaos * 100.0)
    }

    /// Orig `percent_coherence` (`models.py:186-188`).
    pub(crate) fn percent_coherence(&self) -> f64 {
        round_half_even_3(self.coherence() * 100.0)
    }

    /// Orig `raw` (`models.py:190-195`).
    pub(crate) fn raw(&self) -> &[u8] {
        &self.payload
    }

    /// Orig `submatch` (`models.py:197-199`).
    pub(crate) fn submatch(&self) -> &[CharsetMatch] {
        &self.leaves
    }

    /// Orig `has_submatch` (`models.py:201-203`).
    pub(crate) fn has_submatch(&self) -> bool {
        !self.leaves.is_empty()
    }

    /// Orig `alphabets` (`models.py:205-213`): sorted unique detected ranges.
    pub(crate) fn alphabets(&mut self) -> &[String] {
        if self.unicode_ranges.is_none() {
            let text = self.text_or_lossy();
            let mut detected: Vec<String> = Vec::new();
            for c in text.chars() {
                if let Some(range) = unicode_range(c) {
                    if !detected.iter().any(|r| r == range) {
                        detected.push(range.to_string());
                    }
                }
            }
            detected.sort();
            self.unicode_ranges = Some(detected);
        }
        self.unicode_ranges.as_deref().unwrap_or(&[])
    }

    /// Orig `could_be_from_charset` (`models.py:215-223`).
    pub(crate) fn could_be_from_charset(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(1 + self.leaves.len());
        out.push(self.encoding.clone());
        out.extend(self.leaves.iter().map(|m| m.encoding.clone()));
        out
    }

    /// Orig `output` (`models.py:224-251`): re-encode with `errors="replace"`,
    /// patching a preemptive encoding declaration outside UTF-8 first.
    pub(crate) fn output(&mut self, target: &str) -> Result<Vec<u8>, ModelError> {
        if self.out_encoding.as_deref() != Some(target) {
            // Mirrors `models.py:229-230`: tag first, decode after.
            self.out_encoding = Some(target.to_string());
            let decoded = self.decoded_str()?.to_owned();
            let iana_target = iana_name_strict(target)?;
            let mut text = decoded;
            if let Some(preemptive) = self.preemptive.clone() {
                let lowered = preemptive.to_lowercase();
                if lowered != "utf-8" && lowered != "utf8" && lowered != "utf_8" {
                    text = patch_encoding_declaration(&text, &iana_target);
                }
            }
            self.out_payload = Some(encode_replace(&iana_target, &text)?);
        }
        Ok(self.out_payload.clone().unwrap_or_default())
    }
}

/// Orig `__eq__` (`models.py:40-49`): encoding + fingerprint.
impl PartialEq for CharsetMatch {
    fn eq(&self, other: &Self) -> bool {
        self.encoding == other.encoding && self.fingerprint() == other.fingerprint()
    }
}

impl Eq for CharsetMatch {}

/// Orig `__repr__` (`models.py:97-98`).
impl fmt::Display for CharsetMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<CharsetMatch '{}' fp({})>",
            self.encoding,
            self.fingerprint()
        )
    }
}

fn order_f64_asc(left: f64, right: f64) -> Ordering {
    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
}

fn order_f64_desc(left: f64, right: f64) -> Ordering {
    order_f64_asc(right, left)
}

/// Orig `__lt__` (`models.py:51-71`), branches in Python order (see header
/// risk 1 for the intransitivity caveat).
impl Ord for CharsetMatch {
    fn cmp(&self, other: &Self) -> Ordering {
        let chaos_difference = (self.chaos - other.chaos).abs();
        let coherence_difference = (self.coherence() - other.coherence()).abs();

        // Below 0.5% chaos difference --> use coherence (descending).
        if chaos_difference < 0.005 && coherence_difference > 0.02 {
            return order_f64_desc(self.coherence(), other.coherence());
        } else if chaos_difference < 0.005 && coherence_difference <= 0.02 {
            // Difficult decision: most multi-byte usage wins; preserve RAM
            // usage on huge payloads by falling back to chaos.
            if self.payload.len() >= TOO_BIG_SEQUENCE {
                return order_f64_asc(self.chaos, other.chaos);
            }
            return order_f64_desc(self.multi_byte_usage(), other.multi_byte_usage());
        }

        order_f64_asc(self.chaos, other.chaos)
    }
}

impl PartialOrd for CharsetMatch {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

static ENCODING_DECLARATION_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(&format!("(?i){RE_ENCODING_INDICATION}"))
        .expect("frozen encoding-indication pattern")
});

fn encoding_declaration_re() -> &'static regex::Regex {
    &ENCODING_DECLARATION_RE
}

/// Orig `models.py:237-248`: rewrite the encoding declaration in the first
/// 8192 *characters* with the dash-style IANA name of the output encoding.
fn patch_encoding_declaration(text: &str, iana_target: &str) -> String {
    let cut = text
        .char_indices()
        .nth(8192)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    let (head, tail) = text.split_at(cut);
    let replacement = iana_target.replace('_', "-");
    let mut patched_head = head.to_string();
    if let Some(caps) = encoding_declaration_re().captures(head) {
        let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        let declared = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let new_span = full.replace(declared, &replacement);
        let m = caps.get(0).expect("captures implies group 0");
        let mut rebuilt = String::with_capacity(head.len() + replacement.len());
        rebuilt.push_str(&head[..m.start()]);
        rebuilt.push_str(&new_span);
        rebuilt.push_str(&head[m.end()..]);
        patched_head = rebuilt;
    }
    patched_head + tail
}

/// Mirrors `text.encode(target, "replace")` for the targets named in header
/// risk 3. Unmappable chars become `b'?'`.
fn encode_replace(iana_target: &str, text: &str) -> Result<Vec<u8>, ModelError> {
    match iana_target {
        name if name == "utf_8" => Ok(text.as_bytes().to_vec()),
        name if name == "ascii" => Ok(text
            .chars()
            .map(|c| {
                if (c as u32) < 0x80 {
                    c as u8
                } else {
                    b'?'
                }
            })
            .collect()),
        // `latin_1` normalizes to `iso8859_1`: identity byte mapping.
        name if name == "iso8859_1" => Ok(text
            .chars()
            .map(|c| {
                let n = c as u32;
                if n <= 0xff {
                    n as u8
                } else {
                    b'?'
                }
            })
            .collect()),
        name if name == "utf_16" => {
            let mut out = vec![0xff, 0xfe];
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(out)
        }
        name if name == "utf_16_le" => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(out)
        }
        name if name == "utf_16_be" => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_be_bytes());
            }
            Ok(out)
        }
        name if name == "utf_32" => {
            let mut out = vec![0xff, 0xfe, 0x00, 0x00];
            for c in text.chars() {
                out.extend_from_slice(&(c as u32).to_le_bytes());
            }
            Ok(out)
        }
        name if name == "utf_32_le" => {
            let mut out = Vec::with_capacity(text.len() * 4);
            for c in text.chars() {
                out.extend_from_slice(&(c as u32).to_le_bytes());
            }
            Ok(out)
        }
        name if name == "utf_32_be" => {
            let mut out = Vec::with_capacity(text.len() * 4);
            for c in text.chars() {
                out.extend_from_slice(&(c as u32).to_be_bytes());
            }
            Ok(out)
        }
        name => {
            // Single-byte codec via first-wins inversion of the frozen decode
            // table. Probe two universally mapped bytes first so unknown or
            // multibyte names fail here instead of emitting all-`?`.
            if decode_1b(name, 0x20).is_none() && decode_1b(name, 0x41).is_none() {
                return Err(ModelError::CannotEncode(name.to_string()));
            }
            let mut out = Vec::with_capacity(text.len());
            for c in text.chars() {
                let mut byte = b'?';
                for b in 0u8..=255u8 {
                    if decode_1b(name, b) == Some(c) {
                        byte = b;
                        break;
                    }
                }
                out.push(byte);
            }
            Ok(out)
        }
    }
}

/// Orig `CharsetMatches` (`models.py:261-334`).
#[derive(Debug, Clone, Default)]
pub(crate) struct CharsetMatches {
    pub matches: Vec<CharsetMatch>,
}

impl CharsetMatches {
    /// Orig `__init__` (`models.py:267-269`): eagerly stable-sorted.
    pub(crate) fn new(results: Option<Vec<CharsetMatch>>) -> Self {
        let mut matches = results.unwrap_or_default();
        matches.sort();
        Self { matches }
    }

    /// Orig `__iter__` (`models.py:276-278`).
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, CharsetMatch> {
        self.matches.iter()
    }

    /// Orig `__getitem__` int branch (`models.py:285-287`).
    pub(crate) fn get(&self, index: usize) -> Option<&CharsetMatch> {
        self.matches.get(index)
    }

    /// Orig `__getitem__` str branch (`models.py:288-293`): lossy IANA
    /// normalize, first match whose `could_be_from_charset` contains it.
    pub(crate) fn get_by_encoding(&self, name: &str) -> Option<&CharsetMatch> {
        let wanted = iana_name_lossy(name);
        self.matches.iter().find(|m| {
            m.could_be_from_charset()
                .iter()
                .any(|encoding| *encoding == wanted)
        })
    }

    /// Orig `__len__` (`models.py:295-296`).
    pub(crate) fn len(&self) -> usize {
        self.matches.len()
    }

    /// Orig `__bool__` (`models.py:298-299`).
    pub(crate) fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Orig `append` (`models.py:301-319`): dedup into submatches on
    /// fingerprint + chaos equality (small payloads only), else insert
    /// sorted. Returns `Err` only where the original raises (`add_submatch`
    /// on an equal item).
    pub(crate) fn append(&mut self, item: CharsetMatch) -> Result<(), ModelError> {
        // Orig `models.py:313`: submatch factoring disabled for huge inputs.
        #[allow(clippy::float_cmp)]
        if item.payload.len() < TOO_BIG_SEQUENCE {
            if let Some(index) = self
                .matches
                .iter()
                .position(|m| m.fingerprint() == item.fingerprint() && m.chaos == item.chaos)
            {
                self.matches[index].add_submatch(item)?;
                return Ok(());
            }
        }
        self.matches.push(item);
        self.matches.sort();
        Ok(())
    }

    /// Orig `best` (`models.py:321-328`).
    pub(crate) fn best(&self) -> Option<&CharsetMatch> {
        self.matches.first()
    }

    /// Orig `first` (`models.py:330-334`): kept for BC, calls `best()`.
    pub(crate) fn first(&self) -> Option<&CharsetMatch> {
        self.best()
    }
}

impl<'a> IntoIterator for &'a CharsetMatches {
    type Item = &'a CharsetMatch;
    type IntoIter = std::slice::Iter<'a, CharsetMatch>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Orig `CoherenceMatch`/`CoherenceMatches` (`models.py:337-338`).
pub(crate) type CoherenceMatch = (String, f64);
/// Orig `CoherenceMatch`/`CoherenceMatches` (`models.py:337-338`).
pub(crate) type CoherenceMatches = Vec<CoherenceMatch>;

/// Orig `CliDetectionResult.__init__` (`models.py:341-366`).
#[derive(Debug, Clone)]
pub(crate) struct CliDetectionResult {
    pub path: String,
    pub unicode_path: Option<String>,
    pub encoding: Option<String>,
    pub encoding_aliases: Vec<String>,
    pub alternative_encodings: Vec<String>,
    pub language: String,
    pub alphabets: Vec<String>,
    pub has_sig_or_bom: bool,
    pub chaos: f64,
    pub coherence: f64,
    pub is_preferred: bool,
}

impl CliDetectionResult {
    /// Orig `CliDetectionResult.__init__` (`models.py:342-366`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        path: String,
        encoding: Option<String>,
        encoding_aliases: Vec<String>,
        alternative_encodings: Vec<String>,
        language: String,
        alphabets: Vec<String>,
        has_sig_or_bom: bool,
        chaos: f64,
        coherence: f64,
        unicode_path: Option<String>,
        is_preferred: bool,
    ) -> Self {
        Self {
            path,
            unicode_path,
            encoding,
            encoding_aliases,
            alternative_encodings,
            language,
            alphabets,
            has_sig_or_bom,
            chaos,
            coherence,
            is_preferred,
        }
    }

    fn push_key(out: &mut String, first: &mut bool, key: &str) {
        if !*first {
            out.push(',');
        }
        *first = false;
        out.push('\n');
        out.push_str("    \"");
        out.push_str(key);
        out.push_str("\": ");
    }

    fn push_json_str(out: &mut String, text: &str) {
        out.push('"');
        json_escape_into(out, text);
        out.push('"');
    }

    fn push_json_str_list(out: &mut String, items: &[String]) {
        if items.is_empty() {
            out.push_str("[]");
            return;
        }
        out.push('[');
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('\n');
            out.push_str("        ");
            Self::push_json_str(out, item);
        }
        out.push('\n');
        out.push_str("    ]");
    }

    /// Orig `to_json` (`models.py:384-387`): `json.dumps(self.__dict__,
    /// ensure_ascii=True, indent=4)` with the `__dict__` key order from
    /// `models.py:368-382`.
    pub(crate) fn to_json(&self) -> String {
        let mut out = String::from("{");
        let mut first = true;
        Self::push_key(&mut out, &mut first, "path");
        Self::push_json_str(&mut out, &self.path);
        Self::push_key(&mut out, &mut first, "encoding");
        match self.encoding.as_ref() {
            Some(encoding) => Self::push_json_str(&mut out, encoding),
            None => out.push_str("null"),
        }
        Self::push_key(&mut out, &mut first, "encoding_aliases");
        Self::push_json_str_list(&mut out, &self.encoding_aliases);
        Self::push_key(&mut out, &mut first, "alternative_encodings");
        Self::push_json_str_list(&mut out, &self.alternative_encodings);
        Self::push_key(&mut out, &mut first, "language");
        Self::push_json_str(&mut out, &self.language);
        Self::push_key(&mut out, &mut first, "alphabets");
        Self::push_json_str_list(&mut out, &self.alphabets);
        Self::push_key(&mut out, &mut first, "has_sig_or_bom");
        out.push_str(if self.has_sig_or_bom { "true" } else { "false" });
        Self::push_key(&mut out, &mut first, "chaos");
        out.push_str(&py_float_repr(self.chaos));
        Self::push_key(&mut out, &mut first, "coherence");
        out.push_str(&py_float_repr(self.coherence));
        Self::push_key(&mut out, &mut first, "unicode_path");
        match self.unicode_path.as_ref() {
            Some(path) => Self::push_json_str(&mut out, path),
            None => out.push_str("null"),
        }
        Self::push_key(&mut out, &mut first, "is_preferred");
        out.push_str(if self.is_preferred { "true" } else { "false" });
        out.push_str("\n}");
        out
    }
}
