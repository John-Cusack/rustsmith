# pyyaml-rust

Stage-1 Rust mirror of [yaml/pyyaml](https://github.com/yaml/pyyaml)
(`pyyaml` 6.0.2, MIT): YAML 1.1 parser and emitter for Python.

## Boundary

The text-processing hot path — the scanner, parser, and emitter state
machines — lives in the reusable `yaml-rust-core` crate (no Python
dependency) with a thin PyO3 binding (`yaml._rust`). Everything else is
the upstream source verbatim:

- ported to Rust: `scanner.py` (tokens, marks, scan errors),
  `parser.py` (events, parse errors), `emitter.py` (event stream to
  text, emitter errors);
- kept in Python: `reader.py` (I/O, BOM/encoding detection),
  `composer.py`, `constructor.py`, `resolver.py`, `serializer.py`,
  `representer.py`, loader/dumper API, `error`/`events`/`nodes`/`tokens`,
  `cyaml.py`.

`yaml/` keeps the exact public interface; when the compiled
extension is absent the modules fall back to the upstream Python loops,
so the tree also imports without a build. `yaml/_yaml.py` provides
`CParser`/`CEmitter` over the same Rust cores, so
`yaml.__with_libyaml__` stays true and the libyaml-path suite grades the
Rust engine on both sides of every comparison.

## Build

```sh
python3 -m venv .venv && . .venv/bin/activate
pip install maturin
maturin develop
python3 -m pytest tests/ -q
```

## Layout

- `yaml-core/` — reusable Rust core (`yaml-rust-core` on crates.io).
- `src/lib.rs` — PyO3 binding (`yaml._rust`).
- `yaml/scanner.py`, `yaml/parser.py`, `yaml/emitter.py` —
  upstream 6.0.2 plus delegating facades (diff against upstream shows
  only the delegation; the original loops stay as the no-extension
  fallback).
- `yaml/_yaml.py` — `CParser`/`CEmitter` over the Rust cores plus
  `get_version`/`get_version_string`.

## Bench (measured 2026-10-14; driver `bench/bench.py`, rows in `bench/results.json`)

| Workload | Track | Baseline | Port | Speedup |
|---|---|---|---|---|
| R1 scan 8.7KB manifests | representative | 47.577ms | 15.648ms | 3.04x |
| R2 safe_load single service | representative | 1.825ms | 0.599ms | 3.05x |
| R3 dump nested object | representative | 0.820ms | 0.427ms | 1.92x |
| S1 load_all 65KB 300-doc stream | showcase | 716.790ms | 166.902ms | 4.29x |
| X1 per-crossing overhead (tiny scan) | overhead | 0.166ms | 0.135ms | 1.23x |
