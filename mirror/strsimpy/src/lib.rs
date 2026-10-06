// SPDX-License-Identifier: MIT
// Provenance: rustsmith Stage-1 mirror of luozhouyang/python-string-similarity (MIT).
//! Stage-1 Rust mirror of `luozhouyang/python-string-similarity`.
//!
//! PyO3 binding over the independent `strsimpy-rust-core` crate: same module
//! boundary (`strsimpy._strsimpy`), same public names, same vectors. No
//! redesign. All arithmetic lives in `strsimpy_core`; this file only
//! translates between Python objects and core values, owns the Python return
//! types (several metrics return `0.0`/`1.0` floats on early exits, ints
//! otherwise), and runs the callback-bound paths (custom cost functions and
//! custom SIFT4 hooks are Python callables, so those loops stay here and
//! delegate every pure step to the core).

use pyo3::exceptions::{PyNotImplementedError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use strsimpy_core::{
    charfreq_tokenize, chars_of, cosine_similarity, damerau_dist, dot_product, jaccard_similarity,
    jaro_matches, jaro_similarity, lcs_length, lev_dist, longer_transposition_cost, metric_lcs,
    ngram_dist, ngram_tokenize, normalized_levenshtein, overlap_similarity, osa_dist, prof_norm,
    profile_vec, py_round, qgram_distance, qgram_profile_distance, reward_length1, reward_length2,
    sift4_default_distance, sift4_matching_eval, sift4_token_match, sorensen_dice_similarity,
    transposition_eval_sub, weighted_levenshtein_default, wordsplit_tokenize,
};

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
        let (ca, cb) = (chars_of(&a), chars_of(&b));
        Ok(int_obj(py, ca.len() as i64 + cb.len() as i64 - 2 * lcs_length(&ca, &cb)))
    }
    #[staticmethod]
    #[pyo3(signature = (s0=None, s1=None))]
    fn length(s0: Option<String>, s1: Option<String>) -> PyResult<i64> {
        let a = s0.ok_or_else(|| none_err("s0"))?;
        let b = s1.ok_or_else(|| none_err("s1"))?;
        Ok(lcs_length(&chars_of(&a), &chars_of(&b)))
    }
}

// ---------------------------------------------------------------------------
// Jaro-Winkler.
// ---------------------------------------------------------------------------

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
        Ok(jaro_similarity(&chars_of(&a), &chars_of(&b), self.threshold))
    }
    #[pyo3(signature = (s0=None, s1=None))]
    fn distance(&self, s0: Option<String>, s1: Option<String>) -> PyResult<f64> {
        Ok(1.0 - self.similarity(s0, s1)?)
    }
    #[staticmethod]
    fn matches(s0: String, s1: String) -> PyResult<Vec<i64>> {
        let (a, b) = (chars_of(&s0), chars_of(&s1));
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
        Ok(normalized_levenshtein(&a, &b))
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
        Ok(metric_lcs(&a, &b))
    }
}

