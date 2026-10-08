"""`Ruler` (rustsmith Stage-1 mirror).

The implementation lives in the compiled ``markdown_it._markdown_it``
extension. This shim re-exports the public names, preserving
``markdown_it.ruler`` imports.
"""

from markdown_it._markdown_it import Ruler

__all__ = ("Ruler",)
