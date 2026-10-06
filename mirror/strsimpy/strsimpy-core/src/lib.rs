// SPDX-License-Identifier: MIT
// Provenance: rustsmith Stage-1 mirror of luozhouyang/python-string-similarity (MIT).
//! Pure-Rust string-similarity core for the `strsimpy` converted project.
//!
//! Behavior-identical port of `luozhouyang/python-string-similarity`: every
//! algorithm replicates the original op-for-op over chars (never bytes) with
//! identical accumulation order, so float outputs are bit-exact and int/float
//! return types match (several metrics return `0.0`/`1.0` floats on early
//! exits, ints otherwise — the early exits live in the binding, which owns
//! the Python return types; the math here is shared).
//!
//! This crate is intentionally independent of Python: it has no `pyo3`
//! dependency, builds as an `rlib`, and its unit tests run under plain
//! `cargo test` (and miri). The Python extension (`strsimpy._strsimpy`,
//! built from the parent `strsimpy-rust` crate) calls this same core, so
//! Rust consumers and Python consumers share one implementation.
//!
//! License: MIT (preserved from the original; see NOTICE).

/// Collect the chars of `s` (all metrics operate on chars, never bytes).
pub fn chars_of(s: &str) -> Vec<char> {
    s.chars().collect()
}

// ---------------------------------------------------------------------------
// Levenshtein.
// ---------------------------------------------------------------------------

// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
pub fn lev_dist(a: &str, b: &str) -> i64 {
    let s0 = chars_of(a);
    let s1 = chars_of(b);
    if s0.is_empty() {
        return s1.len() as i64;
    }
    if s1.is_empty() {
        return s0.len() as i64;
    }
    let mut v0: Vec<i64> = (0..=s1.len() as i64).collect();
    let mut v1: Vec<i64> = vec![0; s1.len() + 1];
    for i in 0..s0.len() {
        v1[0] = i as i64 + 1;
        for j in 0..s1.len() {
            let cost = if s0[i] == s1[j] { 0 } else { 1 };
            let mut best = v1[j] + 1;
            let c = v0[j + 1] + 1;
            if c < best {
                best = c;
            }
            let c = v0[j] + cost;
            if c < best {
                best = c;
            }
            v1[j + 1] = best;
        }
        std::mem::swap(&mut v0, &mut v1);
    }
    v0[s1.len()]
}

// ---------------------------------------------------------------------------
// Damerau (true Damerau-Levenshtein; `da` maps char -> last row).
// ---------------------------------------------------------------------------

// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
pub fn damerau_dist(a: &str, b: &str) -> i64 {
    let s0 = chars_of(a);
    let s1 = chars_of(b);
    let n = s0.len();
    let m = s1.len();
    let inf = (n + m) as i64;
    let mut da: std::collections::HashMap<char, i64> = std::collections::HashMap::new();
    for c in s0.iter().chain(s1.iter()) {
        da.insert(*c, 0);
    }
    let mut h = vec![vec![0i64; m + 2]; n + 2];
    for i in 0..=n {
        h[i + 1][0] = inf;
        h[i + 1][1] = i as i64;
    }
    for j in 0..=m {
        h[0][j + 1] = inf;
        h[1][j + 1] = j as i64;
    }
    for i in 1..=n {
        let mut db = 0i64;
        for j in 1..=m {
            let i1 = *da.get(&s1[j - 1]).unwrap_or(&0);
            let j1 = db;
            let mut cost = 1i64;
            if s0[i - 1] == s1[j - 1] {
                cost = 0;
                db = j as i64;
            }
            let mut best = h[i][j] + cost;
            let c = h[i + 1][j] + 1;
            if c < best {
                best = c;
            }
            let c = h[i][j + 1] + 1;
            if c < best {
                best = c;
            }
            let c = h[i1 as usize][j1 as usize] + (i as i64 - i1 - 1) + 1 + (j as i64 - j1 - 1);
            if c < best {
                best = c;
            }
            h[i + 1][j + 1] = best;
        }
        da.insert(s0[i - 1], i as i64);
    }
    h[n + 1][m + 1]
}

// ---------------------------------------------------------------------------
// Optimal string alignment.
// ---------------------------------------------------------------------------

// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
pub fn osa_dist(a: &str, b: &str) -> i64 {
    let s0 = chars_of(a);
    let s1 = chars_of(b);
    let n = s0.len();
    let m = s1.len();
    let mut d = vec![vec![0i64; m + 2]; n + 2];
    for i in 0..=n {
        d[i][0] = i as i64;
    }
    for j in 0..=m {
        d[0][j] = j as i64;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = if s0[i - 1] == s1[j - 1] { 0 } else { 1 };
            let mut best = d[i - 1][j - 1] + cost;
            let c = d[i][j - 1] + 1;
            if c < best {
                best = c;
            }
            let c = d[i - 1][j] + 1;
            if c < best {
                best = c;
            }
            if i > 1 && j > 1 && s0[i - 1] == s1[j - 2] && s0[i - 2] == s1[j - 1] {
                let c = d[i - 2][j - 2] + cost;
                if c < best {
                    best = c;
                }
            }
            d[i][j] = best;
        }
    }
    d[n][m]
}

// ---------------------------------------------------------------------------
// Longest common subsequence.
// ---------------------------------------------------------------------------

pub fn lcs_length(a: &[char], b: &[char]) -> i64 {
    let (n, m) = (a.len(), b.len());
    let mut matrix = vec![vec![0i64; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            if a[i - 1] == b[j - 1] {
                matrix[i][j] = matrix[i - 1][j - 1] + 1;
            } else {
                matrix[i][j] = matrix[i][j - 1].max(matrix[i - 1][j]);
            }
        }
    }
    matrix[n][m]
}

// ---------------------------------------------------------------------------
// Jaro-Winkler.
// ---------------------------------------------------------------------------

pub fn jaro_matches(s0: &[char], s1: &[char]) -> (i64, i64, i64, i64) {
    let (max_str, min_str) = if s0.len() > s1.len() { (s0, s1) } else { (s1, s0) };
    let ran = ((max_str.len() as f64 / 2.0 - 1.0).max(0.0)) as usize;
    let mut match_indexes: Vec<i64> = vec![-1; min_str.len()];
    let mut match_flags = vec![false; max_str.len()];
    let mut matches = 0i64;
    for (mi, c1) in min_str.iter().enumerate() {
        let lo = mi.saturating_sub(ran);
        let hi = (mi + ran + 1).min(max_str.len());
        for xi in lo..hi {
            if !match_flags[xi] && *c1 == max_str[xi] {
                match_indexes[mi] = xi as i64;
                match_flags[xi] = true;
                matches += 1;
                break;
            }
        }
    }
    let mut ms0 = vec!['\0'; matches as usize];
    let mut ms1 = vec!['\0'; matches as usize];
    let mut si = 0;
    for (i, m) in match_indexes.iter().enumerate() {
        if *m != -1 {
            ms0[si] = min_str[i];
            si += 1;
        }
    }
    si = 0;
    for (j, f) in match_flags.iter().enumerate() {
        if *f {
            ms1[si] = max_str[j];
            si += 1;
        }
    }
    let mut transpositions = 0i64;
    for i in 0..ms0.len() {
        if ms0[i] != ms1[i] {
            transpositions += 1;
        }
    }
    let mut prefix = 0i64;
    for mi in 0..min_str.len() {
        if s0[mi] == s1[mi] {
            prefix += 1;
        } else {
            break;
        }
    }
    (matches, (transpositions as f64 / 2.0) as i64, prefix, max_str.len() as i64)
}

pub fn jaro_similarity(a: &[char], b: &[char], threshold: f64) -> f64 {
    let mtp = jaro_matches(a, b);
    let m = mtp.0;
    if m == 0 {
        return 0.0;
    }
    let j = (m as f64 / a.len() as f64 + m as f64 / b.len() as f64 + (m - mtp.1) as f64 / m as f64) / 3.0;
    let mut jw = j;
    if j > threshold {
        jw = j + (0.1f64).min(1.0 / mtp.3 as f64) * mtp.2 as f64 * (1.0 - j);
    }
    jw
}

// ---------------------------------------------------------------------------
// Normalized Levenshtein + Metric LCS.
// ---------------------------------------------------------------------------

/// Normalized Levenshtein distance, including the equal/empty early exits
/// (which yield `0.0` on the Python surface).
pub fn normalized_levenshtein(a: &str, b: &str) -> f64 {
    if a == b {
        return 0.0;
    }
    let m_len = a.chars().count().max(b.chars().count());
    if m_len == 0 {
        return 0.0;
    }
    lev_dist(a, b) as f64 / m_len as f64
}

/// Metric LCS distance, including the equal/empty early exits.
pub fn metric_lcs(a: &str, b: &str) -> f64 {
    if a == b {
        return 0.0;
    }
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 0.0;
    }
    1.0 - (1.0 * lcs_length(&chars_of(a), &chars_of(b)) as f64) / max_len as f64
}

