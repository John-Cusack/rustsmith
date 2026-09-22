//! Mess-detection core: port of `charset_normalizer/md.py` (3.5.1).
//!
//! Symbols ported (orig `md.py` line refs; `constant.py`/`utils.py` refs where
//! the value lives there):
//! - `CharInfo` + `char_info` (orig `CharInfo`, `md.py:44-243`; cached
//!   `_char_info`, `md.py:248-251`, and `_ASCII_CHAR_INFO`, `md.py:255-257`,
//!   intentionally NOT ported: caching is the integrator's wiring concern,
//!   `char_info` is a pure function of the codepoint).
//! - `MessDetectorPlugin` trait + 10 detectors in FILE ORDER
//!   (`md.py:260-873`).
//! - `is_suspiciously_successive_range` (`md.py:875-908`).
//! - `mess_ratio` (`md.py:911-1065`; the `debug: bool` log-only parameter is
//!   dropped — it only emits TRACE logs, never affects the return value).
//!
//! # Assumed sibling APIs (owned by sibling ports; integrator wires `lib.rs`)
//! - `super::utils::{character_flags, is_punctuation, is_symbol, is_emoticon,
//!   is_separator, remove_accent, unicode_range}` mirroring `utils.py:
//!   39,129,143,157,166,85,104`.
//! - `super::tables_constant::{COMMON_CJK, SAFE_ASCII, range_family}` mirroring
//!   `COMMON_CJK_CHARACTERS` (`constant.py:985`), `COMMON_SAFE_ASCII_CHARACTERS`
//!   (`constant.py:955`), `_RANGE_FAMILIES` (`constant.py:380`).
//! - `super::tables_ucd::ucd_lookup` (frozen CPython 3.12 records) for the
//!   non-ASCII `str.isprintable/isspace` bits and the `_character_flags` bits.
//!
//! # Name risks / known divergences (explicit; all verified by full-codespace
//! differential test against live CPython 3.12, see verification notes)
//! 1. `str.isalpha()` is exactly General_Category Ll/Lm/Lo/Lt/Lu (verified:
//!    U+2118 Sm and every Nl are non-alpha; no exotic Alphabetic non-Letter
//!    is alpha in 3.12).
//! 2. `str.isupper()/islower()` follow the Unicode Uppercase/Lowercase
//!    *properties*, not General_Category (e.g. U+00AA Lo is lower, U+2167 Nl
//!    is upper, U+0345 Mn is lower): implemented with `char::is_uppercase` /
//!    `is_lowercase`, which use those same properties. `case_variable` stays
//!    exactly `lower != upper` (`md.py:206`).
//! 3. `str.isdigit()` is Nd plus the 20 Numeric_Type=Digit ranges in
//!    `EXTRA_DIGIT_RANGES` below (verified: no Nl is a digit; fractions like
//!    U+00BD are not).
//! 4. `str.isspace()/isprintable()` non-ASCII: taken from the frozen UCD table
//!    (CPython-baked), more faithful than `char::is_whitespace`.
//! 5. `remove_accent` returns `Result<char, &'static str>` per the utils port
//!    (Err on tagged/compat decompositions, mirroring Python's ValueError in
//!    `utils.py:85-92`); the call site uses `.unwrap_or(c)`. Python has no
//!    try/except here (`md.py:226-229`) and would propagate, but the Err path
//!    is unreachable for latin+accentuated chars (full-codespace scan: zero
//!    hits), so graceful fallback never diverges on non-crashing inputs.
//! 6. Unmapped range name in `is_suspiciously_successive_range`: orig raises
//!    `KeyError` on `_RANGE_FAMILIES[...]` (`md.py:885-886`); here it returns
//!    `true` (suspicious) defensively. Unreachable while tables are consistent.
//! 7. `round(x, 3)` is banker's rounding in Python; here
//!    `(x * 1000.0).round() / 1000.0` (half-away-from-zero) per the port
//!    contract. Differs only on exact `.0005` ties of the float sum.
//! 8. `_char_info`/`_ASCII_CHAR_INFO` caching is NOT ported; callers needing
//!    the hot-loop table (e.g. `"\n"` flush info, `md.py:1029`) call
//!    `char_info('\n')`, which constructs identical values.
//! 9. `unicode_range` on lone surrogates (U+D800-U+DFFF): orig returns
//!    range names for them, but a Rust `char`/`&str` can never contain a
//!    surrogate, so `char_info`/`mess_ratio` inputs never reach that path.
//!    Verified: range table agrees with orig on every other codepoint.
//!
//! # Float-ordering sensitivities (the complete list — audit aid)
//! Every detector accumulates integer counters; each `ratio()` performs exactly
//! ONE float division at the end, so intra-detector summation order cannot
//! drift. The ONLY float-ordering-sensitive operations in this file are the two
//! verbatim ten-term left-to-right sums in `mess_ratio` (`md.py:1012-1023` and
//! `md.py:1035-1046`), preserved term-for-term below, plus the final round-3.

use super::tables_constant::{COMMON_CJK, SAFE_ASCII, range_family};
use super::tables_ucd::ucd_lookup;
use super::utils::{
    character_flags, is_emoticon, is_punctuation, is_separator, is_symbol, remove_accent,
    unicode_range,
};

// ---------------------------------------------------------------------------
// Flag bits, verbatim values from `constant.py:2423-2435`.
// ---------------------------------------------------------------------------
pub(crate) const FLAG_LATIN: u32 = 1; // `constant.py:2423` (`_LATIN`)
pub(crate) const FLAG_ACCENTUATED: u32 = 1 << 1; // `constant.py:2424`
pub(crate) const FLAG_CJK: u32 = 1 << 2; // `constant.py:2425`
pub(crate) const FLAG_HANGUL: u32 = 1 << 3; // `constant.py:2426`
pub(crate) const FLAG_KATAKANA: u32 = 1 << 4; // `constant.py:2427`
pub(crate) const FLAG_HIRAGANA: u32 = 1 << 5; // `constant.py:2428`
pub(crate) const FLAG_THAI: u32 = 1 << 6; // `constant.py:2429`
pub(crate) const FLAG_ARABIC: u32 = 1 << 7; // `constant.py:2430`
pub(crate) const FLAG_ARABIC_ISOLATED_FORM: u32 = 1 << 8; // `constant.py:2431`
pub(crate) const FLAG_HALFWIDTH_KATAKANA: u32 = 1 << 9; // `constant.py:2432`
pub(crate) const FLAG_LIGATURE: u32 = 1 << 10; // `constant.py:2433`
pub(crate) const FLAG_SUPERSCRIPT: u32 = 1 << 11; // `constant.py:2434`
pub(crate) const FLAG_SENTENCE_OPEN_PUNCTUATION: u32 = 1 << 12; // `constant.py:2435`

/// Combined glyph mask, verbatim from `md.py:41`.
pub(crate) const GLYPH_MASK: u32 =
    FLAG_CJK | FLAG_HANGUL | FLAG_KATAKANA | FLAG_HIRAGANA | FLAG_THAI;

