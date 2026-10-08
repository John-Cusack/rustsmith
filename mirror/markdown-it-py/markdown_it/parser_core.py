"""Parser engine shims (rustsmith Stage-1 mirror).

Implementations live in the compiled ``markdown_it._markdown_it``
extension. These shims re-export the public names, preserving
``markdown_it.parser_*`` imports.
"""

from markdown_it._markdown_it import ParserCore

__all__ = ("ParserCore",)
