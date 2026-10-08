# pyparsing-rust

Stage-1 Rust mirror of [pyparsing/pyparsing](https://github.com/pyparsing/pyparsing)
(`pyparsing` 3.3.3, MIT): a runtime-constructible grammar API for Python.

## Boundary

The text-scan hot path — the remainder loops of `CharsNotIn.parseImpl`
and `White.parseImpl` — lives in the reusable `pyparsing-rust-core`
crate (no Python dependency) with a thin PyO3 binding
(`pyparsing._pyparsing`). Everything else is the upstream source
verbatim:

- ported to Rust: `CharsNotIn` / `White` scan loops (code-point exact,
  including lone surrogates);
- kept in Python: `ParserElement` dispatch (`_parseNoCache`),
  `preParse` whitespace skipping (FFI floor exceeds 1–3-char skips;
  measured), `Word`/`Regex`/`Literal` (already C-speed via `re`),
  `ParseResults`, `SkipTo`, helpers, `common`, testing, unicode,
  diagram, tools.

`pyparsing/` keeps the exact public interface; when the compiled
extension is absent the shims fall back to the upstream Python loops,
so the tree also imports without a build.

## Build

```sh
python3 -m venv .venv && . .venv/bin/activate
pip install maturin
maturin develop --release
python3 -m pytest tests/ -q
```

## Layout

- `pyparsing-core/` — reusable Rust core (`pyparsing-rust-core` on crates.io).
- `src/lib.rs` — PyO3 binding (`pyparsing._pyparsing`).
- `pyparsing/core.py` — upstream 3.3.3 plus two delegating `parseImpl`
  methods (diff against upstream shows only the delegation).
- `tests/` — upstream suite (frozen oracle; do not weaken).
