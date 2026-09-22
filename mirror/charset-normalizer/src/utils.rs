//! Pure-Rust port of `charset_normalizer/utils.py` (charset-normalizer 3.5.1).
//!
//! Every function below mirrors its Python original branch-for-branch, including
//! early-exits, thresholds and error paths. `orig file:line` references point at
//! `~/runs/charset-orig/src/charset_normalizer/utils.py` unless noted otherwise.
//!
//! Unicode truth comes from the frozen tables, NEVER from `char` methods or
//! `unicodedata`:
//! - name-based flags (`_character_flags`, utils.py:39) via
//!   `tables_ucd::ucd_lookup` (flags field carries the exact `_LATIN..` bits from
//!   `constant.py:2423-2435`);
//! - category / space / printable / case-variable via `tables_ucd::ucd_lookup` +
//!   `CATEGORIES` (CPython 3.12 `str`-method semantics baked in by the generator);
//! - accent stripping via `tables_ucd::decomp_first` (`UCD_DECOMP`; `-2` sentinel
//!   = tagged/compatibility decomposition, mirrors the `ValueError` Python raises
//!   in `remove_accent`, utils.py:85-92);
//! - ranges via `tables_constant::UNICODE_RANGES` (verified byte-identical and
//!   pre-sorted vs `UNICODE_RANGES_COMBINED`; binary search replicates
//!   `bisect_right`, utils.py:116).
//!
//! # Name risks / known divergences (explicit)
//! 1. `iana_name` / `any_specified_encoding` need `_IANA_NAMES`
//!    (`constant.py:2492-2496`: `encodings.aliases` + canonical identities +
//!    `IANA_NO_ALIASES`), which is absent from `tables_constant.rs`. It is
//!    embedded here as `IANA_ALIASES` (432 rows, snapshot of CPython 3.12
//!    `encodings.aliases`, keys verbatim — including the single non-lowercase
//!    key `csHPRoman8`, so a lowercased lookup misses exactly as in Python).
//!    Regenerate from `encodings.aliases` if the interpreter version changes.
//! 2. `is_multi_byte_encoding` delegates to the frozen
//!    `tables_constant::is_multi_byte_encoding` match (33 arms: the frozen 32
//!    plus `utf_8_sig` from Python `_KNOWN_MB_DECODERS`,
//!    `constant.py:2467-2479`). The `_codecs_*` provider probing (utils.py:269-274)
//!    is folded into the same frozen match; Main proved exhaustiveness over
//!    IANA99 + all aliases (exactly the 33 MB-true names), so the delegate is
//!    exact and no `lru_cache` equivalent is needed.
//! 3. `str`-method subtleties: `is_separator`/`is_unprintable`/`is_case_variable`
//!    MUST use the UCD bits, not `char::is_whitespace`/`is_alphabetic`/etc.
//!    (`'\x1c'.isspace()` is `True` in Python but `'\x1c'.is_whitespace()` is
//!    `false` in Rust; `'\u{feff}'.isspace()` is `False` while Rust says
//!    whitespace). Category checks use `.contains('P'/'S'/'N'/'Z')` literally,
//!    mirroring `"P" in unicodedata.category(c)` (utils.py:132/146/172).
//! 4. Decoding lives in `super::decoders` (DecodersPort owns it). `decode_ignore`
//!    / `decode_strict` below are thin forwarders; `cp_similarity` and
//!    `cut_sequence_chunks` depend on them.
//! 5. `any_specified_encoding` is the ONLY item using the `regex` crate (the
//!    original uses `re`; `RE_POSSIBLE_ENCODING_INDICATION` is reused verbatim
//!    with case-insensitivity applied at compile time, mirroring `IGNORECASE`).
//! 7. No unsafe blocks, no network, deterministic; std (+ `regex`) only.
use std::sync::LazyLock;
// ---------------------------------------------------------------------------
// Flag bits, verbatim values from constant.py:2423-2435.
// The UCD generator packed these same bits into `UCD_RUNS[i].2`.
// ---------------------------------------------------------------------------

/// `constant.py:2423` (`_LATIN`).
pub(crate) const FLAG_LATIN: u32 = 1;
/// `constant.py:2424` (`_ACCENTUATED`).
pub(crate) const FLAG_ACCENTUATED: u32 = 1 << 1;
/// `constant.py:2425` (`_CJK`).
pub(crate) const FLAG_CJK: u32 = 1 << 2;
/// `constant.py:2426` (`_HANGUL`).
pub(crate) const FLAG_HANGUL: u32 = 1 << 3;
/// `constant.py:2427` (`_KATAKANA`).
pub(crate) const FLAG_KATAKANA: u32 = 1 << 4;
/// `constant.py:2428` (`_HIRAGANA`).
pub(crate) const FLAG_HIRAGANA: u32 = 1 << 5;
/// `constant.py:2429` (`_THAI`).
pub(crate) const FLAG_THAI: u32 = 1 << 6;
/// `constant.py:2430` (`_ARABIC`).
pub(crate) const FLAG_ARABIC: u32 = 1 << 7;
/// `constant.py:2431` (`_ARABIC_ISOLATED_FORM`).
pub(crate) const FLAG_ARABIC_ISOLATED_FORM: u32 = 1 << 8;
/// `constant.py:2432` (`_HALFWIDTH_KATAKANA`).
pub(crate) const FLAG_HALFWIDTH_KATAKANA: u32 = 1 << 9;
/// `constant.py:2433` (`_LIGATURE`).
pub(crate) const FLAG_LIGATURE: u32 = 1 << 10;
/// `constant.py:2434` (`_SUPERSCRIPT`).
pub(crate) const FLAG_SUPERSCRIPT: u32 = 1 << 11;
/// `constant.py:2435` (`_SENTENCE_OPEN_PUNCTUATION`).
pub(crate) const FLAG_SENTENCE_OPEN_PUNCTUATION: u32 = 1 << 12;

/// Mirror of `_character_flags` (utils.py:39-78).
///
/// Python calls `unicodedata.name()` once and substring-matches the name;
/// the frozen `UCD_RUNS` flags field carries exactly those bits, so this is a
/// single table lookup. Unnamed codepoints (Python raises `ValueError` -> `0`)
/// fall out of every run and yield `0` from `ucd_lookup`'s fallback too.
pub(crate) fn character_flags(c: char) -> u32 {
    crate::tables_ucd::ucd_lookup(c as u32).0 as u32
}

