"""PEP 656 support (rustsmith Stage-1 mirror).

Detection logic lives in the compiled ``packaging._packaging`` extension;
this shim keeps the ``_MuslVersion`` tuple and the ``lru_cache`` wrapper,
both of which the suite touches directly.
"""

from __future__ import annotations
import functools
import subprocess  # noqa: F401  (re-exported for the suite's test doubles)
from typing import NamedTuple
from packaging._packaging import (
    musllinux_get_version_uncached as _rust_get_musl_version,
)
from packaging._packaging import (
    musllinux_parse_version as _parse_musl_version,
)
from packaging._packaging import (
    musllinux_platform_tags as platform_tags,
)


class _MuslVersion(NamedTuple):
    major: int
    minor: int


_get_musl_version = functools.lru_cache(_rust_get_musl_version)


if __name__ == "__main__":  # pragma: no cover
    import re
    import sysconfig

    plat = sysconfig.get_platform()
    assert plat.startswith("linux-"), "not linux"

    print("plat:", plat)
    print("musl:", _get_musl_version(__import__("sys").executable))
    print("tags:", end=" ")
    for t in platform_tags([re.sub(r"[.-]", "_", plat.split("-", 1)[-1])]):
        print(t, end="\n      ")