// ---------------------------------------------------------------------------
// NGram (replicates negative-index wraparound at i=0 exactly).
// ---------------------------------------------------------------------------

fn wrap_index(i: i64, n: usize) -> usize {
    ((i % n as i64 + n as i64) % n as i64) as usize
}

pub fn ngram_dist(a: &str, b: &str, n: usize) -> f64 {
    let s0 = chars_of(a);
    let s1 = chars_of(b);
    let sl = s0.len();
    let tl = s1.len();
    if sl == 0 || tl == 0 {
        return 1.0;
    }
    if sl < n || tl < n {
        let mut cost = 0i64;
        for i in 0..sl.min(tl) {
            if s0[i] == s1[i] {
                cost += 1;
            }
        }
        return 1.0 - cost as f64 / sl.max(tl) as f64;
    }
    let special = '\n';
    let mut sa = vec!['\0'; sl + n - 1];
    for (i, cell) in sa.iter_mut().enumerate() {
        // i + 1 - n (not i - n + 1): usize must never go negative.
        *cell = if i < n - 1 { special } else { s0[i + 1 - n] };
    }
    let mut p: Vec<f64> = (0..=sl).map(|i| i as f64).collect();
    let mut d: Vec<f64> = vec![0.0; sl + 1];
    for j in 1..=tl {
        let mut tj = vec!['\0'; n];
        if j < n {
            for cell in tj.iter_mut().take(n - j) {
                *cell = special;
            }
            for ti in (n - j)..n {
                tj[ti] = s1[ti - (n - j)];
            }
        } else {
            for (k, c) in s1[j - n..j].iter().enumerate() {
                tj[k] = *c;
            }
        }
        d[0] = j as f64;
        for i in 0..=sl {
            let mut cost = 0i64;
            let mut tn = n as i64;
            for ni in 0..n {
                if sa[wrap_index(i as i64 - 1 + ni as i64, sa.len())] != tj[ni] {
                    cost += 1;
                } else if sa[wrap_index(i as i64 - 1 + ni as i64, sa.len())] == special {
                    tn -= 1;
                }
            }
            let ec = cost as f64 / tn as f64;
            let mut best = d[wrap_index(i as i64 - 1, sl + 1)] + 1.0;
            let c = p[i] + 1.0;
            if c < best {
                best = c;
            }
            let c = p[wrap_index(i as i64 - 1, sl + 1)] + ec;
            if c < best {
                best = c;
            }
            d[i] = best;
        }
        std::mem::swap(&mut p, &mut d);
    }
    p[sl] / (tl.max(sl) as f64)
}

// ---------------------------------------------------------------------------
// Shingle profiles (insertion-ordered; float sums iterate in that order).
// ---------------------------------------------------------------------------

fn collapse_ws(s: &str) -> String {
    // `re \s+ -> " "` (leading/trailing runs become one space, not stripped).
    let mut out = String::new();
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

pub fn profile_get(prof: &[(String, i64)], k: &str) -> i64 {
    prof.iter().find(|(x, _)| x == k).map(|(_, v)| *v).unwrap_or(0)
}

pub fn profile_vec(s: &str, k: usize) -> Vec<(String, i64)> {
    let norm = collapse_ws(s);
    let chars: Vec<char> = norm.chars().collect();
    let mut prof: Vec<(String, i64)> = Vec::new();
    if k == 0 {
        // Replicates `range(len+1)` empty-shingle quirk exactly.
        prof.push((String::new(), chars.len() as i64 + 1));
        return prof;
    }
    if chars.len() + 1 > k {
        for i in 0..=(chars.len() - k) {
            let sh: String = chars[i..i + k].iter().collect();
            match prof.iter_mut().find(|(x, _)| *x == sh) {
                Some(e) => e.1 += 1,
                None => prof.push((sh, 1)),
            }
        }
    }
    prof
}

pub fn union_len(p0: &[(String, i64)], p1: &[(String, i64)]) -> usize {
    // Distinct keys across both profiles (insertion order irrelevant: count only).
    let mut seen: Vec<&String> = p0.iter().map(|(k, _)| k).collect();
    for (k, _) in p1 {
        if !seen.contains(&k) {
            seen.push(k);
        }
    }
    seen.len()
}

pub fn dot_product(p0: &[(String, i64)], p1: &[(String, i64)]) -> f64 {
    let (small, large) = if p0.len() < p1.len() { (p0, p1) } else { (p1, p0) };
    let mut agg = 0.0f64;
    for (k, v) in small {
        let i = profile_get(large, k);
        if i == 0 {
            continue;
        }
        agg += 1.0 * *v as f64 * i as f64;
    }
    agg
}

pub fn prof_norm(p: &[(String, i64)]) -> f64 {
    let mut agg = 0.0f64;
    for (_, v) in p {
        agg += 1.0 * *v as f64 * *v as f64;
    }
    agg.sqrt()
}

/// Cosine similarity, including the equal/short early exits.
pub fn cosine_similarity(a: &str, b: &str, k: usize) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.chars().count() < k || b.chars().count() < k {
        return 0.0;
    }
    let (p0, p1) = (profile_vec(a, k), profile_vec(b, k));
    dot_product(&p0, &p1) / (prof_norm(&p0) * prof_norm(&p1))
}

