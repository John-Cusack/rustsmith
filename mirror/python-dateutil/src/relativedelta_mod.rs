// SPDX-License-Identifier: Apache-2.0
//! `relativedelta` binding: field storage on Python-compatible shapes,
//! calendar math via `dateutil-core`, datetime operations through real
//! Python `replace`/arithmetic so CPython semantics (fold, tzinfo,
//! subclasses, errors) are exact.

use crate::util::*;
use dateutil_core::{civil, relativedelta as rdcore};
use pyo3::basic::CompareOp;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

/// Stored `relativedelta` fields. Relative time fields keep their Python
/// numeric type (int or float); absolutes keep `None` or the number.
#[pyclass(weakref, name = "relativedelta", subclass, dict, module = "dateutil._dateutil")]
pub struct Relativedelta {
    years: i64,
    months: i64,
    days: PyObject,
    leapdays: PyObject,
    hours: PyObject,
    minutes: PyObject,
    seconds: PyObject,
    microseconds: PyObject,
    year: Option<PyObject>,
    month: Option<PyObject>,
    day: Option<PyObject>,
    weekday: Option<PyObject>,
    hour: Option<PyObject>,
    minute: Option<PyObject>,
    second: Option<PyObject>,
    microsecond: Option<PyObject>,
    has_time: bool,
}

fn num_to_f64(_py: Python<'_>, o: &Bound<'_, PyAny>) -> PyResult<f64> {
    if let Ok(i) = o.extract::<i64>() {
        return Ok(i as f64);
    }
    o.extract::<f64>()
}

fn opt_ref_clone(py: Python<'_>, o: &Option<PyObject>) -> Option<PyObject> {
    o.as_ref().map(|v| v.clone_ref(py))
}

fn is_timedelta(py: Python<'_>, o: &Bound<'_, PyAny>) -> PyResult<bool> {
    let td = timedelta_cls(py)?;
    o.is_instance(td)
}

/// `other.x if other.x is not None else self.x` (for `__add__` absolutes).
fn opt_or(py: Python<'_>, a: &Option<PyObject>, b: &Option<PyObject>) -> Option<PyObject> {
    match (a, b) {
        (Some(x), _) => Some(x.clone_ref(py)),
        (None, Some(y)) => Some(y.clone_ref(py)),
        (None, None) => None,
    }
}

/// `self.x if self.x is not None else other.x` (for `__sub__` absolutes).
fn opt_or_self(py: Python<'_>, a: &Option<PyObject>, b: &Option<PyObject>) -> Option<PyObject> {
    opt_or(py, a, b)
}

fn opt_eq(py: Python<'_>, a: &Option<PyObject>, b: &Option<PyObject>) -> PyResult<bool> {
    match (a, b) {
        (None, None) => Ok(true),
        (Some(_), None) | (None, Some(_)) => Ok(false),
        (Some(x), Some(y)) => py_eq(py, x.bind(py), y.bind(py)),
    }
}

/// Weekday equality with the `None`-vs-`1` normalization from `__eq__`.
fn weekday_eq(_py: Python<'_>, a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
    let aw = a.getattr("weekday")?.extract::<i64>()?;
    let bw = b.getattr("weekday")?.extract::<i64>()?;
    if aw != bw {
        return Ok(false);
    }
    let an = weekday_n(a)?;
    let bn = weekday_n(b)?;
    if an != bn && !((an.is_none() || an == Some(1)) && (bn.is_none() || bn == Some(1))) {
        return Ok(false);
    }
    Ok(true)
}

fn weekday_n(w: &Bound<'_, PyAny>) -> PyResult<Option<i64>> {
    let n = w.getattr("n")?;
    if n.is_none() {
        return Ok(None);
    }
    Ok(Some(n.extract::<i64>()?))
}

fn float_of(py: Python<'_>, o: &Bound<'_, PyAny>) -> PyResult<f64> {
    let f = float_fn(py)?;
    f.call1((o,))?.extract()
}
/// Wrap a computed state in an instance of `cls` (mirrors
/// `self.__class__(...)` for subclass-friendly operators).
fn wrap_dynamic(
    py: Python<'_>,
    cls: Bound<'_, pyo3::types::PyType>,
    rd: Relativedelta,
) -> PyResult<PyObject> {
    let kw = PyDict::new(py);
    kw.set_item("years", rd.years)?;
    kw.set_item("months", rd.months)?;
    kw.set_item("days", &rd.days)?;
    kw.set_item("leapdays", &rd.leapdays)?;
    kw.set_item("hours", &rd.hours)?;
    kw.set_item("minutes", &rd.minutes)?;
    kw.set_item("seconds", &rd.seconds)?;
    kw.set_item("microseconds", &rd.microseconds)?;
    for (name, o) in [
        ("year", &rd.year),
        ("month", &rd.month),
        ("day", &rd.day),
        ("weekday", &rd.weekday),
        ("hour", &rd.hour),
        ("minute", &rd.minute),
        ("second", &rd.second),
        ("microsecond", &rd.microsecond),
    ] {
        if let Some(v) = o {
            kw.set_item(name, v)?;
        }
    }
    Ok(cls.call((), Some(&kw))?.unbind())
}

