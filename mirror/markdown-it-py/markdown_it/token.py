"""`Token` (rustsmith Stage-1 mirror).

The implementation lives in the compiled ``markdown_it._markdown_it``
extension. This shim re-exports the public name, preserving
``markdown_it.token`` imports.
"""

from markdown_it._markdown_it import Token

__all__ = ("Token",)