/// Jaccard similarity, including the equal/short early exits.
pub fn jaccard_similarity(a: &str, b: &str, k: usize) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.chars().count() < k || b.chars().count() < k {
        return 0.0;
    }
    let (p0, p1) = (profile_vec(a, k), profile_vec(b, k));
    let u = union_len(&p0, &p1);
    let inter = (p0.len() + p1.len() - u) as f64;
    1.0 * inter / u as f64
}

/// QGram profile distance over prebuilt profiles.
pub fn qgram_profile_distance(p0: &[(String, i64)], p1: &[(String, i64)]) -> i64 {
    let mut union: Vec<&String> = Vec::new();
    for (k, _) in p0.iter().chain(p1.iter()) {
        if !union.contains(&k) {
            union.push(k);
        }
    }
    let mut agg = 0i64;
    for k in union {
        agg += (profile_get(p0, k) - profile_get(p1, k)).abs();
    }
    agg
}

/// QGram distance over strings.
pub fn qgram_distance(a: &str, b: &str, k: usize) -> i64 {
    let (p0, p1) = (profile_vec(a, k), profile_vec(b, k));
    qgram_profile_distance(&p0, &p1)
}

/// Sorensen-Dice similarity, including the equal early exit.
pub fn sorensen_dice_similarity(a: &str, b: &str, k: usize) -> f64 {
    if a == b {
        return 1.0;
    }
    let (p0, p1) = (profile_vec(a, k), profile_vec(b, k));
    let u = union_len(&p0, &p1);
    let inter = (p0.len() + p1.len() - u) as f64;
    2.0 * inter / (p0.len() + p1.len()) as f64
}

/// Overlap-coefficient similarity, including the equal early exit.
pub fn overlap_similarity(a: &str, b: &str, k: usize) -> f64 {
    if a == b {
        return 1.0;
    }
    let (p0, p1) = (profile_vec(a, k), profile_vec(b, k));
    let u = union_len(&p0, &p1);
    let inter = (p0.len() + p1.len() - u) as f64;
    inter / p0.len().min(p1.len()) as f64
}

// ---------------------------------------------------------------------------
// Weighted Levenshtein, default (unit-cost) path.
// ---------------------------------------------------------------------------

/// Weighted-Levenshtein DP with unit costs (no Python callbacks): the exact
/// arithmetic the binding runs when no cost function is supplied, so Rust
/// consumers share the default path. Custom cost functions stay in the
/// binding, which owns the Python callbacks.
pub fn weighted_levenshtein_default(a: &str, b: &str) -> f64 {
    let (ca, cb) = (chars_of(a), chars_of(b));
    if ca.is_empty() {
        let mut cost = 0.0f64;
        for _ in &cb {
            cost += 1.0;
        }
        return cost;
    }
    if cb.is_empty() {
        let mut cost = 0.0f64;
        for _ in &ca {
            cost += 1.0;
        }
        return cost;
    }
    let mut v0 = vec![0.0f64; cb.len() + 1];
    let mut v1 = vec![0.0f64; cb.len() + 1];
    v0[0] = 0.0;
    for i in 1..v0.len() {
        v0[i] = v0[i - 1] + 1.0;
    }
    for c0 in &ca {
        let deletion_cost = 1.0;
        v1[0] = v0[0] + deletion_cost;
        for (j, c1) in cb.iter().enumerate() {
            let mut cost = 0.0f64;
            if c0 != c1 {
                cost = 1.0;
            }
            let insertion_cost = 1.0;
            let mut best = v1[j] + insertion_cost;
            let c = v0[j + 1] + deletion_cost;
            if c < best {
                best = c;
            }
            let c = v0[j] + cost;
            if c < best {
                best = c;
            }
            v1[j + 1] = best;
        }
        std::mem::swap(&mut v0, &mut v1);
    }
    v0[cb.len()]
}