impl Relativedelta {
    /// Shared construction from validated pieces (runs `_fix`).
    #[allow(clippy::too_many_arguments)]
    fn build(
        py: Python<'_>,
        years: i64,
        months: i64,
        days: PyObject,
        leapdays: PyObject,
        hours: PyObject,
        minutes: PyObject,
        seconds: PyObject,
        microseconds: PyObject,
        year: Option<PyObject>,
        month: Option<PyObject>,
        day: Option<PyObject>,
        weekday: Option<PyObject>,
        hour: Option<PyObject>,
        minute: Option<PyObject>,
        second: Option<PyObject>,
        microsecond: Option<PyObject>,
    ) -> PyResult<Self> {
        let mut rd = Relativedelta {
            years,
            months,
            days,
            leapdays,
            hours,
            minutes,
            seconds,
            microseconds,
            year,
            month,
            day,
            weekday,
            hour,
            minute,
            second,
            microsecond,
            has_time: false,
        };
        rd.fix(py)?;
        Ok(rd)
    }

    /// `_fix` cascade + `_has_time`, mirroring the original exactly.
    fn fix(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.months.abs() > 11 {
            let s = if self.months < 0 { -1 } else { 1 };
            let (div, modu) = civil::py_divmod(self.months * s, 12);
            self.months = modu * s;
            self.years += div * s;
        }
        let f = |o: &PyObject| num_to_f64(py, o.bind(py));
        let (days, hours, minutes, seconds, micros) = rdcore::fix_time(
            f(&self.days)?,
            f(&self.hours)?,
            f(&self.minutes)?,
            f(&self.seconds)?,
            f(&self.microseconds)?,
        );
        self.days = days.into_pyobject(py)?.into_any().unbind();
        self.hours = hours.into_pyobject(py)?.into_any().unbind();
        self.minutes = minutes.into_pyobject(py)?.into_any().unbind();
        self.seconds = seconds.into_pyobject(py)?.into_any().unbind();
        self.microseconds = micros.into_pyobject(py)?.into_any().unbind();
        self.has_time = hours != 0.0
            || minutes != 0.0
            || seconds != 0.0
            || micros != 0.0
            || self.hour.is_some()
            || self.minute.is_some()
            || self.second.is_some()
            || self.microsecond.is_some();
        Ok(())
    }

    /// `self` scaled by `f` (for `__mul__`/`__neg__`): integer fields via
    /// `int(x * f)`, absolute/weekday/leapdays preserved.
    fn scaled_state(&self, py: Python<'_>, f: f64) -> PyResult<Relativedelta> {
        let mul_num = |o: &PyObject| -> PyResult<PyObject> {
            let v = num_to_f64(py, o.bind(py))?;
            // `int(self.days * f)`: truncation toward zero via Python int()
            // on the float product, mirroring `int()` exactly.
            let prod = v * f;
            let as_int = if prod.is_finite() {
                let i = int_fn(py)?;
                i.call1((prod,))?.unbind()
            } else {
                // `int(inf)` raises OverflowError, like the original.
                let i = int_fn(py)?;
                i.call1((prod,))?.unbind()
            };
            Ok(as_int)
        };
        Ok(Self::build(
                py,
                ((self.years as f64 * f).trunc()) as i64,
                ((self.months as f64 * f).trunc()) as i64,
                mul_num(&self.days)?,
                self.leapdays.clone_ref(py),
                mul_num(&self.hours)?,
                mul_num(&self.minutes)?,
                mul_num(&self.seconds)?,
                mul_num(&self.microseconds)?,
                opt_ref_clone(py, &self.year),
                opt_ref_clone(py, &self.month),
                opt_ref_clone(py, &self.day),
                opt_ref_clone(py, &self.weekday),
                opt_ref_clone(py, &self.hour),
                opt_ref_clone(py, &self.minute),
                opt_ref_clone(py, &self.second),
                opt_ref_clone(py, &self.microsecond),
            )?)
    }
}

