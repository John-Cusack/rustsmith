// SPDX-License-Identifier: Apache-2.0
//! `rrule`/`rruleset` binding: constructor validation via Python builtins,
//! wall-candidate generation via `dateutil-core`, and all filtering,
//! comparison, caching, and merging through real Python operations so
//! semantics (including errors) match the original exactly.

use crate::util::*;
use dateutil_core::{civil, rrule as core};
use pyo3::basic::CompareOp;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};
use parking_lot::Mutex;

/// `original_rule` values (Python objects, for `__str__`/`replace`).
#[derive(Default)]
struct OriginalData {
    bysetpos: Option<Vec<PyObject>>,
    bymonth: Option<Vec<PyObject>>,
    bymonthday: Option<Vec<PyObject>>,
    byyearday: Option<Vec<PyObject>>,
    byweekday: Option<Vec<PyObject>>,
    byweekno: Option<Vec<PyObject>>,
    byhour: Option<Vec<PyObject>>,
    byminute: Option<Vec<PyObject>>,
    bysecond: Option<Vec<PyObject>>,
    byeaster: Option<Vec<PyObject>>,
}

/// Immutable rule data (constructor outputs).
struct RruleData {
    params: core::Params,
    dtstart: PyObject,
    dtstart_tz: Option<PyObject>,
    dtstart_wall_y: i64,
    dtstart_wall_m: i64,
    dtstart_wall_d: i64,
    dtstart_wall_hh: i64,
    dtstart_wall_mm: i64,
    dtstart_wall_ss: i64,
    until: Option<PyObject>,
    count: Option<i64>,
    original: OriginalData,
}

/// Raw engine pull as a Python iterator: materializes wall candidates and
/// applies until/dtstart/count filtering through Python comparisons,
/// mirroring one `_iter` generator.
#[pyclass(weakref, module = "dateutil._dateutil")]
struct EnginePull {
    engine: Option<core::Engine>,
    dtstart: PyObject,
    dtstart_tz: Option<PyObject>,
    until: Option<PyObject>,
    count_left: Option<i64>,
    pending: Vec<core::Candidate>,
}

#[pymethods]
impl EnginePull {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        loop {
            // Drain buffered candidates from the previous step first.
            while let Some((abs_ord, h, m, s)) = self.pending.pop() {
                let (y, mo, d) = civil::from_ordinal(abs_ord);
                let res = make_datetime(
                    py, y, mo as i64, d as i64, h as i64, m as i64, s as i64, 0,
                    self.dtstart_tz.as_ref(), 0,
                )?;
                let res_b = res.bind(py);
                if let Some(until) = &self.until {
                    if res_b.gt(until.bind(py))? {
                        self.engine = None;
                        self.pending.clear();
                        return Ok(None);
                    }
                }
                if res_b.lt(self.dtstart.bind(py))? {
                    continue;
                }
                if let Some(left) = self.count_left.as_mut() {
                    *left -= 1;
                    if *left < 0 {
                        self.engine = None;
                        self.pending.clear();
                        return Ok(None);
                    }
                }
                return Ok(Some(res));
            }
            let engine = match self.engine.as_mut() {
                None => return Ok(None),
                Some(e) => e,
            };
            if engine.done {
                self.engine = None;
                return Ok(None);
            }
            let mut batch = match engine.next_step() {
                Ok(b) => b,
                Err(core::EngineError::EmptyRule) => {
                    return value_error(
                        "Invalid combination of interval and byhour resulting in empty rule."
                            .to_string(),
                    );
                }
                Err(core::EngineError::UnpackNone) => {
                    return type_error("cannot unpack non-iterable NoneType object".to_string());
                }
                Err(core::EngineError::ZeroDiv) => {
                    return zero_division_error("integer division or modulo by zero".to_string());
                }
                Err(core::EngineError::Index) => {
                    return index_error("list assignment index out of range".to_string());
                }
            };
            // Buffer the step (reversed for pop-order) and loop back to drain.
            batch.reverse();
            self.pending = batch;
        }
    }
}

/// One merge source: a Python iterator with an eagerly-read head
/// (mirrors `_genitem`: the head is read at wrap time).
struct Src {
    it: Option<Py<PyIterator>>,
    cur: Option<PyObject>,
}

impl Src {
    fn prime(py: Python<'_>, iterable: &Bound<'_, PyAny>) -> PyResult<Src> {
        let it = PyIterator::from_object(iterable)?;
        let mut s = Src { it: Some(it.unbind()), cur: None };
        s.advance(py)?;
        Ok(s)
    }

    fn advance(&mut self, py: Python<'_>) -> PyResult<()> {
        let it = match self.it.take() {
            None => return Ok(()),
            Some(it) => it,
        };
        let mut bound = it.into_bound(py);
        match bound.next() {
            None => {
                self.cur = None;
                Ok(())
            }
            Some(Ok(dt)) => {
                self.cur = Some(dt.unbind());
                self.it = Some(bound.unbind());
                Ok(())
            }
            Some(Err(e)) => Err(e),
        }
    }
}

/// Merge state: inclusive/exclusive sources plus the dedup cursor.
struct MergeState {
    inclusive: Vec<Src>,
    exclusive: Vec<Src>,
    lastdt: Option<PyObject>,
}

/// Least head (source index, value) among live sources, or `None`.
fn least_head(py: Python<'_>, srcs: &[Src]) -> PyResult<Option<(usize, PyObject)>> {
    let mut best: Option<(usize, PyObject)> = None;
    for (n, s) in srcs.iter().enumerate() {
        if let Some(cur) = &s.cur {
            let take = match &best {
                None => true,
                Some((_, b)) => cur.bind(py).lt(b.bind(py))?,
            };
            if take {
                best = Some((n, s.cur.as_ref().unwrap().clone_ref(py)));
            }
        }
    }
    Ok(best)
}

/// Advance the merge one step, mirroring `rruleset._iter` exactly:
/// least inclusive head (deduped by `lastdt`), drained past exclusions.
fn pull_merge(py: Python<'_>, st: &mut MergeState) -> PyResult<Option<PyObject>> {
    loop {
        let (rn, rdt) = match least_head(py, &st.inclusive)? {
            None => return Ok(None),
            Some(v) => v,
        };
        let dominated = match &st.lastdt {
            None => false,
            Some(last) => py_eq(py, last.bind(py), rdt.bind(py))?,
        };
        if !dominated {
            // Drain exclusions below the head (`while exlist and ex[0] < ritem`).
            loop {
                match least_head(py, &st.exclusive)? {
                    None => break,
                    Some((en, edt)) => {
                        if !edt.bind(py).lt(rdt.bind(py))? {
                            break;
                        }
                        st.exclusive[en].advance(py)?;
                    }
                }
            }
            // `if not exlist or ritem != exlist[0]`.
            let mut excluded = false;
            if let Some((_, edt)) = least_head(py, &st.exclusive)? {
                if py_eq(py, rdt.bind(py), edt.bind(py))? {
                    excluded = true;
                }
            }
            if !excluded {
                st.lastdt = Some(rdt.clone_ref(py));
                st.inclusive[rn].advance(py)?;
                return Ok(Some(rdt));
            }
            st.lastdt = Some(rdt.clone_ref(py));
        }
        st.inclusive[rn].advance(py)?;
    }
}

/// Shared iteration/cache state behind one rule object.
struct Shared {
    merge: Option<MergeState>,
    fill_total: usize,
    len: Option<usize>,
    cache_items: Option<Vec<PyObject>>,
    cache_complete: bool,
    cache_lock: Option<PyObject>,
}

impl Shared {
    fn fresh(cache: bool, lock: Option<PyObject>) -> Shared {
        Shared {
            merge: None,
            fill_total: 0,
            len: None,
            cache_items: if cache { Some(Vec::new()) } else { None },
            cache_complete: false,
            cache_lock: lock,
        }
    }
}

#[pyclass(weakref, name = "rrule", subclass, dict, module = "dateutil._dateutil")]
pub struct Rrule {
    data: RruleData,
    shared: Mutex<Shared>,
}

#[pyclass(weakref, name = "rruleset", subclass, dict, module = "dateutil._dateutil")]
pub struct Rruleset {
    lists: Mutex<RrulesetLists>,
    shared: Mutex<Shared>,
}

#[derive(Default)]
struct RrulesetLists {
    rrules: Vec<PyObject>,
    rdates: Vec<PyObject>,
    exrules: Vec<PyObject>,
    exdates: Vec<PyObject>,
}

/// Extract constructor ints, mirroring `isinstance(x, integer_types)`:
/// bools count as ints; anything else is an error at this position.
fn as_int_list(py: Python<'_>, tup: &Bound<'_, PyAny>) -> PyResult<Vec<i64>> {
    let mut out = Vec::new();
    for o in tup.iter()? {
        let o = o?;
        match o.extract::<i64>() {
            Ok(i) => out.push(i),
            Err(_) => {
                // Non-ints survive in the original (stored, never matching);
                // the sentinel never matches any mask.
                out.push(i64::MIN);
            }
        }
    }
    let _ = py;
    Ok(out)
}

