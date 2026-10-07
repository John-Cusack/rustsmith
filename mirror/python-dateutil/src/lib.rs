// SPDX-License-Identifier: Apache-2.0
// dateutil._dateutil: PyO3 binding over dateutil-core (rrule, relativedelta,
// tz engine). Behavior mirrors dateutil 2.9.0.post0; datetimes cross the
// boundary as Python objects so CPython semantics stay exact.

mod relativedelta_mod;
mod rrule_mod;
mod tz_mod;
mod util;

use pyo3::prelude::*;

/// Re-exported for the shims (mirrors `dateutil.relativedelta.weekdays`
/// lookup shape for integer weekday construction).
#[pymodule]
fn _dateutil(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<relativedelta_mod::Relativedelta>()?;
    m.add_class::<rrule_mod::Rrule>()?;
    m.add_class::<rrule_mod::Rruleset>()?;
    m.add_function(wrap_pyfunction!(rrule_mod::rrulestr, m)?)?;
    for (name, v) in [
        ("YEARLY", 0),
        ("MONTHLY", 1),
        ("WEEKLY", 2),
        ("DAILY", 3),
        ("HOURLY", 4),
        ("MINUTELY", 5),
        ("SECONDLY", 6),
    ] {
        m.add(name, v)?;
    }
    m.add_class::<tz_mod::TzUtc>()?;
    m.add_class::<tz_mod::TzOffset>()?;
    m.add_class::<tz_mod::TzLocal>()?;
    m.add_class::<tz_mod::TzFile>()?;
    m.add_class::<tz_mod::TtInfo>()?;
    m.add_class::<tz_mod::TzRange>()?;
    m.add_class::<tz_mod::TzStr>()?;
    m.add_function(wrap_pyfunction!(tz_mod::enfold, m)?)?;
    m.add_function(wrap_pyfunction!(tz_mod::datetime_ambiguous, m)?)?;
    m.add_function(wrap_pyfunction!(tz_mod::datetime_exists, m)?)?;
    m.add_function(wrap_pyfunction!(tz_mod::resolve_imaginary, m)?)?;
    // UTC singleton (the shim's _TzSingleton metaclass interns tzutc()
    // calls to this same object).
    let utc = tz_mod::TzUtc::new_singleton(py)?;
    m.add("UTC", utc)?;
    Ok(())
}
