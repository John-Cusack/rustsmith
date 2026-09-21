// SPDX-License-Identifier: MIT
// Provenance: rustsmith Stage-1 mirror of luozhouyang/python-string-similarity (MIT).
//! Pure-Rust port of python-string-similarity. Every algorithm replicates the
//! original op-for-op over chars (never bytes) with identical accumulation
//! order, so float outputs are bit-exact and int/float return types match
//! (several metrics return `0.0`/`1.0` floats on early exits, ints otherwise).

use pyo3::exceptions::{PyNotImplementedError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn ch(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn none_err(arg: &str) -> PyErr {
    PyTypeError::new_err(format!("Argument {arg} is NoneType."))
}

fn int_obj(py: Python<'_>, v: i64) -> PyObject {
    v.into_pyobject(py).unwrap().into_any().unbind()
}

fn float_obj(py: Python<'_>, v: f64) -> PyObject {
    v.into_pyobject(py).unwrap().into_any().unbind()
}

// ---------------------------------------------------------------------------
// Base classes (plain; raise like the originals).
// ---------------------------------------------------------------------------

macro_rules! base_class {
    ($name:ident, $method:ident) => {
        #[pyclass]
        struct $name;

        #[pymethods]
        impl $name {
            #[new]
            fn new() -> Self {
                $name
            }
            fn $method(&self) -> PyResult<()> {
                Err(PyNotImplementedError::new_err("not implemented"))
            }
        }
    };
}

base_class!(StringDistance, distance);
base_class!(NormalizedStringDistance, distance);
base_class!(MetricStringDistance, distance);
base_class!(StringSimilarity, similarity);
base_class!(NormalizedStringSimilarity, similarity);

// ---------------------------------------------------------------------------
// Levenshtein.
// ---------------------------------------------------------------------------

// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
fn lev_dist(a: &str, b: &str) -> i64 {
    let s0 = ch(a);
    let s1 = ch(b);
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

#[pyclass]
struct Levenshtein;

#[pymethods]
impl Levenshtein {
    #[new]
    fn new() -> Self {
        Levenshtein
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, py: Python<'_>, s0: Option<String>, s1: Option<String>) -> PyResult<PyObject> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(float_obj(py, 0.0));
        }
        Ok(int_obj(py, lev_dist(&a, &b)))
    }
}

// ---------------------------------------------------------------------------
// Damerau (true Damerau-Levenshtein; `da` maps char -> last row).
// ---------------------------------------------------------------------------

// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
fn damerau_dist(a: &str, b: &str) -> i64 {
    let s0 = ch(a);
    let s1 = ch(b);
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

#[pyclass]
struct Damerau;

#[pymethods]
impl Damerau {
    #[new]
    fn new() -> Self {
        Damerau
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, py: Python<'_>, s0: Option<String>, s1: Option<String>) -> PyResult<PyObject> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(float_obj(py, 0.0));
        }
        Ok(int_obj(py, damerau_dist(&a, &b)))
    }
}

// ---------------------------------------------------------------------------
// Optimal string alignment (empty quirk: 0.0 float, not the length).
// ---------------------------------------------------------------------------

// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
fn osa_dist(a: &str, b: &str) -> i64 {
    let s0 = ch(a);
    let s1 = ch(b);
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

#[pyclass]
struct OptimalStringAlignment;

#[pymethods]
impl OptimalStringAlignment {
    #[new]
    fn new() -> Self {
        OptimalStringAlignment
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, py: Python<'_>, s0: Option<String>, s1: Option<String>) -> PyResult<PyObject> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(float_obj(py, 0.0));
        }
        if a.is_empty() || b.is_empty() {
            return Ok(float_obj(py, 0.0));
        }
        Ok(int_obj(py, osa_dist(&a, &b)))
    }
}

// ---------------------------------------------------------------------------
// Longest common subsequence.
// ---------------------------------------------------------------------------

fn lcs_length(a: &[char], b: &[char]) -> i64 {
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

#[pyclass]
struct LongestCommonSubsequence;

#[pymethods]
impl LongestCommonSubsequence {
    #[new]
    fn new() -> Self {
        LongestCommonSubsequence
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, py: Python<'_>, s0: Option<String>, s1: Option<String>) -> PyResult<PyObject> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(float_obj(py, 0.0));
        }
        let (ca, cb) = (ch(&a), ch(&b));
        Ok(int_obj(py, ca.len() as i64 + cb.len() as i64 - 2 * lcs_length(&ca, &cb)))
    }
    #[staticmethod]
    #[pyo3(signature = (s0=None, s1=None))]
    fn length(s0: Option<String>, s1: Option<String>) -> PyResult<i64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        Ok(lcs_length(&ch(&a), &ch(&b)))
    }
}

// ---------------------------------------------------------------------------
// Jaro-Winkler.
// ---------------------------------------------------------------------------

fn jaro_matches(s0: &[char], s1: &[char]) -> (i64, i64, i64, i64) {
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

fn jaro_similarity(a: &[char], b: &[char], threshold: f64) -> f64 {
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

#[pyclass]
struct JaroWinkler {
    threshold: f64,
}

#[pymethods]
impl JaroWinkler {
    #[new]
    #[pyo3(signature = (threshold=0.7))]
    fn new(threshold: f64) -> Self {
        JaroWinkler { threshold }
    }
    fn get_threshold(&self) -> f64 {
        self.threshold
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn similarity(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(1.0);
        }
        Ok(jaro_similarity(&ch(&a), &ch(&b), self.threshold))
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        Ok(1.0 - self.similarity(s0, s1)?)
    }
    #[staticmethod]
    fn matches(s0: String, s1: String) -> PyResult<Vec<i64>> {
        let (a, b) = (ch(&s0), ch(&s1));
        let mtp = jaro_matches(&a, &b);
        Ok(vec![mtp.0, mtp.1, mtp.2, mtp.3])
    }
}

// ---------------------------------------------------------------------------
// Normalized Levenshtein + Metric LCS.
// ---------------------------------------------------------------------------

#[pyclass]
struct NormalizedLevenshtein;

#[pymethods]
impl NormalizedLevenshtein {
    #[new]
    fn new() -> Self {
        NormalizedLevenshtein
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(0.0);
        }
        let m_len = a.chars().count().max(b.chars().count());
        if m_len == 0 {
            return Ok(0.0);
        }
        Ok(lev_dist(&a, &b) as f64 / m_len as f64)
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn similarity(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        Ok(1.0 - self.distance(s0, s1)?)
    }
}

#[pyclass]
struct MetricLCS;

#[pymethods]
impl MetricLCS {
    #[new]
    fn new() -> Self {
        MetricLCS
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(0.0);
        }
        let max_len = a.chars().count().max(b.chars().count());
        if max_len == 0 {
            return Ok(0.0);
        }
        Ok(1.0 - (1.0 * lcs_length(&ch(&a), &ch(&b)) as f64) / max_len as f64)
    }
}

// ---------------------------------------------------------------------------
// NGram (replicates negative-index wraparound at i=0 exactly).
// ---------------------------------------------------------------------------

fn wrap_index(i: i64, n: usize) -> usize {
    ((i % n as i64 + n as i64) % n as i64) as usize
}

fn ngram_dist(a: &str, b: &str, n: usize) -> f64 {
    let s0 = ch(a);
    let s1 = ch(b);
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

#[pyclass]
struct NGram {
    n: usize,
}

#[pymethods]
impl NGram {
    #[new]
    #[pyo3(signature = (n=2))]
    fn new(n: usize) -> Self {
        NGram { n }
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(0.0);
        }
        Ok(ngram_dist(&a, &b, self.n))
    }
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

fn profile_get(prof: &[(String, i64)], k: &str) -> i64 {
    prof.iter().find(|(x, _)| x == k).map(|(_, v)| *v).unwrap_or(0)
}

fn profile_vec(s: &str, k: usize) -> Vec<(String, i64)> {
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
fn union_len(p0: &[(String, i64)], p1: &[(String, i64)]) -> usize {
    // Distinct keys across both profiles (insertion order irrelevant: count only).
    let mut seen: Vec<&String> = p0.iter().map(|(k, _)| k).collect();
    for (k, _) in p1 {
        if !seen.contains(&k) {
            seen.push(k);
        }
    }
    seen.len()
}

fn pydict_from_profile(py: Python<'_>, prof: &[(String, i64)]) -> PyResult<PyObject> {
    let d = PyDict::new(py);
    for (k, v) in prof {
        d.set_item(k, v)?;
    }
    Ok(d.into_pyobject(py)?.into_any().unbind())
}

fn profile_from_pydict(d: &Bound<'_, PyDict>) -> PyResult<Vec<(String, i64)>> {
    let mut out = Vec::new();
    for (k, v) in d.iter() {
        out.push((k.extract::<String>()?, v.extract::<i64>()?));
    }
    Ok(out)
}

#[pyclass]
struct ShingleBased {
    k: usize,
}

#[pymethods]
impl ShingleBased {
    #[new]
    #[pyo3(signature = (k=3))]
    fn new(k: usize) -> Self {
        ShingleBased { k }
    }
    fn get_k(&self) -> usize {
        self.k
    }
    fn get_profile(&self, py: Python<'_>, string: String) -> PyResult<PyObject> {
        pydict_from_profile(py, &profile_vec(&string, self.k))
    }
}

fn dot_product(p0: &[(String, i64)], p1: &[(String, i64)]) -> f64 {
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

fn prof_norm(p: &[(String, i64)]) -> f64 {
    let mut agg = 0.0f64;
    for (_, v) in p {
        agg += 1.0 * *v as f64 * *v as f64;
    }
    agg.sqrt()
}


#[pyclass]
struct Cosine {
    k: usize,
}

#[pymethods]
impl Cosine {
    #[new]
    fn new(k: usize) -> Self {
        Cosine { k }
    }
    fn get_k(&self) -> usize {
        self.k
    }
    fn get_profile(&self, py: Python<'_>, string: String) -> PyResult<PyObject> {
        pydict_from_profile(py, &profile_vec(&string, self.k))
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn similarity(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(1.0);
        }
        if a.chars().count() < self.k || b.chars().count() < self.k {
            return Ok(0.0);
        }
        let (p0, p1) = (profile_vec(&a, self.k), profile_vec(&b, self.k));
        Ok(dot_product(&p0, &p1) / (prof_norm(&p0) * prof_norm(&p1)))
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        Ok(1.0 - self.similarity(s0, s1)?)
    }
    fn similarity_profiles(&self, p0: &Bound<'_, PyDict>, p1: &Bound<'_, PyDict>) -> PyResult<f64> {
        let (a, b) = (profile_from_pydict(p0)?, profile_from_pydict(p1)?);
        Ok(dot_product(&a, &b) / (prof_norm(&a) * prof_norm(&b)))
    }
    #[staticmethod]
    fn _dot_product(p0: &Bound<'_, PyDict>, p1: &Bound<'_, PyDict>) -> PyResult<f64> {
        let (a, b) = (profile_from_pydict(p0)?, profile_from_pydict(p1)?);
        Ok(dot_product(&a, &b))
    }
    #[staticmethod]
    fn _norm(p: &Bound<'_, PyDict>) -> PyResult<f64> {
        Ok(prof_norm(&profile_from_pydict(p)?))
    }
}

#[pyclass]
struct Jaccard {
    k: usize,
}

#[pymethods]
impl Jaccard {
    #[new]
    fn new(k: usize) -> Self {
        Jaccard { k }
    }
    fn get_k(&self) -> usize {
        self.k
    }
    fn get_profile(&self, py: Python<'_>, string: String) -> PyResult<PyObject> {
        pydict_from_profile(py, &profile_vec(&string, self.k))
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn similarity(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(1.0);
        }
        if a.chars().count() < self.k || b.chars().count() < self.k {
            return Ok(0.0);
        }
        let (p0, p1) = (profile_vec(&a, self.k), profile_vec(&b, self.k));
        let u = union_len(&p0, &p1);
        let inter = (p0.len() + p1.len() - u) as f64;
        Ok(1.0 * inter / u as f64)
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        Ok(1.0 - self.similarity(s0, s1)?)
    }
}

#[pyclass]
struct QGram {
    k: usize,
}

#[pymethods]
impl QGram {
    #[new]
    #[pyo3(signature = (k=3))]
    fn new(k: usize) -> Self {
        QGram { k }
    }
    fn get_k(&self) -> usize {
        self.k
    }
    fn get_profile(&self, py: Python<'_>, string: String) -> PyResult<PyObject> {
        pydict_from_profile(py, &profile_vec(&string, self.k))
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, py: Python<'_>, s0: Option<String>, s1: Option<String>) -> PyResult<PyObject> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(float_obj(py, 0.0));
        }
        let (p0, p1) = (profile_vec(&a, self.k), profile_vec(&b, self.k));
        Ok(int_obj(py, Self::distance_profile_rs(&p0, &p1)))
    }
    #[staticmethod]
    fn distance_profile(p0: &Bound<'_, PyDict>, p1: &Bound<'_, PyDict>) -> PyResult<i64> {
        let (a, b) = (profile_from_pydict(p0)?, profile_from_pydict(p1)?);
        Ok(Self::distance_profile_rs(&a, &b))
    }
}

impl QGram {
    fn distance_profile_rs(p0: &[(String, i64)], p1: &[(String, i64)]) -> i64 {
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
}

#[pyclass]
struct SorensenDice {
    k: usize,
}

#[pymethods]
impl SorensenDice {
    #[new]
    #[pyo3(signature = (k=3))]
    fn new(k: usize) -> Self {
        SorensenDice { k }
    }
    fn get_k(&self) -> usize {
        self.k
    }
    fn get_profile(&self, py: Python<'_>, string: String) -> PyResult<PyObject> {
        pydict_from_profile(py, &profile_vec(&string, self.k))
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn similarity(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(1.0);
        }
        let (p0, p1) = (profile_vec(&a, self.k), profile_vec(&b, self.k));
        let u = union_len(&p0, &p1);
        let inter = (p0.len() + p1.len() - u) as f64;
        Ok(2.0 * inter / (p0.len() + p1.len()) as f64)
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        Ok(1.0 - self.similarity(s0, s1)?)
    }
}

#[pyclass]
struct OverlapCoefficient {
    k: usize,
}

#[pymethods]
impl OverlapCoefficient {
    #[new]
    #[pyo3(signature = (k=3))]
    fn new(k: usize) -> Self {
        OverlapCoefficient { k }
    }
    fn get_k(&self) -> usize {
        self.k
    }
    fn get_profile(&self, py: Python<'_>, string: String) -> PyResult<PyObject> {
        pydict_from_profile(py, &profile_vec(&string, self.k))
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn similarity(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(1.0);
        }
        let (p0, p1) = (profile_vec(&a, self.k), profile_vec(&b, self.k));
        let u = union_len(&p0, &p1);
        let inter = (p0.len() + p1.len() - u) as f64;
        Ok(inter / p0.len().min(p1.len()) as f64)
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        Ok(1.0 - self.similarity(s0, s1)?)
    }
}

fn call_cost1(py: Python<'_>, f: &Option<Py<PyAny>>, a: String) -> PyResult<f64> {
    match f {
        None => Ok(1.0),
        Some(cb) => cb.bind(py).call1((a,))?.extract::<f64>(),
    }
}

fn call_cost2(py: Python<'_>, f: &Option<Py<PyAny>>, a: String, b: String) -> PyResult<f64> {
    match f {
        None => Ok(1.0),
        Some(cb) => cb.bind(py).call1((a, b))?.extract::<f64>(),
    }
}

#[pyclass]
struct WeightedLevenshtein {
    sub: Option<Py<PyAny>>,
    ins: Option<Py<PyAny>>,
    del: Option<Py<PyAny>>,
}

#[pymethods]
impl WeightedLevenshtein {
    #[new]
    #[pyo3(signature = (substitution_cost_fn=None, insertion_cost_fn=None, deletion_cost_fn=None))]
    fn new(
        substitution_cost_fn: Option<Py<PyAny>>,
        insertion_cost_fn: Option<Py<PyAny>>,
        deletion_cost_fn: Option<Py<PyAny>>,
    ) -> Self {
        WeightedLevenshtein { sub: substitution_cost_fn, ins: insertion_cost_fn, del: deletion_cost_fn }
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, py: Python<'_>, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        if a == b {
            return Ok(0.0);
        }
        let (ca, cb) = (ch(&a), ch(&b));
        if ca.is_empty() {
            let mut cost = 0.0f64;
            for c in &cb {
                cost += call_cost1(py, &self.ins, c.to_string())?;
            }
            return Ok(cost);
        }
        if cb.is_empty() {
            let mut cost = 0.0f64;
            for c in &ca {
                cost += call_cost1(py, &self.del, c.to_string())?;
            }
            return Ok(cost);
        }
        let mut v0 = vec![0.0f64; cb.len() + 1];
        let mut v1 = vec![0.0f64; cb.len() + 1];
        v0[0] = 0.0;
        for i in 1..v0.len() {
            v0[i] = v0[i - 1] + call_cost1(py, &self.ins, cb[i - 1].to_string())?;
        }
        for c0 in &ca {
            let deletion_cost = call_cost1(py, &self.del, c0.to_string())?;
            v1[0] = v0[0] + deletion_cost;
            for (j, c1) in cb.iter().enumerate() {
                let mut cost = 0.0f64;
                if c0 != c1 {
                    cost = call_cost2(py, &self.sub, c0.to_string(), c1.to_string())?;
                }
                let insertion_cost = call_cost1(py, &self.ins, c1.to_string())?;
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
        Ok(v0[cb.len()])
    }
}

// ---------------------------------------------------------------------------
// SIFT4 (default path is int-exact; core runs in f64, exact below 2^53).
// ---------------------------------------------------------------------------

enum TokHook {
    Default,
    Wordsplit,
    CharFreq,
    Ngram,
    Custom(Py<PyAny>),
}

enum MatchHook {
    Eq,
    Sift4,
    Custom(Py<PyAny>),
}

enum MatchEvalHook {
    One,
    Sift4Eval,
    Custom(Py<PyAny>),
}

enum LocalLenHook {
    Identity,
    Reward1,
    Reward2,
    Custom(Py<PyAny>),
}

enum TransCostHook {
    One,
    Longer,
    Custom(Py<PyAny>),
}

enum TransEvalHook {
    Sub,
    Custom(Py<PyAny>),
}

struct SiftOpts {
    maxdistance: i64,
    tok: TokHook,
    mat: MatchHook,
    meval: MatchEvalHook,
    llen: LocalLenHook,
    tcost: TransCostHook,
    teval: TransEvalHook,
}

impl SiftOpts {
    fn defaults() -> Self {
        SiftOpts {
            maxdistance: 0,
            tok: TokHook::Default,
            mat: MatchHook::Eq,
            meval: MatchEvalHook::One,
            llen: LocalLenHook::Identity,
            tcost: TransCostHook::One,
            teval: TransEvalHook::Sub,
        }
    }
}

enum Tok {
    S(String),
    I(i64),
    O(Py<PyAny>),
}

fn tok_to_py(py: Python<'_>, t: &Tok) -> PyObject {
    match t {
        Tok::S(s) => s.clone().into_pyobject(py).unwrap().into_any().unbind(),
        Tok::I(v) => v.into_pyobject(py).unwrap().into_any().unbind(),
        Tok::O(o) => o.clone_ref(py),
    }
}

fn tok_len(py: Python<'_>, t: &Tok) -> PyResult<usize> {
    match t {
        Tok::S(s) => Ok(s.chars().count()),
        Tok::I(_) => Err(PyTypeError::new_err("object of type 'int' has no len()")),
        Tok::O(o) => o.bind(py).len(),
    }
}

fn tok_str(py: Python<'_>, t: &Tok) -> PyResult<String> {
    match t {
        Tok::S(s) => Ok(s.clone()),
        Tok::I(v) => Ok(v.to_string()),
        Tok::O(o) => o.bind(py).str()?.extract::<String>(),
    }
}

fn apply_tok(py: Python<'_>, hook: &TokHook, s: &str) -> PyResult<Vec<Tok>> {
    match hook {
        TokHook::Default => Ok(s.chars().map(|c| Tok::S(c.to_string())).collect()),
        TokHook::Wordsplit => Ok(s.split_whitespace().map(|w| Tok::S(w.to_string())).collect()),
        TokHook::CharFreq => {
            let lowered = s.to_lowercase();
            Ok(('a'..='z')
                .map(|c| Tok::I(lowered.matches(c).count() as i64))
                .collect())
        }
        TokHook::Ngram => Err(PyTypeError::new_err(
            "ngramtokenizer() missing 1 required positional argument: 'n'",
        )),
        TokHook::Custom(f) => {
            let out = f.call1(py, (s,))?;
            let list = out.bind(py).downcast::<pyo3::types::PyList>().map_err(|_| {
                PyTypeError::new_err("tokenizer must return a list")
            })?;
            let mut toks = Vec::new();
            for item in list.iter() {
                if let Ok(st) = item.extract::<String>() {
                    toks.push(Tok::S(st));
                } else if let Ok(n) = item.extract::<i64>() {
                    toks.push(Tok::I(n));
                } else {
                    toks.push(Tok::O(item.unbind()));
                }
            }
            Ok(toks)
        }
    }
}

fn sift4_inner_default(s1: &str, s2: &str) -> i64 {
    // Recursion target for the sift4 matcher/evaluator (default options).
    let o = SiftOpts::defaults();
    // No Python callbacks on this path; a dummy GIL token is unavailable here,
    // so run the int-exact core directly.
    sift4_core_int(&o, s1, s2, 5)
}

fn apply_match(py: Python<'_>, hook: &MatchHook, t1: &Tok, t2: &Tok) -> PyResult<bool> {
    match hook {
        MatchHook::Eq => Ok(match (t1, t2) {
            (Tok::S(a), Tok::S(b)) => a == b,
            (Tok::I(a), Tok::I(b)) => a == b,
            (Tok::O(a), Tok::O(b)) => a.bind(py).eq(b.bind(py))?,
            _ => false,
        }),
        MatchHook::Sift4 => {
            let (a, b) = (tok_str(py, t1)?, tok_str(py, t2)?);
            let maxl = tok_len(py, t1)?.max(tok_len(py, t2)?);
            let d = sift4_inner_default(&a, &b);
            Ok(1.0 - d as f64 / maxl as f64 > 0.7)
        }
        MatchHook::Custom(f) => {
            let r = f.bind(py).call1((tok_to_py(py, t1), tok_to_py(py, t2)))?;
            r.extract::<bool>()
        }
    }
}

fn apply_meval(py: Python<'_>, hook: &MatchEvalHook, t1: &Tok, t2: &Tok) -> PyResult<f64> {
    match hook {
        MatchEvalHook::One => Ok(1.0),
        MatchEvalHook::Sift4Eval => {
            let (a, b) = (tok_str(py, t1)?, tok_str(py, t2)?);
            let maxl = tok_len(py, t1)?.max(tok_len(py, t2)?);
            let d = sift4_inner_default(&a, &b);
            Ok(1.0 - d as f64 / maxl as f64)
        }
        MatchEvalHook::Custom(f) => {
            let r = f.bind(py).call1((tok_to_py(py, t1), tok_to_py(py, t2)))?;
            r.extract::<f64>()
        }
    }
}

fn apply_llen(py: Python<'_>, hook: &LocalLenHook, l: f64) -> PyResult<f64> {
    match hook {
        LocalLenHook::Identity => Ok(l),
        LocalLenHook::Reward1 => Ok(if l < 1.0 { l } else { l - 1.0 / (l + 1.0) }),
        LocalLenHook::Reward2 => Ok(l.powf(1.5)),
        LocalLenHook::Custom(f) => {
            let r = f.bind(py).call1((l,))?;
            r.extract::<f64>()
        }
    }
}

fn apply_tcost(py: Python<'_>, hook: &TransCostHook, c1: i64, c2: i64) -> PyResult<f64> {
    match hook {
        TransCostHook::One => Ok(1.0),
        TransCostHook::Longer => Ok((c2 - c1).abs() as f64 / 9.0 + 1.0),
        TransCostHook::Custom(f) => {
            let r = f.bind(py).call1((c1, c2))?;
            r.extract::<f64>()
        }
    }
}

fn apply_teval(py: Python<'_>, hook: &TransEvalHook, lcss: f64, trans: f64) -> PyResult<f64> {
    match hook {
        TransEvalHook::Sub => Ok(lcss - trans),
        TransEvalHook::Custom(f) => {
            let r = f.bind(py).call1((lcss, trans))?;
            r.extract::<f64>()
        }
    }
}

/// Python round() without ndigits (banker's rounding to int).
fn py_round(f: f64) -> i64 {
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
fn sift4_core_int(o: &SiftOpts, s1: &str, s2: &str, maxoffset: i64) -> i64 {
    debug_assert!(matches!(o.tok, TokHook::Default));
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

/// Generic SIFT4 core (custom hooks may yield floats).
// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
fn sift4_core(py: Python<'_>, o: &SiftOpts, s1: &str, s2: &str, maxoffset: i64) -> PyResult<i64> {
    if matches!(o.tok, TokHook::Default)
        && matches!(o.mat, MatchHook::Eq)
        && matches!(o.meval, MatchEvalHook::One)
        && matches!(o.llen, LocalLenHook::Identity)
        && matches!(o.tcost, TransCostHook::One)
        && matches!(o.teval, TransEvalHook::Sub)
    {
        return Ok(sift4_core_int(o, s1, s2, maxoffset));
    }
    let t1 = apply_tok(py, &o.tok, s1)?;
    let t2 = apply_tok(py, &o.tok, s2)?;
    let (l1, l2) = (t1.len() as i64, t2.len() as i64);
    if l1 == 0 {
        return Ok(l2);
    }
    if l2 == 0 {
        return Ok(l1);
    }
    let (mut c1, mut c2) = (0i64, 0i64);
    let (mut lcss, mut local_cs, mut trans) = (0.0f64, 0.0f64, 0.0f64);
    let mut offs: Vec<(i64, i64, bool)> = Vec::new();
    while c1 < l1 && c2 < l2 {
        if apply_match(py, &o.mat, &t1[c1 as usize], &t2[c2 as usize])? {
            local_cs += apply_meval(py, &o.meval, &t1[c1 as usize], &t2[c2 as usize])?;
            let mut is_trans = false;
            let mut i = 0usize;
            while i < offs.len() {
                let (oc1, oc2, otrans) = offs[i];
                if c1 <= oc1 || c2 <= oc2 {
                    is_trans = (c2 - c1).abs() >= (oc2 - oc1).abs();
                    if is_trans {
                        trans += apply_tcost(py, &o.tcost, c1, c2)?;
                    } else if !otrans {
                        offs[i].2 = true;
                        trans += apply_tcost(py, &o.tcost, oc1, oc2)?;
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
            lcss += apply_llen(py, &o.llen, local_cs)?;
            local_cs = 0.0;
            if c1 != c2 {
                let m = c1.min(c2);
                c1 = m;
                c2 = m;
            }
            for i in 0..maxoffset.max(0) {
                if c1 + i < l1 && apply_match(py, &o.mat, &t1[(c1 + i) as usize], &t2[c2 as usize])? {
                    c1 += i - 1;
                    c2 -= 1;
                    break;
                }
                if c2 + i < l2 && apply_match(py, &o.mat, &t1[c1 as usize], &t2[(c2 + i) as usize])? {
                    c1 -= 1;
                    c2 += i - 1;
                    break;
                }
            }
        }
        c1 += 1;
        c2 += 1;
        if o.maxdistance != 0 {
            let temporary = apply_llen(py, &o.llen, c1.max(c2) as f64)?
                - apply_teval(py, &o.teval, lcss, trans)?;
            if temporary >= o.maxdistance as f64 {
                return Ok(py_round(temporary));
            }
        }
        if (c1 >= l1) || (c2 >= l2) {
            lcss += apply_llen(py, &o.llen, local_cs)?;
            local_cs = 0.0;
            let m = c1.min(c2);
            c1 = m;
            c2 = m;
        }
    }
    lcss += apply_llen(py, &o.llen, local_cs)?;
    Ok(py_round(
        apply_llen(py, &o.llen, l1.max(l2) as f64)? - apply_teval(py, &o.teval, lcss, trans)?,
    ))
}

fn hook_name_error(key: &str, table: &[(&str, &str)]) -> PyErr {
    let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
    PyValueError::new_err(format!("Option {} should be callable or one of [{}]", key, names.join(", ")))
}

fn resolve_sift_opts(py: Python<'_>, options: Option<Py<PyAny>>) -> PyResult<SiftOpts> {
    let mut o = SiftOpts::defaults();
    let Some(obj) = options else {
        return Ok(o);
    };
    let d = obj.bind(py).downcast::<PyDict>().map_err(|_| {
        PyValueError::new_err("options should be a dictionary")
    })?;
    for (k, v) in d.iter() {
        let key = k.extract::<String>().unwrap_or_else(|_| "?".to_string());
        match key.as_str() {
            "maxdistance" => {
                o.maxdistance = v.extract::<i64>().map_err(|_| {
                    PyValueError::new_err("Option maxdistance should be int")
                })?;
            }
            "tokenizer" | "tokenmatcher" | "matchingevaluator" | "locallengthevaluator" | "transpositioncostevaluator"
            | "transpositionsevaluator" => {
                if let Ok(name) = v.extract::<String>() {
                    match (key.as_str(), name.as_str()) {
                        ("tokenizer", "ngram") => o.tok = TokHook::Ngram,
                        ("tokenizer", "wordsplit") => o.tok = TokHook::Wordsplit,
                        ("tokenizer", "characterfrequency") => o.tok = TokHook::CharFreq,
                        ("tokenmatcher", "sift4tokenmatcher") => o.mat = MatchHook::Sift4,
                        ("matchingevaluator", "sift4matchingevaluator") => o.meval = MatchEvalHook::Sift4Eval,
                        ("locallengthevaluator", "rewardlengthevaluator") => o.llen = LocalLenHook::Reward1,
                        ("locallengthevaluator", "rewardlengthevaluator2") => o.llen = LocalLenHook::Reward2,
                        ("transpositioncostevaluator", "longertranspositionsaremorecostly") => {
                            o.tcost = TransCostHook::Longer;
                        }
                        _ => {
                            // Name tables replicate the original (including its
                            // 'tokematcher' spelling); unknown names error.
                            let table: &[(&str, &str)] = match key.as_str() {
                                "tokenizer" => &[("ngram", ""), ("wordsplit", ""), ("characterfrequency", "")],
                                "tokenmatcher" => &[("sift4tokenmatcher", "")],
                                "matchingevaluator" => &[("sift4matchingevaluator", "")],
                                "locallengthevaluator" => {
                                    &[("rewardlengthevaluator", ""), ("rewardlengthevaluator2", "")]
                                }
                                "transpositioncostevaluator" => &[("longertranspositionsaremorecostly", "")],
                                _ => &[],
                            };
                            return Err(hook_name_error(&key, table));
                        }
                    }
                } else if v.is_callable() {
                    let cb = v.unbind();
                    match key.as_str() {
                        "tokenizer" => o.tok = TokHook::Custom(cb),
                        "tokenmatcher" => o.mat = MatchHook::Custom(cb),
                        "matchingevaluator" => o.meval = MatchEvalHook::Custom(cb),
                        "locallengthevaluator" => o.llen = LocalLenHook::Custom(cb),
                        "transpositioncostevaluator" => o.tcost = TransCostHook::Custom(cb),
                        "transpositionsevaluator" => o.teval = TransEvalHook::Custom(cb),
                        _ => unreachable!(),
                    }
                } else {
                    let table: &[(&str, &str)] = match key.as_str() {
                        "tokenizer" => &[("ngram", ""), ("wordsplit", ""), ("characterfrequency", "")],
                        "tokenmatcher" => &[("sift4tokenmatcher", "")],
                        "matchingevaluator" => &[("sift4matchingevaluator", "")],
                        "locallengthevaluator" => {
                            &[("rewardlengthevaluator", ""), ("rewardlengthevaluator2", "")]
                        }
                        "transpositioncostevaluator" => &[("longertranspositionsaremorecostly", "")],
                        _ => &[],
                    };
                    return Err(hook_name_error(&key, table));
                }
            }
            _ => {
                return Err(PyValueError::new_err(format!("Option {} not recognized.", key)));
            }
        }
    }
    Ok(o)
}

#[pyclass]
struct SIFT4Options {
    opts: SiftOpts,
}

#[pymethods]
impl SIFT4Options {
    #[new]
    #[pyo3(signature = (options=None))]
    fn new(py: Python<'_>, options: Option<Py<PyAny>>) -> PyResult<Self> {
        Ok(SIFT4Options { opts: resolve_sift_opts(py, options)? })
    }
    #[getter]
    fn maxdistance(&self) -> i64 {
        self.opts.maxdistance
    }
    fn distance(&self) -> PyResult<f64> {
        Err(PyNotImplementedError::new_err("not implemented"))
    }
    #[staticmethod]
    fn ngramtokenizer(s: String, n: usize) -> Vec<String> {
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
    #[staticmethod]
    fn wordsplittokenizer(s: String) -> Vec<String> {
        if s.is_empty() {
            return vec![];
        }
        s.split_whitespace().map(|w| w.to_string()).collect()
    }
    #[staticmethod]
    fn characterfrequencytokenizer(s: String) -> Vec<i64> {
        let lowered = s.to_lowercase();
        ('a'..='z').map(|c| lowered.matches(c).count() as i64).collect()
    }
    #[staticmethod]
    fn sift4tokenmatcher(t1: String, t2: String) -> PyResult<bool> {
        let maxl = t1.chars().count().max(t2.chars().count());
        let d = sift4_inner_default(&t1, &t2);
        Ok(1.0 - d as f64 / maxl as f64 > 0.7)
    }
    #[staticmethod]
    fn sift4matchingevaluator(t1: String, t2: String) -> f64 {
        let maxl = t1.chars().count().max(t2.chars().count());
        let d = sift4_inner_default(&t1, &t2);
        1.0 - d as f64 / maxl as f64
    }
    #[staticmethod]
    fn rewardlengthevaluator(l: f64) -> f64 {
        if l < 1.0 {
            l
        } else {
            l - 1.0 / (l + 1.0)
        }
    }
    #[staticmethod]
    fn rewardlengthevaluator2(l: f64) -> f64 {
        l.powf(1.5)
    }
    #[staticmethod]
    fn longertranspositionsaremorecostly(c1: i64, c2: i64) -> f64 {
        (c2 - c1).abs() as f64 / 9.0 + 1.0
    }
}

#[pyclass]
struct SIFT4;

#[pymethods]
impl SIFT4 {
    #[new]
    fn new() -> Self {
        SIFT4
    }
    #[pyo3(signature = (s1, s2, maxoffset=5, options=None))]
    fn distance(
        &self,
        py: Python<'_>,
        s1: String,
        s2: String,
        maxoffset: i64,
        options: Option<Py<PyAny>>,
    ) -> PyResult<i64> {
        let opts = resolve_sift_opts(py, options)?;
        sift4_core(py, &opts, &s1, &s2, maxoffset)
    }
}

// ---------------------------------------------------------------------------
// Module.
// ---------------------------------------------------------------------------

#[pymodule]
fn _strsimpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Levenshtein>()?;
    m.add_class::<Damerau>()?;
    m.add_class::<JaroWinkler>()?;
    m.add_class::<NormalizedLevenshtein>()?;
    m.add_class::<Cosine>()?;
    m.add_class::<Jaccard>()?;
    m.add_class::<NGram>()?;
    m.add_class::<QGram>()?;
    m.add_class::<SorensenDice>()?;
    m.add_class::<OverlapCoefficient>()?;
    m.add_class::<WeightedLevenshtein>()?;
    m.add_class::<SIFT4>()?;
    m.add_class::<SIFT4Options>()?;
    m.add_class::<ShingleBased>()?;
    m.add_class::<LongestCommonSubsequence>()?;
    m.add_class::<MetricLCS>()?;
    m.add_class::<OptimalStringAlignment>()?;
    m.add_class::<StringDistance>()?;
    m.add_class::<NormalizedStringDistance>()?;
    m.add_class::<MetricStringDistance>()?;
    m.add_class::<StringSimilarity>()?;
    m.add_class::<NormalizedStringSimilarity>()?;
    Ok(())
}