/// `tuple(sorted(set(items)))` through Python builtins (exact ordering,
/// dedup, and errors).
fn sorted_tuple(py: Python<'_>, items: Vec<PyObject>) -> PyResult<Bound<'_, PyAny>> {
    let builtins = py.import("builtins")?;
    let set = builtins.getattr("set")?;
    let sorted = builtins.getattr("sorted")?;
    let tuple = builtins.getattr("tuple")?;
    let list = PyList::new(py, items.iter().map(|o| o.bind(py)))?;
    let s = set.call1((list,))?;
    let so = sorted.call1((s,))?;
    Ok(tuple.call1((so,))?)
}

/// Shared constructor body (also used by `replace` and `rrulestr`).
/// Arguments mirror the `rrule.__init__` signature exactly.
#[allow(clippy::too_many_arguments)]
fn rrule_new(
    py: Python<'_>,
    freq: Bound<'_, PyAny>,
    dtstart: Option<Bound<'_, PyAny>>,
    interval: Option<Bound<'_, PyAny>>,
    wkst: Option<Bound<'_, PyAny>>,
    count: Option<Bound<'_, PyAny>>,
    until: Option<Bound<'_, PyAny>>,
    bysetpos: Option<Bound<'_, PyAny>>,
    bymonth: Option<Bound<'_, PyAny>>,
    bymonthday: Option<Bound<'_, PyAny>>,
    byyearday: Option<Bound<'_, PyAny>>,
    byeaster: Option<Bound<'_, PyAny>>,
    byweekno: Option<Bound<'_, PyAny>>,
    byweekday: Option<Bound<'_, PyAny>>,
    byhour: Option<Bound<'_, PyAny>>,
    byminute: Option<Bound<'_, PyAny>>,
    bysecond: Option<Bound<'_, PyAny>>,
    cache: Option<Bound<'_, PyAny>>,
) -> PyResult<Rrule> {
    let freq_i: i64 = freq.extract()?;
    let freq_u = freq_i as u8;
    // dtstart normalization.
    let dt_mod = py.import("datetime")?;
    let dt_cls = dt_mod.getattr("datetime")?;
    let dtstart_o: Bound<'_, PyAny> = match dtstart {
        None => {
            // `datetime.now(tz)` when until is aware, else `datetime.now()`.
            let now = match &until {
                Some(u) => {
                    let tz = u.getattr("tzinfo")?;
                    if tz.is_none() {
                        dt_cls.call_method0("now")?
                    } else {
                        dt_cls.call_method1("now", (tz,))?
                    }
                }
                None => dt_cls.call_method0("now")?,
            };
            let kw = PyDict::new(py);
            kw.set_item("microsecond", 0)?;
            now.call_method("replace", (), Some(&kw))?
        }
        Some(d) => {
            if is_datetime(py, &d)? {
                let kw = PyDict::new(py);
                kw.set_item("microsecond", 0)?;
                d.call_method("replace", (), Some(&kw))?
            } else {
                // `datetime.fromordinal(dtstart.toordinal())` (AttributeError
                // propagates for non-dates, like the original).
                let o: i64 = d.call_method0("toordinal")?.extract()?;
                dt_cls.call_method1("fromordinal", (o,))?
            }
        }
    };
    let dp = parts_of(&dtstart_o)?;
    let dtstart_tz = clone_opt(py, &dp.tz);
    let interval_i: i64 = match interval {
        None => 1,
        Some(v) => v.extract()?,
    };
    let count_i: Option<i64> = match count {
        None => None,
        Some(v) => {
            if v.is_none() {
                None
            } else {
                Some(v.extract()?)
            }
        }
    };
    // until normalization.
    let until_o: Option<Bound<'_, PyAny>> = match until {
        None => None,
        Some(u) => {
            if u.is_none() {
                None
            } else if is_datetime(py, &u)? {
                Some(u)
            } else {
                let o: i64 = u.call_method0("toordinal")?.extract()?;
                Some(dt_cls.call_method1("fromordinal", (o,))?)
            }
        }
    };
    // Aware/naive mismatch (RFC 5545 UNTIL-in-UTC rule).
    if let Some(u) = &until_o {
        let dt_aware = dtstart_tz.is_some();
        let u_aware = !u.getattr("tzinfo")?.is_none();
        if dt_aware != u_aware {
            return value_error(
                "RRULE UNTIL values must be specified in UTC when DTSTART is timezone-aware"
                    .to_string(),
            );
        }
    }
    if count_i.is_some() && until_o.is_some() {
        warn_with(
            py,
            &py.get_type::<pyo3::exceptions::PyDeprecationWarning>().into_any(),
            "Using both 'count' and 'until' is inconsistent with RFC 5545 and has been deprecated in dateutil. Future versions will raise an error.",
        )?;
    }
    // wkst resolution.
    let wkst_i: i64 = match wkst {
        None => {
            let cal = py.import("calendar")?;
            cal.call_method0("firstweekday")?.extract()?
        }
        Some(w) => {
            if w.is_none() {
                let cal = py.import("calendar")?;
                cal.call_method0("firstweekday")?.extract()?
            } else if let Ok(i) = w.extract::<i64>() {
                i
            } else {
                w.getattr("weekday")?.extract()?
            }
        }
    };
    let mut original = OriginalData::default();
    // bysetpos validation.
    let bysetpos_v: Option<Vec<PyObject>> = match bysetpos {
        None => None,
        Some(v) => {
            if v.is_none() {
                None
            } else {
                let items: Vec<Bound<'_, PyAny>> = if let Ok(i) = v.extract::<i64>() {
                    vec![i.into_pyobject(py)?.into_any()]
                } else {
                    v.try_iter()?.map(|o| o).collect::<PyResult<Vec<_>>>()?
                };
                for pos in &items {
                    let zero = 0i64.into_pyobject(py)?;
                    let lo = (-366i64).into_pyobject(py)?;
                    let hi = 366i64.into_pyobject(py)?;
                    let is_zero: bool = pos.rich_compare(&zero, CompareOp::Eq)?.extract()?;
                    let in_range: bool = pos.rich_compare(&lo, CompareOp::Ge)?.extract()?
                        && pos.rich_compare(&hi, CompareOp::Le)?.extract()?;
                    if is_zero || !in_range {
                        return value_error(
                            "bysetpos must be between 1 and 366, or between -366 and -1".to_string(),
                        );
                    }
                }
                let tup = sorted_tuple(py, items.iter().map(|o| o.clone().unbind()).collect())?;
                let objs: Vec<PyObject> = tup.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
                original.bysetpos = Some(clone_vec(py, &objs));
                Some(objs)
            }
        }
    };
    // Default byxxx rules when all of byweekno/byyearday/bymonthday/
    // byweekday/byeaster are absent.
    let all_none = byweekno.as_ref().map(|v| v.is_none()).unwrap_or(true)
        && byyearday.as_ref().map(|v| v.is_none()).unwrap_or(true)
        && bymonthday.as_ref().map(|v| v.is_none()).unwrap_or(true)
        && byweekday.as_ref().map(|v| v.is_none()).unwrap_or(true)
        && byeaster.as_ref().map(|v| v.is_none()).unwrap_or(true);
    let mut bymonth_defaulted = false;
    let mut bymonthday_defaulted = false;
    let mut byweekday_defaulted = false;
    let (bymonth, bymonthday, byweekday) = if all_none {
        if freq_u == core::YEARLY {
            bymonth_defaulted = bymonth.as_ref().map(|v| v.is_none()).unwrap_or(true);
            let bm = match bymonth {
                None => Some(dp.m.into_pyobject(py)?.into_any()),
                Some(v) if v.is_none() => Some(dp.m.into_pyobject(py)?.into_any()),
                Some(v) => Some(v),
            };
            let bmd: Bound<'_, PyAny> = dp.d.into_pyobject(py)?.into_any();
            bymonthday_defaulted = true;
            (bm, Some(bmd), byweekday)
        } else if freq_u == core::MONTHLY {
            let bmd: Bound<'_, PyAny> = dp.d.into_pyobject(py)?.into_any();
            bymonthday_defaulted = true;
            (bymonth, Some(bmd), byweekday)
        } else if freq_u == core::WEEKLY {
            let wd = dtstart_o.call_method0("weekday")?;
            byweekday_defaulted = true;
            (bymonth, bymonthday, Some(wd))
        } else {
            (bymonth, bymonthday, byweekday)
        }
    } else {
        (bymonth, bymonthday, byweekday)
    };
    // bymonth.
    let bymonth_v: Option<Vec<PyObject>> = match bymonth {
        None => None,
        Some(v) if v.is_none() => None,
        Some(v) => {
            let items: Vec<PyObject> = if let Ok(i) = v.extract::<i64>() {
                vec![i.into_pyobject(py)?.into_any().unbind()]
            } else {
                v.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<Vec<_>>>()?
            };
            let tup = sorted_tuple(py, items)?;
            let objs: Vec<PyObject> = tup.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
            if !bymonth_defaulted {
                original.bymonth = Some(clone_vec(py, &objs));
            }
            Some(objs)
        }
    };
    // byyearday.
    let byyearday_v: Option<Vec<PyObject>> = match byyearday {
        None => None,
        Some(v) if v.is_none() => None,
        Some(v) => {
            let items = single_or_seq(py, &v)?;
            let tup = sorted_tuple(py, items)?;
            let objs: Vec<PyObject> = tup.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
            original.byyearday = Some(clone_vec(py, &objs));
            Some(objs)
        }
    };
    // byeaster: sorted (no dedup), like the original.
    let byeaster_v: Option<Vec<PyObject>> = match byeaster {
        None => None,
        Some(v) if v.is_none() => None,
        Some(v) => {
            let items = single_or_seq(py, &v)?;
            let builtins = py.import("builtins")?;
            let sorted = builtins.getattr("sorted")?;
            let list = PyList::new(py, items.iter().map(|o| o.bind(py)))?;
            let so = sorted.call1((list,))?;
            let objs: Vec<PyObject> = so.iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
            original.byeaster = Some(clone_vec(py, &objs));
            Some(objs)
        }
    };
    // bymonthday: unique split into positive/negative, each sorted.
    let (bymonthday_v, bynmonthday_v): (Vec<PyObject>, Vec<PyObject>) = match bymonthday {
        None => (Vec::new(), Vec::new()),
        Some(v) if v.is_none() => (Vec::new(), Vec::new()),
        Some(v) => {
            let items = single_or_seq(py, &v)?;
            let builtins = py.import("builtins")?;
            let set = builtins.getattr("set")?;
            let list = PyList::new(py, items.iter().map(|o| o.bind(py)))?;
            let uniq = set.call1((list,))?;
            let mut pos: Vec<PyObject> = Vec::new();
            let mut neg: Vec<PyObject> = Vec::new();
            for o in uniq.iter()? {
                let o = o?;
                if o.gt(0i64.into_pyobject(py)?)? {
                    pos.push(o.unbind());
                } else if o.lt(0i64.into_pyobject(py)?)? {
                    neg.push(o.unbind());
                }
            }
            let sp = sorted_tuple(py, pos)?;
            let sn = sorted_tuple(py, neg)?;
            let pos: Vec<PyObject> = sp.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
            let neg: Vec<PyObject> = sn.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
            let mut orig = clone_vec(py, &pos);
            orig.extend(neg.iter().map(|o| o.clone_ref(py)));
            if !bymonthday_defaulted {
                original.bymonthday = Some(orig);
            }
            (pos, neg)
        }
    };
    // byweekno.
    let byweekno_v: Option<Vec<PyObject>> = match byweekno {
        None => None,
        Some(v) if v.is_none() => None,
        Some(v) => {
            let items = single_or_seq(py, &v)?;
            let tup = sorted_tuple(py, items)?;
            let objs: Vec<PyObject> = tup.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
            original.byweekno = Some(clone_vec(py, &objs));
            Some(objs)
        }
    };
    // byweekday / bynweekday.
    let (byweekday_v, bynweekday_v): (Option<Vec<PyObject>>, Option<Vec<(PyObject, PyObject)>>) =
        match byweekday {
            None => (None, None),
            Some(v) if v.is_none() => (None, None),
            Some(v) => {
                let seq: Vec<Bound<'_, PyAny>> = if v.extract::<i64>().is_ok() || v.hasattr("n")? {
                    vec![v.clone()]
                } else {
                    v.try_iter()?.collect::<PyResult<Vec<_>>>()?
                };
                let mut plain: Vec<PyObject> = Vec::new();
                let mut nth: Vec<(PyObject, PyObject)> = Vec::new();
                for wday in &seq {
                    if let Ok(i) = wday.extract::<i64>() {
                        plain.push(i.into_pyobject(py)?.into_any().unbind());
                    } else {
                        let n = wday.getattr("n")?;
                        let n_truthy: bool = n.is_truthy()?;
                        if !n_truthy || freq_u > core::MONTHLY {
                            plain.push(wday.getattr("weekday")?.unbind());
                        } else {
                            nth.push((wday.getattr("weekday")?.unbind(), n.unbind()));
                        }
                    }
                }
                let plain_tup = sorted_tuple(py, plain)?;
                let mut plain_v: Vec<PyObject> = plain_tup.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
                // nth pairs sorted as tuples through Python.
                let builtins = py.import("builtins")?;
                let sorted = builtins.getattr("sorted")?;
                let nth_list = PyList::new(py, nth.iter().map(|(a, b)| {
                    let t = PyTuple::new(py, [a.bind(py), b.bind(py)]).unwrap();
                    t.into_any()
                }))?;
                let nth_sorted = sorted.call1((nth_list,))?;
                let mut nth_v: Vec<(PyObject, PyObject)> = Vec::new();
                for o in nth_sorted.iter()? {
                    let o = o?;
                    let a: Bound<'_, PyAny> = o.get_item(0)?;
                    let b: Bound<'_, PyAny> = o.get_item(1)?;
                    nth_v.push((a.unbind(), b.unbind()));
                }
                let (plain_opt, nth_opt) = if plain_v.is_empty() && nth_v.is_empty() {
                    (None, None)
                } else {
                    (
                        if plain_v.is_empty() { None } else { Some(clone_vec(py, &plain_v)) },
                        if nth_v.is_empty() { None } else { Some(nth_v.iter().map(|(a, b)| (a.clone_ref(py), b.clone_ref(py))).collect::<Vec<(PyObject, PyObject)>>()) },
                    )
                };
                // Reconstruct original weekday objects via the shim class.
                if !byweekday_defaulted {
                    let shim = py.import("dateutil.rrule")?;
                    let wd_cls = shim.getattr("weekday")?;
                    let mut orig_wdays: Vec<PyObject> = Vec::new();
                    if let Some(p) = &plain_opt {
                        for x in p {
                            orig_wdays.push(wd_cls.call1((x,))?.unbind());
                        }
                    }
                    if let Some(n) = &nth_opt {
                        for (w, nn) in n.iter() {
                            orig_wdays.push(wd_cls.call1((w, nn))?.unbind());
                        }
                    }
                    original.byweekday = Some(orig_wdays);
                }
                let _ = plain_v;
                (plain_opt, nth_opt)
            }
        };
    // byhour / byminute / bysecond.
    let dt_h = dp.hh;
    let dt_m = dp.mm;
    let dt_s = dp.ss;
    let byhour_v: Option<Vec<PyObject>> = process_time_part(py, byhour, freq_u, core::HOURLY, dt_h, interval_i, &mut original.byhour)?;
    let byminute_v: Option<Vec<PyObject>> = process_time_part(py, byminute, freq_u, core::MINUTELY, dt_m, interval_i, &mut original.byminute)?;
    let bysecond_v: Option<Vec<PyObject>> = process_time_part(py, bysecond, freq_u, core::SECONDLY, dt_s, interval_i, &mut original.bysecond)?;
    // timeset for freq < HOURLY.
    let timeset: Option<Vec<(u8, u8, u8)>> = if freq_u < core::HOURLY {
        let dt_mod = py.import("datetime")?;
        let time_cls = dt_mod.getattr("time")?;
        let mut times: Vec<PyObject> = Vec::new();
        for h in byhour_v.as_ref().unwrap() {
            for m in byminute_v.as_ref().unwrap() {
                for s in bysecond_v.as_ref().unwrap() {
                    let kw = PyDict::new(py);
                    if let Some(tz) = &dtstart_tz {
                        kw.set_item("tzinfo", tz)?;
                    }
                    let t = time_cls.call((h.bind(py), m.bind(py), s.bind(py)), Some(&kw))?;
                    times.push(t.unbind());
            }
         }
        }
        let list = PyList::new(py, times.iter().map(|o| o.bind(py)))?;
        list.sort()?;
        let mut out = Vec::new();
        for t in list.iter() {
            let h: i64 = t.getattr("hour")?.extract()?;
            let m: i64 = t.getattr("minute")?.extract()?;
            let s: i64 = t.getattr("second")?.extract()?;
            out.push((h as u8, m as u8, s as u8));
        }
        Some(out)
    } else {
        None
    };

