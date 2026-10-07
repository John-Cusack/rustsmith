"""Manylinux platform tags (rustsmith Stage-1 mirror).

Detection logic lives in the compiled ``packaging._packaging`` extension;
this shim keeps the data tables, the ``_GLibCVersion`` tuple, the
``_parse_elf`` context manager, and the ``lru_cache`` wrappers — all three
are monkeypatched by the suite, so they stay Python-visible with identical
semantics.
"""

from __future__ import annotations

import collections
import contextlib
import functools
from typing import NamedTuple

from packaging._elffile import ELFFile
from packaging._packaging import (
    manylinux_confstr as _rust_confstr,
)
from packaging._packaging import (
    manylinux_ctypes as _rust_ctypes,
)
from packaging._packaging import (
    manylinux_get_glibc_version_uncached as _rust_get_glibc_version,
)
from packaging._packaging import (
    manylinux_get_module_uncached as _rust_get_manylinux_module,
)
from packaging._packaging import (
    manylinux_have_compatible_abi as _have_compatible_abi,
)
from packaging._packaging import (
    manylinux_is_armhf as _is_linux_armhf,
)
from packaging._packaging import (
    manylinux_is_compatible as _is_compatible,
)
from packaging._packaging import (
    manylinux_is_i686 as _is_linux_i686,
)
from packaging._packaging import (
    manylinux_parse_glibc_version as _parse_glibc_version,
)
from packaging._packaging import (
    manylinux_platform_tags as platform_tags,
)
from packaging._packaging import (
    manylinux_version_string as _glibc_version_string,
)

EF_ARM_ABIMASK = 0xFF000000
EF_ARM_ABI_VER5 = 0x05000000
EF_ARM_ABI_FLOAT_HARD = 0x00000400

_ALLOWED_ARCHS = {
    "x86_64",
    "aarch64",
    "ppc64",
    "ppc64le",
    "s390x",
    "loongarch64",
    "riscv64",
}


@contextlib.contextmanager
def _parse_elf(path):
    try:
        with open(path, "rb") as f:
            yield ELFFile(f)
    except (OSError, TypeError, ValueError):
        yield None


# If glibc ever changes its major version, we need to know what the last
# minor version was, so we can build the complete list of all versions.
# For now, guess what the highest minor version might be, assume it will
# be 50 for testing. Once this actually happens, update the dictionary
# with the actual value.
_LAST_GLIBC_MINOR: dict[int, int] = collections.defaultdict(lambda: 50)


class _GLibCVersion(NamedTuple):
    major: int
    minor: int


_glibc_version_string_confstr = _rust_confstr
_glibc_version_string_ctypes = _rust_ctypes

_get_glibc_version = functools.lru_cache(_rust_get_glibc_version)
_get_manylinux_module = functools.lru_cache(maxsize=1)(_rust_get_manylinux_module)

_LEGACY_MANYLINUX_MAP: dict[_GLibCVersion, str] = {
    # CentOS 7 w/ glibc 2.17 (PEP 599)
    _GLibCVersion(2, 17): "manylinux2014",
    # CentOS 6 w/ glibc 2.12 (PEP 571)
    _GLibCVersion(2, 12): "manylinux2010",
    # CentOS 5 w/ glibc 2.5 (PEP 513)
    _GLibCVersion(2, 5): "manylinux1",
}
