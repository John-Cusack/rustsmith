//! `from_bytes` detection core (`api.py:50-807`), branch-for-branch.
//!
//! Logging is observable behavior (test_logging.py): every `logger.*` call in
//! the original is reproduced via the [`Log`] sink with identical level +
//! message. Handler add/remove + level save/restore around `explain=True`
//! lives with the integrator (identical net effect to the per-return cleanup).

use std::collections::{HashMap, HashSet};

use super::cd::{coherence_ratio, encoding_languages, mb_encoding_languages, merge_coherence_ratios};
use super::decoders::decode_strict;
use super::models::{CharsetMatch, CharsetMatches};
use super::tables_constant::{
    IANA_MB_FIRST, TOO_BIG_SEQUENCE, TOO_SMALL_SEQUENCE,
};
use super::utils::{
    any_specified_encoding, cut_sequence_chunks, iana_name, identify_sig_or_bom,
    is_multi_byte_encoding, should_strip_sig_or_bom,
};
use super::md::mess_ratio;

/// Python `round(x, nd)` ties-to-even, for f64 values that are exact
/// decimal ties at `nd` digits (banker's rounding; Rust `round` is half-away).
pub(crate) fn py_round(x: f64, nd: u32) -> f64 {
    let m = 10f64.powi(nd as i32);
    let scaled = x * m;
    let frac = scaled.fract();
    if frac.abs() < 0.5 {
        let base = scaled.trunc();
        // Half-exact (within float error of .5): tie to even.
        if (frac.abs() - 0.5).abs() < 1e-9 {
            if (base % 2.0).abs() < 1e-9 {
                return base / m;
            }
            return (base + frac.signum()) / m;
        }
        return (base + frac.trunc()) / m;
    }
    scaled.round() / m
}

/// Logging sink: level 5 == TRACE (`constant.py:999`), 10 == DEBUG.
pub(crate) trait Log {
    fn trace(&self, msg: String);
    fn debug(&self, msg: String);
}

#[derive(Clone, Default)]
pub(crate) struct DetectOptions {
    pub steps: usize,
    pub chunk_size: usize,
    pub threshold: f64,
    pub cp_isolation: Option<Vec<String>>,
    pub cp_exclusion: Option<Vec<String>>,
    pub preemptive: bool,
    pub language_threshold: f64,
    pub enable_fallback: bool,
}


/// Single-arg `from_bytes(data)` with defaults (for `legacy.detect`).
pub(crate) fn from_bytes_1arg(data: &[u8]) -> CharsetMatches {
    from_bytes_with_options(
        data,
        &DetectOptions {
            steps: 5,
            chunk_size: 512,
            threshold: 0.2,
            cp_isolation: None,
            cp_exclusion: None,
            preemptive: true,
            language_threshold: 0.1,
            enable_fallback: true,
        },
    )
}

/// Options-explicit entry (`from_bytes_with_options`).
pub(crate) fn from_bytes_with_options(data: &[u8], opts: &DetectOptions) -> CharsetMatches {
    from_bytes(data, opts, &NullLog)
}

/// Null logger for non-Python contexts.
#[derive(Clone, Copy, Default)]
pub(crate) struct NullLog;

impl Log for NullLog {
    fn trace(&self, _msg: String) {}
    fn debug(&self, _msg: String) {}
}

