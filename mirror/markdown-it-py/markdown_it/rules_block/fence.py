"""`fence` rule + `make_fence_rule` factory (rustsmith Stage-1 mirror)."""

from markdown_it._markdown_it import block_fence as fence
from markdown_it._markdown_it import make_fence_rule

__all__ = ("fence", "make_fence_rule")