/// `relativedelta.__add__` date application shared by `__add__`/`__radd__`
/// and the `dt1/dt2` constructor.
fn add_to(py: Python<'_>, rd: &Relativedelta, other: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    let has_time = rd.has_time;
    let mut other_o = other.clone().unbind();
    if has_time {
        let is_dt = is_datetime(py, other)?;
        if !is_dt {
            // `datetime.fromordinal(other.toordinal())`, like the original.
            let o: i64 = other.call_method0("toordinal")?.extract()?;
            other_o = datetime_cls(py)?.call_method1("fromordinal", (o,))?.unbind();
        }
    }
    let other_b = other_o.bind(py);
    let (oy, om, od) = date_parts(other_b)?;
    let abs_i = |o: &Option<PyObject>| -> PyResult<Option<i64>> {
        match o {
            None => Ok(None),
            Some(v) => {
                let f = num_to_f64(py, v.bind(py))?;
                if f.fract() != 0.0 {
                    return type_error("'float' object cannot be interpreted as an integer".to_string());
                }
                Ok(Some(f as i64))
            }
        }
    };
    // Exact single-carry shape from `__add__` (rel months are in -11..11
    // after `_fix`, so one carry suffices).
    let mut new_year = abs_i(&rd.year)?.unwrap_or(oy) + rd.years;
    let mut new_month = abs_i(&rd.month)?.unwrap_or(om);
    if rd.months != 0 {
        new_month += rd.months;
        if new_month > 12 {
            new_year += 1;
            new_month -= 12;
        } else if new_month < 1 {
            new_year -= 1;
            new_month += 12;
        }
    }
    if !(1 <= new_year && new_year <= 9999) {
        return value_error(format!("year {} is out of range", new_year));
    }
    let abs_day = abs_i(&rd.day)?.unwrap_or(od);
    let dim = civil::days_in_month(new_year, new_month as u8) as i64;
    let new_day = dim.min(abs_day);
    let kwargs = PyDict::new(py);
    kwargs.set_item("year", new_year)?;
    kwargs.set_item("month", new_month)?;
    kwargs.set_item("day", new_day)?;
    for (name, o) in [
        ("hour", &rd.hour),
        ("minute", &rd.minute),
        ("second", &rd.second),
        ("microsecond", &rd.microsecond),
    ] {
        if let Some(v) = o {
            let f = num_to_f64(py, v.bind(py))?;
            if f.fract() != 0.0 {
                return type_error("'float' object cannot be interpreted as an integer".to_string());
            }
            kwargs.set_item(name, f as i64)?;
        }
    }
    let mut ret = other_b.call_method("replace", (), Some(&kwargs))?.unbind();
    // leapdays + timedelta addition via Python (exact normalization).
    let mut leap = num_to_f64(py, rd.leapdays.bind(py))?;
    if leap != 0.0 && new_month > 2 && civil::is_leap(new_year) {
        // `days += self.leapdays` (float-aware).
    } else {
        leap = 0.0;
    }
    let td = timedelta_cls(py)?;
    let kw = PyDict::new(py);
    kw.set_item("days", num_to_f64(py, rd.days.bind(py))? + leap)?;
    kw.set_item("hours", rd.hours.bind(py))?;
    kw.set_item("minutes", rd.minutes.bind(py))?;
    kw.set_item("seconds", rd.seconds.bind(py))?;
    kw.set_item("microseconds", rd.microseconds.bind(py))?;
    let delta = td.call((), Some(&kw))?;
    ret = ret.bind(py).add(&delta)?.unbind();
    // Weekday jump.
    if let Some(w) = &rd.weekday {
        let wb = w.bind(py);
        let wday: i64 = wb.getattr("weekday")?.extract()?;
        let nth: Option<i64> = {
            let n = wb.getattr("n")?;
            if n.is_none() {
                None
            } else {
                Some(n.extract()?)
            }
        };
        let (ry, rm, rdd) = date_parts(ret.bind(py))?;
        let ord = civil::to_ordinal(ry, rm as u8, rdd as u8);
        let target = rdcore::weekday_jump(ord, wday as u8, nth);
        let jump = target - ord;
        let jdelta = td.call1((jump, 0, 0))?;
        ret = ret.bind(py).add(&jdelta)?.unbind();
    }
    Ok(ret)
}

