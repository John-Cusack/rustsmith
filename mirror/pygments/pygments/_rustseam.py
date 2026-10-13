"""Shared guard for the compiled `pygments._pygments` seams.

Rust `String` cannot represent lone surrogates (`U+D800`–`U+DFFF`),
which Python `str` allows. The binding extracts via a lossy conversion,
so any text containing surrogates MUST take the pure-Python fallback —
otherwise the Rust path would silently replace them with `U+FFFD`.

Surrogate input is vanishingly rare (it cannot come from normal file
decodes; only from explicit construction or `surrogateescape`), but a
silent corruption is the worst failure mode, so every seam checks
[`rust_safe_text`] first. The check is C-speed: `str.isascii`
short-circuits pure-ASCII text (the common case) with no allocation;
only non-ASCII text pays for a trial `utf-8` encode.
"""

__all__ = ["rust_safe_text"]


def _encodable(text):
    try:
        text.encode("utf-8")
    except UnicodeEncodeError:
        return False
    return True


def rust_safe_text(text):
    """True when `text` has no lone surrogates (safe to send to Rust)."""
    return text.isascii() or _encodable(text)
