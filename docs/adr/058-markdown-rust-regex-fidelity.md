# ADR-058: markdown-rust regex-engine fidelity (`\<`, dump verification)

- Context: upstream patterns are Python-`re` source; the port compiles
  them with `fancy_regex`, whose dialect differs in at least one place
  that bit this lane: `r'\<'` is a literal `<` in Python but a
  word-boundary assertion in fancy-regex. The hand-written smarty
  angled-quote patterns (`(?s)\<\<`) compiled to zero-width matches and
  generated unbounded placeholder growth (a 10s hang on `<< hi >>`,
  fixed by spelling literal `<<`).
- Decision: (1) all 578 upstream patterns are machine-dumped from live
  compiled instances (flags included) and machine-checked by
  `tests/patterns_diff.rs` plus unit-level `test_delim_*` boundary
  checks — hand transcription of pattern source is banned; (2) inline
  verbatim regexes that are not dumped (shebang, angled quotes) are
  spelled in fancy-regex semantics with a comment citing the Python
  original; (3) a `grep` for `\\<`/`\\>` over dumped consts confirmed no
  other literal-angle pattern is affected (zero hits).
- Consequences: one dialect class eliminated structurally; future
  ports inherit the dump-and-diff harness instead of eyeballing.
- Alternatives: switching engines per pattern (rejected: two engines
  doubles the fidelity surface; one engine + machine verification won).
