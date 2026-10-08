"""Main `MarkdownIt` class (rustsmith Stage-1 mirror).

The implementation lives in the compiled ``markdown_it._markdown_it``
extension (``markdown-it-rust-core`` crate plus this PyO3 binding). This
shim re-exports the public name, preserving ``markdown_it.main`` imports.
"""

from markdown_it._markdown_it import MarkdownIt

__all__ = ("MarkdownIt",)
