"""`strikethrough` rule (rustsmith Stage-1 mirror: implementation in the extension)."""

from markdown_it._markdown_it import inline_strikethrough as tokenize
from markdown_it._markdown_it import inline2_strikethrough as postProcess

__all__ = ("postProcess", "tokenize")
