# This file is dual licensed under the terms of the Apache License, Version
# 2.0, and the BSD License. See the LICENSE file in the root of this repository
# for complete details.
"""Public ``VersionRange`` API (rustsmith Stage-1 mirror).

Implemented in the compiled ``packaging._packaging`` extension
(``packaging-rust-core`` crate, ``ranges`` module).
"""

from __future__ import annotations

from packaging._packaging import VersionRange

__all__ = ["VersionRange"]


def __dir__() -> list[str]:
    return __all__


#: The most ``!=`` exclusion fragments (``!=V`` points or ``!=P.*`` prefixes)
#: that :meth:`VersionRange.to_specifier_set` will materialize to spell a
#: single gap or run.
_MAX_EXCLUSION_RUN = 128