/// `single_or_seq`: int -> single-element vec, else iterate (mirrors the
/// `isinstance(x, integer_types)` branches; bools count as ints).
fn single_or_seq(py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<Vec<PyObject>> {
    if v.extract::<i64>().is_ok() {
        return Ok(vec![v.clone().unbind()]);
    }
    // Strings are iterable (chars), like the original; floats raise
    // TypeError from `try_iter`, like the original.
    v.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<Vec<_>>>()
}

/// byhour/byminute/bysecond processing: default singleton set when the
/// option is absent and freq is below the level; `__construct_byset` at
/// the level; plain set otherwise. Stores the sorted tuple in `original`.
fn process_time_part(
    py: Python<'_>,
    opt: Option<Bound<'_, PyAny>>,
    freq: u8,
    level: u8,
    dtstart_val: i64,
    interval: i64,
    original_slot: &mut Option<Vec<PyObject>>,
) -> PyResult<Option<Vec<PyObject>>> {
    match opt {
        None => {
            if freq < level {
                Ok(Some(vec![dtstart_val.into_pyobject(py)?.into_any().unbind()]))
            } else {
                Ok(None)
            }
        }
        Some(v) if v.is_none() => {
            if freq < level {
                Ok(Some(vec![dtstart_val.into_pyobject(py)?.into_any().unbind()]))
            } else {
                Ok(None)
            }
        }
        Some(v) => {
            let items = single_or_seq(py, &v)?;
            let kept = if freq == level {
                construct_byset_py(py, dtstart_val, items, interval, level_base(level))?
            } else {
                let tup = sorted_tuple(py, items)?;
                tup.iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<Vec<_>>>()?
            };
            *original_slot = Some(clone_vec(py, &kept));
            Ok(Some(kept))
        }
    }
}

fn level_base(level: u8) -> i64 {
    match level {
        core::HOURLY => 24,
        _ => 60,
    }
}

/// `__construct_byset` through Python builtins (exact `divmod` and error
/// semantics for arbitrary inputs).
fn construct_byset_py(
    py: Python<'_>,
    start: i64,
    items: Vec<PyObject>,
    interval: i64,
    base: i64,
) -> PyResult<Vec<PyObject>> {
    let math = py.import("math")?;
    let gcd = math.getattr("gcd")?;
    let builtins = py.import("builtins")?;
    let divmod = builtins.getattr("divmod")?;
    let mut cset: Vec<PyObject> = Vec::new();
    for num in items {
        let g: i64 = gcd.call1((interval, base))?.extract()?;
        let keep = if g == 1 {
            true
        } else {
            let diff = num.bind(py).sub(start.into_pyobject(py)?)?;
            let dm = divmod.call1((diff, g))?;
            let rem: i64 = dm.get_item(1)?.extract()?;
            rem == 0
        };
        if keep {
            let mut dup = false;
            for x in &cset {
                if py_eq(py, x.bind(py), num.bind(py))? {
                    dup = true;
                    break;
                }
            }
            if !dup {
                cset.push(num);
            }
        }
    }
    if cset.is_empty() {
        return value_error("Invalid rrule byxxx generates an empty set.".to_string());
    }
    // Sorted tuple order for the stored value.
    let tup = sorted_tuple(py, cset)?;
    Ok(tup.iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<Vec<_>>>()?)
}

    // Extract i64s for the core (validated ints above; anything else can
    // only come from unvalidated sets, where the original stores the value
    // but never matches it: the sentinel mirrors that).
    let xi = |objs: &Option<Vec<PyObject>>| -> Vec<i64> {
        objs.as_ref()
            .map(|v| {
                v.iter()
                    .map(|o| o.extract::<i64>(py).unwrap_or(i64::MIN))
                    .collect()
            })
            .unwrap_or_default()
    };
    let xn = |objs: &Option<Vec<(PyObject, PyObject)>>| -> Option<Vec<(i64, i64)>> {
        objs.as_ref().map(|v| {
            v.iter()
                .map(|(a, b)| {
                    (
                        a.extract::<i64>(py).unwrap_or(i64::MIN),
                        b.extract::<i64>(py).unwrap_or(i64::MIN),
                    )
                })
                .collect()
        })
    };
    let params = core::Params {
        freq: freq_u,
        interval: interval_i,
        wkst: wkst_i as u8,
        bymonth: if bymonth_v.is_some() { Some(xi(&bymonth_v)) } else { None },
        byweekno: if byweekno_v.is_some() { Some(xi(&byweekno_v)) } else { None },
        byyearday: if byyearday_v.is_some() { Some(xi(&byyearday_v)) } else { None },
        byeaster: if byeaster_v.is_some() { Some(xi(&byeaster_v)) } else { None },
        bymonthday: xi(&Some(bymonthday_v)),
        bynmonthday: xi(&Some(bynmonthday_v)),
        byweekday: if byweekday_v.is_some() { Some(xi(&byweekday_v)) } else { None },
        bynweekday: xn(&bynweekday_v),
        byhour: if byhour_v.is_some() { Some(xi(&byhour_v)) } else { None },
        byminute: if byminute_v.is_some() { Some(xi(&byminute_v)) } else { None },
        bysecond: if bysecond_v.is_some() { Some(xi(&bysecond_v)) } else { None },
        bysetpos: if bysetpos_v.is_some() { Some(xi(&bysetpos_v)) } else { None },
        timeset,
    };
    let cache_on = match &cache {
        None => false,
        Some(v) => v.is_truthy()?,
    };
    let lock = if cache_on {
        let thread = py.import("_thread")?;
        Some(thread.call_method0("allocate_lock")?.unbind())
    } else {
        None
    };
    Ok(Rrule {
        data: RruleData {
            params,
            dtstart: dtstart_o.unbind(),
            dtstart_tz,
            dtstart_wall_y: dp.y,
            dtstart_wall_m: dp.m,
            dtstart_wall_d: dp.d,
            dtstart_wall_hh: dp.hh,
            dtstart_wall_mm: dp.mm,
            dtstart_wall_ss: dp.ss,
            until: until_o.map(|u| u.unbind()),
            count: count_i,
            original,
        },
        shared: Mutex::new(Shared::fresh(cache_on, lock)),
    })
}

