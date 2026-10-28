# rs-port-sqlparse upstream-review follow-up

Date: 2026-10-28. Verdict: **no upstream action**.

The port is a behavior-identical drop-in mirror (public API unchanged,
pure-Python fallback byte-identical for all non-kernel paths). It proposes
no changes to andialbrecht/sqlparse and needs none: there is no vendored
hot loop to offer back as an opt-in core (the win is the whole-unit
Rust port, not a separable Python-level fast path), and no upstream bug
was found in the process (one stale xfail, `test_issue484`, xpasses
identically on pure upstream — not ours to fix unasked).

If upstream ever wants a speed core, the natural seam is the default
lexer configuration (regex table + keyword maps, both data); the port
kept `tokens.py`/`keywords.py` byte-identical precisely so such a seam
would stay stable. No PR filed.
