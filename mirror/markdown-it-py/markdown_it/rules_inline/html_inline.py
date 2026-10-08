"""`html_inline` rule (rustsmith Stage-1 mirror).

`html_inline` itself lives in the extension, but `HTML_TAG_RE` stays a
live module attribute (verbatim import): the rule consults it through the
host on every call, so monkeypatching applies.
"""

from markdown_it._markdown_it import inline_html_inline as html_inline
from markdown_it.common.html_re import HTML_TAG_RE

__all__ = ("HTML_TAG_RE", "html_inline")