// ---------------------------------------------------------------------------
// CharInfo (`md.py:44-243`)
/// Non-Nd codepoints with `str.isdigit() == True` (Numeric_Type=Digit),
/// as 20 inclusive ranges extracted from CPython 3.12 / UCD 15.0
/// (`all(c for c in map(chr, range(0x110000)) if c.isdigit()
/// and unicodedata.category(c) != 'Nd')`). No Nl codepoint is a digit.
const EXTRA_DIGIT_RANGES: &[(u32, u32)] = &[
    (0x00B2, 0x00B3),
    (0x00B9, 0x00B9),
    (0x1369, 0x1371),
    (0x19DA, 0x19DA),
    (0x2070, 0x2070),
    (0x2074, 0x2079),
    (0x2080, 0x2089),
    (0x2460, 0x2468),
    (0x2474, 0x247C),
    (0x2488, 0x2490),
    (0x24EA, 0x24EA),
    (0x24F5, 0x24FD),
    (0x24FF, 0x24FF),
    (0x2776, 0x277E),
    (0x2780, 0x2788),
    (0x278A, 0x2792),
    (0x10A40, 0x10A43),
    (0x10E60, 0x10E68),
    (0x11052, 0x1105A),
    (0x1F100, 0x1F10A),
];
/// Codepoints with the Unicode Uppercase property outside General_Category Lu
/// (UCD 15.0, i.e. CPython 3.12: `all(c for c in map(chr, range(0x110000))
/// if c.isupper() and unicodedata.category(c) != 'Lu')`). Pinned data: the
/// toolchain's `char::is_uppercase` follows a NEWER UCD and disagrees on
/// recently-added cased letters (e.g. U+1C89) and on U+0295.
const UPPER_EXTRA_RANGES: &[(u32, u32)] = &[
    (0x2160, 0x216F),
    (0x24B6, 0x24CF),
    (0x1F130, 0x1F149),
    (0x1F150, 0x1F169),
    (0x1F170, 0x1F189),
];

/// Codepoints with the Unicode Lowercase property outside General_Category Ll
/// (same provenance as `UPPER_EXTRA_RANGES`, with `islower`/`'Ll'`).
const LOWER_EXTRA_RANGES: &[(u32, u32)] = &[
    (0x00AA, 0x00AA),
    (0x00BA, 0x00BA),
    (0x02B0, 0x02B8),
    (0x02C0, 0x02C1),
    (0x02E0, 0x02E4),
    (0x0345, 0x0345),
    (0x037A, 0x037A),
    (0x10FC, 0x10FC),
    (0x1D2C, 0x1D6A),
    (0x1D78, 0x1D78),
    (0x1D9B, 0x1DBF),
    (0x2071, 0x2071),
    (0x207F, 0x207F),
    (0x2090, 0x209C),
    (0x2170, 0x217F),
    (0x24D0, 0x24E9),
    (0x2C7C, 0x2C7D),
    (0xA69C, 0xA69D),
    (0xA770, 0xA770),
    (0xA7F2, 0xA7F4),
    (0xA7F8, 0xA7F9),
    (0xAB5C, 0xAB5F),
    (0xAB69, 0xAB69),
    (0x10780, 0x10780),
    (0x10783, 0x10785),
    (0x10787, 0x107B0),
    (0x107B2, 0x107BA),
    (0x1E030, 0x1E06D),
];

/// (`md.py:44-105`; every slot of `__slots__` is a field here).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CharInfo {
    pub(crate) character: char,
    pub(crate) printable: bool,
    pub(crate) alpha: bool,
    pub(crate) upper: bool,
    pub(crate) lower: bool,
    pub(crate) space: bool,
    pub(crate) digit: bool,
    pub(crate) is_ascii: bool,
    pub(crate) case_variable: bool,
    pub(crate) flags: u32,
    pub(crate) accentuated: bool,
    pub(crate) latin: bool,
    pub(crate) is_cjk: bool,
    pub(crate) is_katakana: bool,
    pub(crate) is_halfwidth_katakana: bool,
    pub(crate) is_arabic: bool,
    pub(crate) is_ligature: bool,
    pub(crate) is_superscript: bool,
    pub(crate) is_sentence_open_punctuation: bool,
    pub(crate) is_glyph: bool,
    pub(crate) punct: bool,
    pub(crate) sym: bool,
    pub(crate) range: Option<&'static str>,
    pub(crate) sep: bool,
    pub(crate) emoticon: bool,
    pub(crate) safe: bool,
    pub(crate) common_cjk: bool,
    pub(crate) unaccented: char,
}

