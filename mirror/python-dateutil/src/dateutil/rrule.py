# SPDX-License-Identifier: Apache-2.0
# Provenance: rustsmith Stage-1 mirror of dateutil/dateutil (Apache-2.0).
# Original: src/dateutil/rrule.py — re-export shim over the Rust
# `dateutil._dateutil` engine. The `weekday` class stays Python
# (subclassing `dateutil._common.weekday` with the `n == 0` guard), so
# weekday identity, equality, and repr are unchanged.
from ._common import weekday as weekdaybase
from ._dateutil import (
    DAILY,
    HOURLY,
    MINUTELY,
    MONTHLY,
    SECONDLY,
    WEEKLY,
    YEARLY,
    rrule,
    rruleset,
    rrulestr,
)

__all__ = ["rrule", "rruleset", "rrulestr",
           "YEARLY", "MONTHLY", "WEEKLY", "DAILY",
           "HOURLY", "MINUTELY", "SECONDLY",
           "MO", "TU", "WE", "TH", "FR", "SA", "SU"]


class weekday(weekdaybase):
    """
    This version of weekday does not allow n = 0.
    """
    def __init__(self, wkday, n=None):
        if n == 0:
            raise ValueError("Can't create weekday with n==0")

        super(weekday, self).__init__(wkday, n)


MO, TU, WE, TH, FR, SA, SU = weekdays = tuple(weekday(x) for x in range(7))
