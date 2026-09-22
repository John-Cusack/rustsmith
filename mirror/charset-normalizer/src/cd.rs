//! Port of `charset_normalizer/cd.py` (3.5.1): language coherence detection.
//!
//! Behavior-identical port; oracle parity with `cd.py` is the bar. Control flow,
//! thresholds, early-exits and rounding mirror the original branch-for-branch.
//! Per-function origin lines are cited on each item (`cd.py:<line>`).
//!
//! # Cache mapping (`functools.lru_cache` -> `OnceLock` statics)
//! - `cd.py:89` `encoding_languages` (`@lru_cache()`, unbounded) -> `ENCODING_LANGUAGES`
//! - `cd.py:113` `mb_encoding_languages` (`@lru_cache()`, unbounded) -> `MB_ENCODING_LANGUAGES`
//! - `cd.py:134` `get_target_features` (`@lru_cache(maxsize=41)`) -> `GET_TARGET_FEATURES`
//!   (41 == `LANGUAGE_SUPPORTED_COUNT`; the key space is the 41 supported
//!   languages, so the map is naturally bounded like the original).
//! `OnceLock<Mutex<HashMap>>` (rather than bare `OnceLock<HashMap>`) because the
//! caches are populated lazily on first miss. Hits clone the cached value.
//! Deterministic: lock only serializes fills; results equal single-threaded order.
//!
//! # Name risks / deliberate divergences (fidelity notes)
//! - `str.isalpha()` (cd.py:309 via `md.CharInfo.alpha`, built on CPython
//!   `str.isalpha`) is approximated by Rust `char::is_alphabetic` for non-ASCII.
//!   Both implement the UCD Alphabetic property; disagreement is expected to be
//!   nil on real text but this is NOT proven identical codepoint-for-codepoint.
//! - `accentuated`/`latin` per character come from `tables_ucd::ucd_lookup`
//!   flags, whose bit 0 (`_LATIN`) / bit 1 (`_ACCENTUATED`) layout matches
//!   `constant.py:2423` (`_LATIN = 1`, `_ACCENTUATED = 1 << 1`) as built from the
//!   single `unicodedata.name()` call in `utils._character_flags`. ASCII keeps
//!   the `md.py:117-159` fast path exactly (letters => latin, never accentuated).
//! - `unicode_range` is re-derived here from `UNICODE_RANGES` by binary search,
//!   mirroring `utils.py:104` (`bisect_right` + `ord < stop`); the frozen table
//!   already starts with `(0, 32, "Control character")`, `(32, 128, "Basic Latin")`,
//!   so the `ord < 32 / < 128` special cases fall out of the same search.
//! - `is_suspiciously_successive_range` (orig `md.py:876`, `@lru_cache`) is
//!   re-implemented locally as a private helper from `range_family()` plus the
//!   frozen compatible-family sets copied verbatim from `constant.py:747-787`.
//!   No memoization: the call sites only invoke it on range transitions over a
//!   handful of live layers, so caching is behavior-neutral perf only.
//! - `md.py`'s `alpha_unicode_split` fast-path caches (`single_layer_key`,
//!   `prev_character_range`; cd.py:290-297,317-322,348-350) are elided: appending
//!   to a layer never changes layer keys, so recomputing the layer target gives
//!   the identical target. Iteration/insertion orders are preserved exactly.
//! - `str.lower()` (cd.py:352) is `str::to_lowercase()`; both are locale-
//!   independent full Unicode lowercasings.
//! - `round(x, 4)` (banker's rounding in Python) is `(x*10000).round()/10000`
//!   (half-away). See ROUNDING comments; exact-half-at-5th-decimal inputs could
//!   differ in the last ulp bucket. Same for `round(mean, 4)` in merging.
//! - `encoding_unicode_range` probing goes through
//!   `super::decoders::decode_ignore_opt` (one fresh stateless call per byte)
//!   instead of a reused `IncrementalDecoder` with `errors="ignore"`; the
//!   contract holder confirms per-byte fresh state == reused decoder state for
//!   these stateless single-byte codecs.
//! - Defensive-only paths: a multibyte name passed to `encoding_unicode_range`
//!   is `Err(ProbeError::MultiByte)` (orig `OSError`, cd.py:29-31); like the
//!   original, `encoding_languages` does NOT swallow it (it only catches the
//!   unknown-codec case, cd.py:95-98), so it panics with the original message.
//!   Unreachable via the `models.py:156-168` dispatch, which checks
//!   `is_multi_byte_encoding` first.
//! - A zero `character_count` in `encoding_unicode_range` (no probed byte
//!   decoded to a ranged character) returns `Ok(vec![])`; Python would raise
//!   `ZeroDivisionError` on the `count / 0` ratio. Unreachable for real codecs
//!   (0x40-0x7E always decode to ranged characters).
//! - Sorts below MUST stay stable (`sort_by`, never `sort_unstable_by`) and
//!   descending: tie order (first-seen) feeds `models.py` ranking. See ORDER.
//! - Language iteration MUST follow `tables_constant::FREQUENCY_LANGS` (file
//!   order == `FREQUENCIES` insertion order, verified 41/41 against
//!   `list(FREQUENCIES.keys())`): oracle diff proved any other order diverges
//!   tie-breaks. `alphabet_languages` and all tie-breaks depend on it.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use super::decoders::decode_ignore_opt;
use super::tables_constant::{
    FREQUENCY_LANGS, KO_NAMES, LANGUAGE_SUPPORTED_COUNT, SECONDARY_RANGE_NAMES,
    TOO_SMALL_SEQUENCE, UNICODE_RANGES, ZH_NAMES, frequencies, frequency_rank,
    is_multi_byte_encoding, range_family,
};
use super::tables_ucd::ucd_lookup;