#[pymethods]
impl Relativedelta {
    #[new]
    #[pyo3(signature = (dt1=None, dt2=None, *, years=None, months=None, days=None, leapdays=None, weeks=None, hours=None, minutes=None, seconds=None, microseconds=None, year=None, month=None, day=None, weekday=None, yearday=None, nlyearday=None, hour=None, minute=None, second=None, microsecond=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        dt1: Option<Bound<'_, PyAny>>,
        dt2: Option<Bound<'_, PyAny>>,
        years: Option<Bound<'_, PyAny>>,
        months: Option<Bound<'_, PyAny>>,
        days: Option<Bound<'_, PyAny>>,
        leapdays: Option<Bound<'_, PyAny>>,
        weeks: Option<Bound<'_, PyAny>>,
        hours: Option<Bound<'_, PyAny>>,
        minutes: Option<Bound<'_, PyAny>>,
        seconds: Option<Bound<'_, PyAny>>,
        microseconds: Option<Bound<'_, PyAny>>,
        year: Option<Bound<'_, PyAny>>,
        month: Option<Bound<'_, PyAny>>,
        day: Option<Bound<'_, PyAny>>,
        weekday: Option<Bound<'_, PyAny>>,
        yearday: Option<Bound<'_, PyAny>>,
        nlyearday: Option<Bound<'_, PyAny>>,
        hour: Option<Bound<'_, PyAny>>,
        minute: Option<Bound<'_, PyAny>>,
        second: Option<Bound<'_, PyAny>>,
        microsecond: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let some1 = dt1.as_ref().map(|o| o.is_truthy().unwrap_or(false)).unwrap_or(false);
        let some2 = dt2.as_ref().map(|o| o.is_truthy().unwrap_or(false)).unwrap_or(false);
        if some1 && some2 {
            return Self::from_diff(py, dt1.unwrap(), dt2.unwrap());
        }
        // Integral years/months (ValueError otherwise, like the original):
        // `x != int(x)` with `int()` errors propagating.
        let int_fn = int_fn(py)?;
        for v in [years.as_ref(), months.as_ref()].into_iter().flatten() {
            let iv = int_fn.call1((v,))?;
            let eq: bool = v.rich_compare(&iv, CompareOp::Eq)?.extract()?;
            if !eq {
                return value_error(
                    "Non-integer years and months are ambiguous and not currently supported."
                        .to_string(),
                );
            }
        }
        let years_i: i64 = match years {
            None => 0,
            Some(v) => int_fn.call1((&v,))?.extract()?,
        };
        let months_i: i64 = match months {
            None => 0,
            Some(v) => int_fn.call1((&v,))?.extract()?,
        };
        let num = |o: Option<Bound<'_, PyAny>>, default: f64| -> PyResult<PyObject> {
            match o {
                None => Ok(default.into_pyobject(py)?.into_any().unbind()),
                Some(v) => Ok(v.unbind()),
            }
        };
        // days = days + weeks*7 via Python (float-aware).
        let days_o = match (days.map(|v| v.unbind()), weeks.map(|v| v.unbind())) {
            (None, None) => 0.0f64.into_pyobject(py)?.into_any().unbind(),
            (Some(d), None) => d,
            (None, Some(w)) => {
                let seven: PyObject = 7i64.into_pyobject(py)?.into_any().unbind();
                w.bind(py).mul(seven.bind(py))?.unbind()
            }
            (Some(d), Some(w)) => {
                let seven: PyObject = 7i64.into_pyobject(py)?.into_any().unbind();
                let w7 = w.bind(py).mul(seven.bind(py))?;
                d.bind(py).add(w7)?.unbind()
            }
        };
        // Absolute float warnings (TypeError propagates, like the original).
        for v in [&year, &month, &day, &hour, &minute, &second, &microsecond]
            .into_iter()
            .flatten()
        {
            let iv = int_fn.call1((v,))?;
            let eq: bool = v.rich_compare(&iv, CompareOp::Eq)?.extract()?;
            if !eq {
                warn_with(
                    py,
                    &py.get_type::<pyo3::exceptions::PyDeprecationWarning>().into_any(),
                    "Non-integer value passed as absolute information. This is not a well-defined condition and will raise errors in future versions.",
                )?;
            }
        }
        // weekday: int -> shim weekdays[int].
        let weekday_o = match weekday {
            None => None,
            Some(w) => {
                if let Ok(i) = w.extract::<i64>() {
                    Some(weekdays_obj(py)?.get_item(i)?.unbind())
                } else {
                    Some(w.unbind())
                }
            }
        };
        // yearday / nlyearday mapping.
        let mut month_o = month.map(|v| v.unbind());
        let mut day_o = day.map(|v| v.unbind());
        let mut leapdays_o: PyObject = num(leapdays, 0.0)?;
        let mut yday: Option<i64> = None;
        if let Some(n) = nlyearday {
            yday = Some(n.extract::<i64>()?);
        } else if let Some(y) = yearday {
            let yv: i64 = y.extract()?;
            if yv > 59 {
                leapdays_o = (-1i64).into_pyobject(py)?.into_any().unbind();
            }
            yday = Some(yv);
        }
        if let Some(yday) = yday {
            let ydayidx = [31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 366];
            let mut found = false;
            for (idx, lim) in ydayidx.iter().enumerate() {
                if yday <= *lim {
                    month_o = Some(((idx + 1) as i64).into_pyobject(py)?.into_any().unbind());
                    let d = if idx == 0 { yday } else { yday - ydayidx[idx - 1] };
                    day_o = Some(d.into_pyobject(py)?.into_any().unbind());
                    found = true;
                    break;
                }
            }
            if !found {
                return value_error(format!("invalid year day ({})", yday));
            }
        }
        Self::build(
            py,
            years_i,
            months_i,
            days_o,
            leapdays_o,
            num(hours, 0.0)?,
            num(minutes, 0.0)?,
            num(seconds, 0.0)?,
            num(microseconds, 0.0)?,
            year.map(|v| v.unbind()),
            month_o,
            day_o,
            weekday_o,
            hour.map(|v| v.unbind()),
            minute.map(|v| v.unbind()),
            second.map(|v| v.unbind()),
            microsecond.map(|v| v.unbind()),
        )
    }

    /// `relativedelta(dt1, dt2)` difference constructor.
    #[staticmethod]
    fn from_diff(py: Python<'_>, dt1: Bound<'_, PyAny>, dt2: Bound<'_, PyAny>) -> PyResult<Self> {
        if !(is_date(py, &dt1)? && is_date(py, &dt2)?) {
            return type_error("relativedelta only diffs datetime/date".to_string());
        }
        let dt1_is_dt = is_datetime(py, &dt1)?;
        let dt2_is_dt = is_datetime(py, &dt2)?;
        let (dt1, dt2) = if dt1_is_dt != dt2_is_dt {
            let dt_cls = datetime_cls(py)?;
            if !dt1_is_dt {
                let o = dt1.call_method0("toordinal")?.extract::<i64>()?;
                (
                    dt_cls.call_method1("fromordinal", (o,))?,
                    dt2,
                )
            } else {
                let o = dt2.call_method0("toordinal")?.extract::<i64>()?;
                (
                    dt1,
                    dt_cls.call_method1("fromordinal", (o,))?,
                )
            }
        } else {
            (dt1, dt2)
        };
        let (y1, m1, _) = date_parts(&dt1)?;
        let (y2, m2, _) = date_parts(&dt2)?;
        let mut months = rdcore::diff_months(y1, m1 as u8, y2, m2 as u8);
        let zero: PyObject = 0.0f64.into_pyobject(py)?.into_any().unbind();
        let mut rd = Self::build(
            py, 0, 0, zero.clone_ref(py), zero.clone_ref(py), zero.clone_ref(py),
            zero.clone_ref(py), zero.clone_ref(py), zero.clone_ref(py),
            None, None, None, None, None, None, None, None,
        )?;
        let mut dtm = {
            let (ys, ms) = rdcore::set_months(months);
            rd.years = ys;
            rd.months = ms;
            add_to(py, &rd, &dt2)?
        };
        // Overshoot adjustment, mirroring the original's branch selection
        // on dt1 vs dt2 (not dtm: clamping can place dtm on either side).
        if dt1.lt(&dt2)? {
            while dtm.bind(py).lt(&dt1)? {
                months += 1;
                let (ys, ms) = rdcore::set_months(months);
                rd.years = ys;
                rd.months = ms;
                dtm = add_to(py, &rd, &dt2)?;
            }
        } else {
            while dt1.lt(&dtm.bind(py))? {
                months -= 1;
                let (ys, ms) = rdcore::set_months(months);
                rd.years = ys;
                rd.months = ms;
                dtm = add_to(py, &rd, &dt2)?;
            }
        }
        let delta = dt1.sub(&dtm.bind(py))?;
        let (ddays, dsecs, dmicros) = delta_parts(&delta)?;
        rd.seconds = ((dsecs + ddays * 86400) as f64).into_pyobject(py)?.into_any().unbind();
        rd.microseconds = (dmicros as f64).into_pyobject(py)?.into_any().unbind();
        // The constructor runs `_fix()` after the diff path (cascading the
        // leftover seconds into days/hours): mirror it exactly.
        rd.fix(py)?;
        Ok(rd)
    }

    #[getter]
    fn years(&self) -> i64 {
        self.years
    }
    #[getter]
    fn months(&self) -> i64 {
        self.months
    }
    #[getter]
    fn days(&self, py: Python<'_>) -> PyObject {
        self.days.clone_ref(py)
    }
    #[getter]
    fn leapdays(&self, py: Python<'_>) -> PyObject {
        self.leapdays.clone_ref(py)
    }
    #[getter]
    fn hours(&self, py: Python<'_>) -> PyObject {
        self.hours.clone_ref(py)
    }
    #[getter]
    fn minutes(&self, py: Python<'_>) -> PyObject {
        self.minutes.clone_ref(py)
    }
    #[getter]
    fn seconds(&self, py: Python<'_>) -> PyObject {
        self.seconds.clone_ref(py)
    }
    #[getter]
    fn microseconds(&self, py: Python<'_>) -> PyObject {
        self.microseconds.clone_ref(py)
    }
    #[getter]
    fn year(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.year)
    }
    #[getter]
    fn month(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.month)
    }
    #[getter]
    fn day(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.day)
    }
    #[getter]
    fn weekday(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.weekday)
    }
    #[getter]
    fn hour(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.hour)
    }
    #[getter]
    fn minute(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.minute)
    }
    #[getter]
    fn second(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.second)
    }
    #[getter]
    fn microsecond(&self, py: Python<'_>) -> PyObject {
        opt_clone(py, &self.microsecond)
    }
    #[getter]
    fn weeks(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = num_to_f64(py, self.days.bind(py))?;
        Ok(((d / 7.0).trunc() as i64).into_pyobject(py)?.into_any().unbind())
    }
    #[setter]
    fn set_weeks(&mut self, py: Python<'_>, value: Bound<'_, PyAny>) -> PyResult<()> {
        let days = num_to_f64(py, self.days.bind(py))?;
        let weeks_now = (days / 7.0).trunc();
        let v = num_to_f64(py, &value)?;
        self.days = (days - weeks_now * 7.0 + v * 7.0).into_pyobject(py)?.into_any().unbind();
        Ok(())
    }

    fn normalized(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        let f = |o: &PyObject| num_to_f64(py, o.bind(py));
        let (d, h, m, s, u) = rdcore::normalized_parts(
            f(&slf.days)?,
            f(&slf.hours)?,
            f(&slf.minutes)?,
            f(&slf.seconds)?,
            f(&slf.microseconds)?,
        );
        let rd = Self::build(
            py,
            slf.years,
            slf.months,
            (d as f64).into_pyobject(py)?.into_any().unbind(),
            slf.leapdays.clone_ref(py),
            (h as f64).into_pyobject(py)?.into_any().unbind(),
            (m as f64).into_pyobject(py)?.into_any().unbind(),
            (s as f64).into_pyobject(py)?.into_any().unbind(),
            (u as f64).into_pyobject(py)?.into_any().unbind(),
            opt_ref_clone(py, &slf.year),
            opt_ref_clone(py, &slf.month),
            opt_ref_clone(py, &slf.day),
            opt_ref_clone(py, &slf.weekday),
            opt_ref_clone(py, &slf.hour),
            opt_ref_clone(py, &slf.minute),
            opt_ref_clone(py, &slf.second),
            opt_ref_clone(py, &slf.microsecond),
        )?;
        wrap_dynamic(py, slf_type(py, &slf), rd)
    }

    fn __add__(slf: PyRef<'_, Self>, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if let Ok(o) = other.extract::<PyRef<'_, Relativedelta>>() {
            return wrap_dynamic(py, slf_type(py, &slf),
                Self::build(
                    py,
                    o.years + slf.years,
                    o.months + slf.months,
                    add_objs(py, &o.days, &slf.days)?,
                    or_objs(py, &o.leapdays, &slf.leapdays)?,
                    add_objs(py, &o.hours, &slf.hours)?,
                    add_objs(py, &o.minutes, &slf.minutes)?,
                    add_objs(py, &o.seconds, &slf.seconds)?,
                    add_objs(py, &o.microseconds, &slf.microseconds)?,
                    opt_or(py, &o.year, &slf.year),
                    opt_or(py, &o.month, &slf.month),
                    opt_or(py, &o.day, &slf.day),
                    opt_or(py, &o.weekday, &slf.weekday),
                    opt_or(py, &o.hour, &slf.hour),
                    opt_or(py, &o.minute, &slf.minute),
                    opt_or(py, &o.second, &slf.second),
                    opt_or(py, &o.microsecond, &slf.microsecond),
                )?,
            );
        }
        if is_timedelta(py, &other)? {
            let (td, ts, tu) = delta_parts(&other)?;
            return wrap_dynamic(py, slf_type(py, &slf),
                Self::build(
                    py,
                    slf.years,
                    slf.months,
                    add_float(py, &slf.days, td as f64)?,
                    slf.leapdays.clone_ref(py),
                    slf.hours.clone_ref(py),
                    slf.minutes.clone_ref(py),
                    add_float(py, &slf.seconds, ts as f64)?,
                    add_float(py, &slf.microseconds, tu as f64)?,
                    opt_ref_clone(py, &slf.year),
                    opt_ref_clone(py, &slf.month),
                    opt_ref_clone(py, &slf.day),
                    opt_ref_clone(py, &slf.weekday),
                    opt_ref_clone(py, &slf.hour),
                    opt_ref_clone(py, &slf.minute),
                    opt_ref_clone(py, &slf.second),
                    opt_ref_clone(py, &slf.microsecond),
                )?,
            );
        }
        if is_date(py, &other)? {
            return add_to(py, &slf, &other);
        }
        Ok(not_implemented(py))
    }

    fn __radd__(slf: PyRef<'_, Self>, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        Self::__add__(slf, py, other)
    }

    fn __sub__(slf: PyRef<'_, Self>, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        if let Ok(o) = other.extract::<PyRef<'_, Relativedelta>>() {
            return wrap_dynamic(py, slf_type(py, &slf),
                Self::build(
                    py,
                    slf.years - o.years,
                    slf.months - o.months,
                    sub_objs(py, &slf.days, &o.days)?,
                    or_objs(py, &slf.leapdays, &o.leapdays)?,
                    sub_objs(py, &slf.hours, &o.hours)?,
                    sub_objs(py, &slf.minutes, &o.minutes)?,
                    sub_objs(py, &slf.seconds, &o.seconds)?,
                    sub_objs(py, &slf.microseconds, &o.microseconds)?,
                    opt_or_self(py, &slf.year, &o.year),
                    opt_or_self(py, &slf.month, &o.month),
                    opt_or_self(py, &slf.day, &o.day),
                    opt_or_self(py, &slf.weekday, &o.weekday),
                    opt_or_self(py, &slf.hour, &o.hour),
                    opt_or_self(py, &slf.minute, &o.minute),
                    opt_or_self(py, &slf.second, &o.second),
                    opt_or_self(py, &slf.microsecond, &o.microsecond),
                )?,
            );
        }
        Ok(not_implemented(py))
    }

    fn __rsub__(slf: PyRef<'_, Self>, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let neg = Self::__neg__(slf, py)?;
        let bound = neg.bind(py);
        let neg_ref: PyRef<'_, Relativedelta> = bound.extract()?;
        Self::__add__(neg_ref, py, other)
    }

    fn __neg__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        wrap_dynamic(py, slf_type(py, &slf), slf.scaled_state(py, -1.0)?)
    }

    fn __abs__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        wrap_dynamic(
            py,
            slf_type(py, &slf),
            Self::build(
                py,
                slf.years.abs(),
                slf.months.abs(),
                abs_obj(py, &slf.days)?,
                slf.leapdays.clone_ref(py),
                abs_obj(py, &slf.hours)?,
                abs_obj(py, &slf.minutes)?,
                abs_obj(py, &slf.seconds)?,
                abs_obj(py, &slf.microseconds)?,
                opt_ref_clone(py, &slf.year),
                opt_ref_clone(py, &slf.month),
                opt_ref_clone(py, &slf.day),
                opt_ref_clone(py, &slf.weekday),
                opt_ref_clone(py, &slf.hour),
                opt_ref_clone(py, &slf.minute),
                opt_ref_clone(py, &slf.second),
                opt_ref_clone(py, &slf.microsecond),
            )?,
        )
    }

    fn __mul__(slf: PyRef<'_, Self>, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        match float_of(py, &other) {
            Ok(f) => wrap_dynamic(py, slf_type(py, &slf), slf.scaled_state(py, f)?),
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyTypeError>(py) {
                    Ok(not_implemented(py))
                } else {
                    Err(e)
                }
            }
        }
    }

    fn __rmul__(slf: PyRef<'_, Self>, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        Self::__mul__(slf, py, other)
    }

    fn __div__(slf: PyRef<'_, Self>, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        match float_of(py, &other) {
            Ok(f) => wrap_dynamic(py, slf_type(py, &slf), slf.scaled_state(py, 1.0 / f)?),
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyTypeError>(py) {
                    Ok(not_implemented(py))
                } else {
                    Err(e)
                }
            }
        }
    }

    fn __truediv__(slf: PyRef<'_, Self>, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        Self::__div__(slf, py, other)
    }

    fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let truthy = |o: &PyObject| o.bind(py).is_truthy().unwrap_or(true);
        Ok(truthy(&self.days)
            || self.years != 0
            || self.months != 0
            || truthy(&self.hours)
            || truthy(&self.minutes)
            || truthy(&self.seconds)
            || truthy(&self.microseconds)
            || truthy(&self.leapdays)
            || self.year.is_some()
            || self.month.is_some()
            || self.day.is_some()
            || self.weekday.is_some()
            || self.hour.is_some()
            || self.minute.is_some()
            || self.second.is_some()
            || self.microsecond.is_some())
    }

    fn __eq__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let o: PyRef<'_, Relativedelta> = match other.extract() {
            Ok(o) => o,
            Err(_) => return Ok(not_implemented(py)),
        };
        let wd_eq = match (&self.weekday, &o.weekday) {
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
            (Some(a), Some(b)) => weekday_eq(py, a.bind(py), b.bind(py))?,
        };
        if !wd_eq {
            return Ok(bool_obj(py, false));
        }
        let eq = self.years == o.years
            && self.months == o.months
            && py_eq(py, self.days.bind(py), o.days.bind(py))?
            && py_eq(py, self.hours.bind(py), o.hours.bind(py))?
            && py_eq(py, self.minutes.bind(py), o.minutes.bind(py))?
            && py_eq(py, self.seconds.bind(py), o.seconds.bind(py))?
            && py_eq(py, self.microseconds.bind(py), o.microseconds.bind(py))?
            && py_eq(py, self.leapdays.bind(py), o.leapdays.bind(py))?
            && opt_eq(py, &self.year, &o.year)?
            && opt_eq(py, &self.month, &o.month)?
            && opt_eq(py, &self.day, &o.day)?
            && opt_eq(py, &self.hour, &o.hour)?
            && opt_eq(py, &self.minute, &o.minute)?
            && opt_eq(py, &self.second, &o.second)?
            && opt_eq(py, &self.microsecond, &o.microsecond)?;
        Ok(bool_obj(py, eq))
    }

    fn __ne__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<PyObject> {
        let eq = self.__eq__(py, other)?;
        let ni = py.NotImplemented().into_any();
        if eq.is(ni.bind(py)) {
            warn_with(
                py,
                &py.get_type::<pyo3::exceptions::PyDeprecationWarning>().into_any(),
                "NotImplemented should not be used in a boolean context",
            )?;
            return Ok(bool_obj(py, false));
        }
        let b: bool = eq.extract(py)?;
        Ok(bool_obj(py, !b))
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let items = vec![
            opt_ref_clone(py, &self.weekday).unwrap_or_else(|| py.None()),
            self.years.into_pyobject(py)?.into_any().unbind(),
            self.months.into_pyobject(py)?.into_any().unbind(),
            self.days.clone_ref(py),
            self.hours.clone_ref(py),
            self.minutes.clone_ref(py),
            self.seconds.clone_ref(py),
            self.microseconds.clone_ref(py),
            self.leapdays.clone_ref(py),
            opt_ref_clone(py, &self.year).unwrap_or_else(|| py.None()),
            opt_ref_clone(py, &self.month).unwrap_or_else(|| py.None()),
            opt_ref_clone(py, &self.day).unwrap_or_else(|| py.None()),
            opt_ref_clone(py, &self.hour).unwrap_or_else(|| py.None()),
            opt_ref_clone(py, &self.minute).unwrap_or_else(|| py.None()),
            opt_ref_clone(py, &self.second).unwrap_or_else(|| py.None()),
            opt_ref_clone(py, &self.microsecond).unwrap_or_else(|| py.None()),
        ];
        let tup = PyTuple::new(py, items)?;
        tup.hash()
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let mut parts: Vec<String> = Vec::new();
        for (attr, o) in [
            ("years", self.years.into_pyobject(py)?.into_any().unbind()),
            ("months", self.months.into_pyobject(py)?.into_any().unbind()),
            ("days", self.days.clone_ref(py)),
            ("leapdays", self.leapdays.clone_ref(py)),
            ("hours", self.hours.clone_ref(py)),
            ("minutes", self.minutes.clone_ref(py)),
            ("seconds", self.seconds.clone_ref(py)),
            ("microseconds", self.microseconds.clone_ref(py)),
        ] {
            if o.bind(py).is_truthy()? {
                let f = num_to_f64(py, o.bind(py))?;
                parts.push(format!("{}={}", attr, civil::py_g_format(f, true)));
            }
        }
        for (attr, o) in [
            ("year", &self.year),
            ("month", &self.month),
            ("day", &self.day),
            ("weekday", &self.weekday),
            ("hour", &self.hour),
            ("minute", &self.minute),
            ("second", &self.second),
            ("microsecond", &self.microsecond),
        ] {
            if let Some(v) = o {
                let r: String = v.bind(py).repr()?.extract()?;
                parts.push(format!("{}={}", attr, r));
            }
        }
        let cls: String = py.get_type::<Relativedelta>().getattr("__name__")?.extract()?;
        Ok(format!("{}({})", cls, parts.join(", ")))
    }

    fn __getstate__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = PyDict::new(py);
        d.set_item("years", self.years)?;
        d.set_item("months", self.months)?;
        d.set_item("days", &self.days)?;
        d.set_item("leapdays", &self.leapdays)?;
        d.set_item("hours", &self.hours)?;
        d.set_item("minutes", &self.minutes)?;
        d.set_item("seconds", &self.seconds)?;
        d.set_item("microseconds", &self.microseconds)?;
        d.set_item("year", opt_clone(py, &self.year))?;
        d.set_item("month", opt_clone(py, &self.month))?;
        d.set_item("day", opt_clone(py, &self.day))?;
        d.set_item("weekday", opt_clone(py, &self.weekday))?;
        d.set_item("hour", opt_clone(py, &self.hour))?;
        d.set_item("minute", opt_clone(py, &self.minute))?;
        d.set_item("second", opt_clone(py, &self.second))?;
        d.set_item("microsecond", opt_clone(py, &self.microsecond))?;
        Ok(d.into_any().unbind())
    }

    fn __setstate__(&mut self, state: Bound<'_, PyAny>) -> PyResult<()> {
        self.years = state.getattr("years")?.extract()?;
        self.months = state.getattr("months")?.extract()?;
        self.days = state.getattr("days")?.unbind();
        self.leapdays = state.getattr("leapdays")?.unbind();
        self.hours = state.getattr("hours")?.unbind();
        self.minutes = state.getattr("minutes")?.unbind();
        self.seconds = state.getattr("seconds")?.unbind();
        self.microseconds = state.getattr("microseconds")?.unbind();
        let opt = |n: &str| -> PyResult<Option<PyObject>> {
            let v = state.getattr(n)?;
            Ok(if v.is_none() { None } else { Some(v.unbind()) })
        };
        self.year = opt("year")?;
        self.month = opt("month")?;
        self.day = opt("day")?;
        self.weekday = opt("weekday")?;
        self.hour = opt("hour")?;
        self.minute = opt("minute")?;
        self.second = opt("second")?;
        self.microsecond = opt("microsecond")?;
        Ok(())
    }

}

/// Python `+` on two stored numbers (int/float-aware, like the original).
fn add_objs(py: Python<'_>, a: &PyObject, b: &PyObject) -> PyResult<PyObject> {
    Ok(a.bind(py).add(b.bind(py))?.unbind())
}

fn sub_objs(py: Python<'_>, a: &PyObject, b: &PyObject) -> PyResult<PyObject> {
    Ok(a.bind(py).sub(b.bind(py))?.unbind())
}

/// `a + float` where `a` is int-or-float (for timedelta parts).
fn add_float(py: Python<'_>, a: &PyObject, b: f64) -> PyResult<PyObject> {
    Ok(a.bind(py).add(b.into_pyobject(py)?)?.unbind())
}

/// `a or b` on numbers (for `leapdays`).
fn or_objs(py: Python<'_>, a: &PyObject, b: &PyObject) -> PyResult<PyObject> {
    if a.bind(py).is_truthy()? {
        Ok(a.clone_ref(py))
    } else {
        Ok(b.clone_ref(py))
    }
}

fn abs_obj(py: Python<'_>, a: &PyObject) -> PyResult<PyObject> {
    Ok(abs_fn(py)?.call1((a.bind(py),))?.unbind())
}