#[pymethods]
impl Rruleset {
    #[new]
    #[pyo3(signature = (cache=None,))]
    fn new(py: Python<'_>, cache: Option<Bound<'_, PyAny>>) -> PyResult<Self> {
        let cache_on = match &cache {
            None => false,
            Some(v) => v.is_truthy()?,
        };
        Ok(rruleset_new_inner(py, cache_on)?)
    }
    fn rrule(slf: PyRef<'_, Self>, py: Python<'_>, rule: Bound<'_, PyAny>) -> PyResult<()> {
        slf.lists.lock().rrules.push(rule.unbind());
        let owned: Py<Rruleset> = slf.into_py(py).extract(py)?;
        invalidate(py, &Owner::Set(owned))
    }

    fn rdate(slf: PyRef<'_, Self>, py: Python<'_>, rdate: Bound<'_, PyAny>) -> PyResult<()> {
        slf.lists.lock().rdates.push(rdate.unbind());
        let owned: Py<Rruleset> = slf.into_py(py).extract(py)?;
        invalidate(py, &Owner::Set(owned))
    }

    fn exrule(slf: PyRef<'_, Self>, py: Python<'_>, rule: Bound<'_, PyAny>) -> PyResult<()> {
        slf.lists.lock().exrules.push(rule.unbind());
        let owned: Py<Rruleset> = slf.into_py(py).extract(py)?;
        invalidate(py, &Owner::Set(owned))
    }

    fn exdate(slf: PyRef<'_, Self>, py: Python<'_>, exdate: Bound<'_, PyAny>) -> PyResult<()> {
        slf.lists.lock().exdates.push(exdate.unbind());
        let owned: Py<Rruleset> = slf.into_py(py).extract(py)?;
        invalidate(py, &Owner::Set(owned))
    }
    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        let owner = set_owner(slf, py)?;
        q_iter(py, owner)
    }

    fn __getitem__(slf: PyRef<'_, Self>, py: Python<'_>, item: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let owner = set_owner(slf, py)?;
        q_getitem(py, owner, item)
    }

    fn __contains__(slf: PyRef<'_, Self>, py: Python<'_>, item: Bound<'_, PyAny>) -> PyResult<bool> {
        let owner = set_owner(slf, py)?;
        q_contains(py, owner, item)
    }

    fn count(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<usize> {
        let owner = set_owner(slf, py)?;
        q_count(py, owner)
    }

    #[pyo3(signature = (dt, inc=None))]
    fn before(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let owner = set_owner(slf, py)?;
        q_before(py, owner, dt, inc)
    }

    #[pyo3(signature = (dt, inc=None))]
    fn after(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let owner = set_owner(slf, py)?;
        q_after(py, owner, dt, inc)
    }

    #[pyo3(signature = (dt, count=None, inc=None))]
    fn xafter(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>, count: Option<Bound<'_, PyAny>>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let owner = set_owner(slf, py)?;
        q_xafter(py, owner, dt, count, inc)
    }

    #[pyo3(signature = (after, before, inc=None, count=None))]
    fn between(slf: PyRef<'_, Self>, py: Python<'_>, after: Bound<'_, PyAny>, before: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>, count: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let owner = set_owner(slf, py)?;
        q_between(py, owner, after, before, inc, count)
    }

    #[getter]
    fn _cache(&self, py: Python<'_>) -> PyResult<PyObject> {
        let sh = self.shared.lock();
        match &sh.cache_items {
            None => Ok(py.None()),
            Some(items) => Ok(PyList::new(py, items.iter().map(|o| o.bind(py)))?.into_any().unbind()),
        }
    }

    #[getter]
    fn _cache_complete(&self) -> bool {
        self.shared.lock().cache_complete
    }

    #[getter]
    fn _len(&self, py: Python<'_>) -> PyResult<PyObject> {
        match self.shared.lock().len {
            None => Ok(py.None()),
            Some(n) => Ok((n as i64).into_pyobject(py)?.into_any().unbind()),
        }
    }

    #[getter]
    fn _cache_lock(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.shared.lock().cache_lock {
            None => Err(pyo3::exceptions::PyAttributeError::new_err(
                "'rruleset' object has no attribute '_cache_lock'",
            )),
            Some(lock) => Ok(lock.clone_ref(py)),
        }
    }

}

/// `rrulestr` (RFC string parsing, mirroring `_rrulestr`).
#[pyfunction]
#[pyo3(signature = (s, **kwargs))]
pub fn rrulestr(
    py: Python<'_>,
    s: Bound<'_, PyAny>,
    kwargs: Option<Bound<'_, PyDict>>,
) -> PyResult<PyObject> {
    let kw = kwargs;
    let get = |name: &str| -> PyResult<PyObject> {
        match &kw {
            None => Ok(py.None()),
            Some(d) => Ok(d.get_item(name)?.map(|v| v.unbind()).unwrap_or_else(|| py.None())),
        }
    };
    let opt = |name: &str| -> PyResult<Option<PyObject>> {
        let v = get(name)?;
        Ok(if v.bind(py).is_none() { None } else { Some(v) })
    };
    let flag = |name: &str| -> PyResult<bool> {
        let v = get(name)?;
        v.bind(py).is_truthy()
    };
    let opts = RrOptions {
        dtstart: opt("dtstart")?,
        cache: flag("cache")?,
        unfold: flag("unfold")?,
        forceset: flag("forceset")?,
        compatible: flag("compatible")?,
        ignoretz: opt("ignoretz")?,
        tzids: opt("tzids")?,
        tzinfos: opt("tzinfos")?,
    };
    parse_rfc(py, &s, &opts)
}

struct RrOptions {
    dtstart: Option<PyObject>,
    cache: bool,
    unfold: bool,
    forceset: bool,
    compatible: bool,
    ignoretz: Option<PyObject>,
    tzids: Option<PyObject>,
    tzinfos: Option<PyObject>,
}