// ---------------------------------------------------------------------------
// SIFT4 tokenizers / evaluators (named hooks; custom callables stay in the
// binding). All pure: same values with or without Python.
// ---------------------------------------------------------------------------

/// `wordsplittokenizer`: split on whitespace runs; empty input yields none.
pub fn wordsplit_tokenize(s: &str) -> Vec<String> {
    if s.is_empty() {
        return vec![];
    }
    s.split_whitespace().map(|w| w.to_string()).collect()
}

/// `characterfrequencytokenizer`: 26 lowercase `a..=z` counts.
pub fn charfreq_tokenize(s: &str) -> Vec<i64> {
    let lowered = s.to_lowercase();
    ('a'..='z').map(|c| lowered.matches(c).count() as i64).collect()
}

/// `ngramtokenizer(s, n)`.
pub fn ngram_tokenize(s: &str, n: usize) -> Vec<String> {
    if s.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    if chars.len() as i64 - n as i64 - 1 > 0 {
        for i in 0..(chars.len() as i64 - n as i64 - 1) {
            out.push(chars[i as usize..(i + n as i64) as usize].iter().collect());
        }
    }
    out
}

/// `rewardlengthevaluator`.
pub fn reward_length1(l: f64) -> f64 {
    if l < 1.0 { l } else { l - 1.0 / (l + 1.0) }
}

/// `rewardlengthevaluator2`.
pub fn reward_length2(l: f64) -> f64 {
    l.powf(1.5)
}

/// `longertranspositionsaremorecostly`.
pub fn longer_transposition_cost(c1: i64, c2: i64) -> f64 {
    (c2 - c1).abs() as f64 / 9.0 + 1.0
}

/// Default transposition evaluation (`lcss - trans`).
pub fn transposition_eval_sub(lcss: f64, trans: f64) -> f64 {
    lcss - trans
}

// ---------------------------------------------------------------------------
// SIFT4 default path (int-exact; core runs in f64, exact below 2^53).
// ---------------------------------------------------------------------------

/// Python round() without ndigits (banker's rounding to int).
pub fn py_round(f: f64) -> i64 {
    let fl = f.floor();
    let frac = f - fl;
    if frac < 0.5 {
        fl as i64
    } else if frac > 0.5 {
        (fl + 1.0) as i64
    } else {
        let i = fl as i64;
        if i % 2 == 0 {
            i
        } else {
            i + 1
        }
    }
}

/// Int-exact SIFT4 core for the all-default path (no Python callbacks).
// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
pub fn sift4_default_distance(s1: &str, s2: &str, maxoffset: i64) -> i64 {
    let t1: Vec<char> = s1.chars().collect();
    let t2: Vec<char> = s2.chars().collect();
    let (l1, l2) = (t1.len() as i64, t2.len() as i64);
    if l1 == 0 {
        return l2;
    }
    if l2 == 0 {
        return l1;
    }
    let (mut c1, mut c2) = (0i64, 0i64);
    let (mut lcss, mut local_cs, mut trans) = (0i64, 0i64, 0i64);
    let mut offs: Vec<(i64, i64, bool)> = Vec::new();
    while c1 < l1 && c2 < l2 {
        if t1[c1 as usize] == t2[c2 as usize] {
            local_cs += 1;
            let mut is_trans = false;
            let mut i = 0usize;
            while i < offs.len() {
                let (oc1, oc2, otrans) = offs[i];
                if c1 <= oc1 || c2 <= oc2 {
                    is_trans = (c2 - c1).abs() >= (oc2 - oc1).abs();
                    if is_trans {
                        trans += 1;
                    } else if !otrans {
                        offs[i].2 = true;
                        trans += 1;
                    }
                    break;
                } else if c1 > oc2 && c2 > oc1 {
                    offs.remove(i);
                    continue;
                } else {
                    i += 1;
                }
            }
            offs.push((c1, c2, is_trans));
        } else {
            lcss += local_cs;
            local_cs = 0;
            if c1 != c2 {
                let m = c1.min(c2);
                c1 = m;
                c2 = m;
            }
            for i in 0..maxoffset.max(0) {
                if c1 + i < l1 && t1[(c1 + i) as usize] == t2[c2 as usize] {
                    c1 += i - 1;
                    c2 -= 1;
                    break;
                }
                if c2 + i < l2 && t1[c1 as usize] == t2[(c2 + i) as usize] {
                    c1 -= 1;
                    c2 += i - 1;
                    break;
                }
            }
        }
        c1 += 1;
        c2 += 1;
        if (c1 >= l1) || (c2 >= l2) {
            lcss += local_cs;
            local_cs = 0;
            let m = c1.min(c2);
            c1 = m;
            c2 = m;
        }
    }
    lcss += local_cs;
    py_round(l1.max(l2) as f64 - (lcss - trans) as f64)
}

