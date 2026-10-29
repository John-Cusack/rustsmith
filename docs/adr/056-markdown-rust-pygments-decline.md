# ADR-056: markdown-rust Pygments decline seam

- Context: `codehilite` with `use_pygments=True` (the default) calls into
  Pygments (arbitrary Python lexers/formatters, including user-supplied
  `pygments_formatter` callables) when installed. No Rust equivalent
  exists, and the plain (non-Pygments) branch is a different code path
  with different output.
- Decision: the bridge declines to the pure-Python fallback exactly when
  upstream would engage Pygments (`pygments` importable AND
  `use_pygments` truthy, checked per render). Otherwise the core takes
  the plain `CodeHilite.hilite()` branch byte-identically (fenced
  `local_config` merge with `css_class` composition, treeprocessor
  `pre > code` rewrite, shebang parsing, `linenums`/`lang_prefix`
  handling). Non-scalar `pygments_formatter` callables already force
  fallback via the scalar-config gate.
- Consequences: environments with Pygments installed run
  codehilite-highlighted renders through pure Python (correct, slower);
  Pygments-free environments (including the frozen bench venv) run the
  whole render in Rust. The 40 Pygments-dependent upstream tests pass
  via fallback; plain-path codehilite tests pass via the core.
- Alternatives: vendoring a Rust highlighter (rejected: output would
  diverge from Pygments token streams; not byte-identical).