/// Scan `TZID=<name>:` occurrences (case-sensitive, pre-upper), like the
/// `re.findall('TZID=(?P<name>[^:]+):', s)` probe.
fn scan_tzids(s: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 5 <= bytes.len() {
        if &bytes[i..i + 5] == b"TZID=" {
            let start = i + 5;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b':' {
                end += 1;
            }
            if end < bytes.len() {
                let name = &s[start..end];
                out.insert(name.to_uppercase(), name.to_string());
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Iteration owner (rule or set) for shared-state pulls.
enum Owner {
    Rule(Py<Rrule>),
    Set(Py<Rruleset>),
}

/// Transient pull handle: cached-vector cursor, shared-state pull, or an
/// owned live merge.
enum QueryPull {
    Vec { items: Vec<PyObject>, idx: usize },
    Shared { owner: Owner, idx: usize },
    Live { merge: MergeState, owner: Owner, total: usize },
}

/// Run `f` under the owner's shared-state lock (guards never escape).
fn with_shared<T>(
    py: Python<'_>,
    owner: &Owner,
    f: impl FnOnce(&mut Shared) -> PyResult<T>,
) -> PyResult<T> {
    match owner {
        Owner::Rule(r) => {
            let b = r.bind(py);
            let inner = b.borrow();
            let mut sh = inner.shared.lock();
            f(&mut sh)
        }
        Owner::Set(s) => {
            let b = s.bind(py);
            let inner = b.borrow();
            let mut sh = inner.shared.lock();
            f(&mut sh)
        }
    }
}

/// Build a fresh single-rule merge state (one raw-engine source).
fn fresh_rule_merge(py: Python<'_>, data: &RruleData) -> PyResult<MergeState> {
    let (y, mo, d) = (data.dtstart_wall_y, data.dtstart_wall_m, data.dtstart_wall_d);
    let eng = EnginePull {
        engine: Some(core::Engine::new(
            data.params.clone(),
            y, mo, d,
            data.dtstart_wall_hh, data.dtstart_wall_mm, data.dtstart_wall_ss,
        )),
        dtstart: data.dtstart.clone_ref(py),
        dtstart_tz: clone_opt(py, &data.dtstart_tz),
        until: clone_opt(py, &data.until),
        count_left: data.count,
        pending: Vec::new(),
    };
    let obj = Py::new(py, eng)?.into_any();
    let src = Src::prime(py, obj.bind(py))?;
    Ok(MergeState { inclusive: vec![src], exclusive: vec![], lastdt: None })
}

/// Build a fresh set merge state (sorted live lists + sub-rule iterators).
fn fresh_set_merge(py: Python<'_>, lists: &mut RrulesetLists) -> PyResult<MergeState> {
    // `self._rdate.sort()` / `self._exdate.sort()` mutate in place.
    py_sort(py, &mut lists.rdates)?;
    py_sort(py, &mut lists.exdates)?;
    let builtins = py.import("builtins")?;
    let iter_fn = builtins.getattr("iter")?;
    let mut inclusive = Vec::new();
    let rdate_list = PyList::new(py, lists.rdates.iter().map(|o| o.bind(py)))?;
    inclusive.push(Src::prime(py, rdate_list.as_any())?);
    for r in &lists.rrules {
        let it = iter_fn.call1((r.bind(py),))?;
        inclusive.push(Src::prime(py, &it)?);
    }
    let mut exclusive = Vec::new();
    let exdate_list = PyList::new(py, lists.exdates.iter().map(|o| o.bind(py)))?;
    exclusive.push(Src::prime(py, exdate_list.as_any())?);
    for r in &lists.exrules {
        let it = iter_fn.call1((r.bind(py),))?;
        exclusive.push(Src::prime(py, &it)?);
    }
    Ok(MergeState { inclusive, exclusive, lastdt: None })
}

/// Pull the `idx`-th item through shared state, filling the cache in
/// batches of up to 10 under the cache lock (mirrors `_iter_cached`).
/// `total` counts produced items; `len` is set at exhaustion.
fn pull_shared(py: Python<'_>, owner: &Owner, idx: usize) -> PyResult<Option<PyObject>> {
    // Ensure a merge exists.
    let needs_merge = with_shared(py, owner, |sh| Ok(sh.merge.is_none()))?;
    if needs_merge {
        let merge = match owner {
            Owner::Rule(r) => {
                let b = r.bind(py);
                let inner = b.borrow();
                fresh_rule_merge(py, &inner.data)?
            }
            Owner::Set(s) => {
                let b = s.bind(py);
                let inner = b.borrow();
                let mut lists = inner.lists.lock();
                fresh_set_merge(py, &mut lists)?
            }
        };
        with_shared(py, owner, |sh| {
            sh.merge = Some(merge);
            sh.fill_total = 0;
            Ok(())
        })?;
    }
    // The serve/fill below re-locks per step (the Python cache lock is
    // only held across a fill batch, like the original).
    loop {
        enum Step {
            Yield(PyObject),
            Fill,
            Done,
        }
        let step = with_shared(py, owner, |sh| {
            if let Some(items) = &sh.cache_items {
                if idx < items.len() {
                    Ok(Step::Yield(items[idx].clone_ref(py)))
                } else if sh.cache_complete {
                    Ok(Step::Done)
                } else {
                    Ok(Step::Fill)
                }
            } else if sh.merge.is_none() {
                Ok(Step::Done)
            } else {
                // Live single pull (no cache, no shared counting here:
                // live sessions track their own total).
                let st = sh.merge.as_mut().unwrap();
                match pull_merge(py, st)? {
                    None => {
                        sh.merge = None;
                        Ok(Step::Done)
                    }
                    Some(dt) => Ok(Step::Yield(dt)),
                }
            }
        })?;
        match step {
            Step::Yield(dt) => return Ok(Some(dt)),
            Step::Done => return Ok(None),
            Step::Fill => {
                fill_batch(py, owner)?;
            }
        }
    }
}

/// Fill the cache with up to 10 pulls under the cache lock.
fn fill_batch(py: Python<'_>, owner: &Owner) -> PyResult<()> {
    let lock = with_shared(py, owner, |sh| Ok(clone_opt(py, &sh.cache_lock)))?;
    if let Some(lock) = lock {
        lock.bind(py).call_method0("acquire")?;
    }
    let result = (|| -> PyResult<()> {
        for _ in 0..10 {
            let next = with_shared(py, owner, |sh| {
                if sh.cache_complete {
                    return Ok(None);
                }
                let st = sh.merge.as_mut().unwrap();
                match pull_merge(py, st)? {
                    None => {
                        sh.cache_complete = true;
                        sh.len = Some(sh.fill_total);
                        Ok(None)
                    }
                    Some(dt) => {
                        sh.fill_total += 1;
                        if let Some(items) = sh.cache_items.as_mut() {
                            items.push(dt.clone_ref(py));
                        }
                        Ok(Some(dt))
                    }
                }
            })?;
            if next.is_none() {
                break;
            }
        }
        Ok(())
    })();
    if let Some(lock) = with_shared(py, owner, |sh| Ok(clone_opt(py, &sh.cache_lock)))? {
        lock.bind(py).call_method0("release")?;
    }
    result
}

impl QueryPull {
    /// Build the pull for an owner: cached-vector cursor when complete,
    /// shared pull in cache mode, otherwise an owned live merge.
    fn for_owner(py: Python<'_>, owner: &Owner) -> PyResult<QueryPull> {
        enum Kind {
            Vec,
            Shared,
            Live,
        }
        let kind = with_shared(py, owner, |sh| {
            Ok(if sh.cache_complete {
                Kind::Vec
            } else if sh.cache_items.is_some() {
                Kind::Shared
            } else {
                Kind::Live
            })
        })?;
        match kind {
            Kind::Vec => {
                let items = with_shared(py, owner, |sh| {
                    Ok(clone_vec(py, sh.cache_items.as_ref().unwrap()))
                })?;
                Ok(QueryPull::Vec { items, idx: 0 })
            }
            Kind::Shared => Ok(QueryPull::Shared { owner: Self::owner_clone(py, owner), idx: 0 }),
            Kind::Live => {
                let merge = match owner {
                    Owner::Rule(r) => {
                        let b = r.bind(py);
                        let inner = b.borrow();
                        fresh_rule_merge(py, &inner.data)?
                    }
                    Owner::Set(s) => {
                        let b = s.bind(py);
                        let inner = b.borrow();
                        let mut lists = inner.lists.lock();
                        fresh_set_merge(py, &mut lists)?
                    }
                };
                Ok(QueryPull::Live { merge, owner: Self::owner_clone(py, owner), total: 0 })
            }
        }
    }

    fn next(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match self {
            QueryPull::Vec { items, idx } => {
                if *idx < items.len() {
                    let dt = items[*idx].clone_ref(py);
                    *idx += 1;
                    Ok(Some(dt))
                } else {
                    Ok(None)
                }
            }
            QueryPull::Shared { owner, idx } => {
                let dt = pull_shared(py, owner, *idx)?;
                if dt.is_some() {
                    *idx += 1;
                }
                Ok(dt)
            }
            QueryPull::Live { merge, owner, total } => {
                match pull_merge(py, merge)? {
                    None => {
                        with_shared(py, owner, |sh| {
                            sh.len = Some(*total);
                            Ok(())
                        })?;
                        Ok(None)
                    }
                    Some(dt) => {
                        *total += 1;
                        Ok(Some(dt))
                    }
                }
            }
        }
    }

fn owner_clone(py: Python<'_>, owner: &Owner) -> Owner {
    match owner {
        Owner::Rule(r) => Owner::Rule(r.clone_ref(py)),
        Owner::Set(s) => Owner::Set(s.clone_ref(py)),
    }
}
}

/// Python iterator over a rule/set pull.
#[pyclass(weakref, module = "dateutil._dateutil")]
struct RuleIter {
    pull: QueryPull,
}

#[pymethods]
impl RuleIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        self.pull.next(py)
    }
}

/// Lazy `xafter` iterator (mirrors the `xafter` generator).
#[pyclass(weakref, module = "dateutil._dateutil")]
struct XAfter {
    pull: QueryPull,
    dt: PyObject,
    count: Option<i64>,
    yielded: i64,
    inc: bool,
}

#[pymethods]
impl XAfter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        loop {
            let d = match self.pull.next(py)? {
                None => return Ok(None),
                Some(d) => d,
            };
            let db = d.bind(py);
            let hit = if self.inc {
                db.ge(self.dt.bind(py))?
            } else {
                db.gt(self.dt.bind(py))?
            };
            if hit {
                if let Some(count) = self.count {
                    self.yielded += 1;
                    if self.yielded > count {
                        return Ok(None);
                    }
                }
                return Ok(Some(d));
            }
        }
    }
}


/// Build a fresh pull for queries/iteration.
fn fresh_pull(py: Python<'_>, owner: &Owner) -> PyResult<QueryPull> {
    QueryPull::for_owner(py, owner)
}

/// `count()`: exhaust (filling cache when present) and return the length.
fn pull_count(py: Python<'_>, owner: &Owner) -> PyResult<usize> {
    let known = with_shared(py, owner, |sh| Ok(sh.len))?;
    if let Some(n) = known {
        return Ok(n);
    }
    let mut pull = fresh_pull(py, owner)?;
    while pull.next(py)?.is_some() {}
    let n = with_shared(py, owner, |sh| Ok(sh.len.unwrap_or(0)))?;
    Ok(n)
}

/// Invalidate caches after mutation (mirrors `_invalidate_cache`).
fn invalidate(py: Python<'_>, owner: &Owner) -> PyResult<()> {
    // Release a locked cache lock first, like the original.
    let locked = with_shared(py, owner, |sh| {
        Ok(match &sh.cache_lock {
            None => false,
            Some(lock) => lock.bind(py).call_method0("locked")?.extract::<bool>()?,
        })
    })?;
    if locked {
        with_shared(py, owner, |sh| {
            if let Some(lock) = &sh.cache_lock {
                lock.bind(py).call_method0("release")?;
            }
            Ok(())
        })?;
    }
    with_shared(py, owner, |sh| {
        sh.merge = None;
        sh.fill_total = 0;
        sh.len = None;
        if sh.cache_items.is_some() {
            sh.cache_items = Some(Vec::new());
        }
        sh.cache_complete = false;
        Ok(())
    })
}

/// Owner + pull builders for `Rrule` methods.
fn rule_owner(slf: PyRef<'_, Rrule>, py: Python<'_>) -> PyResult<Owner> {
    let owned: Py<Rrule> = slf.into_py(py).extract(py)?;
    Ok(Owner::Rule(owned))
}

fn set_owner(slf: PyRef<'_, Rruleset>, py: Python<'_>) -> PyResult<Owner> {
    let owned: Py<Rruleset> = slf.into_py(py).extract(py)?;
    Ok(Owner::Set(owned))
}

/// Pull all remaining items from a pull into a list.
fn pull_all(py: Python<'_>, pull: &mut QueryPull) -> PyResult<Vec<PyObject>> {
    let mut out = Vec::new();
    while let Some(dt) = pull.next(py)? {
        out.push(dt);
    }
    Ok(out)
}

fn q_iter(py: Python<'_>, owner: Owner) -> PyResult<PyObject> {
        let pull = QueryPull::for_owner(py, &owner)?;
        Ok(Py::new(py, RuleIter { pull })?.into_any())
    }

