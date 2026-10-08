"""Text utilities (rustsmith Stage-1 mirror: implementation in the extension)."""

from markdown_it._markdown_it import (
    escapeHtml,
    escapeRE,
    fromCodePoint,
    isLinkClose,
    isLinkOpen,
    isMdAsciiPunct,
    isPunctChar,
    isSpace,
    isStrSpace,
    isValidEntityCode,
    isWhiteSpace,
    mdTrim,
    normalizeReference,
    replaceEntityPattern,
    stripEscape,
    unescapeAll,
)

__all__ = (
    "escapeHtml",
    "escapeRE",
    "fromCodePoint",
    "isLinkClose",
    "isLinkOpen",
    "isMdAsciiPunct",
    "isPunctChar",
    "isSpace",
    "isStrSpace",
    "isValidEntityCode",
    "isWhiteSpace",
    "mdTrim",
    "normalizeReference",
    "replaceEntityPattern",
    "stripEscape",
    "unescapeAll",
)