/// Orig `models.py:337-338`: `CoherenceMatch = Tuple[str, float]`,
/// `CoherenceMatches = List[CoherenceMatch]`. Element order is significant:
/// descending by ratio, ties in first-seen order (see ORDER comments).
pub(crate) type CoherenceMatches = Vec<(String, f64)>;

/// Error paths of `encoding_unicode_range` (cd.py:24).
/// - `MultiByte` <=> `OSError("Function not supported on multi-byte code page")`
///   (cd.py:28-31).
/// - `UnknownEncoding` <=> `ImportError` from `importlib.import_module`
///   (caught by `encoding_languages`, cd.py:95-98).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeError {
    MultiByte,
    UnknownEncoding,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::MultiByte => {
                write!(f, "Function not supported on multi-byte code page")
            }
            ProbeError::UnknownEncoding => write!(f, "Encoding unavailable on this build"),
        }
    }
}

/// OnceLock statics named after the cached fns (see module docs).
static ENCODING_LANGUAGES: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
static MB_ENCODING_LANGUAGES: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
static GET_TARGET_FEATURES: OnceLock<Mutex<HashMap<String, (bool, bool)>>> = OnceLock::new();
/// Minimal per-character view mirroring the `md.CharInfo` fields used by this
/// module: `alpha` (cd.py:309), `accentuated` (cd.py:145,163),
/// `latin` (cd.py:147), `range` (cd.py:44-48,312).
struct CharProps {
    alpha: bool,
    accentuated: bool,
    latin: bool,
    range: Option<&'static str>,
}

fn char_props(ch: char) -> CharProps {
    let cp = ch as u32;
    if cp < 128 {
        // md.py:117-159 ASCII fast path: only A-Z/a-z are alpha+latin; ASCII is
        // never accentuated.
        let is_letter = matches!(cp, 65..=90 | 97..=122);
        return CharProps {
            alpha: is_letter,
            accentuated: false,
            latin: is_letter,
            range: unicode_range_of(cp),
        };
    }
    let (flags, ..) = ucd_lookup(cp);
    CharProps {
        // RISK: CPython str.isalpha() vs UCD Alphabetic; see module docs.
        alpha: ch.is_alphabetic(),
        accentuated: flags & 2 != 0, // _ACCENTUATED = 1 << 1 (constant.py:2424)
        latin: flags & 1 != 0,       // _LATIN = 1 (constant.py:2423)
        range: unicode_range_of(cp),
    }
}

