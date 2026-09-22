# SPDX-License-Identifier: MIT
# Provenance: rustsmith Stage-1 mirror of Ousret/charset_normalizer (MIT).
# Original: src/charset_normalizer/__init__.py — re-export shim over the Rust `_charset_normalizer` module.
# The original per-module files are deleted at merge; this shim plus the
# sys.modules aliases below keep every absolute (`charset_normalizer.x`) and
# relative (`from .x import`) import resolving without any `.py` file present.
import logging as _logging
import re as _re
import sys as _sys
import types as _types

from ._charset_normalizer import (
    CharsetMatch,
    CharsetMatches,
    CharInfo,
    characters_popularity_compare,
    filter_alt_coherence_matches,
    get_target_features,
    cp_similarity,
    is_accentuated,
    cli_detect,
    coherence_ratio,
    detect,
    encoding_languages,
    explain_handler,
    from_bytes,
    from_fp,
    from_path,
    iana_name,
    is_binary,
    is_multi_byte_encoding,
    is_suspiciously_successive_range,
    mb_encoding_languages,
    mess_ratio,
    query_yes_no,
    re_pattern,
    set_logging_handler,
    unicode_range,
    any_specified_encoding,
    char_info,
    iana_supported,
    too_big_sequence,
    too_small_sequence,
    trace_level,
    __version__,
)

VERSION = __version__.split(".")

__all__ = [
    "from_fp",
    "from_path",
    "from_bytes",
    "is_binary",
    "detect",
    "CharsetMatch",
    "CharsetMatches",
    "__version__",
    "VERSION",
    "set_logging_handler",
]

TOO_BIG_SEQUENCE = too_big_sequence()
TOO_SMALL_SEQUENCE = too_small_sequence()
TRACE = trace_level()
IANA_SUPPORTED = iana_supported()

RE_POSSIBLE_ENCODING_INDICATION = _re.compile(re_pattern(), _re.IGNORECASE)

_ns = globals()


def _mkproxy(name, attrs):
    proxy = _types.ModuleType("charset_normalizer." + name)
    for attr in attrs:
        proxy.__dict__[attr] = _ns[attr]
    _sys.modules["charset_normalizer." + name] = proxy
    _ns[name] = proxy


_mkproxy("api", ["from_bytes", "from_fp", "from_path", "is_binary", "explain_handler"])
_mkproxy("models", ["CharsetMatch", "CharsetMatches"])
_mkproxy(
    "md",
    ["mess_ratio", "char_info", "is_suspiciously_successive_range", "CharInfo"],
)
_sys.modules["charset_normalizer.md"].__dict__["_char_info"] = char_info
_sys.modules["charset_normalizer.md"].__dict__["_char_info"] = char_info
_mkproxy(
    "cd",
    [
        "coherence_ratio",
        "characters_popularity_compare",
        "filter_alt_coherence_matches",
        "get_target_features",
        "encoding_languages",
        "mb_encoding_languages",
        "is_multi_byte_encoding",
    ],
)
_mkproxy(
    "utils",
    [
        "iana_name",
        "is_multi_byte_encoding",
        "unicode_range",
        "any_specified_encoding",
        "set_logging_handler",
        "cp_similarity",
        "is_accentuated",
    ],
)
_mkproxy("legacy", ["detect"])
_mkproxy(
    "constant",
    [
        "TOO_BIG_SEQUENCE",
        "TOO_SMALL_SEQUENCE",
        "TRACE",
        "IANA_SUPPORTED",
        "RE_POSSIBLE_ENCODING_INDICATION",
    ],
)
_mkproxy("version", ["__version__", "VERSION"])
_mkproxy("cli", ["cli_detect", "query_yes_no"])

_logging.getLogger("charset_normalizer").addHandler(_logging.NullHandler())

del _ns, _mkproxy