/// Build the `CharInfo` for `c`, mirroring `CharInfo.__init__`
/// (`md.py:107-243`) branch-for-branch, including the ASCII fast path
/// (`md.py:114-195`) and the non-ASCII path (`md.py:196-240`).
pub(crate) fn char_info(c: char) -> CharInfo {
    let o = c as u32;
    // ASCII fast path (`md.py:115`): skip `_character_flags()` entirely.
    if o < 128 {
        let mut tmp = [0u8; 4];
        let s: &str = c.encode_utf8(&mut tmp);
        // `md.py:121`: `self.safe = character in COMMON_SAFE_ASCII_CHARACTERS`.
        let safe = SAFE_ASCII.iter().any(|q| *q == s);
        // Shared ASCII defaults resolved per branch below (`md.py:116-195`).
        let (printable, alpha, upper, lower, space, digit, case_variable, flags, latin, punct, sym);
        if (65..=90).contains(&o) {
            // Uppercase ASCII letter (`md.py:131-143`).
            printable = true;
            alpha = true;
            upper = true;
            lower = false;
            space = false;
            digit = false;
            case_variable = true;
            flags = FLAG_LATIN; // `md.py:140`
            latin = true;
            punct = false;
            sym = false;
        } else if (97..=122).contains(&o) {
            // Lowercase ASCII letter (`md.py:144-156`).
            printable = true;
            alpha = true;
            upper = false;
            lower = true;
            space = false;
            digit = false;
            case_variable = true;
            flags = FLAG_LATIN; // `md.py:153`
            latin = true;
            punct = false;
            sym = false;
        } else if (48..=57).contains(&o) {
            // ASCII digit 0-9 (`md.py:157-169`).
            printable = true;
            alpha = false;
            upper = false;
            lower = false;
            space = false;
            digit = true;
            case_variable = false;
            flags = 0; // `md.py:166`
            latin = false;
            punct = false;
            sym = false;
        } else if o == 32 || (9..=13).contains(&o) {
            // Space, tab, newline, etc. (`md.py:170-182`).
            printable = o == 32; // `md.py:177`
            alpha = false;
            upper = false;
            lower = false;
            space = true;
            digit = false;
            case_variable = false;
            flags = 0; // `md.py:179`
            latin = false;
            punct = false;
            sym = false;
        } else {
            // Other ASCII (punctuation, symbols, control chars, `md.py:183-195`).
            // `character.isprintable()` on ASCII == non-control.
            printable = !c.is_control(); // `md.py:185`
            alpha = false;
            upper = false;
            lower = false;
            space = false;
            digit = false;
            case_variable = false;
            flags = 0; // `md.py:192`
            latin = false;
            // `md.py:194-195`: gated on printable.
            punct = printable && is_punctuation(c);
            sym = printable && is_symbol(c);
        }
        CharInfo {
            character: c,
            printable,
            alpha,
            upper,
            lower,
            space,
            digit,
            is_ascii: true,               // `md.py:116`
            case_variable,                // (per branch above)
            flags,                        // (per branch above)
            accentuated: false,           // `md.py:117`
            latin,                        // (per branch above)
            is_cjk: false,                // `md.py:122`
            is_katakana: false,           // `md.py:123`
            is_halfwidth_katakana: false, // `md.py:124`
            is_arabic: false,             // `md.py:125`
            is_ligature: false,           // `md.py:126`
            is_superscript: false,        // `md.py:127`
            is_sentence_open_punctuation: false, // `md.py:128`
            is_glyph: false,              // `md.py:129`
            punct,
            sym,
            range: unicode_range(c), // `md.py:242`
            sep: is_separator(c), // `md.py:243`
            emoticon: false, // `md.py:119`
            safe, // `md.py:121`
            common_cjk: false, // `md.py:120`
            unaccented: c, // `md.py:118`
        }
    } else {
        // Non-ASCII path (`md.py:196-240`).
        // `ucd_lookup` returns CPython-baked (flags, category, isspace,
        // isprintable, casevar); category indices follow `CATEGORIES`
        // (Cc=0 Cf=1 Co=2 Cs=3 Ll=4 Lm=5 Lo=6 Lt=7 Lu=8 Mc=9 Me=10 Mn=11
        // Nd=12 Nl=13 No=14 Pc=15 Pd=16 Pe=17 Pf=18 Pi=19 Po=20 Ps=21
        // Sc=22 Sk=23 Sm=24 So=25 Zl=26 Zp=27 Zs=28 Cn=29).
        let lookup = ucd_lookup(o);
        let (cat, space, printable) = (lookup.1, lookup.2, lookup.3);
        // `md.py:209`: flags come from `_character_flags()` (utils); the
        // category/space/printable bits come from the frozen UCD records.
        let flags = character_flags(c);
        // `md.py:201-205`. `alpha` is General_Category Ll/Lm/Lo/Lt/Lu;
        // `upper`/`lower` are the Unicode properties (NOT the category);
        // `digit` is Nd plus `EXTRA_DIGIT_RANGES` (see header risk 3).
        let alpha = matches!(cat, 4 | 5 | 6 | 7 | 8);
        let upper = cat == 8
            || UPPER_EXTRA_RANGES
                .iter()
                .any(|&(lo, hi)| lo <= o && o <= hi);
        let lower = cat == 4
            || LOWER_EXTRA_RANGES
                .iter()
                .any(|&(lo, hi)| lo <= o && o <= hi);
        let digit = cat == 12
            || EXTRA_DIGIT_RANGES
                .iter()
                .any(|&(lo, hi)| lo <= o && o <= hi);
        let case_variable = lower != upper; // `md.py:206`, verbatim
        // `md.py:210-213`: emoticon only computed for non-alpha.
        let emoticon = if alpha { false } else { is_emoticon(c) };
        let accentuated = flags & FLAG_ACCENTUATED != 0; // `md.py:215`
        let latin = flags & FLAG_LATIN != 0; // `md.py:216`
        let is_cjk = flags & FLAG_CJK != 0; // `md.py:217`
        let is_katakana = flags & FLAG_KATAKANA != 0; // `md.py:218`
        let is_halfwidth_katakana = flags & FLAG_HALFWIDTH_KATAKANA != 0; // `md.py:219`
        let is_arabic = flags & FLAG_ARABIC != 0; // `md.py:220`
        let is_ligature = flags & FLAG_LIGATURE != 0; // `md.py:221`
        let is_superscript = flags & FLAG_SUPERSCRIPT != 0; // `md.py:222`
        let is_sentence_open_punctuation = flags & FLAG_SENTENCE_OPEN_PUNCTUATION != 0; // `md.py:223`
        let is_glyph = flags & GLYPH_MASK != 0; // `md.py:224`
        // `md.py:226-229`. `.unwrap_or(c)`: Err (tagged decomposition) is
        // unreachable for latin+accentuated chars; Python would propagate
        // the ValueError there, so fallback never diverges on live inputs.
        let unaccented = if latin && accentuated {
            remove_accent(c).unwrap_or(c)
        } else {
            c
        };
        // `md.py:231`.
        let common_cjk = is_cjk && COMMON_CJK.contains(c);
        // `md.py:235-240`: eager punct/sym gated on printable.
        let (punct, sym) = if printable {
            (is_punctuation(c), is_symbol(c))
        } else {
            (false, false)
        };
        CharInfo {
            character: c,
            printable,      // `md.py:200`
            alpha,          // `md.py:201`
            upper,          // `md.py:202`
            lower,          // `md.py:203`
            space,          // `md.py:204`
            digit,          // `md.py:205`
            is_ascii: false, // `md.py:198`
            case_variable,  // `md.py:206`
            flags,          // `md.py:214`
            accentuated,
            latin,
            is_cjk,
            is_katakana,
            is_halfwidth_katakana,
            is_arabic,
            is_ligature,
            is_superscript,
            is_sentence_open_punctuation,
            is_glyph,
            punct,
            sym,
            range: unicode_range(c), // `md.py:242`
            sep: is_separator(c),    // `md.py:243`
            emoticon,
            safe: false, // `md.py:199`
            common_cjk,
            unaccented,
        }
    }
}

// ---------------------------------------------------------------------------
// Detector base (`md.py:260-287`): every plugin implements `new`, `reset`,
// `feed_info` and `ratio`. (`MessDetectorPlugin` raising NotImplementedError
// has no runtime content to port; this trait documents the contract.)
// ---------------------------------------------------------------------------

/// Contract of `MessDetectorPlugin` (`md.py:260-287`).
pub(crate) trait MessDetectorPlugin {
    fn reset(&mut self);
    fn feed_info(&mut self, c: char, info: &CharInfo);
    fn ratio(&self) -> f64;
}

// ---------------------------------------------------------------------------
// 1. TooManySymbolOrPunctuationPlugin (`md.py:290-331`)
// ---------------------------------------------------------------------------

pub(crate) struct TooManySymbolOrPunctuationPlugin {
    punctuation_count: usize,       // `md.py:292`
    symbol_count: usize,            // `md.py:293`
    character_count: usize,         // `md.py:294`
    last_printable_char: Option<char>, // `md.py:295`
}

impl TooManySymbolOrPunctuationPlugin {
    pub(crate) fn new() -> Self {
        // `md.py:298-303`.
        Self {
            punctuation_count: 0,
            symbol_count: 0,
            character_count: 0,
            last_printable_char: None,
        }
    }
}

impl MessDetectorPlugin for TooManySymbolOrPunctuationPlugin {
    fn feed_info(&mut self, c: char, info: &CharInfo) {
        // `md.py:305-315`.
        self.character_count += 1;
        if Some(c) != self.last_printable_char && !info.safe {
            // `md.py:309`
            if info.punct {
                // `md.py:310-311`
                self.punctuation_count += 1;
            } else if !info.digit && info.sym && !info.emoticon {
                // `md.py:312-313`
                self.symbol_count += 2;
            }
        }
        self.last_printable_char = Some(c); // `md.py:315`
    }

