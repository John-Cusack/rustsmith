# MIT License
#
# Copyright (c) 2018 luozhouyang
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.
# SPDX-License-Identifier: MIT
# Provenance: rustsmith Stage-1 mirror of luozhouyang/python-string-similarity (MIT).
# Original: strsimpy/__init__.py — re-export shim over the Rust `_strsimpy` module.
# The original per-module files are deleted at merge; this shim plus the
# sys.modules aliases below keep every absolute (`strsimpy.x`) and relative
# (`from .x import`) import resolving without any `.py` file present.
import sys as _sys
import types as _types

from ._strsimpy import (
    Cosine,
    Damerau,
    Jaccard,
    JaroWinkler,
    Levenshtein,
    LongestCommonSubsequence,
    MetricLCS,
    MetricStringDistance,
    NGram,
    NormalizedLevenshtein,
    NormalizedStringDistance,
    NormalizedStringSimilarity,
    OptimalStringAlignment,
    OverlapCoefficient,
    QGram,
    ShingleBased,
    SIFT4,
    SIFT4Options,
    SorensenDice,
    StringDistance,
    StringSimilarity,
    WeightedLevenshtein,
)

__all__ = [
    "Cosine",
    "Damerau",
    "Jaccard",
    "JaroWinkler",
    "Levenshtein",
    "LongestCommonSubsequence",
    "MetricLCS",
    "MetricStringDistance",
    "NGram",
    "NormalizedLevenshtein",
    "NormalizedStringDistance",
    "NormalizedStringSimilarity",
    "OptimalStringAlignment",
    "OverlapCoefficient",
    "QGram",
    "ShingleBased",
    "SIFT4",
    "SIFT4Options",
    "SorensenDice",
    "StringDistance",
    "StringSimilarity",
    "WeightedLevenshtein",
]

_MODULE_ATTRS = {
    "cosine": ["Cosine"],
    "damerau": ["Damerau"],
    "jaccard": ["Jaccard"],
    "jaro_winkler": ["JaroWinkler"],
    "levenshtein": ["Levenshtein"],
    "longest_common_subsequence": ["LongestCommonSubsequence"],
    "metric_lcs": ["MetricLCS"],
    "ngram": ["NGram"],
    "normalized_levenshtein": ["NormalizedLevenshtein"],
    "optimal_string_alignment": ["OptimalStringAlignment"],
    "overlap_coefficient": ["OverlapCoefficient"],
    "qgram": ["QGram"],
    "shingle_based": ["ShingleBased"],
    "sift4": ["SIFT4", "SIFT4Options"],
    "sorensen_dice": ["SorensenDice"],
    "string_distance": ["StringDistance", "NormalizedStringDistance", "MetricStringDistance"],
    "string_similarity": ["StringSimilarity", "NormalizedStringSimilarity"],
    "weighted_levenshtein": ["WeightedLevenshtein"],
}

_ns = globals()
for _mod, _names in _MODULE_ATTRS.items():
    _proxy = _types.ModuleType(f"strsimpy.{_mod}")
    for _n in _names:
        setattr(_proxy, _n, _ns[_n])
    _sys.modules[f"strsimpy.{_mod}"] = _proxy
    _ns[_mod] = _proxy
del _ns, _mod, _names, _n, _proxy

__name__ = 'strsimpy'
__version__ = '0.2.1'
