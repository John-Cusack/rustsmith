# This file is dual licensed under the terms of the Apache License, Version
# 2.0, and the BSD License. See the LICENSE file in the root of this repository
# for complete details.
"""Wheel tags (rustsmith Stage-1 mirror).

Tag computation lives in the compiled ``packaging._packaging`` extension.
This shim keeps the module data the suite patches directly
(``EXTENSION_SUFFIXES``, ``INTERPRETER_SHORT_NAMES``, ``_32_BIT_INTERPRETER``,
``logger``) plus the ``__all__`` contract; every helper resolves through
this module's attributes so the doubles apply.
"""

from __future__ import annotations

import logging
import struct
from collections.abc import Sequence
from importlib.machinery import EXTENSION_SUFFIXES

from packaging import _manylinux, _musllinux  # noqa: F401
from packaging._packaging import (
    InvalidTag,
    Tag,
    TooManyTagsError,
    UnsortedTagsError,
)
from packaging._packaging import tags_android_platforms as android_platforms
from packaging._packaging import tags_compatible_tags as compatible_tags
from packaging._packaging import tags_cpython_abis as _cpython_abis
from packaging._packaging import tags_cpython_tags as cpython_tags
from packaging._packaging import tags_create_selector as create_compatible_tags_selector
from packaging._packaging import tags_emscripten_platforms as _emscripten_platforms
from packaging._packaging import tags_generic_abi as _generic_abi
from packaging._packaging import tags_generic_platforms as _generic_platforms
from packaging._packaging import tags_generic_tags as generic_tags
from packaging._packaging import tags_get_config_var as _get_config_var
from packaging._packaging import tags_interpreter_abi as interpreter_abi
from packaging._packaging import tags_interpreter_name as interpreter_name
from packaging._packaging import tags_interpreter_version as interpreter_version
from packaging._packaging import tags_ios_platforms as ios_platforms
from packaging._packaging import tags_linux_platforms as _linux_platforms
from packaging._packaging import tags_mac_arch as _mac_arch
from packaging._packaging import tags_mac_binary_formats as _mac_binary_formats
from packaging._packaging import tags_mac_platforms as mac_platforms
from packaging._packaging import tags_parse_tag as parse_tag
from packaging._packaging import tags_platform_tags as platform_tags
from packaging._packaging import tags_pure_python_tags as pure_python_tags
from packaging._packaging import tags_sys_tags as sys_tags
from packaging._packaging import tags_version_nodot as _version_nodot

__all__ = [
    "INTERPRETER_SHORT_NAMES",
    "AppleVersion",
    "InvalidTag",
    "PythonVersion",
    "Tag",
    "TooManyTagsError",
    "UnsortedTagsError",
    "android_platforms",
    "compatible_tags",
    "cpython_tags",
    "create_compatible_tags_selector",
    "generic_tags",
    "interpreter_abi",
    "interpreter_name",
    "interpreter_version",
    "ios_platforms",
    "mac_platforms",
    "parse_tag",
    "platform_tags",
    "pure_python_tags",
    "sys_tags",
]


def __dir__() -> list[str]:
    return __all__


logger = logging.getLogger(__name__)

PythonVersion = Sequence[int]
"""
A sequence of integers describing a Python version, e.g. ``(3, 13)``.

.. versionadded:: 20.0
"""

AppleVersion = tuple[int, int]
"""
A ``(major, minor)`` integer pair describing an Apple OS version.

.. versionadded:: 24.2
"""

INTERPRETER_SHORT_NAMES: dict[str, str] = {
    "python": "py",  # Generic.
    "cpython": "cp",
    "pypy": "pp",
    "ironpython": "ip",
    "jython": "jy",
}


# This function can be unit tested without reloading the module
# (Unlike _32_BIT_INTERPRETER)
def _compute_32_bit_interpreter() -> bool:
    return struct.calcsize("P") == 4


_32_BIT_INTERPRETER = _compute_32_bit_interpreter()