/// Orig `utils.py:104` `unicode_range`: `ord < 32` -> "Control character",
/// `ord < 128` -> "Basic Latin", else rightmost range with `start <= ord` and
/// `ord < stop`, else `None`. The frozen `UNICODE_RANGES` table is sorted,
/// non-overlapping `(start, stop_exclusive, name)` and already opens with the
/// two special-case rows, so one interval binary search reproduces all branches.
fn unicode_range_of(cp: u32) -> Option<&'static str> {
    let mut lo = 0usize;
    let mut hi = UNICODE_RANGES.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let (start, stop, name) = UNICODE_RANGES[mid];
        if cp < start {
            hi = mid;
        } else if cp >= stop {
            lo = mid + 1;
        } else {
            return Some(name);
        }
    }
    None
}

/// Orig `utils.py:211` `is_unicode_range_secondary`: membership in the
/// precomputed `_SECONDARY_RANGE_NAMES` frozenset; here the frozen
/// `SECONDARY_RANGE_NAMES` table.
fn is_secondary(range_name: &str) -> bool {
    SECONDARY_RANGE_NAMES.contains(&range_name)
}

/// Orig `md.py:876` `is_suspiciously_successive_range` (with `@lru_cache`,
/// elided here as behavior-neutral; see module docs). Argument order matters
/// (`discovered_range`, `character_range`; the Basic Latin arms are asymmetric).
fn is_suspiciously_successive_range(
    unicode_range_a: Option<&str>,
    unicode_range_b: Option<&str>,
) -> bool {
    let (Some(a), Some(b)) = (unicode_range_a, unicode_range_b) else {
        return true; // md.py:884-885
    };
    let (Some(family_a), Some(family_b)) = (range_family(a), range_family(b)) else {
        return true; // No orig branch (table is exhaustive); fail suspicious.
    };
    if family_a == family_b {
        return false; // md.py:890-891
    }
    // md.py:893-898 `_COMPATIBLE_WITH_ANY_RANGE_FAMILIES`.
    if family_a == "Combining"
        || family_b == "Combining"
        || family_a == "Variation Selectors"
        || family_b == "Variation Selectors"
        || family_a == "Emoticons"
        || family_b == "Emoticons"
        || family_a == "Pictographs"
        || family_b == "Pictographs"
    {
        return false;
    }
    if compatible_families(family_a, family_b) {
        return false; // md.py:900-901
    }
    // md.py:905-912: Basic Latin mixes freely with East Asian scripts only.
    if a == "Basic Latin" {
        return !basic_latin_compatible(family_b);
    }
    if b == "Basic Latin" {
        return !basic_latin_compatible(family_a);
    }
    true // md.py:914
}

/// Unordered membership in `_COMPATIBLE_RANGE_FAMILIES` (constant.py:747-775).
fn compatible_families(family_a: &str, family_b: &str) -> bool {
    let (x, y) = if family_a <= family_b {
        (family_a, family_b)
    } else {
        (family_b, family_a)
    };
    matches!(
        (x, y),
        ("Alphabetic Presentation Forms", "Armenian")
            | ("Alphabetic Presentation Forms", "Hebrew")
            | ("Alphabetic Presentation Forms", "Latin")
            | ("Bopomofo", "CJK")
            | ("CJK", "Halfwidth and Fullwidth Forms")
            | ("CJK", "Hangul")
            | ("CJK", "Hiragana")
            | ("CJK", "Kana")
            | ("CJK", "Kanbun")
            | ("CJK", "Katakana")
            | ("Halfwidth and Fullwidth Forms", "Hangul")
            | ("Halfwidth and Fullwidth Forms", "Hiragana")
            | ("Halfwidth and Fullwidth Forms", "Kana")
            | ("Halfwidth and Fullwidth Forms", "Katakana")
            | ("Halfwidth and Fullwidth Forms", "Latin")
            | ("Hiragana", "Kana")
            | ("Hiragana", "Katakana")
            | ("IPA", "Latin")
            | ("IPA", "Phonetic")
            | ("IPA", "Spacing Modifier Letters")
            | ("Kana", "Katakana")
            | ("Latin", "Phonetic")
            | ("Latin", "Spacing Modifier Letters")
            | ("Phonetic", "Spacing Modifier Letters")
    )
}

/// `_BASIC_LATIN_COMPATIBLE_RANGE_FAMILIES` (constant.py:780-782).
fn basic_latin_compatible(family: &str) -> bool {
    matches!(
        family,
        "CJK" | "Hangul" | "Hiragana" | "Katakana" | "Kana" | "Bopomofo" | "Kanbun"
    )
}