    fn reset(&mut self) {
        // `md.py:317-320`: NOTE `last_printable_char` is NOT reset.
        self.punctuation_count = 0;
        self.character_count = 0;
        self.symbol_count = 0;
    }

    fn ratio(&self) -> f64 {
        // `md.py:322-331`.
        if self.character_count == 0 {
            // `md.py:324-325`
            return 0.0;
        }
        let ratio_of_punctuation = (self.punctuation_count + self.symbol_count) as f64
            / self.character_count as f64; // `md.py:327-329`
        if ratio_of_punctuation >= 0.3 {
            // `md.py:331`
            ratio_of_punctuation
        } else {
            0.0
        }
    }
}

// ---------------------------------------------------------------------------
// 2. TooManyAccentuatedPlugin (`md.py:334-358`)
// ---------------------------------------------------------------------------

pub(crate) struct TooManyAccentuatedPlugin {
    character_count: usize,   // `md.py:335`
    accentuated_count: usize, // `md.py:335`
}

impl TooManyAccentuatedPlugin {
    pub(crate) fn new() -> Self {
        // `md.py:337-339`.
        Self {
            character_count: 0,
            accentuated_count: 0,
        }
    }
}

impl MessDetectorPlugin for TooManyAccentuatedPlugin {
    fn feed_info(&mut self, _c: char, info: &CharInfo) {
        // `md.py:341-346`.
        self.character_count += 1;
        if info.accentuated {
            self.accentuated_count += 1;
        }
    }

    fn reset(&mut self) {
        // `md.py:348-350`.
        self.character_count = 0;
        self.accentuated_count = 0;
    }

    fn ratio(&self) -> f64 {
        // `md.py:352-358`.
        if self.character_count < 8 {
            // `md.py:354`
            return 0.0;
        }
        let ratio_of_accentuation =
            self.accentuated_count as f64 / self.character_count as f64; // `md.py:357`
        if ratio_of_accentuation >= 0.35 {
            // `md.py:358`
            ratio_of_accentuation
        } else {
            0.0
        }
    }
}

// ---------------------------------------------------------------------------
// 3. UnprintablePlugin (`md.py:361-395`)
// ---------------------------------------------------------------------------

pub(crate) struct UnprintablePlugin {
    unprintable_count: usize, // `md.py:362`
    character_count: usize,   // `md.py:362`
    has_escape: bool,         // `md.py:362`
}

impl UnprintablePlugin {
    pub(crate) fn new() -> Self {
        // `md.py:364-367`.
        Self {
            unprintable_count: 0,
            character_count: 0,
            has_escape: false,
        }
    }
}

impl MessDetectorPlugin for UnprintablePlugin {
    fn feed_info(&mut self, c: char, info: &CharInfo) {
        // `md.py:369-381`.
        if c == '\u{1b}' {
            // `md.py:371-372`
            self.has_escape = true;
        }
        if !info.printable && !info.space && c != '\u{1a}' && c != '\u{feff}' {
            // `md.py:374-379`
            self.unprintable_count += 1;
        }
        self.character_count += 1; // `md.py:381`
    }

    fn reset(&mut self) {
        // `md.py:383-385`: NOTE `character_count` is NOT reset.
        self.unprintable_count = 0;
        self.has_escape = false;
    }

    fn ratio(&self) -> f64 {
        // `md.py:387-395`.
        if self.character_count == 0 {
            // `md.py:389-390`
            return 0.0;
        }
        if self.has_escape {
            // `md.py:392-393`
            return 1.0;
        }
        (self.unprintable_count * 8) as f64 / self.character_count as f64 // `md.py:395`
    }
}

// ---------------------------------------------------------------------------
// 4. SuspiciousDuplicateAccentPlugin (`md.py:398-439`)
// ---------------------------------------------------------------------------

pub(crate) struct SuspiciousDuplicateAccentPlugin {
    successive_count: usize, // `md.py:400`
    character_count: usize,  // `md.py:401`
    // Orig keeps `CharInfo | None` (`md.py:403`) and reads `.upper` /
    // `.unaccented` off it; only those two properties are ever read, so only
    // they are stored. `has_last` == `... is not None`.
    last_upper: bool,
    last_unaccented: char,
    has_last: bool,
    last_was_accentuated: bool, // `md.py:404`
}

impl SuspiciousDuplicateAccentPlugin {
    pub(crate) fn new() -> Self {
        // `md.py:406-411`.
        Self {
            successive_count: 0,
            character_count: 0,
            last_upper: false,
            last_unaccented: '\0',
            has_last: false,
            last_was_accentuated: false,
        }
    }
}

impl MessDetectorPlugin for SuspiciousDuplicateAccentPlugin {
    fn feed_info(&mut self, _c: char, info: &CharInfo) {
        // `md.py:413-426`.
        self.character_count += 1;
        if self.has_last && info.accentuated && self.last_was_accentuated {
            // `md.py:416-420`
            if info.upper && self.last_upper {
                // `md.py:421-422`
                self.successive_count += 1;
            }
            if info.unaccented == self.last_unaccented {
                // `md.py:423-424`
                self.successive_count += 1;
            }
        }
        self.last_upper = info.upper; // `md.py:425` (via stored CharInfo)
        self.last_unaccented = info.unaccented;
        self.has_last = true;
        self.last_was_accentuated = info.accentuated; // `md.py:426`
    }

    fn reset(&mut self) {
        // `md.py:428-432`.
        self.successive_count = 0;
        self.character_count = 0;
        self.last_upper = false;
        self.last_unaccented = '\0';
        self.has_last = false;
        self.last_was_accentuated = false;
    }

    fn ratio(&self) -> f64 {
        // `md.py:434-439`.
        if self.character_count == 0 {
            // `md.py:436-437`
            return 0.0;
        }
        (self.successive_count * 2) as f64 / self.character_count as f64 // `md.py:439`
    }
}

// ---------------------------------------------------------------------------
// 5. SuspiciousRange (`md.py:442-496`; note: no `Plugin` suffix in orig)
// ---------------------------------------------------------------------------

pub(crate) struct SuspiciousRange {
    suspicious_successive_range_count: usize, // `md.py:444`
    character_count: usize,                   // `md.py:445`
    last_printable_seen: Option<char>,        // `md.py:446`
    last_printable_range: Option<&'static str>, // `md.py:447`
}

impl SuspiciousRange {
    pub(crate) fn new() -> Self {
        // `md.py:450-454`.
        Self {
            suspicious_successive_range_count: 0,
            character_count: 0,
            last_printable_seen: None,
            last_printable_range: None,
        }
    }
}

impl MessDetectorPlugin for SuspiciousRange {
    fn feed_info(&mut self, c: char, info: &CharInfo) {
        // `md.py:456-479`.
        self.character_count += 1;
        if info.space || info.punct || info.safe {
            // `md.py:460-463`
            self.last_printable_seen = None;
            self.last_printable_range = None;
            return;
        }
        if self.last_printable_seen.is_none() {
            // `md.py:465-468`
            self.last_printable_seen = Some(c);
            self.last_printable_range = info.range;
            return;
        }
        let unicode_range_a = self.last_printable_range; // `md.py:470`
        let unicode_range_b = info.range; // `md.py:471`
        // Identical non-None ranges can never be suspicious (`md.py:473-476`).
        if unicode_range_a != unicode_range_b || unicode_range_a.is_none() {
            if is_suspiciously_successive_range(unicode_range_a, unicode_range_b) {
                self.suspicious_successive_range_count += 1;
            }
        }
        self.last_printable_seen = Some(c); // `md.py:478`
        self.last_printable_range = unicode_range_b; // `md.py:479`
    }