/// Two-letter General_Category for `c` via `CATEGORIES` (utils.py:130/144/170).
fn ucd_category(c: char) -> &'static str {
    let (_, index, _, _, _) = crate::tables_ucd::ucd_lookup(c as u32);
    crate::tables_ucd::CATEGORIES[index as usize]
}

/// `(isspace, isprintable, case_variable)` for `c`.
///
/// These are CPython `str.isspace()` / `str.isprintable()` /
/// `islower() != isupper()` semantics frozen by the table generator — see
/// header risk 3 for why `char` methods MUST NOT be used instead.
fn ucd_bits(c: char) -> (bool, bool, bool) {
    let (_, _, space, printable, casevar) = crate::tables_ucd::ucd_lookup(c as u32);
    (space, printable, casevar)
}

/// Mirror of `is_accentuated` (utils.py:81-82).
pub(crate) fn is_accentuated(c: char) -> bool {
    character_flags(c) & FLAG_ACCENTUATED != 0
}

/// Mirror of `remove_accent` (utils.py:85-92).
///
/// Python splits `unicodedata.decomposition(c)` on spaces and takes the first
/// code; a leading `<tag>` (compat/noBreak/super/...) makes `int()` raise
/// `ValueError`. `UCD_DECOMP` stores the first codepoint directly and `-2` for
/// tagged decompositions (verified: the only negative value present), so:
/// - no entry (Python: empty decomposition) -> `Ok(c)` unchanged;
/// - `-2` -> `Err`, like Python's `ValueError` (e.g. NBSP, `ﬁ`, superscript);
/// - otherwise -> `Ok(first decomposition char)`.
pub(crate) fn remove_accent(c: char) -> Result<char, &'static str> {
    match crate::tables_ucd::decomp_first(c as u32) {
        None => Ok(c),
        Some(-2) => Err("tagged decomposition has no single base character"),
        Some(first) => {
            char::from_u32(first as u32).ok_or("decomposition head is not a codepoint")
        }
    }
}

/// Mirror of `unicode_range` (utils.py:104-122), incl. the `< 32` / `< 128`
/// fast paths and the `bisect_right(starts, ord) - 1` lookup (here
/// `partition_point`, equivalent on the sorted table).
pub(crate) fn unicode_range(c: char) -> Option<&'static str> {
    let ord = c as u32;
    if ord < 32 {
        return Some("Control character");
    }
    if ord < 128 {
        return Some("Basic Latin");
    }
    let ranges = crate::tables_constant::UNICODE_RANGES;
    let idx = ranges.partition_point(|entry| entry.0 <= ord);
    if idx == 0 {
        return None;
    }
    let (_, stop, name) = ranges[idx - 1];
    if ord < stop { Some(name) } else { None }
}

/// Mirror of `is_latin` (utils.py:125-126).
pub(crate) fn is_latin(c: char) -> bool {
    character_flags(c) & FLAG_LATIN != 0
}

/// Mirror of `is_punctuation` (utils.py:129-140).
///
/// EXACT rule: category containing `P` wins; otherwise the range name must
/// contain `"Punctuation"`. There is deliberately NO Supplement/secondary-range
/// carve-out here (unlike `md.py`'s mess table, which lives in `md.rs`).
pub(crate) fn is_punctuation(c: char) -> bool {
    let category = ucd_category(c);
    if category.contains('P') {
        return true;
    }
    match unicode_range(c) {
        None => false,
        Some(range) => range.contains("Punctuation"),
    }
}

/// Mirror of `is_symbol` (utils.py:143-154).
///
/// EXACT rule: category containing `S` or `N` wins; otherwise the range must
/// contain `"Forms"` AND the category must not be `Lo`.
pub(crate) fn is_symbol(c: char) -> bool {
    let category = ucd_category(c);
    if category.contains('S') || category.contains('N') {
        return true;
    }
    match unicode_range(c) {
        None => false,
        Some(range) => range.contains("Forms") && category != "Lo",
    }
}

/// Mirror of `is_emoticon` (utils.py:157-163).
pub(crate) fn is_emoticon(c: char) -> bool {
    match unicode_range(c) {
        None => false,
        Some(range) => range.contains("Emoticons") || range.contains("Pictographs"),
    }
}

/// Mirror of `is_separator` (utils.py:166-172).
///
/// `isspace` is the UCD bit (NOT `char::is_whitespace`; header risk 3).
/// The explicit set is FULLWIDTH VERTICAL LINE U+FF5C (`｜`), `+`, `<`, `>`
/// (their categories are `Sm`/`Sm`/`Sm`/`Sm`-family, so the category tail alone
/// would miss them). Tail: `"Z" in category` or `Po`/`Pd`/`Pc`.
pub(crate) fn is_separator(c: char) -> bool {
    if ucd_bits(c).0 || matches!(c, '｜' | '+' | '<' | '>') {
        return true;
    }
    let category = ucd_category(c);
    category.contains('Z') || matches!(category, "Po" | "Pd" | "Pc")
}

/// Mirror of `is_case_variable` (utils.py:175-176): `islower() != isupper()`.
pub(crate) fn is_case_variable(c: char) -> bool {
    ucd_bits(c).2
}

/// Mirror of `is_cjk` (utils.py:179-180).
pub(crate) fn is_cjk(c: char) -> bool {
    character_flags(c) & FLAG_CJK != 0
}

/// Mirror of `is_hiragana` (utils.py:183-184).
pub(crate) fn is_hiragana(c: char) -> bool {
    character_flags(c) & FLAG_HIRAGANA != 0
}

/// Mirror of `is_katakana` (utils.py:187-188).
pub(crate) fn is_katakana(c: char) -> bool {
    character_flags(c) & FLAG_KATAKANA != 0
}

/// Mirror of `is_hangul` (utils.py:191-192).
pub(crate) fn is_hangul(c: char) -> bool {
    character_flags(c) & FLAG_HANGUL != 0
}

/// Mirror of `is_thai` (utils.py:195-196).
pub(crate) fn is_thai(c: char) -> bool {
    character_flags(c) & FLAG_THAI != 0
}

/// Mirror of `is_arabic` (utils.py:199-200).
pub(crate) fn is_arabic(c: char) -> bool {
    character_flags(c) & FLAG_ARABIC != 0
}

