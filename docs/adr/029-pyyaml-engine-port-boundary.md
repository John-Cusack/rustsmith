# ADR 029: pyyaml engine port boundary (Rust scan/parse/emit cores, Python object layer)

- Context: PyYAML 6.0.2 `lib/yaml/` is 5890 lines: reader/scanner/parser/
  composer/constructor/resolver/serializer/representer/emitter plus the
  `yaml._yaml` libyaml C extension behind `cyaml.py`. The graded oracle is
  2608 tests including `test_yaml_ext.py`, which compares the C
  (`CLoader`/`CDumper`) and pure-Python (`PyLoader`/`PyDumper`) paths
  token-for-token, event-for-event, mark-for-mark over the whole data corpus.
- Decision: port the three CPU-hot state machines to a reusable
  `yaml-rust-core` crate (no Python dependency) with a thin PyO3 binding
  (`yaml._rust`): scanner (tokens + marks + scan errors), parser (events +
  parse errors), emitter (event stream to text). Keep in Python verbatim:
  reader (I/O, BOM/encoding detection, `ReaderError`), composer,
  constructor, resolver, serializer, representer, loader/dumper API,
  `error`/`events`/`nodes`/`tokens`. Python `Scanner`/`Parser`/`Emitter`
  become facades that lazily pull token/event descriptors from the core
  (one FFI call per token/event) and build the exact upstream object
  types; `lib/yaml/_yaml.py` (new, pure Python) provides `CParser`/
  `CEmitter` over the same cores plus `get_version`/`get_version_string`,
  so `yaml.__with_libyaml__` stays true and the ext suite grades the Rust
  engine on both sides of every comparison (identical by construction).
  The original Python scan/parse/emit loops stay in-file as the
  no-extension fallback (established pyparsing pattern: the tree imports
  without a build).
- Consequences: lazy error ordering, mark counters, and all public
  classes preserved; constructor/representer (arbitrary-Python-object
  glue, no CPU win) never cross FFI. Per-token/event FFI cost is bounded
  and measured in the §5 bench; the char-level loops (the POC hot path)
  leave Python entirely.
- Alternatives: full-Rust `CParser`/`CEmitter` extension classes
  (rejected: MRO-mixing with Python constructors risks layout conflicts;
  Python facades keep init semantics verbatim); batch scan-all upfront
  (rejected: surfaces late-stream scan errors earlier than lazy pull,
  observably different on multi-doc generators).
