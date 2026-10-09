# markdown-it-py-rust (Rust mirror of executablebooks/markdown-it-py)

Stage-1 Rust mirror of
[`executablebooks/markdown-it-py`](https://github.com/executablebooks/markdown-it-py)
(MIT). Behavior-identical port of `markdown_it/*.py` exposed as the
`markdown_it._markdown_it` extension module via PyO3, with thin
`markdown_it/*.py` re-export shims preserving the original public API
(`MarkdownIt`, `Token`, presets, plugins, CLI, ...).

## Layout (publishable)

One implementation, two distributions:

- `markdown-it-core/` — reusable pure-Rust core crate
  `markdown-it-rust-core` (crates.io). No Python dependency: `rlib`,
  `cargo test` clean. Rust consumers depend on this crate directly.
- `src/` — thin PyO3 binding crate `markdown-it-py-rust` calling that same
  core. Ships on PyPI as distribution **`markdown-it-py-rust`**; the import
  name is unchanged (`import markdown_it`), so users swap the install with
  zero code changes: `pip install markdown-it-py-rust` instead of `pip install
  markdown-it-py`.
- `markdown_it/` — Python shims (one per original module; `tree.py`,
  `cli/`, `presets/`, and `utils.read_fixture_file` stay verbatim Python).
- `release.toml` — release configuration (names, version, platforms,
  Python versions, publishing repo + workflow identity, license, upstream
  attribution). Read by `rustsmith release-prep`; see `docs/RELEASE.md`.

No redesign: same token stream, same plugin rule protocol, same HTML.

## Porting boundary

Everything is ported to Rust except three host-callback seams (matching
the original behavior exactly through the `Host` trait):

- `mdurl` (`normalizeLink`/`normalizeLinkText`) and `linkify-it-py`
  (`md.linkify`) stay Python packages, called through the GIL;
- plugin rule callables (`Ruler.before/after/at/push`, render-rule
  overrides, `highlight`) run as Python callbacks over the live core state;
- `Token.meta` supports JSON-shaped values (`None`/bool/int/float/str);
  arbitrary objects raise `TypeError` on assignment.

## Install

```sh
pip install markdown-it-py-rust
```

```python
from markdown_it import MarkdownIt
MarkdownIt().render("# hi")
```
