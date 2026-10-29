"""Equivalence: stock vs tier-1-patched arrow over a format/ISO corpus.

Compares parsed datetimes (or exception type+message) for each case.
Usage: run with stock venv python, then patched venv python, diff outputs.
"""

import arrow

CASES = [
    ("2021-01-02 03:04:05", "YYYY-MM-DD HH:mm:ss"),
    ("2021-01-02T03:04:05+00:00", None),
    ("2021-01-02 03:04:05.123456", "YYYY-MM-DD HH:mm:ss.S"),
    ("12/25/2021", "MM/DD/YYYY"),
    ("25-12-2021 15:30", "DD-MM-YYYY HH:mm"),
    ("2021-W52-6", "YYYY-WW-d"),
    ("Jan 5, 2021", "MMM D, YYYY"),
    ("January 5th, 2021", "MMMM Do, YYYY"),
    ("Monday, 04-Jul-2022", "dddd, DD-MMM-YYYY"),
    ("2021-01-02 03:04:05 PM", "YYYY-MM-DD HH:mm:ss A"),
    ("2021-01-02 03:04:05 am", "YYYY-MM-DD HH:mm:ss a"),
    ("2021-01-02T03:04:05.123+02:00", None),
    ("20210102", "YYYYMMDD"),
    ("03:04:05", "HH:mm:ss"),
    ("2021", "YYYY"),
    ("not a date", "YYYY-MM-DD"),
    ("2021-13-45", "YYYY-MM-DD"),
    ("2021-01-02", "YYYY/MM/DD"),
    ("2021-01-02 03:04:05", "BOGUS-FMT-XYZ"),
    ("  2021-01-02   03:04:05  ", "YYYY-MM-DD HH:mm:ss"),
]

for s, fmt in CASES:
    try:
        if fmt is None:
            r = arrow.get(s).datetime.isoformat()
        else:
            r = arrow.get(s, fmt).datetime.isoformat()
        print(f"OK   {s!r} {fmt!r} -> {r}")
    except Exception as e:  # noqa: BLE001 - comparing exception behavior
        print(f"ERR  {s!r} {fmt!r} -> {type(e).__name__}: {e}")
