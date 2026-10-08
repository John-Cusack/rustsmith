"""Link normalization (rustsmith Stage-1 mirror: implementation in the extension)."""

from markdown_it._markdown_it import normalizeLink, normalizeLinkText, validateLink

RECODE_HOSTNAME_FOR = ("http:", "https:", "mailto:")

__all__ = ("normalizeLink", "normalizeLinkText", "validateLink")
