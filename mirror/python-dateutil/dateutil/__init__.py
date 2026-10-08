# SPDX-License-Identifier: Apache-2.0
# Provenance: rustsmith Stage-1 mirror of dateutil/dateutil (Apache-2.0).
# Source-tree import-package marker. The shippable package lives in
# `src/dateutil/` (maturin `python-source = "src"` builds the wheel from
# there, so this file never ships); this marker exists so the release
# layout carries the original import name at the tree top and so running
# from a source checkout resolves the same submodules. It defines no API
# itself: submodule imports fall through to `src/dateutil/` below.
import os as _os

_src = _os.path.normpath(
    _os.path.join(_os.path.dirname(_os.path.abspath(__file__)), "..", "src", "dateutil")
)
if _os.path.isdir(_src):
    __path__.append(_src)

del _os, _src

__all__ = ["easter", "parser", "relativedelta", "rrule", "tz", "utils", "zoneinfo"]


def __getattr__(name):
    import importlib

    if name in __all__:
        return importlib.import_module("." + name, __name__)
    raise AttributeError(
        "module {!r} has no attribute {!r}".format(__name__, name)
    )
