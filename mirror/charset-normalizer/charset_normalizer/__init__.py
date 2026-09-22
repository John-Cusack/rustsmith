# SPDX-License-Identifier: MIT
# Provenance: rustsmith Stage-1 mirror of Ousret/charset_normalizer (MIT).
# Original: src/charset_normalizer/__init__.py — re-export shim over the Rust `_charset_normalizer` module.
# The original per-module files are deleted at merge; this shim plus the
# sys.modules aliases below keep every absolute (`charset_normalizer.x`) and
# relative (`from .x import`) import resolving without any `.py` file present.
import sys as _sys
import types as _types

from ._charset_normalizer import (
    CharsetMatch,
    coherence_ratio,
    detect,
    from_bytes,
    from_fp,
    from_path,
    is_binary,
    mess_ratio,
)

__all__ = [
    "from_fp",
    "from_path",
    "from_bytes",
    "is_binary",
    "detect",
    "CharsetMatch",
    "mess_ratio",
    "coherence_ratio",
]

_MODULE_ATTRS = {
    "api": ["from_bytes", "from_path", "from_fp", "is_binary"],
    "models": ["CharsetMatch"],
    "md": ["mess_ratio"],
    "cd": ["coherence_ratio"],
    "legacy": ["detect"],
    "utils": [],
    "constant": [],
    "version": [],
    "cli": [],
}

_ns = globals()
for _mod, _names in _MODULE_ATTRS.items():
    _proxy = _types.ModuleType(f"charset_normalizer.{_mod}")
    for _n in _names:
        setattr(_proxy, _n, _ns[_n])
    _sys.modules[f"charset_normalizer.{_mod}"] = _proxy
    _ns[_mod] = _proxy
del _ns, _mod, _names

__version__ = "3.5.1"
VERSION = __version__.split(".")
