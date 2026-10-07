# SPDX-License-Identifier: Apache-2.0
# Provenance: rustsmith Stage-1 mirror of dateutil/dateutil (Apache-2.0).
# Original: src/dateutil/parser/isoparser.py — re-export shim over the Rust
# `dateutil._dateutil` ISO engine. Strict ISO parsing (already the fastest
# path) now runs in `dateutil-core`; the public names are unchanged.
"""This module offers a parser for ISO-8601 strings.

It is intended to support all valid date, time and datetime formats per the
ISO-8601 specification.

..versionadded:: 2.7.0
"""
from dateutil._dateutil import DEFAULT_ISOPARSER, isoparse, isoparser

__all__ = ["isoparse", "isoparser"]
