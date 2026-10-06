"""
ELF file parser (rustsmith Stage-1 mirror).

``ELFFile`` is implemented in the compiled ``packaging._packaging`` extension
(``packaging-rust-core`` crate, ``elf`` module); the enums are pure-Python
``IntEnum``s kept verbatim since tests compare parsed integer fields against
their members.
"""

from __future__ import annotations

import enum

from packaging._packaging import ELFFile, ELFInvalid


class EIClass(enum.IntEnum):
    C32 = 1
    C64 = 2


class EIData(enum.IntEnum):
    Lsb = 1
    Msb = 2


class EMachine(enum.IntEnum):
    I386 = 3
    S390 = 22
    Arm = 40
    X8664 = 62
    AArch64 = 183