/// `from_bytes` core. `data` is the raw payload; `log` receives every
/// logger call the original would emit. Mirrors `api.py:50-807`.
pub(crate) fn from_bytes<L: Log>(
    data: &[u8],
    opts: &DetectOptions,
    log: &L,
) -> CharsetMatches {
    let mut steps = opts.steps;
    let mut chunk_size = opts.chunk_size;
    let threshold = opts.threshold;
    let length = data.len();

    // api.py:94-99 empty payload shortcut.
    if length == 0 {
        log.debug("Encoding detection on empty bytes, assuming utf_8 intention.".to_string());
        let mut out = CharsetMatches::new(None);
        let _ = out.append(CharsetMatch::new(Vec::new(), "utf_8".to_string(), 0.0, false, Vec::new(), None, None));
        return out;
    }

    // api.py:101-121 isolation/exclusion normalization (lossy iana).
    let cp_isolation: Vec<String> = match &opts.cp_isolation {
        Some(list) => {
            log.trace(format!(
                "cp_isolation is set. use this flag for debugging purpose. limited list of encoding allowed : {}.",
                list.join(", ")
            ));
            list.iter().map(|cp| iana_name(cp, false).unwrap_or_else(|_| cp.clone())).collect()
        }
        None => Vec::new(),
    };
    let cp_exclusion: Vec<String> = match &opts.cp_exclusion {
        Some(list) => {
            log.trace(format!(
                "cp_exclusion is set. use this flag for debugging purpose. limited list of encoding excluded : {}.",
                list.join(", ")
            ));
            list.iter().map(|cp| iana_name(cp, false).unwrap_or_else(|_| cp.clone())).collect()
        }
        None => Vec::new(),
    };

    // api.py:123-136 steps/chunk_size reduction.
    if length <= chunk_size * steps {
        log.trace(format!(
            "override steps ({}) and chunk_size ({}) as content does not fit ({} byte(s) given) parameters.",
            steps, chunk_size, length
        ));
        steps = 1;
        chunk_size = length;
    }
    if steps > 1 && length / steps < chunk_size {
        chunk_size = length / steps;
    }

    let is_too_small_sequence = length < TOO_SMALL_SEQUENCE;
    let is_too_large_sequence = length >= TOO_BIG_SEQUENCE;
    if is_too_small_sequence {
        log.trace(format!(
            "Trying to detect encoding from a tiny portion of ({}) byte(s).",
            length
        ));
    } else if is_too_large_sequence {
        log.trace(format!(
            "Using lazy str decoding because the payload is quite large, ({}) byte(s).",
            length
        ));
    }

    let mut prioritized_encodings: Vec<String> = Vec::new();
    let specified_encoding: Option<String> = if opts.preemptive {
        any_specified_encoding(data, 8192).map(|s| s.to_string())
    } else {
        None
    };
    if let Some(ref spec) = specified_encoding {
        prioritized_encodings.push(spec.clone());
        log.trace(format!(
            "Detected declarative mark in sequence. Priority +1 given for {}.",
            spec
        ));
    }

    let mut tested: HashSet<String> = HashSet::new();
    let mut tested_but_hard_failure: Vec<String> = Vec::new();
    let mut tested_but_soft_failure: Vec<String> = Vec::new();
    let mut soft_failure_skip: HashSet<String> = HashSet::new();

    // Per-call caches (api.py: lru_cache(maxsize=None) instances).
    let mut cached_mess: HashMap<String, f64> = HashMap::new();
    let mut cached_coh: HashMap<String, Vec<(String, f64)>> = HashMap::new();

    let mut definitive_match_found = false;
    let mut definitive_target_languages: HashSet<String> = HashSet::new();
    let mut post_definitive_sb_success_count: usize = 0;
    const POST_DEFINITIVE_SB_CAP: usize = 7;
    let mut mb_definitive_match_found = false;

    let mut fallback_ascii: Option<CharsetMatch> = None;
    let mut fallback_u8: Option<CharsetMatch> = None;
    let mut fallback_specified: Option<CharsetMatch> = None;

    let mut results = CharsetMatches::new(None);
    let mut early_stop_results = CharsetMatches::new(None);

    let (sig_encoding, sig_payload) = identify_sig_or_bom(data);
    if let Some(sig) = sig_encoding {
        prioritized_encodings.insert(0, sig.to_string());
        log.trace(format!(
            "Detected a SIG or BOM mark on first {} byte(s). Priority +1 given for {}.",
            sig_payload.len(),
            sig
        ));
    }
    prioritized_encodings.push("ascii".to_string());
    if !prioritized_encodings.contains(&"utf_8".to_string()) {
        prioritized_encodings.push("utf_8".to_string());
    }

    let mut candidates: Vec<String> = prioritized_encodings;
    candidates.extend(IANA_MB_FIRST.iter().map(|s| s.to_string()));

    for encoding_iana in candidates {
        if !cp_isolation.is_empty() && !cp_isolation.contains(&encoding_iana) {
            continue;
        }
        if !cp_exclusion.is_empty() && cp_exclusion.contains(&encoding_iana) {
            continue;
        }
        if tested.contains(&encoding_iana) {
            continue;
        }
        tested.insert(encoding_iana.clone());

        let mut decoded_payload: Option<String> = None;
        let bom_or_sig_available = sig_encoding == Some(encoding_iana.as_str());
        let strip_sig_or_bom =
            bom_or_sig_available && should_strip_sig_or_bom(&encoding_iana);

        // api.py:248-262 BOM-gated skips.
        if (encoding_iana == "utf_16" || encoding_iana == "utf_32") && !bom_or_sig_available {
            log.trace(format!(
                "Encoding {} won't be tested as-is because it require a BOM. Will try some sub-encoder LE/BE.",
                encoding_iana
            ));
            continue;
        }
        if encoding_iana == "utf_7" && !bom_or_sig_available {
            log.trace(format!(
                "Encoding {} won't be tested as-is because detection is unreliable without BOM/SIG.",
                encoding_iana
            ));
            continue;
        }
        if soft_failure_skip.contains(&encoding_iana) {
            log.trace(format!(
                "{} is deemed too similar to a code page that was already considered unsuited. Continuing!",
                encoding_iana
            ));
            continue;
        }

        // api.py: is_multi_byte probe (never fails in Rust; tables exhaustive).
        let is_multi_byte_decoder = is_multi_byte_encoding(&encoding_iana);

        if definitive_match_found {
            let enc_languages: HashSet<String> = if !is_multi_byte_decoder {
                encoding_languages(&encoding_iana).into_iter().collect()
            } else {
                mb_encoding_languages(&encoding_iana).into_iter().collect()
            };
            if enc_languages.intersection(&definitive_target_languages).next().is_none() {
                log.trace(format!(
                    "Skipping {}: definitive match already found, this encoding targets different languages ({:?} vs {:?}).",
                    encoding_iana, enc_languages, definitive_target_languages
                ));
                continue;
            }
        }
        if definitive_match_found
            && !is_multi_byte_decoder
            && post_definitive_sb_success_count >= POST_DEFINITIVE_SB_CAP
        {
            log.trace(format!(
                "Skipping {}: already accumulated {} same-family results after definitive match (cap={}).",
                encoding_iana, post_definitive_sb_success_count, POST_DEFINITIVE_SB_CAP
            ));
            continue;
        }
        if mb_definitive_match_found && !is_multi_byte_decoder {
            log.trace(format!(
                "Skipping single-byte {}: multi-byte definitive match already found.",
                encoding_iana
            ));
            continue;
        }

        let deferred_decoding = !is_multi_byte_decoder && !is_too_large_sequence;

        // api.py: eager decode (strict). LookupError impossible (tables exhaustive).
        let mut hard_failed = false;
        if is_too_large_sequence && !is_multi_byte_decoder {
            let probe = if !strip_sig_or_bom {
                &data[..data.len().min(50000)]
            } else {
                &data[sig_payload.len().min(data.len())..data.len().min(50000)]
            };
            if decode_strict(&encoding_iana, probe).is_err() {
                hard_failed = true;
            }
        } else if !deferred_decoding {
            let payload_slice: &[u8] = if encoding_iana == "utf_7" && bom_or_sig_available {
                data
            } else if !strip_sig_or_bom {
                data
            } else {
                &data[sig_payload.len().min(data.len())..]
            };
            match decode_strict(&encoding_iana, payload_slice) {
                Ok(mut s) => {
                    if encoding_iana == "utf_7" && bom_or_sig_available {
                        if s.starts_with('\u{feff}') {
                            s = s['\u{feff}'.len_utf8()..].to_string();
                        }
                    }
                    decoded_payload = Some(s);
                }
                Err(()) => {
                    log.trace(format!(
                        "Code page {} does not fit given bytes sequence at ALL. strict decode failed",
                        encoding_iana
                    ));
                    tested_but_hard_failure.push(encoding_iana.clone());
                    continue;
                }
            }
        }
        if hard_failed {
            log.trace(format!(
                "Code page {} does not fit given bytes sequence at ALL. strict decode failed",
                encoding_iana
            ));
            tested_but_hard_failure.push(encoding_iana.clone());
            continue;
        }

        // api.py: r_ offsets.
        let step = (length / steps).max(1);
        let r_start = if bom_or_sig_available { sig_payload.len() } else { 0 };
        let r_: Vec<usize> = (r_start..length).step_by(step).collect();

        let multi_byte_bonus = is_multi_byte_decoder
            && decoded_payload.is_some()
            && decoded_payload.as_ref().map(|s| s.chars().count()).unwrap_or(usize::MAX) < length;
        if multi_byte_bonus {
            log.trace(format!(
                "Code page {} is a multi byte encoding table and it appear that at least one character was encoded using n-bytes.",
                encoding_iana
            ));
        }

        let mut max_chunk_gave_up = r_.len() / 4;
        if max_chunk_gave_up < 2 {
            max_chunk_gave_up = 2;
        }
        let mut early_stop_count: usize = 0;
        let mut lazy_str_hard_failure = false;
        let mut md_chunks: Vec<String> = Vec::new();
        let mut md_ratios: Vec<f64> = Vec::new();

        // api.py: chunk loop (cut_sequence_chunks raises only on deferred path).
        let chunks = cut_sequence_chunks(
            data,
            &encoding_iana,
            r_.iter().copied(),
            chunk_size,
            bom_or_sig_available,
            strip_sig_or_bom,
            sig_payload,
            is_multi_byte_decoder,
            decoded_payload.as_deref(),
            deferred_decoding,
        );
        let mut chunk_failed = false;
        let chunks = match chunks {
            Ok(v) => v,
            Err(()) => {
                if deferred_decoding {
                    log.trace(format!(
                        "Code page {} does not fit given bytes sequence at ALL. chunk decode failed",
                        encoding_iana
                    ));
                    tested_but_hard_failure.push(encoding_iana.clone());
                    chunk_failed = true;
                    Vec::new()
                } else {
                    log.trace(format!(
                        "LazyStr Loading: After MD chunk decode, code page {} does not fit given bytes sequence at ALL. chunk decode failed",
                        encoding_iana
                    ));
                    early_stop_count = max_chunk_gave_up;
                    lazy_str_hard_failure = true;
                    Vec::new()
                }
            }
        };
        if chunk_failed {
            continue;
        }
        // NOTE: orig iterates the chunk GENERATOR lazily with early break;
        // chunks here are materialized first (identical items), then probed
        // with the same break rule.
        let mut broke_early = false;
        for chunk in &chunks {
            md_chunks.push(chunk.clone());
            let ratio = *cached_mess.entry(chunk.clone()).or_insert_with(|| {
                mess_ratio(chunk, threshold)
            });
            // NOTE: orig passes `explain and 1 <= len(cp_isolation) <= 2` as
            // mess debug flag (log-only); detection value unaffected.
            md_ratios.push(ratio);
            if ratio >= threshold {
                early_stop_count += 1;
            }
            if early_stop_count >= max_chunk_gave_up
                || (bom_or_sig_available && !strip_sig_or_bom)
            {
                broke_early = true;
                break;
            }
        }
        let _ = broke_early;

        let mean_mess_ratio: f64 = if md_ratios.is_empty() {
            0.0
        } else {
            md_ratios.iter().sum::<f64>() / md_ratios.len() as f64
        };

        // api.py: too-large final strict lookup.
        if !lazy_str_hard_failure
            && is_too_large_sequence
            && !is_multi_byte_decoder
            && mean_mess_ratio < threshold
            && early_stop_count < max_chunk_gave_up
        {
            if decode_strict(&encoding_iana, &data[50000.min(length)..]).is_err() {
                log.trace(format!(
                    "LazyStr Loading: After final lookup, code page {} does not fit given bytes sequence at ALL.",
                    encoding_iana
                ));
                tested_but_hard_failure.push(encoding_iana.clone());
                continue;
            }
        }

        if mean_mess_ratio >= threshold || early_stop_count >= max_chunk_gave_up {
            tested_but_soft_failure.push(encoding_iana.clone());
            if let Some(similar) = similar_of(&encoding_iana) {
                for s in similar {
                    soft_failure_skip.insert(s);
                }
            }
            log.trace(format!(
                "{} was excluded because of initial chaos probing. Gave up {} time(s). Computed mean chaos is {} %%.",
                encoding_iana,
                early_stop_count,
                py_round(mean_mess_ratio * 100.0, 3)
            ));
            if opts.enable_fallback
                && (encoding_iana == "ascii"
                    || encoding_iana == "utf_8"
                    || Some(encoding_iana.clone()) == specified_encoding
                    || encoding_iana == "utf_16"
                    || encoding_iana == "utf_32")
                && !lazy_str_hard_failure
            {
                if decoded_payload.is_none() {
                    let slice: &[u8] = if !strip_sig_or_bom {
                        data
                    } else {
                        &data[sig_payload.len().min(data.len())..]
                    };
                    match decode_strict(&encoding_iana, slice) {
                        Ok(s) => {
                            decoded_payload = if is_too_large_sequence { None } else { Some(s) };
                        }
                        Err(()) => {
                            log.trace(format!(
                                "{} does not decode the whole payload: fallback entry withheld.",
                                encoding_iana
                            ));
                            continue;
                        }
                    }
                    if is_too_large_sequence {
                        decoded_payload = None;
                    }
                }
                let entry = CharsetMatch::new(
                    data.to_vec(),
                    encoding_iana.clone(),
                    threshold,
                    bom_or_sig_available,
                    Vec::new(),
                    decoded_payload.clone(),
                    specified_encoding.clone(),
                );
                if Some(encoding_iana.clone()) == specified_encoding {
                    fallback_specified = Some(entry);
                } else if encoding_iana == "ascii" {
                    fallback_ascii = Some(entry);
                } else {
                    fallback_u8 = Some(entry);
                }
            }
            continue;
        }

        if deferred_decoding {
            let slice: &[u8] = if !strip_sig_or_bom {
                data
            } else {
                &data[sig_payload.len().min(data.len())..]
            };
            match decode_strict(&encoding_iana, slice) {
                Ok(s) => {
                    decoded_payload = Some(s);
                }
                Err(()) => {
                    log.trace(format!(
                        "Code page {} does not fit given bytes sequence at ALL. deferred decode failed",
                        encoding_iana
                    ));
                    tested_but_hard_failure.push(encoding_iana.clone());
                    continue;
                }
            }
        }

        log.trace(format!(
            "{} passed initial chaos probing. Mean measured chaos is {} %%",
            encoding_iana,
            py_round(mean_mess_ratio * 100.0, 3)
        ));

        let target_languages: Vec<String> = if !is_multi_byte_decoder {
            encoding_languages(&encoding_iana)
        } else {
            mb_encoding_languages(&encoding_iana)
        };
        if !target_languages.is_empty() {
            log.trace(format!(
                "{} should target any language(s) of {:?}",
                encoding_iana, target_languages
            ));
        }

        // api.py: coherence over chunks (skipped for ascii).
        let mut cd_ratios: Vec<Vec<(String, f64)>> = Vec::new();
        if encoding_iana != "ascii" {
            let lg_inclusion: Option<String> = if target_languages.is_empty() {
                None
            } else {
                Some(target_languages.join(","))
            };
            for chunk in &md_chunks {
                let key = format!("{}\u{1f}\u{1f}{}\u{1f}\u{1f}{}", chunk, opts.language_threshold, lg_inclusion.as_deref().unwrap_or(""));
                let ratios = if let Some(v) = cached_coh.get(&key) {
                    v.clone()
                } else {
                    let v = coherence_ratio(chunk, opts.language_threshold, lg_inclusion.as_deref());
                    cached_coh.insert(key, v.clone());
                    v
                };
                cd_ratios.push(ratios);
            }
        }
        let cd_ratios_merged = merge_coherence_ratios(&cd_ratios);
        if !cd_ratios_merged.is_empty() {
            log.trace(format!(
                "We detected language {:?} using {}",
                cd_ratios_merged, encoding_iana
            ));
        }

        let current_match = CharsetMatch::new(
            data.to_vec(),
            encoding_iana.clone(),
            mean_mess_ratio,
            bom_or_sig_available,
            cd_ratios_merged.clone(),
            if !is_too_large_sequence
                || Some(encoding_iana.clone()) == specified_encoding
                || encoding_iana == "ascii"
                || encoding_iana == "utf_8"
            {
                decoded_payload.clone()
            } else {
                None
            },
            specified_encoding.clone(),
        );
        let _ = results.append(current_match.clone());

        if definitive_match_found && !is_multi_byte_decoder && mean_mess_ratio < 0.02 {
            post_definitive_sb_success_count += 1;
        }

        // api.py: prioritized early-stop paths.
        if (Some(encoding_iana.clone()) == specified_encoding
            || encoding_iana == "ascii"
            || encoding_iana == "utf_8")
            && mean_mess_ratio < 0.1
        {
            if mean_mess_ratio == 0.0 {
                log.debug(format!(
                    "Encoding detection: {} is most likely the one.",
                    current_match.encoding()
                ));
                let mut single = CharsetMatches::new(None);
                let _ = single.append(current_match);
                return single;
            }
            let _ = early_stop_results.append(current_match.clone());
        }
        if !early_stop_results.is_empty()
            && (specified_encoding.is_none()
                || specified_encoding
                    .as_ref()
                    .map(|s| tested.contains(s))
                    .unwrap_or(false))
            && tested.contains("ascii")
            && tested.contains("utf_8")
        {
            if let Some(probable) = early_stop_results.best() {
                let probable = probable.clone();
                log.debug(format!(
                    "Encoding detection: {} is most likely the one.",
                    probable.encoding()
                ));
                let mut single = CharsetMatches::new(None);
                let _ = single.append(probable);
                return single;
            }
        }

        if !definitive_match_found && !is_multi_byte_decoder {
            let best_coherence: f64 = cd_ratios_merged
                .iter()
                .map(|(_, v)| *v)
                .fold(0.0f64, f64::max);
            if best_coherence >= 0.5 && tested.contains("ascii") && tested.contains("utf_8") {
                definitive_match_found = true;
                definitive_target_languages.extend(target_languages.iter().cloned());
                log.trace(format!(
                    "Definitive match found: {} (chaos={:.3}, coherence={:.2}). Encodings targeting different language families will be skipped.",
                    encoding_iana, mean_mess_ratio, best_coherence
                ));
            }
        }

        if !mb_definitive_match_found
            && is_multi_byte_decoder
            && multi_byte_bonus
            && decoded_payload.is_some()
            && decoded_payload.as_ref().map(|s| s.chars().count() as f64).unwrap_or(f64::MAX) < length as f64 * 0.98
            && !matches!(
                encoding_iana.as_str(),
                "utf_8" | "utf_8_sig" | "utf_16" | "utf_16_be" | "utf_16_le" | "utf_32"
                    | "utf_32_be" | "utf_32_le" | "utf_7"
            )
            && tested.contains("ascii")
            && tested.contains("utf_8")
        {
            mb_definitive_match_found = true;
            log.trace(format!(
                "Multi-byte definitive match: {} (chaos={:.3}, decoded={}/{}={:.1}%). Single-byte encodings will be skipped.",
                encoding_iana,
                mean_mess_ratio,
                decoded_payload.as_ref().map(|s| s.chars().count()).unwrap_or(0),
                length,
                decoded_payload.as_ref().map(|s| s.chars().count()).unwrap_or(0) as f64 / length as f64 * 100.0
            ));
        }

        if sig_encoding == Some(encoding_iana.as_str()) {
            log.debug(format!(
                "Encoding detection: {} is most likely the one as we detected a BOM or SIG within the beginning of the sequence.",
                encoding_iana
            ));
            let mut single = CharsetMatches::new(None);
            if let Some(m) = results.get_by_encoding(&encoding_iana).cloned() {
                let _ = single.append(m);
            }
            return single;
        }
    }

    // api.py: fallback chain.
    if results.is_empty() {
        if fallback_u8.is_some() || fallback_ascii.is_some() || fallback_specified.is_some() {
            log.trace(
                "Nothing got out of the detection process. Using ASCII/UTF-8/Specified fallback.".to_string(),
            );
        }
        if let Some(entry) = fallback_specified {
            log.debug(format!(
                "Encoding detection: {} will be used as a fallback match",
                entry.encoding()
            ));
            let _ = results.append(entry);
        } else if fallback_u8.is_some() {
            // api.py: the three `or` disjuncts reduce to `fallback_u8 is not
            // None` (the third subsumes the fingerprint comparison, which is
            // dead but preserved here in spirit): u8 always wins when present.
            log.debug("Encoding detection: utf_8 will be used as a fallback match".to_string());
            let _ = results.append(fallback_u8.unwrap());
        } else if let Some(entry) = fallback_ascii {
            log.debug(format!(
                "Encoding detection: {} will be used as a fallback match",
                entry.encoding()
            ));
            let _ = results.append(entry);
        }
    }

    if !results.is_empty() {
        if let Some(best) = results.best() {
            log.debug(format!(
                "Encoding detection: Found {} as plausible (best-candidate) for content. With {} alternatives.",
                best.encoding(),
                results.len() - 1
            ));
        }
    } else {
        log.debug("Encoding detection: Unable to determine any suitable charset.".to_string());
    }

    results
}

fn similar_of(name: &str) -> Option<Vec<String>> {
    let slice = super::tables_constant::similar_encodings(name);
    if slice.is_empty() {
        None
    } else {
        Some(slice.iter().map(|s| s.to_string()).collect())
    }
}
