# python-multipart-rust

Stage-1 Rust mirror of [Kludex/python-multipart](https://github.com/Kludex/python-multipart)
(`python-multipart` 0.0.32, Apache-2.0): a streaming multipart parser for Python.

## Boundary

The CPU hot path — the two byte-level state machines
(`MultipartParser._internal_write`, `QuerystringParser._internal_write`)
plus `parse_options_header`/`_parseparam` — lives in the reusable
`multipart-rust-core` crate (no Python dependency) with a thin PyO3
binding (`python_multipart._python_multipart`). Everything else is the
upstream source verbatim:

- ported to Rust: multipart + querystring state machines, options-header parsing;
- kept in Python: `Field`, `File` (file I/O + `tempfile` spooling),
  `BaseParser`, `OctetStreamParser` (no byte loop), `FormParser`,
  `create_form_parser`, `parse_form`, decoders, exceptions.

`python_multipart/` keeps the exact public interface (including
`MultipartState`/`QuerystringState` member identity and per-chunk error
offsets); the legacy `multipart/` alias package ships unchanged.

## Build

```sh
python3 -m venv .venv && . .venv/bin/activate
pip install maturin
maturin develop --release
python3 -m pytest tests/ -q
```

## Layout

- `multipart-core/` — reusable Rust core (`multipart-rust-core` on crates.io).
- `src/lib.rs` — PyO3 binding (`python_multipart._python_multipart`).
- `python_multipart/` — Python package (hot paths delegate to the core).
- `multipart/` — legacy alias (upstream-verbatim).
- `tests/` — upstream suite (frozen oracle; do not weaken).
