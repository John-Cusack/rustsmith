# ADR-038: pygments mirror boundary (scan-kernel delegation)

Date: 2026-10-13. Status: accepted. Scope: `mirror/pygments` only.

## Context

Pygments 2.19.1 is ~124.6K lines of pure Python across 337 files
(500+ lexer definitions, 15 formatters, plugin entry points). The hot
path per highlight call: `Lexer._preprocess_lexer_input` (text
normalization) → `RegexLexer.get_tokens_unprocessed` (rule dispatch
over live `re` patterns with Python callbacks) → formatter escaping
(`HtmlFormatter._translate_parts`, cached).

## Decision

Scan-kernel delegation with pure-Python fallback (pyparsing seam
pattern), four kernels in `pygments-rust-core`:

1. `escape_html_text` — per-token HTML escaping (html + svg).
2. `normalize_newlines` — `\r\n`/`\r` → `\n`.
3. `expand_tabs_and_ensure` — tab expansion + trailing newline.
4. `regex_opt` — keyword-list regex builder (import-time path).

Kept in Python verbatim: `RegexLexer` dispatch (rule tables are live
`re` patterns with callbacks like `bygroups`/`using`; re-implementing
the dispatcher in Rust would call back into the interpreter per rule
and cannot beat the C `re` engine — measured W2 lex-only delta is
1.00x, confirming the dispatcher is not the bottleneck), token types,
styles, all lexer definitions, `guess_lexer*`, filters, cmdline.

## Negative boundaries (measured rationale, not taste)

- **Full `RegexLexer` engine in Rust**: rejected. The per-position
  loop calls Python callables (token callbacks, `using` sub-lexers)
  and matches `re.Pattern` objects owned by user subclasses; a Rust
  engine would pay FFI + GIL per rule attempt. Bench W2 (lex-only,
  110KB Python): 1.00x — the `re` engine dominates, so there is no
  headroom to chase.
- **`str.strip` in `preprocess`**: stays in Python. CPython's strip
  set (`Py_UNICODE_ISSPACE`) is version-dependent; re-deriving it in
  Rust risks edge divergence for zero measurable gain (strip touches
  only string edges).
- **Custom lexers/formatters + entry-point plugins**: documented
  non-port surface. They execute as Python against the unchanged
  `Lexer`/`Formatter` API; the seams are additive (try/except import
  + `None`-check), so third-party plugins see zero interface change.
- **Lone surrogates**: routed to the fallback by `pygments/_rustseam.py`
  (`test_surrogate_input_uses_fallback`). Rust `String` cannot hold
  them; silent `U+FFFD` corruption is worse than no speedup.
- **`re.escape` non-ASCII**: ported exactly (ASCII specials only) after
  the differential caught an over-escaping bug during development —
  evidence the kernel tests earn their keep.

## Consequences

Parity: upstream suite 5175 passed / 16 skipped / 0 failed, identical
on the Rust and fallback paths. Bench (110KB Python → HTML):
pipeline 1.05x, lex-only 1.00x, escape-heavy 0.99x (wash —
`_translate_parts` lru_cache amortizes escaping), `regex_opt`
9.8x (import-time keyword builder). Memory: peak RSS and
Python-alloc peak unchanged within noise.
