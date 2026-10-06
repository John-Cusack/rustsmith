# This file is dual licensed under the terms of the Apache License, Version
# 2.0, and the BSD License. See the LICENSE file in the root of this repository
# for complete details.
"""PEP 440 version parsing (rustsmith Stage-1 mirror).

The implementation lives in the compiled ``packaging._packaging`` extension
(``packaging-rust-core`` crate, ``version`` module). This shim re-exports the
public names and keeps the pure-Python glue that is version-dependent or
introspection-shaped: ``_BaseVersion``, the ``_Version`` backward-compat
tuple, ``_VersionReplace``, ``_TrimmedRelease``, and the ``VERSION_PATTERN``
constant (possessive/non-possessive selection identical to the original).
"""

from __future__ import annotations

import sys
import typing
from collections.abc import Callable
from typing import Any, Literal, NamedTuple, TypedDict

from packaging._packaging import InvalidVersion, Version, normalize_pre, parse

if typing.TYPE_CHECKING:
    from typing_extensions import Self, Unpack

__all__ = ["VERSION_PATTERN", "InvalidVersion", "Version", "normalize_pre", "parse"]


def __dir__() -> list[str]:
    return __all__


LocalType = tuple[int | str, ...]

CmpLocalType = tuple[tuple[int, str], ...]
CmpSuffix = tuple[int, int, int, int, int, int]
CmpKey = (
    tuple[int, tuple[int, ...], CmpSuffix]
    | tuple[int, tuple[int, ...], CmpSuffix, CmpLocalType]
)
VersionComparisonMethod = Callable[[CmpKey, CmpKey], bool]


class _VersionReplace(TypedDict, total=False):
    epoch: int | None
    release: tuple[int, ...] | None
    pre: tuple[str, int] | None
    post: int | None
    dev: int | None
    local: str | None


class _BaseVersion:
    __slots__ = ()

    # This can also be a normal member (see the packaging_legacy package);
    # we are just requiring it to be readable. Actually defining a property
    # has runtime effect on subclasses, so it's typing only.
    if typing.TYPE_CHECKING:

        @property
        def _key(self) -> tuple[Any, ...]: ...

    def __hash__(self) -> int:
        return hash(self._key)  # type: ignore[attr-defined]

    # Please keep the duplicated `isinstance` check
    # in the six comparisons hereunder
    # unless you find a way to avoid adding overhead function calls.
    def __lt__(self, other: _BaseVersion) -> bool:
        if not isinstance(other, _BaseVersion):
            return NotImplemented

        return self._key < other._key  # type: ignore[attr-defined]

    def __le__(self, other: _BaseVersion) -> bool:
        if not isinstance(other, _BaseVersion):
            return NotImplemented

        return self._key <= other._key  # type: ignore[attr-defined]

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, _BaseVersion):
            return NotImplemented

        return self._key == other._key  # type: ignore[attr-defined]

    def __ge__(self, other: _BaseVersion) -> bool:
        if not isinstance(other, _BaseVersion):
            return NotImplemented

        return self._key >= other._key  # type: ignore[attr-defined]

    def __gt__(self, other: _BaseVersion) -> bool:
        if not isinstance(other, _BaseVersion):
            return NotImplemented

        return self._key > other._key  # type: ignore[attr-defined]

    def __ne__(self, other: object) -> bool:
        if not isinstance(other, _BaseVersion):
            return NotImplemented

        return self._key != other._key  # type: ignore[attr-defined]


# Deliberately not anchored to the start and end of the string, to make it
# easier for 3rd party code to reuse

# Note that ++ doesn't behave identically on CPython and PyPy, so not using it here
_VERSION_PATTERN = r"""
    v?+                                                   # optional leading v
    (?a:
        (?:(?P<epoch>[0-9]+)!)?+                          # epoch
        (?P<release>[0-9]+(?:\.[0-9]+)*+)                 # release segment
        (?P<pre>                                          # pre-release
            [._-]?+
            (?P<pre_l>alpha|a|beta|b|preview|pre|c|rc)
            [._-]?+
            (?P<pre_n>[0-9]+)?
        )?+
        (?P<post>                                         # post release
            (?:-(?P<post_n1>[0-9]+))
            |
            (?:
                [._-]?
                (?P<post_l>post|rev|r)
                [._-]?
                (?P<post_n2>[0-9]+)?
            )
        )?+
        (?P<dev>                                          # dev release
            [._-]?+
            (?P<dev_l>dev)
            [._-]?+
            (?P<dev_n>[0-9]+)?
        )?+
    )
    (?a:\+
        (?P<local>                                        # local version
            [a-z0-9]+
            (?:[._-][a-z0-9]+)*+
        )
    )?+
"""

_VERSION_PATTERN_OLD = _VERSION_PATTERN.replace("*+", "*").replace("?+", "?")

# Possessive qualifiers were added in Python 3.11.
# CPython 3.11.0-3.11.4 had a bug: https://github.com/python/cpython/pull/107795
# Older PyPy also had a bug.
VERSION_PATTERN = (
    _VERSION_PATTERN_OLD
    if (sys.implementation.name == "cpython" and sys.version_info < (3, 11, 5))
    or (sys.implementation.name == "pypy" and sys.version_info < (3, 11, 13))
    or sys.version_info < (3, 11)
    else _VERSION_PATTERN
)
"""
A string containing the regular expression used to match a valid version.

The pattern is not anchored at either end, and is intended for embedding in larger
expressions (for example, matching a version number as part of a file name). The
regular expression should be compiled with the ``re.VERBOSE`` and ``re.IGNORECASE``
flags set.

.. versionchanged:: 26.0

   The regex now uses possessive qualifiers on Python 3.11 if they are
   supported (CPython 3.11.5+, PyPy 3.11.13+).

:meta hide-value:
"""


# Backward compatibility for internals before 26.0. Do not use.
class _Version(NamedTuple):
    epoch: int
    release: tuple[int, ...]
    dev: tuple[Literal["dev"], int] | None
    pre: tuple[Literal["a", "b", "rc"], int] | None
    post: tuple[Literal["post"], int] | None
    local: LocalType | None


class _TrimmedRelease(Version):
    __slots__ = ()

    @property
    def release(self) -> tuple[int, ...]:
        """
        Release segment without any trailing zeros.

        >>> _TrimmedRelease('1.0.0').release
        (1,)
        >>> _TrimmedRelease('0.0').release
        (0,)
        """
        # This leaves one 0.
        rel = super().release
        len_release = len(rel)
        i = len_release
        while i > 1 and rel[i - 1] == 0:
            i -= 1
        return rel if i == len_release else rel[:i]
