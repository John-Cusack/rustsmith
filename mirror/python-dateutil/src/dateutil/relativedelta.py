# SPDX-License-Identifier: Apache-2.0
# Provenance: rustsmith Stage-1 mirror of dateutil/dateutil (Apache-2.0).
# Original: src/dateutil/relativedelta.py — re-export shim over the Rust
# `dateutil._dateutil.relativedelta` engine. The weekday class stays
# Python (`dateutil._common.weekday`), so duck-typed weekday behavior and
# `MO`..`SU` identity are unchanged.
from ._common import weekday
from ._dateutil import relativedelta

MO, TU, WE, TH, FR, SA, SU = weekdays = tuple(weekday(x) for x in range(7))

__all__ = ["relativedelta", "MO", "TU", "WE", "TH", "FR", "SA", "SU"]
