# ADR 017: markdown-it-py data pack (no tool-code gap)

- Context: `markdown-it-py` (`executablebooks/markdown-it-py`, README KEEP
  rank 5) needed only a `repo-content.json` entry plus a `gen-samples.json`
  pin to run the full flow. Recon succeeds unchanged: package discovered
  from `[project] name` (`markdown-it-py`, flat layout, single-Python
  spine), oracle freezes the pytest suite (1032 collected), the import
  extractor finds an acyclic 60-unit DAG, `pairs3` pins render against the
  pristine original.
- Decision: data-only enablement. Ten porting rules (char-not-byte
  indexing, lookahead regexes, mdurl/linkify host callbacks, opaque plugin
  callables, Token sharing, validated-options shape, emphasis odd-match,
  smartquotes gating, reference normalization, ruler cache invalidation).
  Static held-outs cover the plugin API (12), adversarial renders (11),
  and ruler/state internals (9); the `pairs3` generator pins
  `render(a)/render(b)/render(a+b)`. No adapter/gate/oracle change was
  needed: the flit-core sdist test skips deterministically (absent from the
  grade interpreter, same as baseline), and the pytest>=9 collection
  firewall (ADR-012) already covers the suite.
- Consequences: the port PR carries `mirror/markdown-it-py` against this
  entry. Grade-environment note (not a code gap): the run needs
  `linkify-it-py` plus the `pytest-regressions`/`pytest-datadir` plugins on
  the grading interpreter, since `gfm-like` enables linkify and the suite
  uses `file_regression`/`data_regression` fixtures; baseline and graded
  runs share the interpreter, so parity is unaffected.
- Alternatives: vendoring linkify-it-py into the template (pollutes the
  port; the optional-dependency shape is upstream behavior) — rejected.
- Spec: no SPEC change; data packs remain the only per-repo surface.