/// ROUNDING point: `round(x, 4)` (cd.py:372-375,460). Half-away via
/// scale-round-unscale; see module docs on banker's-rounding divergence.
fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

/// ORDER: descending by ratio, STABLE (ties keep first-seen/insertion order).
/// Mirrors `sorted(..., key=lambda x: x[1], reverse=True)` (cd.py:380,465),
/// whose stability preserves dict/insertion order on ties. MUST stay `sort_by`.
fn sort_desc(matches: &mut CoherenceMatches) {
    matches.sort_by(|a, b| b.1.total_cmp(&a.1));
}

/// Orig cd.py:24 `encoding_unicode_range`. Probes bytes `0x40..0xFF`
/// (`range(0x40, 0xFF)`, stop exclusive) through the ignore-errors decoder,
/// skipping empty chunks (cd.py:42 `if chunk:`) and rangeless characters
/// (cd.py:50-51), ignoring secondary ranges (cd.py:52), and keeping ranges with
/// `count / total >= 0.15` (cd.py:59-65), returned `sorted()`.
pub(crate) fn encoding_unicode_range(iana_name: &str) -> Result<Vec<String>, ProbeError> {
    if is_multi_byte_encoding(iana_name) {
        return Err(ProbeError::MultiByte); // cd.py:28-31
    }
    let mut seen: HashMap<&'static str, usize> = HashMap::new();
    let mut character_count: usize = 0;
    for byte in 0x40u8..0xFFu8 {
        // `None` == unknown codec == orig `ImportError` (cd.py:95-98 path).
        let chunk = match decode_ignore_opt(iana_name, &[byte]) {
            None => return Err(ProbeError::UnknownEncoding),
            Some(chunk) => chunk,
        };
        if chunk.is_empty() {
            continue; // cd.py:42
        }
        // Single-byte decode of one byte yields one char (`ord(chunk)`, cd.py:43).
        let ch = chunk.chars().next().expect("non-empty chunk has a char");
        let Some(character_range) = unicode_range_of(ch as u32) else {
            continue; // cd.py:50-51
        };
        if is_secondary(character_range) {
            continue; // cd.py:52-53
        }
        *seen.entry(character_range).or_insert(0) += 1; // cd.py:54-56
        character_count += 1; // cd.py:57
    }
    if character_count == 0 {
        return Ok(Vec::new()); // Defensive; Python raises ZeroDivisionError (see docs).
    }
    let mut out: Vec<String> = seen
        .iter()
        .filter(|(_, &n)| n as f64 / character_count as f64 >= 0.15) // cd.py:63
        .map(|(r, _)| r.to_string())
        .collect();
    out.sort(); // cd.py:59 `sorted()`
    Ok(out)
}

/// Orig cd.py:68 `unicode_range_languages`. Iterates languages in `FREQUENCIES`
/// insertion order (`FREQUENCY_LANGS`); first character whose range equals the
/// primary range associates the language (cd.py:74-84, `break` per language).
pub(crate) fn unicode_range_languages(primary_range: &str) -> Vec<String> {
    let mut languages: Vec<String> = Vec::new();
    for language in FREQUENCY_LANGS {
        let freq = frequencies(language).expect("FREQUENCY_LANGS must match frequencies()");
        for ch in freq.chars() {
            if char_props(ch).range == Some(primary_range) {
                languages.push(language.to_string());
                break; // cd.py:84
            }
        }
    }
    languages
}