fn q_getitem(py: Python<'_>, owner: Owner, item: Bound<'_, PyAny>) -> PyResult<PyObject> {
        // Cache-complete fast path (`self._cache[item]`).
        let complete = with_shared(py, &owner, |sh| Ok(sh.cache_complete))?;
        if complete {
            let items = with_shared(py, &owner, |sh| {
                Ok(clone_vec(py, sh.cache_items.as_ref().unwrap()))
            })?;
            let list = PyList::new(py, items.iter().map(|o| o.bind(py)))?;
            return Ok(list.as_any().get_item(item)?.unbind());
        }
        let slice_type = py.get_type::<pyo3::types::PySlice>();
        if item.is_instance(&slice_type)? {
            let start: Option<i64> = item.getattr("start")?.extract()?;
            let stop: Option<i64> = item.getattr("stop")?.extract()?;
            let step: Option<i64> = item.getattr("step")?.extract()?;
            if step.map(|s| s < 0).unwrap_or(false) {
                let all = pull_all(py, &mut QueryPull::for_owner(py, &owner)?)?;
                let list = PyList::new(py, all.iter().map(|o| o.bind(py)))?;
                return Ok(list.as_any().get_item(item)?.unbind());
            }
            // `itertools.islice` with the original's defaults.
            let itertools = py.import("itertools")?;
            let islice = itertools.getattr("islice")?;
            let pull = QueryPull::for_owner(py, &owner)?;
            let it = Py::new(py, RuleIter { pull })?.into_any();
            let seq = islice.call1((
                it.bind(py),
                start.unwrap_or(0),
                stop.unwrap_or(i64::MAX),
                step.unwrap_or(1),
            ))?;
            let out: Vec<PyObject> = seq.try_iter()?.map(|o| o.map(|b| b.unbind())).collect::<PyResult<_>>()?;
            let list = PyList::new(py, out.iter().map(|o| o.bind(py)))?;
            return Ok(list.into_any().unbind());
        }
        let idx: i64 = item.extract()?;
        if idx >= 0 {
            let mut pull = QueryPull::for_owner(py, &owner)?;
            let mut res: Option<PyObject> = None;
            for _ in 0..idx + 1 {
                res = pull.next(py)?;
                if res.is_none() {
                    break;
                }
            }
            match res {
                Some(dt) => Ok(dt),
                None => index_error("".to_string()),
            }
        } else {
            let all = pull_all(py, &mut QueryPull::for_owner(py, &owner)?)?;
            let list = PyList::new(py, all.iter().map(|o| o.bind(py)))?;
            Ok(list.as_any().get_item(idx)?.unbind())
        }
    }

fn q_contains(py: Python<'_>, owner: Owner, item: Bound<'_, PyAny>) -> PyResult<bool> {
        let mut pull = QueryPull::for_owner(py, &owner)?;
        while let Some(d) = pull.next(py)? {
            if py_eq(py, d.bind(py), &item)? {
                return Ok(true);
            } else if d.bind(py).gt(&item)? {
                return Ok(false);
            }
        }
        Ok(false)
    }

fn q_count(py: Python<'_>, owner: Owner) -> PyResult<usize> {
        pull_count(py, &owner)
    }

fn q_before(py: Python<'_>, owner: Owner, dt: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let inc_b: bool = inc.map(|v| v.is_truthy()).transpose()?.unwrap_or(false);
        let mut pull = QueryPull::for_owner(py, &owner)?;
        let mut last: Option<PyObject> = None;
        while let Some(d) = pull.next(py)? {
            let db = d.bind(py);
            let stop = if inc_b { db.gt(&dt)? } else { db.ge(&dt)? };
            if stop {
                break;
            }
            last = Some(d);
        }
        Ok(last.unwrap_or_else(|| py.None()))
    }

fn q_after(py: Python<'_>, owner: Owner, dt: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let inc_b: bool = inc.map(|v| v.is_truthy()).transpose()?.unwrap_or(false);
        let mut pull = QueryPull::for_owner(py, &owner)?;
        while let Some(d) = pull.next(py)? {
            let db = d.bind(py);
            let hit = if inc_b { db.ge(&dt)? } else { db.gt(&dt)? };
            if hit {
                return Ok(d);
            }
        }
        Ok(py.None())
    }

fn q_xafter(py: Python<'_>, owner: Owner, dt: Bound<'_, PyAny>, count: Option<Bound<'_, PyAny>>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let inc_b: bool = inc.map(|v| v.is_truthy()).transpose()?.unwrap_or(false);
        let count_i: Option<i64> = match count {
            None => None,
            Some(v) => {
                if v.is_none() {
                    None
                } else {
                    Some(v.extract()?)
                }
            }
        };
        let pull = QueryPull::for_owner(py, &owner)?;
        Ok(Py::new(py, XAfter { pull, dt: dt.unbind(), count: count_i, yielded: 0, inc: inc_b })?.into_any())
    }

fn q_between(py: Python<'_>, owner: Owner, after: Bound<'_, PyAny>, before: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>, count: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let _ = count;
        let inc_b: bool = inc.map(|v| v.is_truthy()).transpose()?.unwrap_or(false);
        let mut pull = QueryPull::for_owner(py, &owner)?;
        let mut out: Vec<PyObject> = Vec::new();
        let mut started = false;
        while let Some(d) = pull.next(py)? {
            let db = d.bind(py);
            if inc_b {
                if db.gt(&before)? {
                    break;
                } else if !started {
                    if db.ge(&after)? {
                        started = true;
                        out.push(d);
                    }
                } else {
                    out.push(d);
                }
            } else {
                if db.ge(&before)? {
                    break;
                } else if !started {
                    if db.gt(&after)? {
                        started = true;
                        out.push(d);
                    }
                } else {
                    out.push(d);
                }
            }
        }
        let list = PyList::new(py, out.iter().map(|o| o.bind(py)))?;
        Ok(list.into_any().unbind())
    }