/// Mirror of `is_arabic_isolated_form` (utils.py:203-204).
pub(crate) fn is_arabic_isolated_form(c: char) -> bool {
    character_flags(c) & FLAG_ARABIC_ISOLATED_FORM != 0
}

/// Mirror of `is_cjk_uncommon` (utils.py:207-208): membership in the combined
/// common-CJK set (`COMMON_CHINESE + COMMON_JAPANESE + COMMON_KOREAN`,
/// `constant.py:985-993`; frozen here as `COMMON_CJK`).
pub(crate) fn is_cjk_uncommon(c: char) -> bool {
    !crate::tables_constant::COMMON_CJK.contains(c)
}

/// Mirror of `is_unicode_range_secondary` (utils.py:211-212).
pub(crate) fn is_unicode_range_secondary(range_name: &str) -> bool {
    crate::tables_constant::SECONDARY_RANGE_NAMES
        .iter()
        .any(|name| *name == range_name)
}

/// Mirror of `is_unprintable` (utils.py:215-222).
///
/// `not isspace and not isprintable`, exempting SUB (`\x1a`, the ASCII
/// substitute character) and U+FEFF (CPython does not treat the zero-width
/// no-break space as a space, yet it must not count as unprintable mess).
pub(crate) fn is_unprintable(c: char) -> bool {
    let (isspace, isprintable, _) = ucd_bits(c);
    !isspace && !isprintable && c != '\u{1a}' && c != '\u{feff}'
}

// ---------------------------------------------------------------------------
// any_specified_encoding (utils.py:225-250)
// ---------------------------------------------------------------------------

/// Default `search_zone` for [`any_specified_encoding`] (utils.py:226).
pub(crate) const DEFAULT_SEARCH_ZONE: usize = 8192;

static ENCODING_INDICATION_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::RegexBuilder::new(crate::tables_constant::RE_ENCODING_INDICATION)
        .case_insensitive(true) // mirrors re.IGNORECASE (constant.py:803-806)
        .build()
        .expect("frozen RE_ENCODING_INDICATION must compile")
});