    fn reset(&mut self) {
        // `md.py:481-485`.
        self.character_count = 0;
        self.suspicious_successive_range_count = 0;
        self.last_printable_seen = None;
        self.last_printable_range = None;
    }

    fn ratio(&self) -> f64 {
        // `md.py:487-496`.
        if self.character_count <= 13 {
            // `md.py:489`
            return 0.0;
        }
        (self.suspicious_successive_range_count * 2) as f64 / self.character_count as f64 // `md.py:492-494`
    }
}

// ---------------------------------------------------------------------------
// 6. SuperWeirdWordPlugin (`md.py:499-675`)
// ---------------------------------------------------------------------------

pub(crate) struct SuperWeirdWordPlugin {
    word_count: usize,                   // `md.py:501`
    bad_word_count: usize,               // `md.py:502`
    foreign_long_count: usize,           // `md.py:503`
    is_current_word_bad: bool,           // `md.py:504`
    foreign_long_watch: bool,            // `md.py:505`
    character_count: usize,              // `md.py:506`
    bad_character_count: usize,          // `md.py:507`
    buffer_length: usize,                // `md.py:508`
    buffer_last_char: Option<char>,      // `md.py:509`
    buffer_last_char_accentuated: bool,  // `md.py:510`
    buffer_accent_count: usize,          // `md.py:511`
    buffer_glyph_count: usize,           // `md.py:512`
    buffer_upper_count: usize,           // `md.py:513`
    buffer_first_lower: bool,            // `md.py:514`
    buffer_has_non_ascii: bool,          // `md.py:515`
    buffer_last_char_ligature: bool,     // `md.py:516`
    buffer_has_internal_ligature: bool,  // `md.py:517`
    is_current_word_invalid: bool,       // `md.py:518`
    invalid_word_count: usize,           // `md.py:519`
}

impl SuperWeirdWordPlugin {
    pub(crate) fn new() -> Self {
        // `md.py:522-544`.
        Self {
            word_count: 0,
            bad_word_count: 0,
            foreign_long_count: 0,
            is_current_word_bad: false,
            foreign_long_watch: false,
            character_count: 0,
            bad_character_count: 0,
            buffer_length: 0,
            buffer_last_char: None,
            buffer_last_char_accentuated: false,
            buffer_accent_count: 0,
            buffer_glyph_count: 0,
            buffer_upper_count: 0,
            buffer_first_lower: false,
            buffer_has_non_ascii: false,
            buffer_last_char_ligature: false,
            buffer_has_internal_ligature: false,
            is_current_word_invalid: false,
            invalid_word_count: 0,
        }
    }
}

impl MessDetectorPlugin for SuperWeirdWordPlugin {
    fn feed_info(&mut self, c: char, info: &CharInfo) {
        // `md.py:546-644`.
        if info.alpha {
            // `md.py:548-570`
            if self.buffer_last_char_ligature {
                // `md.py:549-550`
                self.buffer_has_internal_ligature = true;
            }
            self.buffer_last_char_ligature = info.is_ligature; // `md.py:551`
            if self.buffer_length == 0 {
                // `md.py:552-553`
                self.buffer_first_lower = info.lower;
            }
            self.buffer_length += 1; // `md.py:554`
            self.buffer_last_char = Some(c); // `md.py:555`
            if info.upper {
                // `md.py:557-558`
                self.buffer_upper_count += 1;
            }
            if !info.is_ascii {
                // `md.py:559-560`
                self.buffer_has_non_ascii = true;
            }
            self.buffer_last_char_accentuated = info.accentuated; // `md.py:562`
            if info.accentuated {
                // `md.py:564-565`
                self.buffer_accent_count += 1;
            }
            if info.is_glyph {
                // `md.py:566-567`
                self.buffer_glyph_count += 1;
            } else if !self.foreign_long_watch && (!info.latin || info.accentuated) {
                // `md.py:568-569`
                self.foreign_long_watch = true;
            }
            return; // `md.py:570`
        }
        if self.buffer_length == 0 {
            // `md.py:571-572`
            return;
        }
        if info.is_sentence_open_punctuation
            || (info.is_superscript && self.buffer_has_internal_ligature)
        {
            // `md.py:573-577`
            self.is_current_word_bad = true;
            self.is_current_word_invalid = true;
        }
        if info.space || info.punct || info.sep {
            // `md.py:578-635`
            self.word_count += 1; // `md.py:579`
            let buffer_length = self.buffer_length; // `md.py:580`
            self.character_count += buffer_length; // `md.py:582`
            if buffer_length >= 4 {
                // `md.py:584`
                if self.buffer_accent_count as f64 / buffer_length as f64 >= 0.5 {
                    // `md.py:585-586`
                    self.is_current_word_bad = true;
                } else if self.buffer_last_char_accentuated
                    && self.buffer_last_char.map_or(false, |ch| ch.is_uppercase())
                    && self.buffer_upper_count != buffer_length
                {
                    // `md.py:587-593` (`buffer_last_char.isupper()`;
                    // non-None here since the buffer is non-empty)
                    self.foreign_long_count += 1;
                    self.is_current_word_bad = true;
                } else if self.buffer_glyph_count == 1 {
                    // `md.py:594-596`
                    self.is_current_word_bad = true;
                    self.foreign_long_count += 1;
                } else if self.buffer_has_non_ascii
                    && self.buffer_first_lower
                    && self.buffer_upper_count == buffer_length - 1
                {
                    // Inverse capitalization detector (`md.py:597-606`,
                    // see https://github.com/jawah/charset_normalizer/issues/731).
                    self.foreign_long_count += 1;
                    self.is_current_word_bad = true;
                }
            }
            if buffer_length >= 24 && self.foreign_long_watch {
                // `md.py:607`
                let probable_camel_cased = self.buffer_upper_count > 0
                    && self.buffer_upper_count as f64 / buffer_length as f64 <= 0.3; // `md.py:608-611`
                if !probable_camel_cased {
                    // `md.py:613-615`
                    self.foreign_long_count += 1;
                    self.is_current_word_bad = true;
                }
            }
            if self.is_current_word_bad {
                // `md.py:617-620`
                self.bad_word_count += 1;
                self.bad_character_count += buffer_length;
                self.is_current_word_bad = false;
            }
            if self.is_current_word_invalid {
                // `md.py:621-623`
                self.invalid_word_count += 1;
                self.is_current_word_invalid = false;
            }
            self.foreign_long_watch = false; // `md.py:625`
            self.buffer_length = 0; // `md.py:626`
            self.buffer_last_char = None; // `md.py:627`
            self.buffer_last_char_accentuated = false; // `md.py:628`
            self.buffer_accent_count = 0; // `md.py:629`
            self.buffer_glyph_count = 0; // `md.py:630`
            self.buffer_upper_count = 0; // `md.py:631`
            self.buffer_first_lower = false; // `md.py:632`
            self.buffer_has_non_ascii = false; // `md.py:633`
            self.buffer_last_char_ligature = false; // `md.py:634`
            self.buffer_has_internal_ligature = false; // `md.py:635`
        } else if !matches!(c, '<' | '>' | '-' | '=' | '~' | '|' | '_')
            && !info.digit
            && info.sym
        {
            // `md.py:636-644`
            self.is_current_word_bad = true;
            self.buffer_length += 1;
            self.buffer_last_char = Some(c);
            self.buffer_last_char_accentuated = false;
        }
    }