/// Orig cd.py:89 `encoding_languages` (`@lru_cache()` -> `ENCODING_LANGUAGES`).
/// Unknown codec -> `[]` (cd.py:95-98); no non-Latin range -> `["Latin Based"]`
/// (cd.py:107-108); else primary (first non-`"Latin"`) range's languages
/// (cd.py:102-110). The multibyte `Err` is NOT caught here (uncaught `OSError`
/// analog: panic with the original message).
pub(crate) fn encoding_languages(iana_name: &str) -> Vec<String> {
    if let Some(hit) = ENCODING_LANGUAGES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(iana_name)
    {
        return hit.clone();
    }
    let result = match encoding_unicode_range(iana_name) {
        Err(ProbeError::UnknownEncoding) => Vec::new(), // cd.py:97-98
        Err(ProbeError::MultiByte) => panic!("Function not supported on multi-byte code page"),
        Ok(unicode_ranges) => {
            let mut primary_range: Option<&str> = None;
            for specified_range in &unicode_ranges {
                if !specified_range.contains("Latin") {
                    primary_range = Some(specified_range);
                    break; // cd.py:102-105
                }
            }
            match primary_range {
                None => vec!["Latin Based".to_string()], // cd.py:107-108
                Some(primary) => unicode_range_languages(primary), // cd.py:110
            }
        }
    };
    ENCODING_LANGUAGES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(iana_name.to_string(), result.clone());
    result
}

/// Orig cd.py:113 `mb_encoding_languages` (`@lru_cache()` ->
/// `MB_ENCODING_LANGUAGES`). Prefix/exact-name rules, branch-for-branch
/// (cd.py:119-131); fallthrough -> `[]`.
pub(crate) fn mb_encoding_languages(iana_name: &str) -> Vec<String> {
    if let Some(hit) = MB_ENCODING_LANGUAGES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(iana_name)
    {
        return hit.clone();
    }
    let result = if iana_name.starts_with("shift_")
        || iana_name.starts_with("iso2022_jp")
        || iana_name.starts_with("euc_j")
        || iana_name == "cp932"
    {
        vec!["Japanese".to_string()] // cd.py:119-125
    } else if iana_name.starts_with("gb") || ZH_NAMES.contains(&iana_name) {
        vec!["Chinese".to_string()] // cd.py:126-127
    } else if iana_name.starts_with("iso2022_kr") || KO_NAMES.contains(&iana_name) {
        vec!["Korean".to_string()] // cd.py:128-129
    } else {
        Vec::new() // cd.py:131
    };
    MB_ENCODING_LANGUAGES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(iana_name.to_string(), result.clone());
    result
}

/// Orig cd.py:134 `get_target_features`
/// (`@lru_cache(maxsize=LANGUAGE_SUPPORTED_COUNT)` -> `GET_TARGET_FEATURES`).
/// Full scan per language (no early exit, cd.py:142-150): whether any
/// frequency character is accentuated, and whether all are latin.
pub(crate) fn get_target_features(language: &str) -> (bool, bool) {
    if let Some(&hit) = GET_TARGET_FEATURES
        .get_or_init(|| Mutex::new(HashMap::with_capacity(LANGUAGE_SUPPORTED_COUNT)))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(language)
    {
        return hit;
    }
    let freq = frequencies(language).expect("unknown language in get_target_features");
    let mut target_have_accents = false;
    let mut target_pure_latin = true;
    for ch in freq.chars() {
        let props = char_props(ch);
        if !target_have_accents && props.accentuated {
            target_have_accents = true; // cd.py:145-146
        }
        if target_pure_latin && !props.latin {
            target_pure_latin = false; // cd.py:147-148
        }
    }
    let result = (target_have_accents, target_pure_latin); // cd.py:150
    GET_TARGET_FEATURES
        .get_or_init(|| Mutex::new(HashMap::with_capacity(LANGUAGE_SUPPORTED_COUNT)))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(language.to_string(), result);
    result
}

