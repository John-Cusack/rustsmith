"""Options and fixture helpers (rustsmith Stage-1 mirror).

`OptionsDict` lives in the compiled ``markdown_it._markdown_it``
extension; `read_fixture_file` is verbatim from the original (test-fixture
IO glue with no hot path). The `OptionsType`/`PresetType` aliases are
preserved for typing imports.
"""

from __future__ import annotations

from collections.abc import Callable
from pathlib import Path
from typing import Any, TypedDict

from typing_extensions import NotRequired

from markdown_it._markdown_it import OptionsDict


class OptionsType(TypedDict):
    """Options for parsing."""

    maxNesting: int
    """Internal protection, recursion limit."""
    html: bool
    """Enable HTML tags in source."""
    linkify: bool
    """Enable autoconversion of URL-like texts to links."""
    typographer: bool
    """Enable smartquotes and replacements."""
    quotes: str
    """Quote characters."""
    xhtmlOut: bool
    """Use '/' to close single tags (<br />)."""
    breaks: bool
    """Convert newlines in paragraphs into <br>."""
    langPrefix: str
    """CSS language prefix for fenced blocks."""
    highlight: Callable[[str, str, str], str] | None
    """Highlighter function: (content, langName, langAttrs) -> str."""
    store_labels: NotRequired[bool]
    """Store link label in link/image token's metadata (under Token.meta['label'])."""
    tasklists: NotRequired[bool]
    """Enable GFM task list checkbox detection in list items."""
    alerts: NotRequired[bool]
    """Enable GitHub-style alert detection in blockquotes."""
    tasklists_editable: NotRequired[bool]
    """When True, rendered task list checkboxes are interactive (no disabled attribute)."""
    strikethrough_single_tilde: NotRequired[bool]
    """Allow single tilde ``~text~`` for strikethrough in addition to double."""


class PresetType(TypedDict):
    """Preset configuration for markdown-it."""

    options: OptionsType
    """Options for parsing."""
    components: dict[str, dict[str, list[str]]]
    """Components for parsing and rendering."""


EnvType = dict[str, Any]
"""Type for the environment sandbox used in parsing and rendering."""


def read_fixture_file(path: str | Path) -> list[list[Any]]:
    text = Path(path).read_text(encoding="utf-8")
    tests = []
    section = 0
    last_pos = 0
    lines = text.splitlines(keepends=True)
    for i in range(len(lines)):
        if lines[i].rstrip() == ".":
            if section == 0:
                tests.append([i, lines[i - 1].strip()])
                section = 1
            elif section == 1:
                tests[-1].append("".join(lines[last_pos + 1 : i]))
                section = 2
            elif section == 2:
                tests[-1].append("".join(lines[last_pos + 1 : i]))
                section = 0

            last_pos = i
    return tests