    fn reset(&mut self) {
        // `md.py:646-665`.
        self.buffer_length = 0;
        self.buffer_last_char = None;
        self.buffer_last_char_accentuated = false;
        self.is_current_word_bad = false;
        self.foreign_long_watch = false;
        self.bad_word_count = 0;
        self.word_count = 0;
        self.character_count = 0;
        self.bad_character_count = 0;
        self.foreign_long_count = 0;
        self.buffer_accent_count = 0;
        self.buffer_glyph_count = 0;
        self.buffer_upper_count = 0;
        self.buffer_first_lower = false;
        self.buffer_has_non_ascii = false;
        self.buffer_last_char_ligature = false;
        self.buffer_has_internal_ligature = false;
        self.is_current_word_invalid = false;
        self.invalid_word_count = 0;
    }

    fn ratio(&self) -> f64 {
        // `md.py:667-675`.
        if self.invalid_word_count != 0 {
            // `md.py:669` (`if self._invalid_word_count:`)
            return 1.0;
        }
        if self.word_count <= 10 && self.foreign_long_count == 0 {
            // `md.py:672-673`
            return 0.0;
        }
        // `md.py:675`; float division (no integer-division panic possible).
        self.bad_character_count as f64 / self.character_count as f64
    }
}

// ---------------------------------------------------------------------------
// 7. CjkUncommonPlugin (`md.py:678-711`)
// ---------------------------------------------------------------------------

/// Detect messy CJK text that probably means nothing (`md.py:678-681`).
pub(crate) struct CjkUncommonPlugin {
    character_count: usize, // `md.py:683`
    uncommon_count: usize,  // `md.py:683`
}

impl CjkUncommonPlugin {
    pub(crate) fn new() -> Self {
        // `md.py:685-687`.
        Self {
            character_count: 0,
            uncommon_count: 0,
        }
    }
}

impl MessDetectorPlugin for CjkUncommonPlugin {
    fn feed_info(&mut self, _c: char, info: &CharInfo) {
        // `md.py:689-694`.
        self.character_count += 1;
        if !info.common_cjk {
            self.uncommon_count += 1;
        }
    }

    fn reset(&mut self) {
        // `md.py:696-698`.
        self.character_count = 0;
        self.uncommon_count = 0;
    }

    fn ratio(&self) -> f64 {
        // `md.py:700-711`.
        if self.character_count < 4 {
            // `md.py:702-703`
            return 0.0;
        }
        // `md.py:705-707`: integer numerator, single float division by
        // `5 * max(character_count, 16)`.
        let uncommon_form_usage = (2 * self.uncommon_count as i64 - self.character_count as i64)
            as f64
            / (5 * self.character_count.max(16)) as f64;
        // `md.py:711`.
        uncommon_form_usage.max(0.0)
    }
}

// ---------------------------------------------------------------------------
// 8. SuspiciousKatakanaPlugin (`md.py:714-757`)
// ---------------------------------------------------------------------------

/// Detect implausible halfwidth Katakana and uncommon CJK combinations
/// (`md.py:714-715`).
pub(crate) struct SuspiciousKatakanaPlugin {
    katakana_count: usize,           // `md.py:718`
    halfwidth_katakana_count: usize, // `md.py:719`
    cjk_count: usize,                // `md.py:720`
    uncommon_cjk_count: usize,       // `md.py:721`
}

impl SuspiciousKatakanaPlugin {
    pub(crate) fn new() -> Self {
        // `md.py:724-728`.
        Self {
            katakana_count: 0,
            halfwidth_katakana_count: 0,
            cjk_count: 0,
            uncommon_cjk_count: 0,
        }
    }
}

impl MessDetectorPlugin for SuspiciousKatakanaPlugin {
    fn feed_info(&mut self, _c: char, info: &CharInfo) {
        // `md.py:730-740`.
        if info.is_katakana {
            self.katakana_count += 1; // `md.py:733`
            if info.is_halfwidth_katakana {
                // `md.py:734-735`
                self.halfwidth_katakana_count += 1;
            }
            return; // `md.py:736`
        }
        self.cjk_count += 1; // `md.py:738`
        if !info.common_cjk {
            // `md.py:739-740`
            self.uncommon_cjk_count += 1;
        }
    }

    fn reset(&mut self) {
        // `md.py:742-746`.
        self.katakana_count = 0;
        self.halfwidth_katakana_count = 0;
        self.cjk_count = 0;
        self.uncommon_cjk_count = 0;
    }

    fn ratio(&self) -> f64 {
        // `md.py:748-757`; `3 <= cjk == uncommon` is a chained comparison.
        if self.halfwidth_katakana_count >= 4 // `md.py:751`
            && self.halfwidth_katakana_count == self.katakana_count // `md.py:752`
            && self.cjk_count >= 3 // `md.py:753` (first half of the chain)
            && self.cjk_count == self.uncommon_cjk_count
        // `md.py:753` (second half of the chain)
        {
            return 1.0; // `md.py:755`
        }
        0.0 // `md.py:757`
    }
}

// ---------------------------------------------------------------------------
// 9. ArchaicUpperLowerPlugin (`md.py:760-844`)
// ---------------------------------------------------------------------------

pub(crate) struct ArchaicUpperLowerPlugin {
    buf: bool,                               // `md.py:762`
    character_count_since_last_sep: usize,   // `md.py:763`
    successive_upper_lower_count: usize,     // `md.py:764`
    successive_upper_lower_count_final: usize, // `md.py:765`
    character_count: usize,                  // `md.py:766`
    last_alpha_seen_upper: bool,             // `md.py:767`
    last_alpha_seen_lower: bool,             // `md.py:768`
    current_ascii_only: bool,                // `md.py:769`
}

impl ArchaicUpperLowerPlugin {
    pub(crate) fn new() -> Self {
        // `md.py:772-784`.
        Self {
            buf: false,
            character_count_since_last_sep: 0,
            successive_upper_lower_count: 0,
            successive_upper_lower_count_final: 0,
            character_count: 0,
            last_alpha_seen_upper: false,
            last_alpha_seen_lower: false,
            current_ascii_only: true,
        }
    }
}