// ---------------------------------------------------------------------------
// NGram.
// ---------------------------------------------------------------------------

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
// Shingle profiles (dict conversions stay here; profile math is core).
// ---------------------------------------------------------------------------

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
        Ok(cosine_similarity(&a, &b, self.k))
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
        Ok(jaccard_similarity(&a, &b, self.k))
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
        Ok(int_obj(py, qgram_distance(&a, &b, self.k)))
    }
    #[staticmethod]
    fn distance_profile(p0: &Bound<'_, PyDict>, p1: &Bound<'_, PyDict>) -> PyResult<i64> {
        let (a, b) = (profile_from_pydict(p0)?, profile_from_pydict(p1)?);
        Ok(qgram_profile_distance(&a, &b))
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
        Ok(sorensen_dice_similarity(&a, &b, self.k))
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
        Ok(overlap_similarity(&a, &b, self.k))
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
        // Unit-cost fast path: the exact default arithmetic, shared with Rust
        // consumers (no Python callbacks involved).
        if self.sub.is_none() && self.ins.is_none() && self.del.is_none() {
            return Ok(weighted_levenshtein_default(&a, &b));
        }
        let (ca, cb) = (chars_of(&a), chars_of(&b));
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
// SIFT4 (custom hooks are Python callables, so the generic loop stays here;
// every pure step delegates to the core).
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

    /// True when every hook is a named (non-custom) hook, i.e. the whole
    /// evaluation is pure and could run in the core.
    fn is_default(&self) -> bool {
        matches!(self.tok, TokHook::Default)
            && matches!(self.mat, MatchHook::Eq)
            && matches!(self.meval, MatchEvalHook::One)
            && matches!(self.llen, LocalLenHook::Identity)
            && matches!(self.tcost, TransCostHook::One)
            && matches!(self.teval, TransEvalHook::Sub)
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
        TokHook::Wordsplit => Ok(wordsplit_tokenize(s).into_iter().map(Tok::S).collect()),
        TokHook::CharFreq => Ok(charfreq_tokenize(s).into_iter().map(Tok::I).collect()),
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

/// Recursion target for the sift4 matcher/evaluator (default options): the
/// shared int-exact core, no Python callbacks on this path.
fn sift4_inner_default(s1: &str, s2: &str) -> i64 {
    sift4_default_distance(s1, s2, 5)
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
            // `tok_len` first: int tokens raise here, exactly like the original.
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
        LocalLenHook::Reward1 => Ok(reward_length1(l)),
        LocalLenHook::Reward2 => Ok(reward_length2(l)),
        LocalLenHook::Custom(f) => {
            let r = f.bind(py).call1((l,))?;
            r.extract::<f64>()
        }
    }
}

fn apply_tcost(py: Python<'_>, hook: &TransCostHook, c1: i64, c2: i64) -> PyResult<f64> {
    match hook {
        TransCostHook::One => Ok(1.0),
        TransCostHook::Longer => Ok(longer_transposition_cost(c1, c2)),
        TransCostHook::Custom(f) => {
            let r = f.bind(py).call1((c1, c2))?;
            r.extract::<f64>()
        }
    }
}

fn apply_teval(py: Python<'_>, hook: &TransEvalHook, lcss: f64, trans: f64) -> PyResult<f64> {
    match hook {
        TransEvalHook::Sub => Ok(transposition_eval_sub(lcss, trans)),
        TransEvalHook::Custom(f) => {
            let r = f.bind(py).call1((lcss, trans))?;
            r.extract::<f64>()
        }
    }
}

/// Generic SIFT4 core (custom hooks may yield floats).
// index-faithful to the original DP
#[allow(clippy::needless_range_loop)]
fn sift4_core(py: Python<'_>, o: &SiftOpts, s1: &str, s2: &str, maxoffset: i64) -> PyResult<i64> {
    if o.is_default() {
        return Ok(sift4_default_distance(s1, s2, maxoffset));
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
        ngram_tokenize(&s, n)
    }
    #[staticmethod]
    fn wordsplittokenizer(s: String) -> Vec<String> {
        wordsplit_tokenize(&s)
    }
    #[staticmethod]
    fn characterfrequencytokenizer(s: String) -> Vec<i64> {
        charfreq_tokenize(&s)
    }
    #[staticmethod]
    fn sift4tokenmatcher(t1: String, t2: String) -> PyResult<bool> {
        Ok(sift4_token_match(&t1, &t2))
    }
    #[staticmethod]
    fn sift4matchingevaluator(t1: String, t2: String) -> f64 {
        sift4_matching_eval(&t1, &t2)
    }
    #[staticmethod]
    fn rewardlengthevaluator(l: f64) -> f64 {
        reward_length1(l)
    }
    #[staticmethod]
    fn rewardlengthevaluator2(l: f64) -> f64 {
        reward_length2(l)
    }
    #[staticmethod]
    fn longertranspositionsaremorecostly(c1: i64, c2: i64) -> f64 {
        longer_transposition_cost(c1, c2)
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