/// Orig cd.py:153 `alphabet_languages`. `characters` are single characters
/// (orig `list[str]` of one-char strings). Source-accent scan breaks on first
/// accentuated char (cd.py:163-168); per-language filters (cd.py:173-177);
/// `match / total >= 0.2` keeps `(language, ratio)` (cd.py:179-186); final
/// ORDER: stable descending sort by ratio (cd.py:188), names only (cd.py:190).
pub(crate) fn alphabet_languages(characters: &[char], ignore_non_latin: bool) -> Vec<String> {
    let characters_set: HashSet<char> = characters.iter().copied().collect(); // cd.py:161
    let mut source_have_accents = false;
    for &ch in characters {
        if char_props(ch).accentuated {
            source_have_accents = true;
            break; // cd.py:166-168
        }
    }
    let mut languages: Vec<(String, f64)> = Vec::new();
    for language in FREQUENCY_LANGS {
        let (target_have_accents, target_pure_latin) = get_target_features(language); // cd.py:171
        if ignore_non_latin && !target_pure_latin {
            continue; // cd.py:173-174
        }
        if !target_have_accents && source_have_accents {
            continue; // cd.py:176-177
        }
        let freq = frequencies(language).expect("FREQUENCY_LANGS must match frequencies()");
        let character_count = freq.chars().count(); // cd.py:179
        // == len(_FREQUENCIES_SET[language] & characters_set) (cd.py:181):
        // frequency strings hold unique characters.
        let character_match_count = freq.chars().filter(|c| characters_set.contains(c)).count();
        let ratio = character_match_count as f64 / character_count as f64; // cd.py:183
        if ratio >= 0.2 {
            languages.push((language.to_string(), ratio)); // cd.py:185-186
        }
    }
    // ORDER: stable desc (ties keep FREQUENCIES order). MUST stay `sort_by`.
    languages.sort_by(|a, b| b.1.total_cmp(&a.1)); // cd.py:188
    languages.into_iter().map(|(name, _)| name).collect() // cd.py:190
}

/// Orig cd.py:193 `characters_popularity_compare`. Unknown language raises
/// (orig `ValueError`, cd.py:201-202; here a panic with the same message).
/// Rank-projection and before/after counting use exact integer comparisons
/// (`5 * count >= 2 * span`, cd.py:269,275); the `large_alphabet` threshold
/// `len / 3` is a float compare (cd.py:211,239). The rank-0 fast accept
/// (cd.py:245-252) holds structurally (`before` can never exceed 0 there).
pub(crate) fn characters_popularity_compare(language: &str, ordered_characters: &[char]) -> f64 {
    let rank_table =
        frequency_rank(language).unwrap_or_else(|| panic!("{language} not available")); // cd.py:201-202
    let lang_rank: HashMap<char, usize> =
        rank_table.iter().map(|&(c, r)| (c, r as usize)).collect(); // cd.py:205
    let ordered_characters_count = ordered_characters.len(); // cd.py:207
    let target_language_characters_count = frequencies(language)
        .expect("frequency_rank and frequencies must agree")
        .chars()
        .count(); // cd.py:208
    let large_alphabet = target_language_characters_count > 26; // cd.py:210
    let large_alphabet_threshold = target_language_characters_count as f64 / 3.0; // cd.py:211
    let expected_projection_ratio =
        target_language_characters_count as f64 / ordered_characters_count as f64; // cd.py:213-215

    // Single pass collecting (language rank, popularity rank) pairs for
    // characters present in the language vocabulary (cd.py:220-226).
    let mut common_lr: Vec<usize> = Vec::new();
    let mut common_orr: Vec<usize> = Vec::new();
    for (popularity_rank, &ch) in ordered_characters.iter().enumerate() {
        if let Some(&language_rank) = lang_rank.get(&ch) {
            common_lr.push(language_rank);
            common_orr.push(popularity_rank);
        }
    }

    let mut character_approved_count: usize = 0; // cd.py:204
    for (&character_rank_in_language, &character_rank) in common_lr.iter().zip(common_orr.iter())
    {
        // cd.py:229 `int(character_rank * expected_projection_ratio)` truncates
        // toward zero (non-negative here, so == floor).
        let character_rank_projection =
            (character_rank as f64 * expected_projection_ratio).floor() as usize;
        let projection_gap: i64 =
            character_rank_projection as i64 - character_rank_in_language as i64; // cd.py:232,239
        if !large_alphabet && projection_gap.abs() > 4 {
            continue; // cd.py:231-235
        }
        if large_alphabet && (projection_gap.abs() as f64) < large_alphabet_threshold {
            character_approved_count += 1;
            continue; // cd.py:237-243
        }
        if character_rank_in_language == 0 {
            character_approved_count += 1;
            continue; // cd.py:245-252
        }
        let after_len = target_language_characters_count - character_rank_in_language; // cd.py:254
        let mut before_match_count: usize = 0; // cd.py:262
        let mut after_match_count: usize = 0; // cd.py:263
        // cd.py:265: includes the current pair itself (lands in the `else`
        // arm via `orr_i >= character_rank`).
        for (&lr_i, &orr_i) in common_lr.iter().zip(common_orr.iter()) {
            if lr_i < character_rank_in_language {
                if orr_i < character_rank {
                    before_match_count += 1;
                    if 5 * before_match_count >= 2 * character_rank_in_language {
                        character_approved_count += 1; // cd.py:269-271
                        break;
                    }
                }
            } else if orr_i >= character_rank {
                after_match_count += 1;
                if 5 * after_match_count >= 2 * after_len {
                    character_approved_count += 1; // cd.py:274-277
                    break;
                }
            }
        }
    }
    character_approved_count as f64 / ordered_characters.len() as f64 // cd.py:279
}