#[pymethods]
impl Rrule {
    #[new]
    #[pyo3(signature = (freq, dtstart=None, interval=None, wkst=None, count=None, until=None, bysetpos=None, bymonth=None, bymonthday=None, byyearday=None, byeaster=None, byweekno=None, byweekday=None, byhour=None, byminute=None, bysecond=None, cache=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        freq: Bound<'_, PyAny>,
        dtstart: Option<Bound<'_, PyAny>>,
        interval: Option<Bound<'_, PyAny>>,
        wkst: Option<Bound<'_, PyAny>>,
        count: Option<Bound<'_, PyAny>>,
        until: Option<Bound<'_, PyAny>>,
        bysetpos: Option<Bound<'_, PyAny>>,
        bymonth: Option<Bound<'_, PyAny>>,
        bymonthday: Option<Bound<'_, PyAny>>,
        byyearday: Option<Bound<'_, PyAny>>,
        byeaster: Option<Bound<'_, PyAny>>,
        byweekno: Option<Bound<'_, PyAny>>,
        byweekday: Option<Bound<'_, PyAny>>,
        byhour: Option<Bound<'_, PyAny>>,
        byminute: Option<Bound<'_, PyAny>>,
        bysecond: Option<Bound<'_, PyAny>>,
        cache: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        rrule_new(
            py, freq, dtstart, interval, wkst, count, until, bysetpos, bymonth,
            bymonthday, byyearday, byeaster, byweekno, byweekday, byhour,
            byminute, bysecond, cache,
        )
    }
    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        let owner = rule_owner(slf, py)?;
        q_iter(py, owner)
    }

    fn __getitem__(slf: PyRef<'_, Self>, py: Python<'_>, item: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let owner = rule_owner(slf, py)?;
        q_getitem(py, owner, item)
    }

    fn __contains__(slf: PyRef<'_, Self>, py: Python<'_>, item: Bound<'_, PyAny>) -> PyResult<bool> {
        let owner = rule_owner(slf, py)?;
        q_contains(py, owner, item)
    }

    fn count(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<usize> {
        let owner = rule_owner(slf, py)?;
        q_count(py, owner)
    }

    #[pyo3(signature = (dt, inc=None))]
    fn before(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let owner = rule_owner(slf, py)?;
        q_before(py, owner, dt, inc)
    }

    #[pyo3(signature = (dt, inc=None))]
    fn after(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let owner = rule_owner(slf, py)?;
        q_after(py, owner, dt, inc)
    }

    #[pyo3(signature = (dt, count=None, inc=None))]
    fn xafter(slf: PyRef<'_, Self>, py: Python<'_>, dt: Bound<'_, PyAny>, count: Option<Bound<'_, PyAny>>, inc: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let owner = rule_owner(slf, py)?;
        q_xafter(py, owner, dt, count, inc)
    }

    #[pyo3(signature = (after, before, inc=None, count=None))]
    fn between(slf: PyRef<'_, Self>, py: Python<'_>, after: Bound<'_, PyAny>, before: Bound<'_, PyAny>, inc: Option<Bound<'_, PyAny>>, count: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let owner = rule_owner(slf, py)?;
        q_between(py, owner, after, before, inc, count)
    }

    #[getter]
    fn _cache(&self, py: Python<'_>) -> PyResult<PyObject> {
        let sh = self.shared.lock();
        match &sh.cache_items {
            None => Ok(py.None()),
            Some(items) => Ok(PyList::new(py, items.iter().map(|o| o.bind(py)))?.into_any().unbind()),
        }
    }

    #[getter]
    fn _cache_complete(&self) -> bool {
        self.shared.lock().cache_complete
    }

    #[getter]
    fn _len(&self, py: Python<'_>) -> PyResult<PyObject> {
        match self.shared.lock().len {
            None => Ok(py.None()),
            Some(n) => Ok((n as i64).into_pyobject(py)?.into_any().unbind()),
        }
    }

    #[getter]
    fn _cache_lock(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.shared.lock().cache_lock {
            None => Err(pyo3::exceptions::PyAttributeError::new_err(
                "'rrule' object has no attribute '_cache_lock'",
            )),
            Some(lock) => Ok(lock.clone_ref(py)),
        }
    }
    fn __str__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<String> {
        let data = &slf.data;
        let mut output: Vec<String> = Vec::new();
        let dtstart = data.dtstart.bind(py);
        let ds: String = dtstart
            .call_method1("strftime", ("DTSTART:%Y%m%dT%H%M%S",))?
            .extract()?;
        output.push(ds);
        let freq_usize = data.params.freq as usize;
        let mut parts = vec![format!("FREQ={}", FREQNAMES[freq_usize])];
        if data.params.interval != 1 {
            parts.push(format!("INTERVAL={}", data.params.interval));
        }
        if data.params.wkst != 0 {
            // `repr(weekday(self._wkst))[0:2]` via the shim class.
            let shim = py.import("dateutil.rrule")?;
            let wd_cls = shim.getattr("weekday")?;
            let w = wd_cls.call1((data.params.wkst as i64,))?;
            let r: String = w.repr()?.extract()?;
            let short: String = r.chars().take(2).collect();
            parts.push(format!("WKST={}", short));
        }
        if let Some(count) = data.count {
            parts.push(format!("COUNT={}", count));
        }
        if let Some(until) = &data.until {
            let us: String = until
                .bind(py)
                .call_method1("strftime", ("UNTIL=%Y%m%dT%H%M%S",))?
                .extract()?;
            parts.push(us);
        }
        // byweekday strings from the stored original weekday objects.
        let mut byweekday_strs: Option<Vec<String>> = None;
        if let Some(wdays) = &data.original.byweekday {
            let mut strs = Vec::new();
            for w in wdays {
                let wb = w.bind(py);
                let n: Bound<'_, PyAny> = wb.getattr("n")?;
                if n.is_truthy()? {
                    let ni: i64 = n.extract()?;
                    let r: String = wb.repr()?.extract()?;
                    let short: String = r.chars().take(2).collect();
                    strs.push(format!("{:+}{}", ni, short));
                } else {
                    let r: String = wb.repr()?.extract()?;
                    strs.push(r);
                }
            }
            byweekday_strs = Some(strs);
        }
        let get = |key: &str| -> Option<&Vec<PyObject>> {
            match key {
                "bysetpos" => data.original.bysetpos.as_ref(),
                "bymonth" => data.original.bymonth.as_ref(),
                "bymonthday" => data.original.bymonthday.as_ref(),
                "byyearday" => data.original.byyearday.as_ref(),
                "byweekno" => data.original.byweekno.as_ref(),
                "byhour" => data.original.byhour.as_ref(),
                "byminute" => data.original.byminute.as_ref(),
                "bysecond" => data.original.bysecond.as_ref(),
                "byeaster" => data.original.byeaster.as_ref(),
                _ => None,
            }
        };
        for (name, key) in [
            ("BYSETPOS", "bysetpos"),
            ("BYMONTH", "bymonth"),
            ("BYMONTHDAY", "bymonthday"),
            ("BYYEARDAY", "byyearday"),
            ("BYWEEKNO", "byweekno"),
            ("BYDAY", "byweekday"),
            ("BYHOUR", "byhour"),
            ("BYMINUTE", "byminute"),
            ("BYSECOND", "bysecond"),
            ("BYEASTER", "byeaster"),
        ] {
            if key == "byweekday" {
                if let Some(strs) = &byweekday_strs {
                    if !strs.is_empty() {
                        parts.push(format!("{}={}", name, strs.join(",")));
                    }
                }
                continue;
            }
            if let Some(vals) = get(key) {
                if !vals.is_empty() {
                    parts.push(format!("{}={}", name, fmt_vals(py, vals)?));
                }
            }
        }
        output.push(format!("RRULE:{}", parts.join(";")));
        Ok(output.join("\n"))
    }

    #[pyo3(signature = (**kwargs))]
    fn replace(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        kwargs: Option<Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let owned: Py<Rrule> = slf.into_py(py).extract(py)?;
        let owner = Owner::Rule(owned.clone_ref(py));
        let cache_on = with_shared(py, &owner, |sh| Ok(sh.cache_items.is_some()))?;
        let bound = owned.bind(py).to_owned();
        let inner = bound.borrow();
        let data = &inner.data;
        let kw = PyDict::new(py);
        kw.set_item("interval", data.params.interval)?;
        kw.set_item(
            "count",
            match data.count {
                None => py.None(),
                Some(c) => c.into_pyobject(py)?.into_any().unbind(),
            },
        )?;
        kw.set_item("dtstart", &data.dtstart)?;
        kw.set_item("freq", data.params.freq as i64)?;
        kw.set_item("until", opt_clone(py, &data.until))?;
        kw.set_item("wkst", data.params.wkst as i64)?;
        kw.set_item("cache", cache_on)?;
        for (key, vals) in [
            ("bysetpos", &data.original.bysetpos),
            ("bymonth", &data.original.bymonth),
            ("bymonthday", &data.original.bymonthday),
            ("byyearday", &data.original.byyearday),
            ("byweekno", &data.original.byweekno),
            ("byweekday", &data.original.byweekday),
            ("byhour", &data.original.byhour),
            ("byminute", &data.original.byminute),
            ("bysecond", &data.original.bysecond),
            ("byeaster", &data.original.byeaster),
        ] {
            if let Some(v) = vals {
                let tup = PyTuple::new(py, v.iter().map(|o| o.bind(py)))?;
                kw.set_item(key, tup)?;
            }
        }
        if let Some(user) = kwargs {
            for (k, v) in user.iter() {
                kw.set_item(k, v)?;
            }
        }
        let shim = py.import("dateutil.rrule")?;
        let cls = shim.getattr("rrule")?;
        Ok(cls.call((), Some(&kw))?.unbind())
    }
}

const FREQNAMES: [&str; 7] = ["YEARLY", "MONTHLY", "WEEKLY", "DAILY", "HOURLY", "MINUTELY", "SECONDLY"];

/// Format one original-rule value list (`','.join(str(v) ...)`).
fn fmt_vals(py: Python<'_>, vals: &[PyObject]) -> PyResult<String> {
    let mut parts = Vec::new();
    for v in vals {
        let s: String = v.bind(py).str()?.extract()?;
        parts.push(s);
    }
    Ok(parts.join(","))
}

/// Top-level `_parse_rfc` mirror.
fn parse_rfc(py: Python<'_>, s: &Bound<'_, PyAny>, opts: &RrOptions) -> PyResult<PyObject> {
    let forceset = opts.forceset || opts.compatible;
    let unfold = opts.unfold || opts.compatible;
    let tzid_names = {
        let orig: String = s.str()?.extract()?;
        scan_tzids(&orig)
    };
    let upper: String = s.call_method0("upper")?.extract()?;
    if upper.trim().is_empty() {
        return value_error("empty string".to_string());
    }
    let lines: Vec<String> = if unfold {
        match dateutil_core::ical::unfold_lines(&upper) {
            Ok(v) => v,
            Err(e) => return value_error(e.0),
        }
    } else {
        upper.split_whitespace().map(|l| l.to_string()).collect()
    };
    if !forceset && lines.len() == 1 && (upper.find(':').is_none() || upper.starts_with("RRULE:")) {
        let r = parse_rfc_rrule(py, &lines[0], clone_opt(py, &opts.dtstart), opts.cache, opts)?;
        return Ok(r.into_any());
    }
    let mut rrulevals: Vec<String> = Vec::new();
    let mut rdatevals: Vec<String> = Vec::new();
    let mut exrulevals: Vec<String> = Vec::new();
    let mut exdatevals: Vec<PyObject> = Vec::new();
    let mut dtstart = clone_opt(py, &opts.dtstart);
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = match line.split_once(':') {
            None => ("RRULE".to_string(), line.clone()),
            Some((n, v)) => (n.to_string(), v.to_string()),
        };
        let mut parms: Vec<&str> = name.split(';').collect();
        let pname = parms.remove(0).to_string();
        if pname == "RRULE" {
            for parm in parms {
                return value_error(format!("unsupported RRULE parm: {}", parm));
            }
            rrulevals.push(value);
        } else if pname == "RDATE" {
            for parm in parms {
                if parm != "VALUE=DATE-TIME" {
                    return value_error(format!("unsupported RDATE parm: {}", parm));
                }
            }
            rdatevals.push(value);
        } else if pname == "EXRULE" {
            for parm in parms {
                return value_error(format!("unsupported EXRULE parm: {}", parm));
            }
            exrulevals.push(value);
        } else if pname == "EXDATE" {
            exdatevals.extend(parse_date_value(py, &value, &parms, &tzid_names, opts)?);
        } else if pname == "DTSTART" {
            let dtvals = parse_date_value(py, &value, &parms, &tzid_names, opts)?;
            if dtvals.len() != 1 {
                return value_error(format!("Multiple DTSTART values specified:{}", value));
            }
            dtstart = Some(dtvals.into_iter().next().unwrap());
        } else {
            return value_error(format!("unsupported property: {}", pname));
        }
    }
    if forceset || rrulevals.len() > 1 || !rdatevals.is_empty() || !exrulevals.is_empty() || !exdatevals.is_empty() {
        let rset = Py::new(py, rruleset_new_inner(py, opts.cache)?)?.into_any();
        let rset_b = rset.bind(py);
        for value in &rrulevals {
            let r = parse_rfc_rrule(py, value, clone_opt(py, &dtstart), false, opts)?;
            rset_b.call_method1("rrule", (r.bind(py),))?;
        }
        for value in &rdatevals {
            for datestr in value.split(',') {
                let d = parser_parse(py, datestr, opts)?;
                rset_b.call_method1("rdate", (d.bind(py),))?;
            }
        }
        for value in &exrulevals {
            let r = parse_rfc_rrule(py, value, clone_opt(py, &dtstart), false, opts)?;
            rset_b.call_method1("exrule", (r.bind(py),))?;
        }
        for d in &exdatevals {
            rset_b.call_method1("exdate", (d.bind(py),))?;
        }
        if opts.compatible {
            if let Some(ds) = dtstart {
                rset_b.call_method1("rdate", (ds.bind(py),))?;
            }
        }
        return Ok(rset.into_any());
    }
    let r = parse_rfc_rrule(py, &rrulevals[0], dtstart, opts.cache, opts)?;
    Ok(r.into_any())
}