impl MessDetectorPlugin for ArchaicUpperLowerPlugin {
    fn feed_info(&mut self, _c: char, info: &CharInfo) {
        // `md.py:786-827`.
        let is_concerned = info.alpha && info.case_variable; // `md.py:788`
        let chunk_sep = !is_concerned; // `md.py:789`
        if chunk_sep && self.character_count_since_last_sep > 0 {
            // `md.py:791-807`
            if self.character_count_since_last_sep <= 64 // `md.py:793`
                && !info.digit // `md.py:794`
                && !self.current_ascii_only
            // `md.py:795`
            {
                self.successive_upper_lower_count_final += self.successive_upper_lower_count; // `md.py:797-799`
            }
            self.successive_upper_lower_count = 0; // `md.py:801`
            self.character_count_since_last_sep = 0; // `md.py:802`
            self.buf = false; // `md.py:803`
            self.character_count += 1; // `md.py:804`
            self.current_ascii_only = true; // `md.py:805`
            return; // `md.py:807`
        }
        if self.current_ascii_only && !info.is_ascii {
            // `md.py:809-810`
            self.current_ascii_only = false;
        }
        if self.character_count_since_last_sep > 0 {
            // `md.py:812`
            if (info.upper && self.last_alpha_seen_lower)
                || (info.lower && self.last_alpha_seen_upper)
            {
                // `md.py:813-815`
                if self.buf {
                    // `md.py:816-818`
                    self.successive_upper_lower_count += 2;
                    self.buf = false;
                } else {
                    // `md.py:819-820`
                    self.buf = true;
                }
            } else {
                // `md.py:821-822`
                self.buf = false;
            }
        }
        self.character_count += 1; // `md.py:824`
        self.character_count_since_last_sep += 1; // `md.py:825`
        self.last_alpha_seen_upper = info.upper; // `md.py:826`
        self.last_alpha_seen_lower = info.lower; // `md.py:827`
    }

    fn reset(&mut self) {
        // `md.py:829-837`.
        self.character_count = 0;
        self.character_count_since_last_sep = 0;
        self.successive_upper_lower_count = 0;
        self.successive_upper_lower_count_final = 0;
        self.last_alpha_seen_upper = false;
        self.last_alpha_seen_lower = false;
        self.buf = false;
        self.current_ascii_only = true;
    }

    fn ratio(&self) -> f64 {
        // `md.py:839-844`.
        if self.character_count == 0 {
            // `md.py:841-842`
            return 0.0;
        }
        self.successive_upper_lower_count_final as f64 / self.character_count as f64 // `md.py:844`
    }
}

// ---------------------------------------------------------------------------
// 10. ArabicIsolatedFormPlugin (`md.py:847-872`)
// ---------------------------------------------------------------------------

pub(crate) struct ArabicIsolatedFormPlugin {
    character_count: usize,     // `md.py:848`
    isolated_form_count: usize, // `md.py:848`
}

impl ArabicIsolatedFormPlugin {
    pub(crate) fn new() -> Self {
        // `md.py:850-852`.
        Self {
            character_count: 0,
            isolated_form_count: 0,
        }
    }
}

impl MessDetectorPlugin for ArabicIsolatedFormPlugin {
    fn feed_info(&mut self, _c: char, info: &CharInfo) {
        // `md.py:858-863`.
        self.character_count += 1;
        if info.flags & FLAG_ARABIC_ISOLATED_FORM != 0 {
            // `md.py:862-863`
            self.isolated_form_count += 1;
        }
    }

    fn reset(&mut self) {
        // `md.py:854-856`.
        self.character_count = 0;
        self.isolated_form_count = 0;
    }

    fn ratio(&self) -> f64 {
        // `md.py:865-872`.
        if self.character_count < 8 {
            // `md.py:867-868`
            return 0.0;
        }
        self.isolated_form_count as f64 / self.character_count as f64 // `md.py:870`
    }
}

// ---------------------------------------------------------------------------
// Range compatibility (`constant.py:731-782`, used by `md.py:875-908`)
// ---------------------------------------------------------------------------

/// Unordered compatible family pairs, verbatim from
/// `_COMPATIBLE_RANGE_FAMILIES` (`constant.py:747-774`). Orig models each pair
/// as `CompatibleFamillyRange` (`constant.py:731-744`, frozenset equality, i.e.
/// order-insensitive); the lookup below checks both orders, which is exactly
/// equivalent without hashing.
const COMPATIBLE_RANGE_PAIRS: &[(&str, &str)] = &[
    ("CJK", "Hiragana"),                          // `constant.py:749`
    ("CJK", "Katakana"),                          // `constant.py:750`
    ("CJK", "Kana"),                              // `constant.py:751`
    ("CJK", "Bopomofo"),                          // `constant.py:752`
    ("CJK", "Kanbun"),                            // `constant.py:753`
    ("Hiragana", "Katakana"),                     // `constant.py:754`
    ("Hiragana", "Kana"),                         // `constant.py:755`
    ("Katakana", "Kana"),                         // `constant.py:756`
    ("CJK", "Hangul"),                            // `constant.py:757`
    ("Latin", "IPA"),                             // `constant.py:758`
    ("Latin", "Phonetic"),                        // `constant.py:759`
    ("IPA", "Phonetic"),                          // `constant.py:760`
    ("Latin", "Spacing Modifier Letters"),        // `constant.py:761`
    ("IPA", "Spacing Modifier Letters"),          // `constant.py:762`
    ("Phonetic", "Spacing Modifier Letters"),     // `constant.py:763`
    ("Alphabetic Presentation Forms", "Latin"),   // `constant.py:764`
    ("Alphabetic Presentation Forms", "Armenian"), // `constant.py:765`
    ("Alphabetic Presentation Forms", "Hebrew"),  // `constant.py:766`
    ("Halfwidth and Fullwidth Forms", "Latin"),   // `constant.py:767`
    ("Halfwidth and Fullwidth Forms", "CJK"),     // `constant.py:768`
    ("Halfwidth and Fullwidth Forms", "Hiragana"), // `constant.py:769`
    ("Halfwidth and Fullwidth Forms", "Katakana"), // `constant.py:770`
    ("Halfwidth and Fullwidth Forms", "Kana"),    // `constant.py:771`
    ("Halfwidth and Fullwidth Forms", "Hangul"),  // `constant.py:772`
];

/// Verbatim from `_COMPATIBLE_WITH_ANY_RANGE_FAMILIES` (`constant.py:776-778`).
const COMPATIBLE_WITH_ANY_RANGE_FAMILIES: &[&str] = &[
    "Combining",
    "Variation Selectors",
    "Emoticons",
    "Pictographs",
];

/// Verbatim from `_BASIC_LATIN_COMPATIBLE_RANGE_FAMILIES`
/// (`constant.py:780-782`).
const BASIC_LATIN_COMPATIBLE_RANGE_FAMILIES: &[&str] = &[
    "CJK",
    "Hangul",
    "Hiragana",
    "Katakana",
    "Kana",
    "Bopomofo",
    "Kanbun",
];