/// Mirror of `any_specified_encoding` (utils.py:225-250).
///
/// Scans at most `search_zone` leading bytes: the cheap ASCII-lowercased
/// literal pre-filter first (`b"coding"` also matches `encoding`, since
/// `"encoding"` contains `"coding"`), then the indication regex over the
/// ASCII-ignored decode. Returns the canonicalised IANA name of the FIRST
/// match that resolves via [`iana_lookup`], else `None`.
pub(crate) fn any_specified_encoding(
    sequence: &[u8],
    search_zone: usize,
) -> Option<&'static str> {
    let zone = &sequence[..sequence.len().min(search_zone)];
    // Cheap literal pre-filter (utils.py:237-240). `windows` on a short zone
    // yields nothing, matching `b"coding" not in b"" -> return None`.
    let lowered: Vec<u8> = zone.iter().map(|b| b.to_ascii_lowercase()).collect();
    if !lowered.windows(6).any(|w| w == b"coding")
        && !lowered.windows(7).any(|w| w == b"charset")
    {
        return None;
    }
    // `decode("ascii", errors="ignore")` (utils.py:242): drop non-ASCII bytes.
    let decoded: String = zone
        .iter()
        .filter(|b| **b < 128)
        .map(|b| *b as char)
        .collect();
    for caps in ENCODING_INDICATION_RE.captures_iter(&decoded) {
        // `match.group(1).lower().replace("-", "_")` (utils.py:245); the group
        // is ASCII-only by construction, so ASCII-lowercasing is exact.
        let specified = caps[1].to_ascii_lowercase().replace('-', "_");
        if let Some(iana) = iana_lookup(&specified) {
            return Some(iana);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// is_multi_byte_encoding / identify_sig_or_bom / should_strip_sig_or_bom
// (utils.py:253-297)
// ---------------------------------------------------------------------------

/// Mirror of `is_multi_byte_encoding` (utils.py:253-275).
///
/// Delegates to the frozen match in `tables_constant` (header risk 2),
/// which now covers `utf_8_sig` and is exhaustive per Main's probe.
pub(crate) fn is_multi_byte_encoding(name: &str) -> bool {
    crate::tables_constant::is_multi_byte_encoding(name)
}

/// SIG/BOM marks in `ENCODING_MARKS` dict order
/// (`constant.py:9-20`): `utf_8`, `utf_7`, `gb18030`, `utf_32`, `utf_16`.
///
/// Order is load-bearing, NOT longest-prefix sorting: `utf_32` is checked
/// before `utf_16`, so `FF FE 00 00` resolves to `utf_32` (LE) rather than
/// `utf_16` (LE). Byte values equal `codecs.BOM_UTF8/16/32_*` (verified
/// `efbbbf feff fffe 0000feff fffe0000`).
const SIG_MARKS: &[(&str, &[&[u8]])] = &[
    ("utf_8", &[&[0xEF, 0xBB, 0xBF]]),
    (
        "utf_7",
        &[
            &[0x2B, 0x2F, 0x76, 0x38],
            &[0x2B, 0x2F, 0x76, 0x39],
            &[0x2B, 0x2F, 0x76, 0x2B],
            &[0x2B, 0x2F, 0x76, 0x2F],
        ],
    ),
    ("gb18030", &[&[0x84, 0x31, 0x95, 0x33]]),
    ("utf_32", &[&[0x00, 0x00, 0xFE, 0xFF], &[0xFF, 0xFE, 0x00, 0x00]]),
    ("utf_16", &[&[0xFE, 0xFF], &[0xFF, 0xFE]]),
];

/// Mirror of `identify_sig_or_bom` (utils.py:278-293).
///
/// Returns the IANA name plus the matched mark, or `(None, b"")`.
pub(crate) fn identify_sig_or_bom(sequence: &[u8]) -> (Option<&'static str>, &'static [u8]) {
    for (iana_encoding, marks) in SIG_MARKS {
        for mark in *marks {
            if sequence.starts_with(mark) {
                return (Some(*iana_encoding), *mark);
            }
        }
    }
    (None, b"")
}

/// Mirror of `should_strip_sig_or_bom` (utils.py:296-297).
pub(crate) fn should_strip_sig_or_bom(iana_encoding: &str) -> bool {
    iana_encoding != "utf_16" && iana_encoding != "utf_32"
}

// ---------------------------------------------------------------------------
// iana_name (utils.py:300-311) + IANA_ALIASES
// ---------------------------------------------------------------------------

/// Frozen `_IANA_NAMES` (`constant.py:2492-2496`): `encodings.aliases`
/// (CPython 3.12 snapshot, 326 entries) overlaid with canonical-identity
/// entries and `IANA_NO_ALIASES` identities, sorted for binary search.
///
/// Keys are verbatim (note `csHPRoman8`: a lowercased input can therefore miss
/// exactly as in Python). Values are the canonical Python codec names, which
/// coincide with the IANA names used across this port.
const IANA_ALIASES: &[(&str, &str)] = &[
    ("037", "cp037"),
    ("1026", "cp1026"),
    ("1125", "cp1125"),
    ("1140", "cp1140"),
    ("1250", "cp1250"),
    ("1251", "cp1251"),
    ("1252", "cp1252"),
    ("1253", "cp1253"),
    ("1254", "cp1254"),
    ("1255", "cp1255"),
    ("1256", "cp1256"),
    ("1257", "cp1257"),
    ("1258", "cp1258"),
    ("273", "cp273"),
    ("424", "cp424"),
    ("437", "cp437"),
    ("500", "cp500"),
    ("646", "ascii"),
    ("775", "cp775"),
    ("850", "cp850"),
    ("852", "cp852"),
    ("855", "cp855"),
    ("857", "cp857"),
    ("858", "cp858"),
    ("860", "cp860"),
    ("861", "cp861"),
    ("862", "cp862"),
    ("863", "cp863"),
    ("864", "cp864"),
    ("865", "cp865"),
    ("866", "cp866"),
    ("869", "cp869"),
    ("8859", "latin_1"),
    ("932", "cp932"),
    ("936", "gbk"),
    ("949", "cp949"),
    ("950", "cp950"),
    ("ansi", "mbcs"),
    ("ansi_x3.4_1968", "ascii"),
    ("ansi_x3.4_1986", "ascii"),
    ("ansi_x3_4_1968", "ascii"),
    ("arabic", "iso8859_6"),
    ("ascii", "ascii"),
    ("asmo_708", "iso8859_6"),
    ("base64", "base64_codec"),
    ("base64_codec", "base64_codec"),
    ("base_64", "base64_codec"),
    ("big5", "big5"),
    ("big5_hkscs", "big5hkscs"),
    ("big5_tw", "big5"),
    ("big5hkscs", "big5hkscs"),
    ("bz2", "bz2_codec"),
    ("bz2_codec", "bz2_codec"),
    ("chinese", "gb2312"),
    ("cp037", "cp037"),
    ("cp1006", "cp1006"),
    ("cp1026", "cp1026"),
    ("cp1051", "hp_roman8"),
    ("cp1125", "cp1125"),
    ("cp1140", "cp1140"),
    ("cp1250", "cp1250"),
    ("cp1251", "cp1251"),
    ("cp1252", "cp1252"),
    ("cp1253", "cp1253"),
    ("cp1254", "cp1254"),
    ("cp1255", "cp1255"),
    ("cp1256", "cp1256"),
    ("cp1257", "cp1257"),
    ("cp1258", "cp1258"),
    ("cp1361", "johab"),
    ("cp154", "ptcp154"),
    ("cp273", "cp273"),
    ("cp367", "ascii"),
    ("cp424", "cp424"),
    ("cp437", "cp437"),
    ("cp500", "cp500"),
    ("cp65001", "utf_8"),
    ("cp720", "cp720"),
    ("cp737", "cp737"),
    ("cp775", "cp775"),
    ("cp819", "latin_1"),
    ("cp850", "cp850"),
    ("cp852", "cp852"),
    ("cp855", "cp855"),
    ("cp856", "cp856"),
    ("cp857", "cp857"),
    ("cp858", "cp858"),
    ("cp860", "cp860"),
    ("cp861", "cp861"),
    ("cp862", "cp862"),
    ("cp863", "cp863"),
    ("cp864", "cp864"),
    ("cp865", "cp865"),
    ("cp866", "cp866"),
    ("cp866u", "cp1125"),
    ("cp869", "cp869"),
    ("cp874", "cp874"),
    ("cp875", "cp875"),
    ("cp932", "cp932"),
    ("cp936", "gbk"),
    ("cp949", "cp949"),
    ("cp950", "cp950"),
    ("cp_gr", "cp869"),
    ("cp_is", "cp861"),
    ("csHPRoman8", "hp_roman8"),
    ("csascii", "ascii"),
    ("csbig5", "big5"),
    ("csibm037", "cp037"),
    ("csibm1026", "cp1026"),
    ("csibm273", "cp273"),
    ("csibm424", "cp424"),
    ("csibm500", "cp500"),
    ("csibm855", "cp855"),
    ("csibm857", "cp857"),
    ("csibm858", "cp858"),
    ("csibm860", "cp860"),
    ("csibm861", "cp861"),
    ("csibm863", "cp863"),
    ("csibm864", "cp864"),
    ("csibm865", "cp865"),
    ("csibm866", "cp866"),
    ("csibm869", "cp869"),
    ("csiso2022jp", "iso2022_jp"),
    ("csiso2022kr", "iso2022_kr"),
    ("csiso58gb231280", "gb2312"),
    ("csisolatin1", "latin_1"),
    ("csisolatin2", "iso8859_2"),
    ("csisolatin3", "iso8859_3"),
    ("csisolatin4", "iso8859_4"),
    ("csisolatin5", "iso8859_9"),
    ("csisolatin6", "iso8859_10"),
    ("csisolatinarabic", "iso8859_6"),
    ("csisolatincyrillic", "iso8859_5"),
    ("csisolatingreek", "iso8859_7"),
    ("csisolatinhebrew", "iso8859_8"),
    ("cskoi8r", "koi8_r"),
    ("cspc775baltic", "cp775"),
    ("cspc850multilingual", "cp850"),
    ("cspc862latinhebrew", "cp862"),
    ("cspc8codepage437", "cp437"),
    ("cspcp852", "cp852"),
    ("csptcp154", "ptcp154"),
    ("csshiftjis", "shift_jis"),
    ("cyrillic", "iso8859_5"),
    ("cyrillic_asian", "ptcp154"),
    ("dbcs", "mbcs"),
    ("ebcdic_cp_be", "cp500"),
    ("ebcdic_cp_ca", "cp037"),
    ("ebcdic_cp_ch", "cp500"),
    ("ebcdic_cp_he", "cp424"),
    ("ebcdic_cp_nl", "cp037"),
    ("ebcdic_cp_us", "cp037"),
    ("ebcdic_cp_wt", "cp037"),
    ("ecma_114", "iso8859_6"),
    ("ecma_118", "iso8859_7"),
    ("elot_928", "iso8859_7"),
    ("euc_cn", "gb2312"),
    ("euc_jis2004", "euc_jis_2004"),
    ("euc_jis_2004", "euc_jis_2004"),
    ("euc_jisx0213", "euc_jisx0213"),
    ("euc_jp", "euc_jp"),
    ("euc_kr", "euc_kr"),
    ("euccn", "gb2312"),
    ("eucgb2312_cn", "gb2312"),
    ("eucjis2004", "euc_jis_2004"),
    ("eucjisx0213", "euc_jisx0213"),
    ("eucjp", "euc_jp"),
    ("euckr", "euc_kr"),
    ("gb18030", "gb18030"),
    ("gb18030_2000", "gb18030"),
    ("gb2312", "gb2312"),
    ("gb2312_1980", "gb2312"),
    ("gb2312_80", "gb2312"),
    ("gbk", "gbk"),
    ("greek", "iso8859_7"),
    ("greek8", "iso8859_7"),
    ("hebrew", "iso8859_8"),
    ("hex", "hex_codec"),
    ("hex_codec", "hex_codec"),
    ("hkscs", "big5hkscs"),
    ("hp_roman8", "hp_roman8"),
    ("hz", "hz"),
    ("hz_gb", "hz"),
    ("hz_gb_2312", "hz"),
    ("hzgb", "hz"),
    ("ibm037", "cp037"),
    ("ibm039", "cp037"),
    ("ibm1026", "cp1026"),
    ("ibm1051", "hp_roman8"),
    ("ibm1125", "cp1125"),
    ("ibm1140", "cp1140"),
    ("ibm273", "cp273"),
    ("ibm367", "ascii"),
    ("ibm424", "cp424"),
    ("ibm437", "cp437"),
    ("ibm500", "cp500"),
    ("ibm775", "cp775"),
    ("ibm819", "latin_1"),
    ("ibm850", "cp850"),
    ("ibm852", "cp852"),
    ("ibm855", "cp855"),
    ("ibm857", "cp857"),
    ("ibm858", "cp858"),
    ("ibm860", "cp860"),
    ("ibm861", "cp861"),
    ("ibm862", "cp862"),
    ("ibm863", "cp863"),
    ("ibm864", "cp864"),
    ("ibm865", "cp865"),
    ("ibm866", "cp866"),
    ("ibm869", "cp869"),
    ("iso2022_jp", "iso2022_jp"),
    ("iso2022_jp_1", "iso2022_jp_1"),
    ("iso2022_jp_2", "iso2022_jp_2"),
    ("iso2022_jp_2004", "iso2022_jp_2004"),
    ("iso2022_jp_3", "iso2022_jp_3"),
    ("iso2022_jp_ext", "iso2022_jp_ext"),
    ("iso2022_kr", "iso2022_kr"),
    ("iso2022jp", "iso2022_jp"),
    ("iso2022jp_1", "iso2022_jp_1"),
    ("iso2022jp_2", "iso2022_jp_2"),
    ("iso2022jp_2004", "iso2022_jp_2004"),
    ("iso2022jp_3", "iso2022_jp_3"),
    ("iso2022jp_ext", "iso2022_jp_ext"),
    ("iso2022kr", "iso2022_kr"),
    ("iso646_us", "ascii"),
    ("iso8859", "latin_1"),
    ("iso8859_1", "latin_1"),
    ("iso8859_10", "iso8859_10"),
    ("iso8859_11", "iso8859_11"),
    ("iso8859_13", "iso8859_13"),
    ("iso8859_14", "iso8859_14"),
    ("iso8859_15", "iso8859_15"),
    ("iso8859_16", "iso8859_16"),
    ("iso8859_2", "iso8859_2"),
    ("iso8859_3", "iso8859_3"),
    ("iso8859_4", "iso8859_4"),
    ("iso8859_5", "iso8859_5"),
    ("iso8859_6", "iso8859_6"),
    ("iso8859_7", "iso8859_7"),
    ("iso8859_8", "iso8859_8"),
    ("iso8859_9", "iso8859_9"),
    ("iso_2022_jp", "iso2022_jp"),
    ("iso_2022_jp_1", "iso2022_jp_1"),
    ("iso_2022_jp_2", "iso2022_jp_2"),
    ("iso_2022_jp_2004", "iso2022_jp_2004"),
    ("iso_2022_jp_3", "iso2022_jp_3"),
    ("iso_2022_jp_ext", "iso2022_jp_ext"),
    ("iso_2022_kr", "iso2022_kr"),
    ("iso_646.irv_1991", "ascii"),
    ("iso_8859_1", "latin_1"),
    ("iso_8859_10", "iso8859_10"),
    ("iso_8859_10_1992", "iso8859_10"),
    ("iso_8859_11", "iso8859_11"),
    ("iso_8859_11_2001", "iso8859_11"),
    ("iso_8859_13", "iso8859_13"),
    ("iso_8859_14", "iso8859_14"),
    ("iso_8859_14_1998", "iso8859_14"),
    ("iso_8859_15", "iso8859_15"),
    ("iso_8859_16", "iso8859_16"),
    ("iso_8859_16_2001", "iso8859_16"),
    ("iso_8859_1_1987", "latin_1"),
    ("iso_8859_2", "iso8859_2"),
    ("iso_8859_2_1987", "iso8859_2"),
    ("iso_8859_3", "iso8859_3"),
    ("iso_8859_3_1988", "iso8859_3"),
    ("iso_8859_4", "iso8859_4"),
    ("iso_8859_4_1988", "iso8859_4"),
    ("iso_8859_5", "iso8859_5"),
    ("iso_8859_5_1988", "iso8859_5"),
    ("iso_8859_6", "iso8859_6"),
    ("iso_8859_6_1987", "iso8859_6"),
    ("iso_8859_7", "iso8859_7"),
    ("iso_8859_7_1987", "iso8859_7"),
    ("iso_8859_8", "iso8859_8"),
    ("iso_8859_8_1988", "iso8859_8"),
    ("iso_8859_9", "iso8859_9"),
    ("iso_8859_9_1989", "iso8859_9"),
    ("iso_celtic", "iso8859_14"),
    ("iso_ir_100", "latin_1"),
    ("iso_ir_101", "iso8859_2"),
    ("iso_ir_109", "iso8859_3"),
    ("iso_ir_110", "iso8859_4"),
    ("iso_ir_126", "iso8859_7"),
    ("iso_ir_127", "iso8859_6"),
    ("iso_ir_138", "iso8859_8"),
    ("iso_ir_144", "iso8859_5"),
    ("iso_ir_148", "iso8859_9"),
    ("iso_ir_157", "iso8859_10"),
    ("iso_ir_166", "tis_620"),
    ("iso_ir_199", "iso8859_14"),
    ("iso_ir_226", "iso8859_16"),
    ("iso_ir_58", "gb2312"),
    ("iso_ir_6", "ascii"),
    ("jisx0213", "euc_jis_2004"),
    ("johab", "johab"),
    ("koi8_r", "koi8_r"),
    ("koi8_t", "koi8_t"),
    ("koi8_u", "koi8_u"),
    ("korean", "euc_kr"),
    ("ks_c_5601", "euc_kr"),
    ("ks_c_5601_1987", "euc_kr"),
    ("ks_x_1001", "euc_kr"),
    ("ksc5601", "euc_kr"),
    ("ksx1001", "euc_kr"),
    ("kz1048", "kz1048"),
    ("kz_1048", "kz1048"),
    ("l1", "latin_1"),
    ("l10", "iso8859_16"),
    ("l2", "iso8859_2"),
    ("l3", "iso8859_3"),
    ("l4", "iso8859_4"),
    ("l5", "iso8859_9"),
    ("l6", "iso8859_10"),
    ("l7", "iso8859_13"),
    ("l8", "iso8859_14"),
    ("l9", "iso8859_15"),
    ("latin", "latin_1"),
    ("latin1", "latin_1"),
    ("latin10", "iso8859_16"),
    ("latin2", "iso8859_2"),
    ("latin3", "iso8859_3"),
    ("latin4", "iso8859_4"),
    ("latin5", "iso8859_9"),
    ("latin6", "iso8859_10"),
    ("latin7", "iso8859_13"),
    ("latin8", "iso8859_14"),
    ("latin9", "iso8859_15"),
    ("latin_1", "latin_1"),
    ("mac_centeuro", "mac_latin2"),
    ("mac_cyrillic", "mac_cyrillic"),
    ("mac_greek", "mac_greek"),
    ("mac_iceland", "mac_iceland"),
    ("mac_latin2", "mac_latin2"),
    ("mac_roman", "mac_roman"),
    ("mac_turkish", "mac_turkish"),
    ("maccentraleurope", "mac_latin2"),
    ("maccyrillic", "mac_cyrillic"),
    ("macgreek", "mac_greek"),
    ("maciceland", "mac_iceland"),
    ("macintosh", "mac_roman"),
    ("maclatin2", "mac_latin2"),
    ("macroman", "mac_roman"),
    ("macturkish", "mac_turkish"),
    ("mbcs", "mbcs"),
    ("ms1361", "johab"),
    ("ms932", "cp932"),
    ("ms936", "gbk"),
    ("ms949", "cp949"),
    ("ms950", "cp950"),
    ("ms_kanji", "cp932"),
    ("mskanji", "cp932"),
    ("pt154", "ptcp154"),
    ("ptcp154", "ptcp154"),
    ("quopri", "quopri_codec"),
    ("quopri_codec", "quopri_codec"),
    ("quoted_printable", "quopri_codec"),
    ("quotedprintable", "quopri_codec"),
    ("r8", "hp_roman8"),
    ("rk1048", "kz1048"),
    ("roman8", "hp_roman8"),
    ("rot13", "rot_13"),
    ("rot_13", "rot_13"),
    ("ruscii", "cp1125"),
    ("s_jis", "shift_jis"),
    ("s_jis_2004", "shift_jis_2004"),
    ("s_jisx0213", "shift_jisx0213"),
    ("shift_jis", "shift_jis"),
    ("shift_jis_2004", "shift_jis_2004"),
    ("shift_jisx0213", "shift_jisx0213"),
    ("shiftjis", "shift_jis"),
    ("shiftjis2004", "shift_jis_2004"),
    ("shiftjisx0213", "shift_jisx0213"),
    ("sjis", "shift_jis"),
    ("sjis_2004", "shift_jis_2004"),
    ("sjisx0213", "shift_jisx0213"),
    ("strk1048_2002", "kz1048"),
    ("thai", "iso8859_11"),
    ("tis620", "tis_620"),
    ("tis_620", "tis_620"),
    ("tis_620_0", "tis_620"),
    ("tis_620_2529_0", "tis_620"),
    ("tis_620_2529_1", "tis_620"),
    ("u16", "utf_16"),
    ("u32", "utf_32"),
    ("u7", "utf_7"),
    ("u8", "utf_8"),
    ("u_jis", "euc_jp"),
    ("uhc", "cp949"),
    ("ujis", "euc_jp"),
    ("unicode_1_1_utf_7", "utf_7"),
    ("unicodebigunmarked", "utf_16_be"),
    ("unicodelittleunmarked", "utf_16_le"),
    ("us", "ascii"),
    ("us_ascii", "ascii"),
    ("utf", "utf_8"),
    ("utf16", "utf_16"),
    ("utf32", "utf_32"),
    ("utf7", "utf_7"),
    ("utf8", "utf_8"),
    ("utf8_ucs2", "utf_8"),
    ("utf8_ucs4", "utf_8"),
    ("utf_16", "utf_16"),
    ("utf_16_be", "utf_16_be"),
    ("utf_16_le", "utf_16_le"),
    ("utf_16be", "utf_16_be"),
    ("utf_16le", "utf_16_le"),
    ("utf_32", "utf_32"),
    ("utf_32_be", "utf_32_be"),
    ("utf_32_le", "utf_32_le"),
    ("utf_32be", "utf_32_be"),
    ("utf_32le", "utf_32_le"),
    ("utf_7", "utf_7"),
    ("utf_8", "utf_8"),
    ("uu", "uu_codec"),
    ("uu_codec", "uu_codec"),
    ("windows_1250", "cp1250"),
    ("windows_1251", "cp1251"),
    ("windows_1252", "cp1252"),
    ("windows_1253", "cp1253"),
    ("windows_1254", "cp1254"),
    ("windows_1255", "cp1255"),
    ("windows_1256", "cp1256"),
    ("windows_1257", "cp1257"),
    ("windows_1258", "cp1258"),
    ("x_mac_japanese", "shift_jis"),
    ("x_mac_korean", "euc_kr"),
    ("x_mac_simp_chinese", "gb2312"),
    ("x_mac_trad_chinese", "big5"),
    ("zip", "zlib_codec"),
    ("zlib", "zlib_codec"),
    ("zlib_codec", "zlib_codec"),
];

/// Lookup in the frozen `_IANA_NAMES` image (utils.py:246 via
/// `any_specified_encoding`, utils.py:304 via `iana_name`).
pub(crate) fn iana_lookup(normalized: &str) -> Option<&'static str> {
    IANA_ALIASES
        .binary_search_by_key(&normalized, |(alias, _)| *alias)
        .ok()
        .map(|i| IANA_ALIASES[i].1)
}

/// Mirror of `iana_name` (utils.py:300-311).
///
/// Returns the Python-normalised encoding name (underscores, lowercased).
/// strict miss -> `Err` (mirrors `ValueError("Unable to retrieve IANA for
/// '{cp_name}'")`); non-strict miss -> `Ok` of the normalised input unchanged
/// (lossy echo, utils.py:311).
pub(crate) fn iana_name(cp_name: &str, strict: bool) -> Result<String, String> {
    // `cp_name.lower().replace("-", "_")` (utils.py:302); both sides are
    // Unicode-aware lowercasings, matching Python for the ASCII names that
    // occur in practice.
    let normalized = cp_name.to_lowercase().replace('-', "_");
    if let Some(iana) = iana_lookup(&normalized) {
        return Ok(iana.to_string());
    }
    if strict {
        return Err(format!("Unable to retrieve IANA for '{normalized}'"));
    }
    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Decoding forwarders (owned by DecodersPort) + cp_similarity / is_cp_similar
// (utils.py:314-342)
// ---------------------------------------------------------------------------

/// Decode with undecodable bytes dropped (`errors="ignore"`).
///
/// Precondition/dependency: implemented by `super::decoders` (DecodersPort);
/// this forwarder keeps utils call sites shaped like the Python originals.
pub(crate) fn decode_ignore(iana_name: &str, data: &[u8]) -> String {
    super::decoders::decode_ignore(iana_name, data)
}

/// Strict decode: `Err(())` on any undecodable byte or unknown name (mirrors
/// `UnicodeDecodeError`). Message-free by core convention: `api.rs` logging owns
/// messages, so the payload is unit (DecodersPort contract).
///
/// Precondition/dependency: implemented by `super::decoders` (DecodersPort).
pub(crate) fn decode_strict(iana_name: &str, data: &[u8]) -> Result<String, ()> {
    super::decoders::decode_strict(iana_name, data)
}

/// Mirror of `cp_similarity` (utils.py:314-331).
///
/// Precondition: both names must be single-byte encodings (multi-byte pairs
/// short-circuit to `0.0`, utils.py:315-316); decoding of each of the 256
/// byte values goes through [`decode_ignore`], mirroring the per-byte
/// `IncrementalDecoder(errors="ignore")` comparison (utils.py:326-329).
pub(crate) fn cp_similarity(iana_name_a: &str, iana_name_b: &str) -> f64 {
    if is_multi_byte_encoding(iana_name_a) || is_multi_byte_encoding(iana_name_b) {
        return 0.0;
    }
    let mut character_match_count: u32 = 0;
    for byte in 0..=255u8 {
        if decode_ignore(iana_name_a, &[byte]) == decode_ignore(iana_name_b, &[byte]) {
            character_match_count += 1;
        }
    }
    f64::from(character_match_count) / 256.0
}

/// Mirror of `is_cp_similar` (utils.py:334-342): the pre-computed
/// `IANA_SUPPORTED_SIMILAR` membership test (frozen as
/// `tables_constant::similar_encodings`).
pub(crate) fn is_cp_similar(iana_name_a: &str, iana_name_b: &str) -> bool {
    crate::tables_constant::similar_encodings(iana_name_a).contains(&iana_name_b)
}

// ---------------------------------------------------------------------------
// set_logging_handler (utils.py:345-355)
// ---------------------------------------------------------------------------

/// Mirror of `set_logging_handler` (utils.py:345-355) as a no-op.
///
/// The pure-Rust core has no logging backend, so there is nothing to attach a
/// handler to; the signature takes no arguments and returns `()`.
pub(crate) fn set_logging_handler() {}

// ---------------------------------------------------------------------------
// cut_sequence_chunks (utils.py:358-452)
// ---------------------------------------------------------------------------

/// `s[start..start+len]` in *characters* (Python string slicing is
/// char-indexed). Out-of-range or zero-length requests yield `""`, exactly
/// like the corresponding Python slice.
fn char_slice(s: &str, start: usize, len: usize) -> &str {
    if len == 0 {
        return "";
    }
    let mut indices = s.char_indices();
    let Some((byte_start, _)) = indices.by_ref().nth(start) else {
        return "";
    };
    match indices.by_ref().nth(len - 1) {
        Some((byte_end, _)) => &s[byte_start..byte_end],
        None => &s[byte_start..],
    }
}

/// Mirror of `cut_sequence_chunks` (utils.py:358-452) as an eager `Vec`.
///
/// Preconditions (mirroring how `models.py`/`api.py` drive the generator):
/// - `offsets` are byte offsets into `sequences` for the raw/deferred
///   branches, and char offsets into `decoded_payload` for the first two
///   branches;
/// - when `decoded_payload` is `Some`, it must be the full decode of
///   `sequences` (the multi-byte miscut detector searches inside it);
/// - `is_multi_byte_decoder` agrees with `is_multi_byte_encoding(encoding_iana)`;
/// - `sig_payload` is the mark previously returned by
///   [`identify_sig_or_bom`], meaningful only when `bom_or_sig_available`.
///
/// Strict-decode failures (`UnicodeDecodeError` in Python, raised mid-iteration
/// out of the generator) abort the whole call with `Err(())` (unit payload per
/// core message-free convention). The `continue` guard
/// `chunk_end > len(sequences) + 8` (utils.py:404) and the 4-byte miscut
/// backtrack with the 32768-radius window (`_MULTIBYTE_SEARCH_RADIUS`,
/// `constant.py:2497`, frozen as `MB_SEARCH_RADIUS`) are replicated literally,
/// including Python's negative-index wrap (`sequences[j:...]` with `j < 0`
/// counts from the end; here `len + j`, clamped like Python slice bounds).
#[allow(clippy::too_many_arguments)]
pub(crate) fn cut_sequence_chunks(
    sequences: &[u8],
    encoding_iana: &str,
    offsets: impl IntoIterator<Item = usize>,
    chunk_size: usize,
    bom_or_sig_available: bool,
    strip_sig_or_bom: bool,
    sig_payload: &[u8],
    is_multi_byte_decoder: bool,
    decoded_payload: Option<&str>,
    deferred_decoding: bool,
) -> Result<Vec<String>, ()> {
    let mut out: Vec<String> = Vec::new();
    // Python truthiness: empty decoded payload == absent (utils.py:371/380).
    let decoded = decoded_payload.filter(|s| !s.is_empty());

    // iso2022 codec is stateful, generic mb cutter isn't going to cut it!
    // (utils.py:370-379)
    if let Some(full) = decoded {
        if encoding_iana.starts_with("iso2022_") {
            let decoded_length = full.chars().count();
            let sequence_length = sequences.len();
            if sequence_length == 0 {
                // Python would raise ZeroDivisionError; unreachable in practice
                // (a non-empty decode implies non-empty input).
                return Ok(out);
            }
            for i in offsets {
                let decoded_offset = i * decoded_length / sequence_length;
                let chunk = char_slice(full, decoded_offset, chunk_size);
                if chunk.is_empty() {
                    break;
                }
                out.push(chunk.to_string());
            }
            return Ok(out);
        }
    }

    // Non-MB fast path over the already-decoded payload (utils.py:380-385).
    if let Some(full) = decoded {
        if !is_multi_byte_decoder {
            for i in offsets {
                let chunk = char_slice(full, i, chunk_size);
                if chunk.is_empty() {
                    break;
                }
                out.push(chunk.to_string());
            }
            return Ok(out);
        }
    }

    // Deferred single-byte probing (utils.py:386-400): single-byte codecs are
    // stateless, so slicing raw bytes then strict-decoding equals slicing the
    // decoded payload; invalid bytes raise, like the whole-payload decode.
    if deferred_decoding {
        let base: &[u8] = if strip_sig_or_bom {
            sequences.get(sig_payload.len()..).unwrap_or(b"")
        } else {
            sequences
        };
        for i in offsets {
            if i >= base.len() {
                break; // Python slice -> b"" -> falsy -> break.
            }
            let cut_sequence = &base[i..(i + chunk_size).min(base.len())];
            if cut_sequence.is_empty() {
                break;
            }
            // `str(cut_sequence, encoding_iana)` is strict (utils.py:400).
            out.push(decode_strict(encoding_iana, cut_sequence)?);
        }
        return Ok(out);
    }

    // Generic cutter (utils.py:401-452).
    for i in offsets {
        let chunk_end = i + chunk_size;
        if chunk_end > sequences.len() + 8 {
            continue;
        }

        // Python slicing clamps; it never panics.
        let raw: &[u8] = if i < sequences.len() {
            &sequences[i..(i + chunk_size).min(sequences.len())]
        } else {
            b""
        };
        let mut cut_sequence: Vec<u8> = raw.to_vec();
        if bom_or_sig_available && !strip_sig_or_bom {
            let mut prefixed = Vec::with_capacity(sig_payload.len() + cut_sequence.len());
            prefixed.extend_from_slice(sig_payload);
            prefixed.extend_from_slice(&cut_sequence);
            cut_sequence = prefixed;
        }

        // `.decode(enc, errors="ignore" if MB else "strict")` (utils.py:412).
        let mut chunk = if is_multi_byte_decoder {
            decode_ignore(encoding_iana, &cut_sequence)
        } else {
            decode_strict(encoding_iana, &cut_sequence)?
        };

        // Multi-byte bad cutting detector and adjustment (utils.py:417-450).
        if is_multi_byte_decoder && i > 0 {
            let chunk_partial_size_chk: usize = chunk_size.min(16);
            let chunk_prefix = char_slice(&chunk, 0, chunk_partial_size_chk).to_string();
            if let Some(full) = decoded {
                let decoded_length = full.chars().count();
                let expected_offset = i * decoded_length / sequences.len().max(1);
                let radius = crate::tables_constant::MB_SEARCH_RADIUS;
                let search_start = expected_offset.saturating_sub(radius);
                let search_end = (expected_offset + radius).min(decoded_length);
                // `decoded_payload.find(prefix, start, end) >= 0`
                // (utils.py:431-434); indices are char offsets.
                let found_nearby = char_slice(full, search_start, search_end - search_start)
                    .contains(chunk_prefix.as_str());
                if !found_nearby && !full.contains(chunk_prefix.as_str()) {
                    for back in 0..4usize {
                        // `range(i, i - 4, -1)` (utils.py:441): i..=i-3.
                        let j = i as isize - back as isize;
                        let seq_len = sequences.len() as isize;
                        // Python negative-index wrap + slice clamping.
                        let start = (if j < 0 { seq_len + j } else { j }).clamp(0, seq_len);
                        let end = (chunk_end as isize).clamp(0, seq_len);
                        let raw: &[u8] = if start < end {
                            &sequences[start as usize..end as usize]
                        } else {
                            b""
                        };
                        let mut retry: Vec<u8> = raw.to_vec();
                        if bom_or_sig_available && !strip_sig_or_bom {
                            let mut prefixed =
                                Vec::with_capacity(sig_payload.len() + retry.len());
                            prefixed.extend_from_slice(sig_payload);
                            prefixed.extend_from_slice(&retry);
                            retry = prefixed;
                        }
                        let attempt = decode_ignore(encoding_iana, &retry);
                        if full.contains(char_slice(&attempt, 0, chunk_partial_size_chk)) {
                            chunk = attempt;
                            break;
                        }
                    }
                }
            }
        }

        out.push(chunk);
    }
    Ok(out)
}
