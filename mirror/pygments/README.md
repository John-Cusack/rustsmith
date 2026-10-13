# pygments-rust

Stage-1 Rust mirror of [pygments/pygments](https://github.com/pygments/pygments)
(`Pygments` 2.19.1, BSD-2-Clause): syntax highlighting for 500+ languages.

## Boundary

The pure-text hot transforms of the highlight path live in the reusable
`pygments-rust-core` crate (no Python dependency) with a thin PyO3 binding
(`pygments._pygments`). Everything else is the upstream source verbatim:

- ported to Rust: `escape_html` (`formatters/html.py`, per-token escaping
  in `HtmlFormatter._format_lines` via `_translate_parts`;
  `formatters/svg.py`, same mapping), newline normalization +
  tab-expansion + trailing-newline guarantee (the `str` tail of
  `Lexer._preprocess_lexer_input`; BOM/decoding and `strip` stay in
  Python), `regex_opt` (`regexopt.py`, keyword-list regex builder);
- kept in Python: `RegexLexer` dispatch (`get_tokens_unprocessed`,
  rule tables are live `re` patterns with Python callbacks such as
  `bygroups`/`using` — calling back into the interpreter from Rust
  would not beat the C `re` engine), token types, styles, all 500+
  lexer definitions, `guess_lexer*`, filters, cmdline, plugin
  entry points.

`pygments/` keeps the exact public interface; when the compiled
extension is absent every seam falls back to the upstream Python,
so the tree also imports and passes the suite without a build.

Custom lexers/formatters (user `RegexLexer` subclasses, entry-point
plugins) are a documented non-port surface: they execute as Python
against the unchanged `Lexer`/`Formatter` API (see ADR-038).

## Accuracy

Upstream suite at pin 2.19.1: **5175 passed / 16 skipped / 0 failed**
(5191 collected; contrast suite included). Against this tree with the
compiled extension: **5175 passed / 16 skipped / 0 failed**; with the
extension blocked (pure-Python fallback): **5175 passed / 16 skipped /
0 failed**. Graded layout: symlink `pygments/` + upstream `tests/`
into one dir (the suite resolves examplefiles via the package path)
and run `pytest tests/`.

## Bench (measured 2026-10-13, CPython 3.12, Linux x86_64)

Driver: `bench/highlight.py` (interleaved medians, 15 reps). Same tree,
extension live vs seam-blocked; import cost identical both arms.

| Workload | Rust | Fallback | Speedup | Notes |
|---|---|---|---|---|
| W1 highlight 110KB Python → HTML | 109.4ms | 114.8ms | 1.05x | full pipeline |
| W2 lex-only 110KB Python | 95.8ms | 96.0ms | 1.00x | `re` engine dominates (ADR-038) |
| W3 highlight 70KB escape-heavy JS → HTML | 28.2ms | 28.0ms | 0.99x | wash; `_translate_parts` cache amortizes escaping |
| W4 `regex_opt` C-keyword list | 0.019ms | 0.188ms | 9.8x | import-time builder path |
| Peak RSS (W1) | 463.0MB | 463.0MB | — | process-level, unchanged |
| Python-alloc peak, tracemalloc (W1) | 1.06MB | 1.05MB | — | Rust-side allocs invisible to tracemalloc; no material change |

Unmeasured (explicit, never guessed): all-formatter sweep, `guess_lexer`
on file corpora, Windows/macOS, PyPy, import-time delta.

## Build

```sh
python3 -m venv .venv && . .venv/bin/activate
pip install maturin
maturin develop --release
python3 -m pytest /path/to/upstream/tests -q
python3 -m pytest tests -q
```

## Layout

- `pygments-core/` — reusable Rust core (`pygments-rust-core`).
- `src/lib.rs` — PyO3 binding (`pygments._pygments`, zero `unsafe`).
- `pygments/lexer.py`, `pygments/regexopt.py`,
  `pygments/formatters/html.py`, `pygments/formatters/svg.py` —
  upstream 2.19.1 plus delegating seams (diff against upstream shows
  only the delegation).
- `tests/` — port's own tests (kernel differentials + fallback
  equivalence; upstream suite is the frozen oracle, not vendored).
