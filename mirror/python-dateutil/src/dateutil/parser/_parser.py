# SPDX-License-Identifier: Apache-2.0
# Provenance: rustsmith Stage-1 mirror of dateutil/dateutil (Apache-2.0).
# Original: src/dateutil/parser/_parser.py — the hot path (`_timelex`,
# default `parserinfo` tables, `_ymd`, `parser._parse`,
# `_parse_numeric_token`) now runs in the Rust `dateutil-core` crate via
# `dateutil._dateutil`; this module re-exports that surface.
#
# `_parser_py.py` alongside is the verbatim upstream module. It stays for
# two reasons: (a) lineage — the kept private pieces (`_tzparser`,
# `_parsetz`, `_resultbase`, `_result`, `_ymd`, `ParserError`,
# `UnknownTimezoneWarning`) import from there, so a future upstream sync
# is a plain re-copy plus diff; (b) the `tzstr` engine path
# (`tz.tzstr -> parser._parsetz`) keeps its exact objects.
"""
This module offers a generic date/time string parser which is able to parse
most known formats to represent a date and/or time.

This module attempts to be forgiving with regards to unlikely input formats,
returning a datetime object even for dates which are ambiguous. If an element
of a date/time stamp is omitted, the following rules are applied:

- If AM or PM is left unspecified, a 24-hour clock is assumed, however, an hour
  on a 12-hour clock (``0 <= hour <= 12``) *must* be specified if AM or PM is
  specified.
- If a time zone is omitted, a timezone-naive datetime is returned.

If any other elements are missing, they are taken from the
:class:`datetime.datetime` object passed to the parameter ``default``. If this
results in a day number exceeding the valid number of days per month, the
value falls back to the end of the month.

Additional resources about date/time string formats can be found below:

- `A summary of the international standard date and time notation
  <https://www.cl.cam.ac.uk/~mgk25/iso-time.html>`_
- `W3C Date and Time Formats <https://www.w3.org/TR/NOTE-datetime>`_
- `Time Formats (Planetary Rings Node) <https://pds-rings.seti.org:443/tools/time_formats.html>`_
- `CPAN ParseDate module
  <https://metacpan.org/pod/release/MUIR/Time-modules-2013.0912/lib/Time/ParseDate.pm>`_
- `Java SimpleDateFormat Class
  <https://docs.oracle.com/javase/6/docs/api/java/text/SimpleDateFormat.html>`_
"""
from dateutil._dateutil import (
    DEFAULTPARSER,
    _timelex,
    parse,
    parser,
    parserinfo,
)

from ._parser_py import (
    DEFAULTTZPARSER,
    ParserError,
    UnknownTimezoneWarning,
    _parsetz,
    _resultbase,
    _tzparser,
    _ymd,
)


class _result(_resultbase):
    __slots__ = ["year", "month", "day", "weekday",
                 "hour", "minute", "second", "microsecond",
                 "tzname", "tzoffset", "ampm", "any_unused_tokens"]

