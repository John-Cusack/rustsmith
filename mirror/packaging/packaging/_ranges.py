# This file is dual licensed under the terms of the Apache License, Version
# 2.0, and the BSD License. See the LICENSE file in the root of this repository
# for complete details.
"""Private version-range helpers (rustsmith Stage-1 mirror).

The interval engine lives in the compiled ``packaging._packaging``
extension (``packaging-rust-core`` crate, ``ranges`` module); this shim
re-exports the names the suite and the public ``ranges`` module use.
``NEG_INF``/``POS_INF`` are built here so they are the same kind of
objects the original module-level singletons are.
"""

from __future__ import annotations

import enum

from packaging._packaging import BoundaryVersion, LowerBound, UpperBound

__all__ = [
    "BoundaryKind",
    "BoundaryVersion",
    "LowerBound",
    "UpperBound",
]


class BoundaryKind(enum.Enum):
    """Where a boundary marker sits in the version ordering."""

    AFTER_LOCALS = enum.auto()  # after V+local, before V.post0
    AFTER_POSTS = enum.auto()  # after V.postN, before next release


NEG_INF = LowerBound(None, False)
POS_INF = UpperBound(None, False)
FULL_RANGE = ((NEG_INF, POS_INF),)