/// `parser.parse(value, ignoretz=..., tzinfos=...)` through the kept
/// Python parser module.
fn parser_parse(py: Python<'_>, value: &str, opts: &RrOptions) -> PyResult<PyObject> {
    let parser = py.import("dateutil.parser")?;
    let parse = parser.getattr("parse")?;
    let kw = PyDict::new(py);
    kw.set_item("ignoretz", opt_clone(py, &opts.ignoretz))?;
    kw.set_item("tzinfos", opt_clone(py, &opts.tzinfos))?;
    Ok(parse.call((value,), Some(&kw))?.unbind())
}

/// `_parse_date_value` mirror.
fn parse_date_value(
    py: Python<'_>,
    date_value: &str,
    parms: &[&str],
    rule_tzids: &std::collections::HashMap<String, String>,
    opts: &RrOptions,
) -> PyResult<Vec<PyObject>> {
    let mut tzid: Option<PyObject> = None;
    let mut value_found = false;
    for parm in parms {
        if parm.starts_with("TZID=") {
            // `rule_tzids[parm.split('TZID=')[-1]]`, KeyError -> skip.
            let tzkey = match rule_tzids.get(&parm["TZID=".len()..]) {
                Some(k) => k.clone(),
                None => continue,
            };
            let tzlookup: Bound<'_, PyAny> = match &opts.tzids {
                None => py.import("dateutil.tz")?.getattr("gettz")?,
                Some(t) => {
                    let tb = t.bind(py);
                    if tb.is_callable() {
                        tb.to_owned()
                    } else {
                        match tb.getattr("get") {
                            Ok(g) => g,
                            Err(_) => {
                                let s: String = tb.str()?.extract()?;
                                return value_error(format!(
                                    "tzids must be a callable, mapping, or None, not {}",
                                    s
                                ));
                            }
                        }
                    }
                }
            };
            tzid = Some(tzlookup.call1((tzkey,))?.unbind());
            continue;
        }
        if *parm != "VALUE=DATE-TIME" && *parm != "VALUE=DATE" {
            return value_error(format!("unsupported parm: {}", parm));
        } else if value_found {
            return value_error(format!("Duplicate value parameter found in: {}", parm));
        } else {
            value_found = true;
        }
    }
    let mut out = Vec::new();
    for datestr in date_value.split(',') {
        let mut date = parser_parse(py, datestr, opts)?;
        if let Some(tz) = &tzid {
            let db = date.bind(py);
            if db.getattr("tzinfo")?.is_none() {
                let kw = PyDict::new(py);
                kw.set_item("tzinfo", tz)?;
                date = db.call_method("replace", (), Some(&kw))?.unbind();
            } else {
                return value_error("DTSTART/EXDATE specifies multiple timezone".to_string());
            }
        }
        out.push(date);
    }
    Ok(out)
}

/// Shared `Rruleset` construction (also used by `rrulestr`).
fn rruleset_new_inner(py: Python<'_>, cache_on: bool) -> PyResult<Rruleset> {
    let lock = if cache_on {
        let thread = py.import("_thread")?;
        Some(thread.call_method0("allocate_lock")?.unbind())
    } else {
        None
    };
    Ok(Rruleset {
        lists: Mutex::new(RrulesetLists::default()),
        shared: Mutex::new(Shared::fresh(cache_on, lock)),
    })
}

/// `_parse_rfc_rrule` mirror: one `RRULE:` line (or bare rule) into an `rrule`.
fn parse_rfc_rrule(
    py: Python<'_>,
    line: &str,
    dtstart: Option<PyObject>,
    cache: bool,
    opts: &RrOptions,
) -> PyResult<Py<Rrule>> {
    let value = match line.find(':') {
        Some(_) => {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() != 2 {
                return value_error("too many values to unpack (expected 2)".to_string());
            }
            if parts[0] != "RRULE" {
                return value_error("unknown parameter name".to_string());
            }
            parts[1].to_string()
        }
        None => line.to_string(),
    };
    // rrkwargs as a dict (keys lowercased, like the handlers set them).
    let rrkwargs = PyDict::new(py);
    for pair in value.split(';') {
        let pv: Vec<&str> = pair.split('=').collect();
        if pv.len() != 2 {
            if pv.len() < 2 {
                return value_error("not enough values to unpack (expected 2, got 1)".to_string());
            } else {
                return value_error("too many values to unpack (expected 2)".to_string());
            }
        }
        let (name, pvalue) = (pv[0].to_uppercase(), pv[1].to_uppercase());
        if let Err(e) = handle_param(py, &rrkwargs, &name, &pvalue, opts) {
            // `except AttributeError: unknown parameter`; `except
            // (KeyError, ValueError): invalid 'NAME': VALUE`.
            if e.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) {
                return value_error(format!("unknown parameter '{}'", name));
            } else if e.is_instance_of::<pyo3::exceptions::PyKeyError>(py)
                || e.is_instance_of::<pyo3::exceptions::PyValueError>(py)
            {
                return value_error(format!("invalid '{}': {}", name, pvalue));
            } else {
                return Err(e);
            }
        }
    }
    // `rrule(dtstart=dtstart, cache=cache, **rrkwargs)` through the
    // constructor core.
    let get = |key: &str| -> Option<Bound<'_, PyAny>> {
        rrkwargs.get_item(key).unwrap_or(None)
    };
    if get("freq").is_none() {
        return type_error(
            "__init__() missing 1 required positional argument: 'freq'".to_string(),
        );
    }
    let r = rrule_new(
        py,
        get("freq").unwrap(),
        dtstart.map(|o| o.bind(py).to_owned()),
        get("interval"),
        get("wkst"),
        get("count"),
        get("until"),
        get("bysetpos"),
        get("bymonth"),
        get("bymonthday"),
        get("byyearday"),
        get("byeaster"),
        get("byweekno"),
        get("byweekday"),
        get("byhour"),
        get("byminute"),
        get("bysecond"),
        Some(bool_obj(py, cache).into_bound(py)),
    )?;
    Ok(Py::new(py, r)?)
}

/// One `_handle_*` dispatch. Unknown names raise `AttributeError` (mapped
/// to "unknown parameter" by the caller); `KeyError`/`ValueError` map to
/// "invalid 'NAME': VALUE".
fn handle_param(
    py: Python<'_>,
    rrkwargs: &Bound<'_, PyDict>,
    name: &str,
    value: &str,
    opts: &RrOptions,
) -> PyResult<()> {
    let builtins = py.import("builtins")?;
    let int_fn = builtins.getattr("int")?;
    match name {
        "INTERVAL" | "COUNT" => {
            rrkwargs.set_item(name.to_lowercase(), int_fn.call1((value,))?)?;
            Ok(())
        }
        "BYSETPOS" | "BYMONTH" | "BYMONTHDAY" | "BYYEARDAY" | "BYEASTER" | "BYWEEKNO" | "BYHOUR"
        | "BYMINUTE" | "BYSECOND" => {
            let mut items = Vec::new();
            for x in value.split(',') {
                items.push(int_fn.call1((x,))?.unbind());
            }
            let list = PyList::new(py, items.iter().map(|o| o.bind(py)))?;
            rrkwargs.set_item(name.to_lowercase(), list)?;
            Ok(())
        }
        "FREQ" => {
            let freq: i64 = match value {
                "YEARLY" => 0,
                "MONTHLY" => 1,
                "WEEKLY" => 2,
                "DAILY" => 3,
                "HOURLY" => 4,
                "MINUTELY" => 5,
                "SECONDLY" => 6,
                _ => {
                    return Err(pyo3::exceptions::PyKeyError::new_err(value.to_string()));
                }
            };
            rrkwargs.set_item("freq", freq)?;
            Ok(())
        }
        "UNTIL" => {
            let parsed = match parser_parse(py, value, opts) {
                Ok(d) => d,
                Err(e) => {
                    if e.is_instance_of::<pyo3::exceptions::PyValueError>(py) {
                        return value_error("invalid until date".to_string());
                    } else {
                        return Err(e);
                    }
                }
            };
            rrkwargs.set_item("until", parsed)?;
            Ok(())
        }
        "WKST" => {
            let wd: i64 = match value {
                "MO" => 0,
                "TU" => 1,
                "WE" => 2,
                "TH" => 3,
                "FR" => 4,
                "SA" => 5,
                "SU" => 6,
                _ => {
                    return Err(pyo3::exceptions::PyKeyError::new_err(value.to_string()));
                }
            };
            rrkwargs.set_item("wkst", wd)?;
            Ok(())
        }
        "BYWEEKDAY" | "BYDAY" => {
            let shim = py.import("dateutil.rrule")?;
            let wd_cls = shim.getattr("weekday")?;
            let wmap = |w: &str| -> PyResult<i64> {
                match w {
                    "MO" => Ok(0),
                    "TU" => Ok(1),
                    "WE" => Ok(2),
                    "TH" => Ok(3),
                    "FR" => Ok(4),
                    "SA" => Ok(5),
                    "SU" => Ok(6),
                    _ => Err(pyo3::exceptions::PyKeyError::new_err(w.to_string())),
                }
            };
            let mut l = Vec::new();
            for wday in value.split(',') {
                if wday.contains('(') {
                    // `TH(+1)` form.
                    let mut splt = wday.split('(');
                    let w = splt.next().unwrap_or("");
                    let n = splt.next().unwrap_or("");
                    let n = &n[..n.len().saturating_sub(1)];
                    let ni: i64 = int_fn.call1((n,))?.extract()?;
                    l.push(wd_cls.call1((wmap(w)?, ni))?.unbind());
                } else if !wday.is_empty() {
                    // `+1MO` form.
                    let bytes = wday.as_bytes();
                    let mut i = 0;
                    while i < bytes.len() && b"+-0123456789".contains(&bytes[i]) {
                        i += 1;
                    }
                    let (ns, w) = (&wday[..i], &wday[i..]);
                    let n: Option<i64> = if ns.is_empty() {
                        None
                    } else {
                        Some(int_fn.call1((ns,))?.extract()?)
                    };
                    match n {
                        None => l.push(wd_cls.call1((wmap(w)?,))?.unbind()),
                        Some(ni) => l.push(wd_cls.call1((wmap(w)?, ni))?.unbind()),
                    }
                } else {
                    return value_error("Invalid (empty) BYDAY specification.".to_string());
                }
            }
            let list = PyList::new(py, l.iter().map(|o| o.bind(py)))?;
            rrkwargs.set_item("byweekday", list)?;
            Ok(())
        }
        _ => Err(pyo3::exceptions::PyAttributeError::new_err(name.to_string())),
    }
}
