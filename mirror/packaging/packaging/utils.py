# This file is dual licensed under the terms of the Apache License, Version
# 2.0, and the BSD License. See the LICENSE file in the root of this repository
# for complete details.
"""Name/version/filename utilities (rustsmith Stage-1 mirror).

Implemented in the compiled ``packaging._packaging`` extension; this shim
re-exports the public names. ``NormalizedName``/``BuildTag`` are typing-only
aliases kept here since annotations live in Python.
"""

from __future__ import annotations

from typing import NewType

from packaging._packaging import (
    InvalidName,
    InvalidSdistFilename,
    InvalidWheelFilename,
)
from packaging._packaging import utils_canonicalize_name as canonicalize_name
from packaging._packaging import utils_canonicalize_version as canonicalize_version
from packaging._packaging import utils_is_normalized_name as is_normalized_name
from packaging._packaging import utils_parse_sdist_filename as parse_sdist_filename
from packaging._packaging import utils_parse_wheel_filename as parse_wheel_filename
# Re-exported (as in the original) for namespace compatibility.
from packaging.tags import InvalidTag, Tag, UnsortedTagsError, parse_tag
from packaging.version import InvalidVersion, Version, _TrimmedRelease

__all__ = [
    "BuildTag",
    "InvalidName",
    "InvalidSdistFilename",
    "InvalidWheelFilename",
    "NormalizedName",
    "canonicalize_name",
    "canonicalize_version",
    "is_normalized_name",
    "parse_sdist_filename",
    "parse_wheel_filename",
]


def __dir__() -> list[str]:
    return __all__


BuildTag = tuple[()] | tuple[int, str]
"""
A wheel build tag: an empty tuple, or a ``(build number, build tag suffix)`` pair.

.. versionadded:: 20.9
"""

NormalizedName = NewType("NormalizedName", str)
"""
A :class:`typing.NewType` of :class:`str`, representing a normalized name.

.. versionadded:: 20.4
"""
