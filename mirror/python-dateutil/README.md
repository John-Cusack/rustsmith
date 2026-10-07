# python-dateutil-rust

Pure-Rust port of the `python-dateutil` engine (`rrule`, `relativedelta`,
`tz`) with Python bindings preserving the `dateutil` interface.
rustsmith Stage-1 mirror of [dateutil/dateutil](https://github.com/dateutil/dateutil)
(Apache-2.0), scoped to the recurrence/calendar/timezone engine —
the fuzzy `parser` module stays Python.

Layout: reusable core crate (`dateutil-core`, no Python dependency) plus a
thin PyO3 binding (`dateutil._dateutil`); `src/dateutil/{rrule,relativedelta,tz/tz}.py`
are re-export shims over the extension.