/// Determine if two Unicode ranges seen next to each other can be considered
/// suspicious (`md.py:875-908`), branch-for-branch.
pub(crate) fn is_suspiciously_successive_range(
    unicode_range_a: Option<&str>,
    unicode_range_b: Option<&str>,
) -> bool {
    if unicode_range_a.is_none() || unicode_range_b.is_none() {
        // `md.py:882-883`
        return true;
    }
    let unicode_range_a = unicode_range_a.unwrap();
    let unicode_range_b = unicode_range_b.unwrap();
    // `md.py:885-886` index `_RANGE_FAMILIES` directly (KeyError when absent);
    // absent mappings defensively count as suspicious (see header risk 6).
    let familly_a = match range_family(unicode_range_a) {
        Some(f) => f,
        None => return true,
    };
    let familly_b = match range_family(unicode_range_b) {
        Some(f) => f,
        None => return true,
    };
    if familly_a == familly_b {
        // `md.py:888-889`
        return false;
    }
    if COMPATIBLE_WITH_ANY_RANGE_FAMILIES.contains(&familly_a)
        || COMPATIBLE_WITH_ANY_RANGE_FAMILIES.contains(&familly_b)
    {
        // `md.py:891-895`
        return false;
    }
    // `md.py:897-898`: `CompatibleFamillyRange(a, b) in _COMPATIBLE_...`
    // with frozenset (order-insensitive) equality.
    if COMPATIBLE_RANGE_PAIRS.contains(&(familly_a, familly_b))
        || COMPATIBLE_RANGE_PAIRS.contains(&(familly_b, familly_a))
    {
        return false;
    }
    // Basic Latin is commonly interspersed with East Asian scripts, but the
    // compatibility must not extend to every range in the Latin family.
    if unicode_range_a == "Basic Latin" {
        // `md.py:902-903`
        return !BASIC_LATIN_COMPATIBLE_RANGE_FAMILIES.contains(&familly_b);
    }
    if unicode_range_b == "Basic Latin" {
        // `md.py:905-906`
        return !BASIC_LATIN_COMPATIBLE_RANGE_FAMILIES.contains(&familly_a);
    }
    true // `md.py:908`
}

// ---------------------------------------------------------------------------
// mess_ratio (`md.py:911-1065`)
// ---------------------------------------------------------------------------

/// Compute a mess ratio given a decoded sequence. The maximum threshold stops
/// the computation early (`md.py:911-916`).
///
/// Chunked evaluation with per-block early exit (`md.py:968-1026`), a trailing
/// newline flush when the scan completes (`md.py:1027-1046`), and round-3
/// (`md.py:1065`).
pub(crate) fn mess_ratio(decoded_sequence: &str, maximum_threshold: f64) -> f64 {
    let seq_len = decoded_sequence.chars().count(); // `md.py:918` (`len()` counts code points)
    // `md.py:920-925`.
    let step = if seq_len < 511 {
        32
    } else if seq_len < 1024 {
        64
    } else {
        128
    };
    // `md.py:930`: `str.isascii()`; the seven detectors that provably keep a
    // 0.0 ratio on ASCII-only input are then not fed at all (`md.py:983-987`).
    let is_pure_ascii = decoded_sequence.is_ascii();

    // Ten detectors as named locals (`md.py:945-954`).
    let mut d_sp = TooManySymbolOrPunctuationPlugin::new();
    let mut d_ta = TooManyAccentuatedPlugin::new();
    let mut d_up = UnprintablePlugin::new();
    let mut d_sda = SuspiciousDuplicateAccentPlugin::new();
    let mut d_sr = SuspiciousRange::new();
    let mut d_sw = SuperWeirdWordPlugin::new();
    let mut d_cu = CjkUncommonPlugin::new();
    let mut d_sk = SuspiciousKatakanaPlugin::new();
    let mut d_au = ArchaicUpperLowerPlugin::new();
    let mut d_ai = ArabicIsolatedFormPlugin::new();

    let mut mean_mess_ratio = 0.0f64;
    // `True` when the scan below runs to completion without `break`
    // (i.e. Python's `for...else` takes the `else`, `md.py:1027`).
    let mut completed = true;
    let mut fed_in_block = 0usize;
    let mut fed_total = 0usize;
    // `md.py:968-969`: `for block_start in range(0, seq_len, step)` with the
    // inner slice `decoded_sequence[block_start:block_start + step]`; block
    // boundaries for the early-exit checkpoints are reproduced with counters.
    for c in decoded_sequence.chars() {
        // Character properties computed once per codepoint, shared across all
        // plugins (`md.py:970-977`; the ASCII-table/`lru_cache` split is
        // integrator wiring — values are identical either way).
        let info = char_info(c);
        // Detectors with `eligible() == always True` (`md.py:979-981`).
        d_up.feed_info(c, &info);
        d_sw.feed_info(c, &info);
        if is_pure_ascii {
            // `md.py:983-987`: the seven remaining detectors provably stay
            // at 0.0 (only `d_sp` sees printable characters).
            if info.printable {
                d_sp.feed_info(c, &info);
            }
        } else {
            d_au.feed_info(c, &info); // `md.py:989`
            // Detectors with `eligible() == isprintable` (`md.py:991-994`).
            if info.printable {
                d_sp.feed_info(c, &info);
                d_sr.feed_info(c, &info);
            }
            // Detectors with `eligible() == isalpha` (`md.py:996-1010`).
            if info.alpha {
                d_ta.feed_info(c, &info); // `md.py:998`
                // SuspiciousDuplicateAccent: isalpha() and is_latin()
                // (`md.py:999-1001`).
                if info.latin {
                    d_sda.feed_info(c, &info);
                }
                // CjkUncommon and SuspiciousKatakana: is_cjk()
                // (`md.py:1002-1007`).
                if info.is_cjk {
                    d_cu.feed_info(c, &info);
                    d_sk.feed_info(c, &info);
                } else if info.is_katakana {
                    d_sk.feed_info(c, &info);
                }
                // ArabicIsolatedForm: is_arabic() (`md.py:1008-1010`).
                if info.is_arabic {
                    d_ai.feed_info(c, &info);
                }
            }
        }
        fed_in_block += 1;
        fed_total += 1;
        if fed_in_block == step || fed_total == seq_len {
            // `md.py:1012-1023`: the ten-term sum, order verbatim.
            mean_mess_ratio = d_sp.ratio()
                + d_ta.ratio()
                + d_up.ratio()
                + d_sda.ratio()
                + d_sr.ratio()
                + d_sw.ratio()
                + d_cu.ratio()
                + d_sk.ratio()
                + d_au.ratio()
                + d_ai.ratio();
            if mean_mess_ratio >= maximum_threshold {
                // `md.py:1025-1026`
                completed = false;
                break;
            }
            fed_in_block = 0;
        }
    }
    if completed {
        // Flush last word buffer in SuperWeirdWordPlugin via trailing newline
        // (`md.py:1027-1033`; `ascii_info[10]` is `char_info('\n')`).
        let nl_info = char_info('\n'); // `md.py:1029`
        d_sw.feed_info('\n', &nl_info); // `md.py:1030`
        if !is_pure_ascii {
            d_au.feed_info('\n', &nl_info); // `md.py:1031-1032`
        }
        d_up.feed_info('\n', &nl_info); // `md.py:1033`
        // `md.py:1035-1046`: same ten-term sum, order verbatim.
        mean_mess_ratio = d_sp.ratio()
            + d_ta.ratio()
            + d_up.ratio()
            + d_sda.ratio()
            + d_sr.ratio()
            + d_sw.ratio()
            + d_cu.ratio()
            + d_sk.ratio()
            + d_au.ratio()
            + d_ai.ratio();
    }
    // `md.py:1065` (`round(mean_mess_ratio, 3)`; see header risk 7).
    (mean_mess_ratio * 1000.0).round() / 1000.0
}