/// `sift4tokenmatcher` similarity over plain strings.
pub fn sift4_token_match(t1: &str, t2: &str) -> bool {
    let maxl = t1.chars().count().max(t2.chars().count());
    let d = sift4_default_distance(t1, t2, 5);
    1.0 - d as f64 / maxl as f64 > 0.7
}

/// `sift4matchingevaluator` similarity over plain strings.
pub fn sift4_matching_eval(t1: &str, t2: &str) -> f64 {
    let maxl = t1.chars().count().max(t2.chars().count());
    let d = sift4_default_distance(t1, t2, 5);
    1.0 - d as f64 / maxl as f64
}

// ---------------------------------------------------------------------------
// Pure-Rust tests (miri-clean subset: no Python API touched).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod core_tests {
    use super::*;

    #[test]
    fn edit_distances_pin_vectors() {
        assert_eq!(lev_dist("kitten", "sitting"), 3);
        assert_eq!(lev_dist("", "abc"), 3);
        assert_eq!(damerau_dist("kitten", "sitting"), 3);
        assert_eq!(damerau_dist("abcd", "acbd"), 1);
        assert_eq!(osa_dist("abcd", "acbd"), 1);
        assert_eq!(lcs_length(&chars_of("abc"), &chars_of("ac")), 2);
        assert_eq!(normalized_levenshtein("abc", "abc"), 0.0);
        assert_eq!(metric_lcs("abc", "abc"), 0.0);
    }

    #[test]
    fn jaro_self_agrees() {
        let a = chars_of("dixon");
        assert_eq!(jaro_similarity(&a, &a, 0.7), 1.0);
        let mtp = jaro_matches(&a, &a);
        assert_eq!((mtp.0, mtp.2), (5, 5));
    }

    #[test]
    fn shingle_metrics_pin_vectors() {
        assert_eq!(cosine_similarity("hello", "hello", 2), 1.0);
        assert_eq!(jaccard_similarity("hello", "hello", 2), 1.0);
        assert_eq!(sorensen_dice_similarity("hello", "hello", 2), 1.0);
        assert_eq!(overlap_similarity("hello", "hello", 2), 1.0);
        assert_eq!(qgram_distance("hello", "hello", 2), 0);
        assert_eq!(qgram_profile_distance(&profile_vec("hello", 2), &profile_vec("hello", 2)), 0);
        assert_eq!(cosine_similarity("a", "b", 2), 0.0);
    }

    #[test]
    fn weighted_default_matches_unit_costs() {
        assert_eq!(weighted_levenshtein_default("kitten", "sitting"), 3.0);
        assert_eq!(weighted_levenshtein_default("", "abc"), 3.0);
        assert_eq!(weighted_levenshtein_default("abc", ""), 3.0);
    }

    #[test]
    fn sift4_default_pins() {
        assert_eq!(sift4_default_distance("", "abc", 5), 3);
        assert_eq!(sift4_default_distance("", "", 5), 0);
        assert_eq!(py_round(2.5), 2);
        assert_eq!(py_round(3.5), 4);
        assert!(sift4_token_match("dixon", "dixon"));
        assert_eq!(reward_length1(2.0), 2.0 - 1.0 / 3.0);
        assert_eq!(reward_length2(4.0), 8.0);
        assert_eq!(longer_transposition_cost(1, 10), 2.0);
        assert_eq!(transposition_eval_sub(3.0, 1.0), 2.0);
    }

    #[test]
    fn tokenizers_pin_vectors() {
        assert_eq!(wordsplit_tokenize("a b "), vec!["a".to_string(), "b".to_string()]);
        assert!(wordsplit_tokenize("").is_empty());
        let cf = charfreq_tokenize("aab");
        assert_eq!(cf.len(), 26);
        assert_eq!((cf[0], cf[1], cf[2]), (2, 1, 0));
        assert_eq!(ngram_tokenize("hello", 2), vec!["he".to_string(), "el".to_string()]);
        assert!(ngram_tokenize("", 2).is_empty());
    }
}