/// Orig cd.py:282 `alpha_unicode_split`. Groups alpha characters into layers by
/// Unicode range: a character joins the first (insertion-ordered) layer whose
/// range is NOT suspiciously successive with its own (cd.py:325-331); otherwise
/// it opens a new layer (cd.py:336-344). Non-alpha (cd.py:309-310) and
/// rangeless (cd.py:314-315) characters are skipped. Output lowercased per
/// layer in first-seen layer order (cd.py:352).
pub(crate) fn alpha_unicode_split(decoded_sequence: &str) -> Vec<String> {
    let mut layers: Vec<(&'static str, String)> = Vec::new(); // cd.py:288
    for ch in decoded_sequence.chars() {
        let props = char_props(ch);
        if !props.alpha {
            continue; // cd.py:309-310
        }
        let Some(character_range) = props.range else {
            continue; // cd.py:314-315
        };
        let mut layer_target: Option<usize> = None;
        for (i, (discovered_range, _)) in layers.iter().enumerate() {
            if !is_suspiciously_successive_range(Some(*discovered_range), Some(character_range)) {
                layer_target = Some(i);
                break; // cd.py:326-331
            }
        }
        match layer_target {
            Some(i) => layers[i].1.push(ch),
            None => {
                let mut s = String::new();
                s.push(ch);
                layers.push((character_range, s)); // cd.py:336-346
            }
        }
    }
    layers.into_iter().map(|(_, s)| s.to_lowercase()).collect() // cd.py:352
}

/// Orig cd.py:355 `merge_coherence_ratios`. Per-language mean of ratios,
/// ROUNDING each mean to 4 decimals (cd.py:369-378), then ORDER: stable
/// descending sort (cd.py:380). `per_language_ratios` insertion order is
/// first-seen across results in order; the `Vec` + linear lookup below
/// preserves exactly that (a `HashMap` iteration would NOT).
pub(crate) fn merge_coherence_ratios(results: &[CoherenceMatches]) -> CoherenceMatches {
    let mut per_language_ratios: Vec<(String, Vec<f64>)> = Vec::new(); // cd.py:360
    for result in results {
        for (language, ratio) in result {
            match per_language_ratios.iter_mut().find(|(l, _)| l == language) {
                Some((_, ratios)) => ratios.push(*ratio), // cd.py:367
                None => per_language_ratios.push((language.clone(), vec![*ratio])), // cd.py:364-366
            }
        }
    }
    let mut merge: Vec<(String, f64)> = per_language_ratios
        .iter()
        .map(|(language, ratios)| {
            (
                language.clone(),
                round4(ratios.iter().sum::<f64>() / ratios.len() as f64), // cd.py:369-378
            )
        })
        .collect();
    sort_desc(&mut merge); // cd.py:380
    merge
}

/// Orig cd.py:383 `filter_alt_coherence_matches`. Groups by the em-dash-free
/// name (`language.replace("\u2014", "")`, cd.py:392); if any group has >1
/// entry, returns one `(name, max)` per group in first-seen group order
/// (cd.py:399-405); otherwise returns the input unchanged (cd.py:407).
pub(crate) fn filter_alt_coherence_matches(results: &CoherenceMatches) -> CoherenceMatches {
    let mut index_results: Vec<(String, Vec<f64>)> = Vec::new(); // cd.py:388
    for (language, ratio) in results {
        let no_em_name = language.replace('\u{2014}', ""); // cd.py:392 "--"
        match index_results.iter_mut().find(|(l, _)| *l == no_em_name) {
            Some((_, ratios)) => ratios.push(*ratio), // cd.py:397
            None => index_results.push((no_em_name, vec![*ratio])), // cd.py:394-395
        }
    }
    if index_results.iter().any(|(_, ratios)| ratios.len() > 1) {
        // cd.py:399
        return index_results
            .iter()
            .map(|(language, ratios)| {
                (
                    language.clone(),
                    ratios.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    // cd.py:402-403 `max(...)`
                )
            })
            .collect(); // cd.py:405
    }
    results.clone() // cd.py:407
}

/// Orig cd.py:410 `coherence_ratio`. Splits the sequence into alphabet layers
/// (`alpha_unicode_split`), skips layers with `len <= TOO_SMALL_SEQUENCE`
/// (cd.py:438-439; `TOO_SMALL_SEQUENCE == 32`), orders each layer's characters
/// with a stable descending count sort reproducing `Counter.most_common()`
/// tie order (cd.py:441-446), then scores candidate languages:
/// explicit `lg_inclusion` (comma-split, cd.py:423) when non-empty, else
/// `alphabet_languages` (cd.py:448-450). Ratios below `threshold` are skipped
/// (default `0.1`; cd.py:455), ratios `>= 0.8` count toward
/// `sufficient_match_count`, which breaks the per-layer language loop at 3
/// (cd.py:457-463; inner loop only, the count persists across layers).
/// Each kept ratio is ROUNDING to 4 decimals (cd.py:460); output is
/// `filter_alt_coherence_matches` + ORDER stable descending sort (cd.py:465-467).
pub(crate) fn coherence_ratio(
    decoded_sequence: &str,
    threshold: f64,
    lg_inclusion: Option<&str>,
) -> CoherenceMatches {
    let mut results: Vec<(String, f64)> = Vec::new(); // cd.py:418
    let mut ignore_non_latin = false; // cd.py:419
    let mut sufficient_match_count: i64 = 0; // cd.py:421
    // cd.py:423 `lg_inclusion.split(",")` (`"".split(",") == [""]`, mirrored).
    let mut lg_inclusion_list: Vec<String> = match lg_inclusion {
        Some(s) => s.split(',').map(|x| x.to_string()).collect(),
        None => Vec::new(),
    };
    if let Some(pos) = lg_inclusion_list.iter().position(|x| x == "Latin Based") {
        ignore_non_latin = true;
        lg_inclusion_list.remove(pos); // cd.py:424-426 `list.remove` drops FIRST only
    }
    for layer in alpha_unicode_split(decoded_sequence) {
        // cd.py:431-433: native counting; `Vec` keeps first-appearance order so
        // the stable sort below ties exactly like `Counter.most_common()`.
        let mut char_counts: Vec<(char, usize)> = Vec::new();
        for layer_character in layer.chars() {
            match char_counts.iter_mut().find(|(c, _)| *c == layer_character) {
                Some((_, n)) => *n += 1,
                None => char_counts.push((layer_character, 1)),
            }
        }
        let character_count = layer.chars().count(); // cd.py:436 `len(layer)` in chars
        if character_count <= TOO_SMALL_SEQUENCE {
            continue; // cd.py:438-439
        }
        // ORDER: stable desc; ties keep first-appearance order. MUST stay `sort_by`.
        char_counts.sort_by(|a, b| b.1.cmp(&a.1)); // cd.py:441-446
        let popular_character_ordered: Vec<char> = char_counts.into_iter().map(|(c, _)| c).collect();
        // cd.py:448 `lg_inclusion_list or alphabet_languages(...)`.
        let languages: Vec<String> = if !lg_inclusion_list.is_empty() {
            lg_inclusion_list.clone()
        } else {
            alphabet_languages(&popular_character_ordered, ignore_non_latin)
        };
        for language in &languages {
            let ratio = characters_popularity_compare(language, &popular_character_ordered); // cd.py:451-453
            if ratio < threshold {
                continue; // cd.py:455-456
            } else if ratio >= 0.8 {
                sufficient_match_count += 1; // cd.py:457-458
            }
            results.push((language.clone(), round4(ratio))); // cd.py:460
            if sufficient_match_count >= 3 {
                break; // cd.py:462-463 (inner language loop only)
            }
        }
    }
    let mut out = filter_alt_coherence_matches(&results);
    sort_desc(&mut out); // cd.py:465-467
    out
}
