"""Parse link label (rustsmith Stage-1 mirror).

``parseLinkLabel`` needs a live inline state, so it stays a thin Python
wrapper over the extension's ``ParserInline.skipToken``.
"""

from __future__ import annotations

from markdown_it._markdown_it import StateInline

__all__ = ("parseLinkLabel",)


def parseLinkLabel(state: StateInline, start: int, disableNested: bool = False) -> int:
    labelEnd = -1
    oldPos = state.pos
    found = False

    state.pos = start + 1
    level = 1

    while state.pos < state.posMax:
        marker = state.src[state.pos]
        if marker == "]":
            level -= 1
            if level == 0:
                found = True
                break

        prevPos = state.pos
        state.md.inline.skipToken(state)
        if marker == "[":
            if prevPos == state.pos - 1:
                level += 1
            elif disableNested:
                state.pos = oldPos
                return -1
    if found:
        labelEnd = state.pos

    state.pos = oldPos

    return labelEnd
