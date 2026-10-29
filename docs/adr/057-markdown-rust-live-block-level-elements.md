# ADR-057: markdown-rust live `block_level_elements`

- Context: `Markdown.block_level_elements` is mutable post-construction
  (`md.block_level_elements.append(...)`), and `md_in_html`'s state
  machine reads it per tag. The bridge fingerprint covers registry
  composition and scalar knobs; a mutation would fingerprint-mismatch to
  the (correct but slow) pure fallback, and two upstream md_in_html
  tests mutate it.
- Decision: `block_level_elements` is removed from the fingerprint and
  flows live per render in the `RenderRequest`; `Context::is_block_level`
  reads the live list (defaulting to upstream's
  `util.BLOCK_LEVEL_ELEMENTS` when absent). No other mutable state takes
  this path: everything else the core reads is either immutable per
  render or covered by the fingerprint (which still guards it).
- Consequences: append-after-construction stays on the fast path and
  matches upstream exactly, including the custom-tag md_in_html tests.
  A stale-list fast path is impossible by construction (the list crosses
  every render).
- Alternatives: keep it fingerprinted (rejected: forces fallback on a
  documented upstream mutation pattern that two suite tests exercise).
